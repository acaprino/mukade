//! Receipt-based native observation. SQLite holds identities and hashes, never native text.
use super::{
    Bus,
    chat_domain::{self as domain, ChatError, Result},
    chat_mutation,
    view::{self, ViewItem, ViewKind},
};
use crate::{Session, reader, roots::StoreRoots};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    path::Path,
    time::Duration,
};
use uuid::Uuid;

const CACHE_BYTES: usize = 16 * 1024 * 1024;
const CACHE_SLOTS: usize = 1024;
const INVALIDATION_NOTICE: &str =
    "pending observation batch invalidated: selected source items or order changed";

pub(super) struct PayloadCache {
    entries: VecDeque<(String, String, usize)>,
    bytes: usize,
}
impl Default for PayloadCache {
    fn default() -> Self {
        Self {
            entries: VecDeque::with_capacity(CACHE_SLOTS),
            bytes: 0,
        }
    }
}
impl PayloadCache {
    fn get(&self, id: &str) -> Option<Value> {
        self.entries
            .iter()
            .find(|(key, _, _)| key == id)
            .and_then(|(_, v, _)| serde_json::from_str(v).ok())
    }
    fn insert(&mut self, id: String, value: Value) {
        let value = value.to_string();
        // Cache serialized payloads, avoiding unbounded serde Value allocation overhead.
        // The fixed slot allocation cannot grow; include its entire retained capacity,
        // even after eviction, alongside both String heap capacities and cache metadata.
        let bytes = value.capacity() + id.capacity();
        let overhead = self.entries.capacity() * std::mem::size_of::<(String, String, usize)>()
            + std::mem::size_of::<Self>();
        let available = CACHE_BYTES - overhead;
        if bytes > available {
            return;
        }
        self.remove(&id);
        while self.bytes + bytes > available || self.entries.len() == CACHE_SLOTS {
            if let Some((_, _, size)) = self.entries.pop_front() {
                self.bytes -= size;
            }
        }
        self.bytes += bytes;
        self.entries.push_back((id, value, bytes));
    }
    fn remove(&mut self, id: &str) {
        if let Some(index) = self.entries.iter().position(|(key, _, _)| key == id) {
            self.bytes -= self.entries.remove(index).unwrap().2;
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Context {
    room_id: String,
    room_name: String,
    observer_id: String,
    observer_name: String,
    target_id: String,
    target_name: String,
    generation: i64,
    harness: String,
    session_id: String,
    source_ref: Option<String>,
}
impl Context {
    fn stream(&self) -> String {
        format!(
            "observation:{}:{}:{}",
            self.observer_id, self.target_id, self.generation
        )
    }
}

fn context(conn: &Connection, room_id: &str, handle: &str, target_id: &str) -> Result<Context> {
    let room = domain::room(conn, room_id)?;
    let observer = domain::member(conn, &room, handle)?;
    domain::require_role(&observer.role, &["observer"])?;
    if observer.id == target_id {
        return Err(ChatError::new(
            "target_refused",
            "An observer must select a different room member",
        ));
    }
    let target: Option<(String,i64,String,String,Option<String>)>=conn.query_row(
        "SELECT name,binding_generation,session_harness,session_id,source_ref FROM members WHERE room=?1 AND chat_id=?2 AND left_at IS NULL AND session_id IS NOT NULL",
        params![room.name,target_id], |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?;
    let (target_name, generation, harness, session_id, source_ref) = target.ok_or_else(|| {
        ChatError::new(
            "target_unbound",
            "The target must be an active member with a native session binding",
        )
    })?;
    Ok(Context {
        room_id: room.id,
        room_name: room.name,
        observer_id: observer.id,
        observer_name: observer.name,
        target_id: target_id.into(),
        target_name,
        generation,
        harness,
        session_id,
        source_ref,
    })
}

#[derive(Default)]
struct Cursor {
    revision: i64,
    version: u32,
    source_ref: Option<String>,
    seen: BTreeMap<String, String>,
    order: Vec<String>,
    exists: bool,
}
fn cursor(conn: &Connection, c: &Context) -> Result<Cursor> {
    let row:Option<(i64,u32,Option<String>,String,String)>=conn.query_row(
        "SELECT revision,projection_version,source_ref,seen_json,order_json FROM chat_observe_cursors WHERE observer_id=?1 AND target_id=?2 AND generation=?3",
        params![c.observer_id,c.target_id,c.generation],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?;
    match row {
        Some((revision, version, source_ref, seen, order)) => Ok(Cursor {
            revision,
            version,
            source_ref,
            seen: serde_json::from_str(&seen)?,
            order: serde_json::from_str(&order)?,
            exists: true,
        }),
        None => Ok(Cursor::default()),
    }
}

#[derive(Serialize, Deserialize)]
struct Pick {
    id: String,
    fingerprint: String,
    kind: String,
}
#[derive(Serialize, Deserialize)]
struct Metadata {
    selected: Vec<Pick>,
    checked: Vec<Pick>,
    order_prefix: Vec<String>,
    next_seen: BTreeMap<String, String>,
    next_order: Vec<String>,
    remaining: usize,
    skipped: usize,
    notices: Vec<String>,
    /// A source invalidation is durable until a replacement receipt carries it.
    #[serde(default)]
    invalidation_pending: bool,
    /// Invalidated receipts whose notice this replacement acknowledges atomically.
    #[serde(default)]
    invalidation_receipts: Vec<String>,
}
struct Pending {
    id: String,
    revision: i64,
    version: u32,
    source_ref: String,
    meta: Metadata,
}
fn pending(conn: &Connection, c: &Context) -> Result<Option<Pending>> {
    let row:Option<(String,i64,u32,String,String)>=conn.query_row(
        "SELECT id,expected_revision,projection_version,source_ref,metadata_json FROM chat_observe_batches WHERE observer_id=?1 AND target_id=?2 AND generation=?3 AND status='pending'",
        params![c.observer_id,c.target_id,c.generation], |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?;
    row.map(|(id, revision, version, source_ref, meta)| {
        Ok(Pending {
            id,
            revision,
            version,
            source_ref,
            meta: serde_json::from_str(&meta)?,
        })
    })
    .transpose()
}
pub(super) fn owns_batch(conn: &Connection, id: &str) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM chat_observe_batches WHERE id=?1",
            [id],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

fn invalidated() -> ChatError {
    ChatError::new(
        "batch_invalidated",
        "This observation batch was invalidated; fetch the current observation again",
    )
}
fn conflict() -> ChatError {
    let mut e = ChatError::new(
        "stream_busy",
        "Observation state changed during source reading; retry this stream",
    );
    e.retryable = true;
    e
}

pub(super) fn acknowledge(
    bus: &mut Bus,
    room_id: &str,
    handle: &str,
    id: &str,
    target_scope: Option<&str>,
) -> Result<Value> {
    let tx = bus
        .conn
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let room = domain::room(&tx, room_id)?;
    let observer = domain::member(&tx, &room, handle)?;
    domain::require_role(&observer.role, &["observer"])?;
    let row:Option<(String,i64,i64,u32,String,String,String)>=tx.query_row(
        "SELECT target_id,generation,expected_revision,projection_version,source_ref,metadata_json,status FROM chat_observe_batches WHERE id=?1 AND observer_id=?2",
        params![id,observer.id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).optional()?;
    let (target, generation, revision, version, source, metadata, status) =
        row.ok_or_else(|| {
            ChatError::new(
                "batch_not_found",
                "No observation batch with that ID belongs to this member",
            )
        })?;
    if target_scope.is_some_and(|scope| scope != target) {
        return Err(ChatError::new(
            "batch_not_found",
            "That batch belongs to a different observation stream",
        ));
    }
    if status == "invalidated" {
        return Err(invalidated());
    }
    let c = context(&tx, room_id, handle, &target).map_err(|e| {
        if e.code == "target_unbound" {
            invalidated()
        } else {
            e
        }
    })?;
    if generation != c.generation || c.source_ref.as_ref().is_some_and(|s| s != &source) {
        return Err(invalidated());
    }
    let meta: Metadata = serde_json::from_str(&metadata)?;
    if status == "pending" {
        if cursor(&tx, &c)?.revision != revision {
            return Err(invalidated());
        }
        tx.execute("INSERT INTO chat_observe_cursors(observer_id,target_id,generation,revision,projection_version,source_ref,seen_json,order_json) VALUES(?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(observer_id,target_id,generation) DO UPDATE SET revision=excluded.revision,projection_version=excluded.projection_version,source_ref=excluded.source_ref,seen_json=excluded.seen_json,order_json=excluded.order_json",
            params![observer.id,target,generation,revision+1,version,source,serde_json::to_string(&meta.next_seen)?,serde_json::to_string(&meta.next_order)?])?;
        tx.execute(
            "UPDATE chat_observe_batches SET status='acknowledged' WHERE id=?1",
            [id],
        )?;
        for invalidated_id in &meta.invalidation_receipts {
            tx.execute("UPDATE chat_observe_batches SET metadata_json=json_set(metadata_json,'$.invalidation_pending',json('false')) WHERE id=?1 AND observer_id=?2 AND target_id=?3 AND generation=?4 AND status='invalidated'",
                params![invalidated_id,observer.id,target,generation])?;
        }
        tx.execute("INSERT INTO observations(room,observer,target,at,entries_shown,entries_left_out) VALUES(?1,?2,?3,?4,?5,?6)",params![room.name,c.observer_name,c.target_name,super::store::now(),meta.selected.len() as i64,meta.skipped as i64])?;
        domain::touch(&tx, &observer.id)?;
    }
    let answer = json!({"status":"ok","service_id":domain::service_id(&tx)?,"room_id":room_id,"member_id":observer.id,"target_member_id":target,"binding_generation":generation,"stream_id":c.stream(),"ack_batch_id":id,"acknowledged_through":(revision+1).to_string()});
    tx.commit()?;
    bus.observation_cache.remove(id);
    Ok(answer)
}

pub(super) fn call(bus: &mut Bus, args: &Value, roots: &StoreRoots) -> Result<Value> {
    match prepare(bus, args)? {
        Prepared::Cached(value) => Ok(value),
        Prepared::Read(work) => finish(bus, work.load(roots)?),
    }
}

/// Owned preparation permits native source I/O without borrowing the shared database.
pub(crate) enum Prepared {
    Cached(Value),
    Read(ReadWork),
}
pub(crate) struct ReadWork {
    args: Value,
    c: Context,
    old: Cursor,
    pending: Option<Pending>,
}
pub(crate) struct Loaded {
    work: ReadWork,
    source: String,
    items: Vec<ViewItem>,
}
impl ReadWork {
    pub(crate) fn load(self, roots: &StoreRoots) -> Result<Loaded> {
        let fence = self
            .c
            .source_ref
            .as_deref()
            .or_else(|| self.pending.as_ref().map(|p| p.source_ref.as_str()))
            .or(self.old.source_ref.as_deref());
        let (source, items) = read_source(&self.c, roots, fence).map_err(|mut e| {
            e.ack_batch_id = domain::arg(&self.args, "ack_batch_id").map(str::to_owned);
            e
        })?;
        Ok(Loaded {
            work: self,
            source,
            items,
        })
    }
}
pub(crate) fn prepare(bus: &mut Bus, args: &Value) -> Result<Prepared> {
    super::chat_tools::validate("observe", args)?;
    let ack = domain::arg(args, "ack_batch_id");
    if let Some(id) = ack {
        acknowledge(
            bus,
            domain::required(args, "room_id"),
            domain::required(args, "member_handle"),
            id,
            Some(domain::required(args, "target_member_id")),
        )?;
    }
    let result = prepare_read(bus, args).map_err(|mut e| {
        e.ack_batch_id = ack.map(str::to_owned);
        e
    })?;
    Ok(match result {
        Prepared::Cached(mut result) => {
            if let Some(id) = ack {
                result["ack_batch_id"] = json!(id);
            }
            Prepared::Cached(result)
        }
        work => work,
    })
}
pub(crate) fn finish(bus: &mut Bus, loaded: Loaded) -> Result<Value> {
    let ack = domain::arg(&loaded.work.args, "ack_batch_id").map(str::to_owned);
    let mut result = stage_loaded(bus, loaded).map_err(|mut e| {
        e.ack_batch_id = ack.clone();
        e
    })?;
    if let Some(id) = ack {
        result["ack_batch_id"] = json!(id);
    }
    Ok(result)
}

fn source_unavailable() -> ChatError {
    let mut e = ChatError::new(
        "source_unavailable",
        "The selected native session could not be read after three attempts",
    );
    e.retryable = true;
    e
}
fn read_source(
    c: &Context,
    roots: &StoreRoots,
    source_fence: Option<&str>,
) -> Result<(String, Vec<ViewItem>)> {
    for attempt in 0..3 {
        let read = || -> anyhow::Result<(String, Vec<ViewItem>)> {
            let binding =
                chat_mutation::resolve_binding(roots, &c.harness, &c.session_id, source_fence)?;
            let path = Path::new(&binding.candidate.path);
            let session = crate::harnesses::get(binding.harness).read_bound(path, &c.session_id)?;
            anyhow::ensure!(session.meta.id == c.session_id, "native identity changed");
            Ok((binding.source_ref, view::build(&session)))
        };
        if let Ok(result) = read() {
            return Ok(result);
        }
        if attempt < 2 {
            std::thread::sleep(Duration::from_millis(200));
        }
    }
    Err(source_unavailable())
}

/// Snapshot only the verified selected database; never launch OpenCode (which may migrate it).
pub(crate) fn read_opencode(path: &Path, id: &str) -> anyhow::Result<Session> {
    let mut conn = Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.busy_timeout(Duration::from_secs(2))?;
    let tx = conn.transaction()?;
    let info:Value=tx.query_row("SELECT id,directory,title FROM session_v2 WHERE id=?1",[id],|r|Ok(json!({"id":r.get::<_,String>(0)?,"directory":r.get::<_,Option<String>>(1)?,"title":r.get::<_,Option<String>>(2)?})))?;
    let mut query = tx
        .prepare("SELECT id,type,seq,data FROM session_message WHERE session_id=?1 ORDER BY seq")?;
    let rows = query.query_map([id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, String>(3)?,
        ))
    })?;
    let mut messages = Vec::new();
    for row in rows {
        let (id, kind, seq, data) = row?;
        messages.push(
            json!({"id":id,"type":kind,"seq":seq,"data":serde_json::from_str::<Value>(&data)?}),
        );
    }
    reader::opencode::parse(
        &json!({"session_v2":[info],"session_message":messages}),
        Some(path),
    )
}

fn response(
    service: &str,
    c: &Context,
    id: Option<&str>,
    meta: &Metadata,
    items: &[ViewItem],
) -> Value {
    let projected:Vec<Value>=meta.selected.iter().map(|pick| {
        let item=items.iter().find(|v|v.id==pick.id).expect("verified selected item");
        json!({"kind":pick.kind,"item":item,"projection_version":view::PROJECTION_VERSION,"target_member_id":c.target_id,"binding_generation":c.generation})
    }).collect();
    json!({"status":"ok","service_id":service,"room_id":c.room_id,"member_id":c.observer_id,"target_member_id":c.target_id,"binding_generation":c.generation,"projection_version":view::PROJECTION_VERSION,"stream_id":c.stream(),"batch_id":id,"items":projected,"remaining":meta.remaining,"skipped":meta.skipped,"notices":meta.notices,"selected_range":{"first_item_id":meta.selected.first().map(|p|&p.id),"last_item_id":meta.selected.last().map(|p|&p.id)}})
}
fn pick(item: &ViewItem, seen: &BTreeMap<String, String>) -> Pick {
    Pick {
        id: item.id.clone(),
        fingerprint: item.fingerprint.clone(),
        kind: if seen.contains_key(&item.id) {
            "updated"
        } else {
            "new"
        }
        .into(),
    }
}
fn matches(batch: &Pending, source: &str, items: &[ViewItem]) -> bool {
    batch.version == view::PROJECTION_VERSION
        && batch.source_ref == source
        && items
            .iter()
            .take(batch.meta.order_prefix.len())
            .map(|v| &v.id)
            .eq(batch.meta.order_prefix.iter())
        && batch.meta.checked.iter().all(|p| {
            items
                .iter()
                .any(|v| v.id == p.id && v.fingerprint == p.fingerprint)
        })
}

fn prepare_read(bus: &mut Bus, args: &Value) -> Result<Prepared> {
    let room = domain::required(args, "room_id");
    let handle = domain::required(args, "member_handle");
    let target = domain::required(args, "target_member_id");
    let c = context(&bus.conn, room, handle, target)?;
    let old = cursor(&bus.conn, &c)?;
    let pending = pending(&bus.conn, &c)?;
    if let Some(batch) = &pending {
        if let Some(cached) = bus.observation_cache.get(&batch.id) {
            return Ok(Prepared::Cached(cached));
        }
    }
    Ok(Prepared::Read(ReadWork {
        args: args.clone(),
        c,
        old,
        pending,
    }))
}
fn stage_loaded(bus: &mut Bus, loaded: Loaded) -> Result<Value> {
    let Loaded {
        work:
            ReadWork {
                args,
                c,
                old,
                mut pending,
            },
        source,
        items,
    } = loaded;
    let args = &args;
    let room = domain::required(args, "room_id");
    let handle = domain::required(args, "member_handle");
    let target = domain::required(args, "target_member_id");
    let mut notices = Vec::new();
    let service = domain::service_id(&bus.conn)?;
    if let Some(batch) = &pending {
        if matches(batch, &source, &items) {
            let tx = bus
                .conn
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            if context(&tx, room, handle, target)? != c
                || cursor(&tx, &c)?.revision != batch.revision
            {
                return Err(conflict());
            }
            let still: Option<String> = tx
                .query_row(
                    "SELECT status FROM chat_observe_batches WHERE id=?1",
                    [&batch.id],
                    |r| r.get(0),
                )
                .optional()?;
            if still.as_deref() != Some("pending") {
                return Err(conflict());
            }
            let answer = response(&service, &c, Some(&batch.id), &batch.meta, &items);
            tx.commit()?;
            bus.observation_cache
                .insert(batch.id.clone(), answer.clone());
            return Ok(answer);
        } else {
            // Persist invalidation even if selecting its replacement later fails (for
            // example because a requested turn boundary cannot be identified).
            let tx = bus
                .conn
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            if context(&tx, room, handle, target)? != c
                || cursor(&tx, &c)?.revision != old.revision
                || pending_id(&tx, &c)?.as_deref() != Some(batch.id.as_str())
            {
                return Err(conflict());
            }
            tx.execute(
                "UPDATE chat_observe_batches SET status='invalidated',metadata_json=json_set(metadata_json,'$.invalidation_pending',json('true')) WHERE id=?1",
                [&batch.id],
            )?;
            tx.commit()?;
            bus.observation_cache.remove(&batch.id);
            pending = None;
        }
    }
    let invalidation_receipts = {
        let mut query=bus.conn.prepare("SELECT id FROM chat_observe_batches WHERE observer_id=?1 AND target_id=?2 AND generation=?3 AND status='invalidated' AND json_extract(metadata_json,'$.invalidation_pending')=1 ORDER BY id")?;
        query
            .query_map(params![c.observer_id, c.target_id, c.generation], |r| {
                r.get::<_, String>(0)
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?
    };
    if !invalidation_receipts.is_empty() {
        notices.push(INVALIDATION_NOTICE.into());
    }

    let mut seen = old.seen.clone();
    let retained: Vec<_> = items
        .iter()
        .filter(|v| seen.contains_key(&v.id))
        .map(|v| v.id.clone())
        .collect();
    if old.exists
        && (retained != old.order
            || old.source_ref.as_ref().is_some_and(|s| s != &source)
            || ![1, view::PROJECTION_VERSION].contains(&old.version))
    {
        notices.push(
            "conversation rewound or changed order; restarting from the active branch".into(),
        );
        seen.clear();
    } else if old.version == 1 {
        for item in &items {
            if seen
                .get(&item.id)
                .is_some_and(|hash| *hash == view::legacy_fingerprint(item))
            {
                seen.insert(item.id.clone(), item.fingerprint.clone());
            }
        }
    }
    let latest = args["latest"].as_bool().unwrap_or(false);
    let turns = args["last_turns"].as_u64();
    if latest && turns.is_some() {
        return Err(ChatError::invalid(
            "latest and last_turns cannot be combined",
        ));
    }
    let mut skipped_items = Vec::new();
    if !old.exists {
        if let Some(turns) = turns {
            let prompts: Vec<usize> = items
                .iter()
                .enumerate()
                .filter(|(_, v)| v.kind == ViewKind::Prompt)
                .map(|(i, _)| i)
                .collect();
            if prompts.is_empty() {
                return Err(ChatError::new(
                    "turn_boundaries_unknown",
                    "No positively identified human prompt defines a turn boundary",
                ));
            }
            let start = prompts[prompts.len().saturating_sub(turns as usize)];
            for item in &items[..start] {
                skipped_items.push(pick(item, &seen));
                seen.insert(item.id.clone(), item.fingerprint.clone());
            }
        }
    }
    let changed: Vec<&ViewItem> = items
        .iter()
        .filter(|v| seen.get(&v.id) != Some(&v.fingerprint))
        .collect();
    let mut meta = Metadata {
        selected: Vec::new(),
        checked: Vec::new(),
        order_prefix: Vec::new(),
        next_seen: seen.clone(),
        next_order: Vec::new(),
        remaining: changed.len(),
        skipped: skipped_items.len(),
        notices,
        invalidation_pending: false,
        invalidation_receipts,
    };
    let id = Uuid::new_v4().to_string();
    let budget = domain::number(args, "budget_chars", 8000) as usize;
    let ack_overhead = domain::arg(args, "ack_batch_id")
        .map(|id| json!({"ack_batch_id":id}).to_string().chars().count() - 1)
        .unwrap_or(0);
    let selection: Vec<&ViewItem> = if latest {
        changed.iter().rev().copied().collect()
    } else {
        changed.clone()
    };
    for item in selection {
        meta.selected.push(pick(item, &seen));
        meta.remaining = changed.len() - meta.selected.len();
        if meta.selected.len() > 1
            && response(&service, &c, Some(&id), &meta, &items)
                .to_string()
                .chars()
                .count()
                + ack_overhead
                > budget
        {
            meta.selected.pop();
            break;
        }
    }
    if latest {
        meta.selected.reverse();
        let omitted = changed.len() - meta.selected.len();
        for item in &changed[..omitted] {
            skipped_items.push(pick(item, &seen));
        }
        meta.skipped = skipped_items.len();
        meta.remaining = 0;
    } else {
        meta.remaining = changed.len() - meta.selected.len();
    }
    meta.checked = skipped_items;
    meta.checked.extend(meta.selected.iter().map(|p| Pick {
        id: p.id.clone(),
        fingerprint: p.fingerprint.clone(),
        kind: p.kind.clone(),
    }));
    for p in &meta.checked {
        meta.next_seen.insert(p.id.clone(), p.fingerprint.clone());
    }
    meta.next_order = items
        .iter()
        .filter(|v| meta.next_seen.contains_key(&v.id))
        .map(|v| v.id.clone())
        .collect();
    let through = items
        .iter()
        .rposition(|v| meta.checked.iter().any(|p| p.id == v.id))
        .map(|i| i + 1)
        .unwrap_or(0);
    meta.order_prefix = items[..through].iter().map(|v| v.id.clone()).collect();
    let has_delta = !meta.checked.is_empty();
    let answer = response(
        &service,
        &c,
        has_delta.then_some(id.as_str()),
        &meta,
        &items,
    );
    let tx = bus
        .conn
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    if context(&tx, room, handle, target)? != c || cursor(&tx, &c)?.revision != old.revision {
        return Err(conflict());
    }
    let current = pending_id(&tx, &c)?;
    if current.as_deref() != pending.as_ref().map(|p| p.id.as_str()) {
        return Err(conflict());
    }
    if let Some(batch) = &pending {
        tx.execute(
            "UPDATE chat_observe_batches SET status='invalidated' WHERE id=?1 AND status='pending'",
            [&batch.id],
        )?;
    }
    if has_delta {
        tx.execute("INSERT INTO chat_observe_batches(id,observer_id,target_id,generation,expected_revision,projection_version,source_ref,metadata_json,status) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'pending')",params![id,c.observer_id,c.target_id,c.generation,old.revision,view::PROJECTION_VERSION,source,serde_json::to_string(&meta)?])?;
        domain::touch(&tx, &c.observer_id)?;
    } else if old.exists && old.version == 1 && meta.notices.is_empty() {
        // Rehash already acknowledged legacy entries once, without consuming any new
        // native item. Otherwise a legacy prefix hash could hide later suffix changes
        // indefinitely when there is no new batch requiring a receipt.
        tx.execute("UPDATE chat_observe_cursors SET projection_version=?4,source_ref=?5,seen_json=?6,order_json=?7,revision=revision+1 WHERE observer_id=?1 AND target_id=?2 AND generation=?3",
            params![c.observer_id,c.target_id,c.generation,view::PROJECTION_VERSION,source,serde_json::to_string(&meta.next_seen)?,serde_json::to_string(&meta.next_order)?])?;
    }
    tx.commit()?;
    if let Some(batch) = pending {
        bus.observation_cache.remove(&batch.id);
    }
    if has_delta {
        bus.observation_cache.insert(id, answer.clone());
    }
    Ok(answer)
}
fn pending_id(conn: &Connection, c: &Context) -> Result<Option<String>> {
    Ok(conn.query_row("SELECT id FROM chat_observe_batches WHERE observer_id=?1 AND target_id=?2 AND generation=?3 AND status='pending'",params![c.observer_id,c.target_id,c.generation],|r|r.get(0)).optional()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn split_source_load_revalidates_binding_and_preserves_committed_receipt() {
        let base = std::env::var_os("MUKADE_TEST_SCRATCH_ROOT")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let dir = base.join(format!("mukade-tests-observe-split-{}", Uuid::new_v4()));
        let home = dir.join("home");
        let source =
            home.join("sessions/project/source_01a17e17-9f07-743d-8072-8b3b13d65ab5.jsonl");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/pi/source/2026-10-27T16-00-39-689Z_01a17e17-9f07-743d-8072-8b3b13d65ab5.jsonl"),&source).unwrap();
        let roots = StoreRoots {
            pi_agent_dir: Some(home),
            ..Default::default()
        };
        let mut bus = Bus::open_in(&dir.join("bus")).unwrap();
        let mutate = |bus: &mut Bus, op: &str, mut args: Value| {
            args["request_id"] = json!(Uuid::new_v4().to_string());
            bus.chat_call(op, args, &roots).unwrap()
        };
        let room = mutate(&mut bus, "open", json!({"name":"split"}));
        let target = mutate(
            &mut bus,
            "join",
            json!({"room_id":room["room_id"],"harness":"pi","role":"peer","session_id":"01a17e17-9f07-743d-8072-8b3b13d65ab5"}),
        );
        let observer = mutate(
            &mut bus,
            "join",
            json!({"room_id":room["room_id"],"harness":"pi","role":"observer"}),
        );
        let mut args = json!({"room_id":room["room_id"],"member_handle":observer["member_handle"],"target_member_id":target["member_id"]});
        let first = call(&mut bus, &args, &roots).unwrap();
        args["ack_batch_id"] = first["batch_id"].clone();
        let Prepared::Read(work) = prepare(&mut bus, &args).unwrap() else {
            panic!("ack must prepare new source read")
        };
        let loaded = work.load(&roots).unwrap();
        mutate(
            &mut bus,
            "bind",
            json!({"room_id":room["room_id"],"member_handle":target["member_handle"],"harness":"pi","session_id":"01a17e17-9f07-743d-8072-8b3b13d65ab5"}),
        );
        let e = finish(&mut bus, loaded).unwrap_err();
        assert_eq!(e.code, "stream_busy");
        assert_eq!(e.ack_batch_id.as_deref(), first["batch_id"].as_str());
    }
    #[test]
    fn cache_accounts_retained_allocation_and_evicts_without_durable_payloads() {
        let mut cache = PayloadCache::default();
        for i in 0..20 {
            cache.insert(i.to_string(), json!({"text":"x".repeat(1024*1024)}));
            let allocated = cache.bytes
                + cache.entries.capacity() * std::mem::size_of::<(String, String, usize)>()
                + std::mem::size_of::<PayloadCache>();
            assert!(allocated <= CACHE_BYTES);
        }
        assert!(cache.get("0").is_none());
        assert!(cache.get("19").is_some());
        cache.insert("oversized".into(), json!({"text":"x".repeat(CACHE_BYTES)}));
        assert!(cache.get("oversized").is_none());
        for i in 0..2000 {
            cache.insert(format!("small-{i}"), json!({"text":"x"}));
        }
        assert_eq!(cache.entries.len(), CACHE_SLOTS);
        assert_eq!(cache.entries.capacity(), CACHE_SLOTS);
        assert!(cache.get("small-1999").is_some());
    }
}
