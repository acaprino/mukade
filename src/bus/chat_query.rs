use crate::{HarnessId, roots::StoreRoots};
use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use uuid::Uuid;

use super::{
    Bus,
    chat_domain::{self as domain, ChatError, EVENT_SELECT, Result, Room, arg, required},
    chat_mutation::{candidate_json, candidates, normalize_harness},
};

const LIFETIME: Duration = Duration::from_secs(600);
const MAX_CURSORS: usize = 4096;
const MAX_SNAPSHOT_BYTES: usize = 16 * 1024 * 1024;
const MAX_PAGE_BYTES: usize = 1024 * 1024;

pub(super) struct Snapshots {
    cursors: HashMap<String, (Arc<Snapshot>, Position)>,
    pub(super) lifetime: Duration,
}

#[derive(Default)]
pub(crate) struct MetadataRows {
    items: Vec<Value>,
    bytes: usize,
}

impl MetadataRows {
    fn push(&mut self, item: Value) -> Result<()> {
        self.bytes += item.to_string().len();
        if self.bytes > MAX_SNAPSHOT_BYTES {
            return Err(ChatError::new(
                "snapshot_capacity",
                "The requested snapshot exceeds the metadata limit",
            ));
        }
        self.items.push(item);
        Ok(())
    }
}

#[derive(Clone, Copy, Default)]
struct Position {
    rows: usize,
    event: i64,
}

struct Snapshot {
    created: Instant,
    id: String,
    filters: Value,
    operation: String,
    room_id: Option<String>,
    member_id: Option<String>,
    high: Option<i64>,
    rows: MetadataRows,
}

impl Default for Snapshots {
    fn default() -> Self {
        Self {
            cursors: HashMap::new(),
            lifetime: LIFETIME,
        }
    }
}

impl Snapshots {
    fn sweep(&mut self) {
        self.cursors
            .retain(|_, (snapshot, _)| snapshot.created.elapsed() < self.lifetime);
    }

    fn next(&mut self, snapshot: Arc<Snapshot>, position: Position) -> Result<String> {
        if let Some((cursor, _)) = self.cursors.iter().find(|(_, (s, p))| {
            Arc::ptr_eq(s, &snapshot) && p.rows == position.rows && p.event == position.event
        }) {
            return Ok(cursor.clone());
        }
        let mut unique: HashMap<&str, usize> = self
            .cursors
            .values()
            .map(|(snapshot, _)| (snapshot.id.as_str(), snapshot.rows.bytes))
            .collect();
        unique.insert(&snapshot.id, snapshot.rows.bytes);
        if self.cursors.len() >= MAX_CURSORS || unique.values().sum::<usize>() > MAX_SNAPSHOT_BYTES
        {
            return Err(ChatError {
                retryable: true,
                ..ChatError::new(
                    "snapshot_capacity",
                    "Too many active query snapshots; retry after expiration",
                )
            });
        }
        let token = Uuid::new_v4().to_string();
        self.cursors.insert(token.clone(), (snapshot, position));
        Ok(token)
    }

    pub(super) fn revoke_room(&mut self, room: &str) {
        self.cursors
            .retain(|_, (snapshot, _)| snapshot.room_id.as_deref() != Some(room));
    }
}

pub(super) fn call(bus: &mut Bus, op: &str, args: &Value, roots: &StoreRoots) -> Result<Value> {
    let rows = load_candidates(op, args, roots)?;
    finish(bus, op, args, rows)
}
pub(crate) fn load_candidates(op: &str, args: &Value, roots: &StoreRoots) -> Result<MetadataRows> {
    let mut candidate_rows = MetadataRows::default();
    if op == "candidates" && arg(args, "cursor").is_none() {
        super::chat_tools::validate(op, args)?;
        let harness: HarnessId = normalize_harness(required(args, "harness"))
            .parse()
            .map_err(|_| ChatError::invalid("Select a supported native harness"))?;
        for row in candidates(harness, arg(args, "cwd"), roots)? {
            candidate_rows.push(candidate_json(roots, harness, &row))?;
        }
    }
    Ok(candidate_rows)
}
pub(crate) fn finish(
    bus: &mut Bus,
    op: &str,
    args: &Value,
    candidate_rows: MetadataRows,
) -> Result<Value> {
    bus.chat_snapshots.sweep();
    let mut filters = args.clone();
    let filter_fields = filters.as_object_mut().unwrap();
    for field in ["cursor", "limit", "member_handle"] {
        filter_fields.shift_remove(field);
    }
    filter_fields.retain(|_, value| !value.is_null());
    filters.sort_all_objects();
    let limit = domain::number(args, "limit", 100) as usize;
    let cursor = arg(args, "cursor");

    // Candidate discovery is the only query that touches native files. It must not run under
    // the snapshot transaction. Later pages come from the captured candidate metadata.

    let tx = bus.conn.unchecked_transaction()?;
    let room = arg(args, "room_id")
        .map(|id| domain::room(&tx, id))
        .transpose()?;
    let member_id = match (&room, arg(args, "member_handle")) {
        (Some(room), Some(handle)) => Some(domain::member(&tx, room, handle)?.id),
        _ => None,
    };
    let (snapshot, position) = if let Some(token) = cursor {
        let (snapshot, position) = bus.chat_snapshots.cursors.get(token).ok_or_else(|| {
            ChatError::new(
                "snapshot_expired",
                "This query cursor expired or belongs to another service",
            )
        })?;
        if snapshot.operation != op
            || snapshot.filters != filters
            || snapshot.member_id != member_id
        {
            return Err(ChatError::new(
                "cursor_mismatch",
                "The cursor belongs to a different query or member",
            ));
        }
        (Arc::clone(snapshot), *position)
    } else {
        let high = room
            .as_ref()
            .map(|r| domain::high_water(&tx, r))
            .transpose()?;
        let rows = match op {
            "rooms" => room_rows(&tx)?,
            "members" => {
                let mut rows = MetadataRows::default();
                collect_members(&tx, room.as_ref().unwrap(), &mut rows, false)?;
                rows
            }
            "export" => export_metadata(&tx, room.as_ref().unwrap())?,
            "candidates" => candidate_rows,
            _ => MetadataRows::default(),
        };
        (
            Arc::new(Snapshot {
                created: Instant::now(),
                id: Uuid::new_v4().to_string(),
                filters,
                operation: op.to_owned(),
                room_id: room.as_ref().map(|r| r.id.clone()),
                member_id,
                high,
                rows,
            }),
            Position::default(),
        )
    };

    let (items, next) = page(&tx, room.as_ref(), &snapshot, position, limit)?;
    let mut result = domain::result(&tx, "ok")?;
    if let Some(room) = &room {
        result["room_id"] = json!(room.id);
    }
    if let Some(high) = snapshot.high {
        result["high_water"] = json!(high.to_string());
    }
    result["snapshot_id"] = json!(snapshot.id);
    result["filters"] = snapshot.filters.clone();
    result["items"] = json!(items);
    tx.commit()?;
    result["next_cursor"] = json!(
        next.map(|p| bus.chat_snapshots.next(snapshot, p))
            .transpose()?
    );
    Ok(result)
}

fn page(
    conn: &Connection,
    room: Option<&Room>,
    snapshot: &Snapshot,
    mut pos: Position,
    limit: usize,
) -> Result<(Vec<Value>, Option<Position>)> {
    let mut items = Vec::new();
    let mut bytes = 0;
    for item in &snapshot.rows.items[pos.rows..] {
        if !add_item(&mut items, &mut bytes, limit, item.clone()) {
            return Ok((items, Some(pos)));
        }
        pos.rows += 1;
    }
    if let Some(room) = room.filter(|_| matches!(snapshot.operation.as_str(), "ledger" | "export"))
    {
        let filters = &snapshot.filters;
        let since = timestamp(arg(filters, "since"))?;
        let until = timestamp(arg(filters, "until"))?;
        if since
            .as_ref()
            .zip(until.as_ref())
            .is_some_and(|(a, b)| a > b)
        {
            return Err(ChatError::invalid("since must be at or before until"));
        }
        let mut stmt = conn.prepare(&format!("{EVENT_SELECT}
            WHERE e.room=?1 AND e.seq>?2 AND e.seq<=?3
            AND (?4 IS NULL OR e.member=?4 OR e.chat_actor_id=?4 OR (?4='local_admin' AND e.member IS NULL))
            AND (?5 IS NULL OR e.kind=?5) AND (?6 IS NULL OR e.at>=?6) AND (?7 IS NULL OR e.at<=?7)
            AND (?8 IS NULL OR EXISTS(SELECT 1 FROM json_each(e.body_json,'$.mentions') WHERE value=?8))
            AND (?9 IS NULL OR instr(coalesce(json_extract(e.body_json,'$.text'),json_extract(e.body_json,'$.reason'),''),?9)>0)
            ORDER BY e.seq LIMIT ?10"))?;
        let mut rows = stmt.query(params![
            room.name,
            pos.event,
            snapshot.high,
            arg(filters, "actor"),
            arg(filters, "kind"),
            since,
            until,
            arg(filters, "mention"),
            arg(filters, "text"),
            (limit - items.len() + 1) as i64
        ])?;
        while let Some(row) = rows.next()? {
            let mut item = domain::event_value(row, &room.id)?;
            if snapshot.operation == "export" {
                item = json!({"type":"event","event":item});
            }
            if !add_item(&mut items, &mut bytes, limit, item) {
                return Ok((items, Some(pos)));
            }
            pos.event = row.get(0)?;
        }
    }
    Ok((items, None))
}

fn add_item(items: &mut Vec<Value>, bytes: &mut usize, limit: usize, item: Value) -> bool {
    let size = item.to_string().len() + 1;
    if items.len() == limit || (!items.is_empty() && *bytes + size > MAX_PAGE_BYTES) {
        return false;
    }
    *bytes += size;
    items.push(item);
    true
}

fn timestamp(s: Option<&str>) -> Result<Option<String>> {
    s.map(|s| {
        DateTime::parse_from_rfc3339(s)
            .map(|d| {
                d.with_timezone(&Utc)
                    .to_rfc3339_opts(SecondsFormat::Millis, true)
            })
            .map_err(|_| ChatError::invalid("since and until must be RFC 3339 timestamps"))
    })
    .transpose()
}

const ROOM_SELECT: &str = "SELECT r.chat_id,r.name,r.opened_at,r.closed_at,r.opened_by,r.kept,
    (SELECT count(*) FROM members m WHERE m.room=r.name),
    (SELECT count(*) FROM events e WHERE e.room=r.name),
    (SELECT max(at) FROM events e WHERE e.room=r.name),
    (SELECT coalesce(max(seq),0) FROM events e WHERE e.room=r.name)
    FROM rooms r";

fn room_json(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    Ok(
        json!({"room_id":row.get::<_,String>(0)?,"name":row.get::<_,String>(1)?,
        "opened_at":row.get::<_,String>(2)?,"closed_at":row.get::<_,Option<String>>(3)?,
        "opened_by":row.get::<_,Option<String>>(4)?,"kept":row.get::<_,bool>(5)?,
        "members":row.get::<_,i64>(6)?,"events":row.get::<_,i64>(7)?,
        "last_activity":row.get::<_,Option<String>>(8)?,"revision":row.get::<_,i64>(9)?.to_string()}),
    )
}

pub(super) fn room_summary(conn: &Connection, room: &Room) -> Result<Value> {
    Ok(conn.query_row(
        &format!("{ROOM_SELECT} WHERE r.chat_id=?1"),
        [&room.id],
        room_json,
    )?)
}

fn room_rows(conn: &Connection) -> Result<MetadataRows> {
    let mut stmt = conn.prepare(&format!("{ROOM_SELECT} ORDER BY r.opened_at,r.name"))?;
    let mut rows = MetadataRows::default();
    for row in stmt.query_map([], room_json)? {
        rows.push(row?)?;
    }
    Ok(rows)
}

fn collect_members(
    conn: &Connection,
    room: &Room,
    rows: &mut MetadataRows,
    export: bool,
) -> Result<()> {
    let fields = "SELECT m.chat_id,m.name,m.harness,m.role,m.joined_at,m.left_at,m.last_command_at,
        m.session_harness,m.session_id,m.session_cwd,m.source_ref,m.binding_generation";
    let source = if export {
        "FROM members m"
    } else {
        ",m.chat_cursor,s.instruction,s.event,s.signal,s.reason
        FROM members m LEFT JOIN chat_signals s ON s.rowid=(
            SELECT latest.rowid FROM chat_signals latest WHERE latest.worker=m.chat_id
            ORDER BY latest.event DESC LIMIT 1
        )"
    };
    let mut stmt = conn.prepare(&format!(
        "{fields} {source} WHERE m.room=?1 ORDER BY m.joined_at,m.rowid"
    ))?;
    for row in stmt.query_map([&room.name], |row| {
        let id: Option<String> = row.get(8)?;
        let binding = match id {
            Some(id) => json!({"id":id,"harness":row.get::<_,Option<String>>(7)?,
                "cwd":row.get::<_,Option<String>>(9)?,"source_ref":row.get::<_,Option<String>>(10)?}),
            None => Value::Null,
        };
        let mut member = json!({"room_id":room.id,"member_id":row.get::<_,String>(0)?,"name":row.get::<_,String>(1)?,
            "harness":row.get::<_,String>(2)?,"role":row.get::<_,String>(3)?,
            "joined_at":row.get::<_,String>(4)?,"left_at":row.get::<_,Option<String>>(5)?,
            "last_command_at":row.get::<_,Option<String>>(6)?,"session":binding,
            "binding_generation":row.get::<_,i64>(11)?});
        if export {
            return Ok(json!({"type":"member","member":member}));
        }
        member["read_cursor"] = json!(row.get::<_,i64>(12)?.to_string());
        member["last_signal"] = match row.get::<_,Option<i64>>(13)? {
            Some(instruction) => json!({
                "instruction_event_id":instruction.to_string(),
                "event_id":row.get::<_,i64>(14)?.to_string(),
                "state":row.get::<_,String>(15)?,
                "reason":row.get::<_,Option<String>>(16)?,
            }),
            None => Value::Null,
        };
        Ok(member)
    })? {
        rows.push(row?)?;
    }
    Ok(())
}

fn export_metadata(conn: &Connection, room: &Room) -> Result<MetadataRows> {
    let mut rows = MetadataRows::default();
    rows.push(json!({"type":"room","room":room_summary(conn,room)?}))?;
    collect_members(conn, room, &mut rows, true)?;
    let mut stmt = conn.prepare(
        "SELECT id,observer,target,at,entries_shown,entries_left_out
        FROM observations WHERE room=?1 ORDER BY id",
    )?;
    for row in stmt.query_map([&room.name], |r| Ok(json!({"type":"observation","observation":{
        "id":r.get::<_,i64>(0)?.to_string(),"room_id":room.id,"observer":r.get::<_,String>(1)?,
        "target":r.get::<_,String>(2)?,"at":r.get::<_,String>(3)?,"entries_shown":r.get::<_,i64>(4)?,
        "entries_left_out":r.get::<_,i64>(5)?
    }})))? {
        rows.push(row?)?;
    }
    Ok(rows)
}
