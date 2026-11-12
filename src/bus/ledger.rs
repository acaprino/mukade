use super::{Bus, BusError, Member, RoomRow, room::require_room};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::Write;

#[derive(Debug, Clone, Default)]
pub struct HistoryFilter {
    pub member: Option<String>,
    pub kind: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub mentions: Option<String>,
    pub text: Option<String>,
    pub from_end: bool,
    pub limit: Option<usize>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRow {
    pub seq: i64,
    pub at: String,
    pub kind: String,
    pub member: Option<String>,
    pub body: Value,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservationRow {
    pub id: i64,
    pub room: String,
    pub observer: String,
    pub target: String,
    pub at: String,
    pub entries_shown: usize,
    pub entries_left_out: usize,
}
impl Bus {
    pub fn ledger_rooms(&self) -> Result<Vec<RoomRow>, BusError> {
        self.rooms()
    }
    pub fn ledger_members(&self, room: &str) -> Result<Vec<Member>, BusError> {
        self.members(room)
    }
    pub fn ledger_history(
        &self,
        room: &str,
        filter: &HistoryFilter,
    ) -> Result<Vec<EventRow>, BusError> {
        require_room(&self.conn, room, false)?;
        let order = if filter.from_end { "DESC" } else { "ASC" };
        let mut query=self.conn.prepare(&format!("SELECT seq,at,kind,member,body_json FROM events WHERE room=?1 AND (?2 IS NULL OR member=?2) AND (?3 IS NULL OR kind=?3) AND (?4 IS NULL OR at>=?4) AND (?5 IS NULL OR at<=?5) AND (?6 IS NULL OR EXISTS (SELECT 1 FROM json_each(events.body_json,'$.mentions') WHERE value=?6)) AND (?7 IS NULL OR instr(json_extract(body_json,'$.text'),?7)>0) ORDER BY seq {order} LIMIT ?8"))?;
        let rows = query.query_map(
            params![
                room,
                filter.member,
                filter.kind,
                filter.since,
                filter.until,
                filter.mentions,
                filter.text,
                filter
                    .limit
                    .map(|n| n.min(i64::MAX as usize) as i64)
                    .unwrap_or(-1)
            ],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, String>(4)?,
                ))
            },
        )?;
        let mut events = Vec::new();
        for row in rows {
            let (seq, at, kind, member, body) = row?;
            events.push(EventRow {
                seq,
                at,
                kind,
                member,
                body: serde_json::from_str(&body)?,
            });
        }
        if filter.from_end {
            events.reverse();
        }
        Ok(events)
    }
    pub fn ledger_observations(&self, room: &str) -> Result<Vec<ObservationRow>, BusError> {
        require_room(&self.conn, room, false)?;
        let mut q=self.conn.prepare("SELECT id,room,observer,target,at,entries_shown,entries_left_out FROM observations WHERE room=?1 ORDER BY id")?;
        Ok(q.query_map([room], |r| {
            Ok(ObservationRow {
                id: r.get(0)?,
                room: r.get(1)?,
                observer: r.get(2)?,
                target: r.get(3)?,
                at: r.get(4)?,
                entries_shown: r.get::<_, i64>(5)? as usize,
                entries_left_out: r.get::<_, i64>(6)? as usize,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?)
    }
    pub fn export_room(&self, room: &str, out: &mut dyn Write) -> Result<(), BusError> {
        let snapshot = self.conn.unchecked_transaction()?;
        let row = self
            .rooms()?
            .into_iter()
            .find(|r| r.name == room)
            .ok_or_else(|| BusError::Refused(format!("no such room {room:?}")))?;
        writeln!(out, "{}", json!({"type":"room","room":row}))?;
        for member in self.ledger_members(room)? {
            writeln!(out, "{}", json!({"type":"member","member":member}))?;
        }
        for event in self.ledger_history(room, &HistoryFilter::default())? {
            writeln!(out, "{}", json!({"type":"event","event":event}))?;
        }
        for observation in self.ledger_observations(room)? {
            writeln!(
                out,
                "{}",
                json!({"type":"observation","observation":observation})
            )?;
        }
        out.flush()?;
        snapshot.commit()?;
        Ok(())
    }
}
