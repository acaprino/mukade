use super::{
    Bus, BusError,
    store::{event, now},
};
use crate::{
    HarnessId,
    reader::{self, SessionInfo},
    roots::StoreRoots,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Observer,
    Peer,
    Lead,
    Worker,
}
impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Observer => "observer",
            Self::Peer => "peer",
            Self::Lead => "lead",
            Self::Worker => "worker",
        }
    }
}
impl std::str::FromStr for Role {
    type Err = BusError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "observer" => Ok(Self::Observer),
            "peer" => Ok(Self::Peer),
            "lead" => Ok(Self::Lead),
            "worker" => Ok(Self::Worker),
            _ => Err(BusError::Refused(format!(
                "unknown role {s:?}; use observer, peer, lead or worker"
            ))),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundSession {
    pub harness: HarnessId,
    pub id: String,
    pub cwd: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Member {
    pub room: String,
    pub name: String,
    pub harness: String,
    pub role: Role,
    pub joined_at: String,
    pub left_at: Option<String>,
    pub last_command_at: Option<String>,
    pub session: Option<BoundSession>,
}
#[derive(Debug, Clone)]
pub struct JoinRequest {
    pub room: String,
    pub harness: String,
    pub role: Role,
    pub name: Option<String>,
    pub rejoin: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomRow {
    pub name: String,
    pub opened_at: String,
    pub closed_at: Option<String>,
    pub opened_by: Option<String>,
    pub kept: bool,
    pub members: usize,
    pub events: usize,
    pub last_activity: Option<String>,
}
pub type RoomSummary = RoomRow;

fn valid(s: &str, max: usize) -> bool {
    !s.is_empty()
        && s.len() <= max
        && s.bytes()
            .next()
            .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_-".contains(&b))
}
pub(crate) fn require_room(c: &Connection, room: &str, open: bool) -> Result<(), BusError> {
    let closed: Option<Option<String>> = c
        .query_row("SELECT closed_at FROM rooms WHERE name=?1", [room], |r| {
            r.get(0)
        })
        .optional()?;
    match closed {
        None => Err(BusError::Refused(format!(
            "no such room {room:?}; open the room first"
        ))),
        Some(Some(_)) if open => Err(BusError::Refused(format!(
            "room {room:?} is closed; reopen it before changing it"
        ))),
        _ => Ok(()),
    }
}
pub(crate) fn members_in(c: &Connection, room: &str) -> Result<Vec<Member>, BusError> {
    require_room(c, room, false)?;
    let mut q=c.prepare("SELECT room,name,harness,role,joined_at,left_at,last_command_at,session_harness,session_id,session_cwd FROM members WHERE room=?1 ORDER BY joined_at,rowid")?;
    let rows = q.query_map([room], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, Option<String>>(5)?,
            r.get::<_, Option<String>>(6)?,
            r.get::<_, Option<String>>(7)?,
            r.get::<_, Option<String>>(8)?,
            r.get::<_, Option<String>>(9)?,
        ))
    })?;
    rows.map(|row| {
        let (room, name, harness, role, joined_at, left_at, last_command_at, sh, si, cwd) = row?;
        let session = match (sh, si) {
            (Some(h), Some(id)) => Some(BoundSession {
                harness: h.parse().map_err(|_| {
                    BusError::Failed("invalid bound harness in bus database".into())
                })?,
                id,
                cwd,
            }),
            _ => None,
        };
        Ok(Member {
            room,
            name,
            harness,
            role: role.parse()?,
            joined_at,
            left_at,
            last_command_at,
            session,
        })
    })
    .collect()
}
pub(crate) fn active_member(c: &Connection, room: &str, name: &str) -> Result<Member, BusError> {
    members_in(c, room)?
        .into_iter()
        .find(|m| m.name == name && m.left_at.is_none())
        .ok_or_else(|| {
            BusError::Refused(format!(
                "member {name:?} is not active in room {room:?}; join or rejoin first"
            ))
        })
}
impl Bus {
    pub fn open_room(&mut self, name: &str) -> Result<(), BusError> {
        if !valid(name, 64) {
            return Err(BusError::Refused("room name must be 1-64 lowercase letters, digits, underscores or hyphens, starting with a letter or digit".into()));
        }
        self.write(|tx| {
            if tx
                .query_row("SELECT 1 FROM rooms WHERE name=?1", [name], |r| {
                    r.get::<_, i64>(0)
                })
                .optional()?
                .is_some()
            {
                return Err(BusError::Refused(format!("room {name:?} already exists")));
            }
            tx.execute(
                "INSERT INTO rooms(name,opened_at) VALUES(?1,?2)",
                params![name, now()],
            )?;
            event(tx, name, "open", None, json!({"name":name}))
        })
    }
    pub fn close_room(&mut self, name: &str) -> Result<(), BusError> {
        self.write(|tx| {
            require_room(tx, name, true)?;
            tx.execute(
                "UPDATE rooms SET closed_at=?2 WHERE name=?1",
                params![name, now()],
            )?;
            event(tx, name, "close", None, json!({}))
        })
    }
    pub fn reopen_room(&mut self, name: &str) -> Result<(), BusError> {
        self.write(|tx| {
            require_room(tx, name, false)?;
            if tx.execute(
                "UPDATE rooms SET closed_at=NULL WHERE name=?1 AND closed_at IS NOT NULL",
                [name],
            )? == 0
            {
                return Err(BusError::Refused(format!("room {name:?} is already open")));
            }
            event(tx, name, "reopen", None, json!({}))
        })
    }
    pub fn rooms(&self) -> Result<Vec<RoomRow>, BusError> {
        let mut q=self.conn.prepare("SELECT r.name,r.opened_at,r.closed_at,r.opened_by,r.kept,(SELECT count(*) FROM members m WHERE m.room=r.name),(SELECT count(*) FROM events e WHERE e.room=r.name),(SELECT max(at) FROM events e WHERE e.room=r.name) FROM rooms r ORDER BY r.opened_at,r.name")?;
        Ok(q.query_map([], |r| {
            Ok(RoomRow {
                name: r.get(0)?,
                opened_at: r.get(1)?,
                closed_at: r.get(2)?,
                opened_by: r.get(3)?,
                kept: r.get(4)?,
                members: r.get::<_, i64>(5)? as usize,
                events: r.get::<_, i64>(6)? as usize,
                last_activity: r.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?)
    }
    pub fn delete_room(&mut self, name: &str) -> Result<RoomSummary, BusError> {
        let summary = self
            .rooms()?
            .into_iter()
            .find(|r| r.name == name)
            .ok_or_else(|| BusError::Refused(format!("no such room {name:?}")))?;
        self.write(|tx| {
            require_room(tx, name, false)?;
            let closed: Option<String> =
                tx.query_row("SELECT closed_at FROM rooms WHERE name=?1", [name], |r| {
                    r.get(0)
                })?;
            if closed.is_none() {
                return Err(BusError::Refused(
                    "close the room before deleting it".into(),
                ));
            }
            tx.execute("DELETE FROM rooms WHERE name=?1", [name])?;
            Ok(())
        })?;
        self.reclaim();
        Ok(summary)
    }
    pub fn members(&self, room: &str) -> Result<Vec<Member>, BusError> {
        members_in(&self.conn, room)
    }
    pub fn join(&mut self, req: &JoinRequest) -> Result<Member, BusError> {
        self.join_bound(req, None)
    }
    /// Join and explicitly bind in one transaction; a refusal leaves no partial membership.
    pub fn join_with_session(
        &mut self,
        req: &JoinRequest,
        harness: HarnessId,
        id: &str,
        roots: &StoreRoots,
    ) -> Result<Member, BusError> {
        let binding = Self::resolve_binding(harness, id, roots)?;
        self.join_bound(req, Some(binding))
    }
    fn join_bound(
        &mut self,
        req: &JoinRequest,
        binding: Option<BoundSession>,
    ) -> Result<Member, BusError> {
        let harness = if req.harness == "claude" {
            "claude_code"
        } else {
            req.harness.as_str()
        };
        if !valid(harness, 24) {
            return Err(BusError::Refused("invalid harness label".into()));
        }
        if let Some(name) = &req.name
            && (!valid(name, 32) || name == "all")
        {
            return Err(BusError::Refused(
                "invalid member name (1-32 lowercase letters, digits, '_' or '-', never 'all')"
                    .into(),
            ));
        }
        if req.name.is_some() && req.rejoin.is_some() {
            return Err(BusError::Refused("choose either a name or rejoin".into()));
        }
        self.write(|tx| {
            require_room(tx,&req.room,true)?;
            let members=members_in(tx,&req.room)?;
            let stamp=now();
            let name;
            if let Some(previous)=&req.rejoin {
                let m=members.iter().find(|m|m.name==*previous).ok_or_else(||BusError::Refused(format!("no previous member {previous:?}")))?;
                if m.left_at.is_none() { return Err(BusError::Refused(format!("member {previous:?} is still active"))); }
                if m.harness!=harness { return Err(BusError::Refused("rejoin must use the member's original harness".into())); }
                name=previous.clone();
                tx.execute("UPDATE members SET left_at=NULL,role=?3,last_command_at=?4,session_harness=NULL,session_id=NULL,session_cwd=NULL WHERE room=?1 AND name=?2",params![req.room,name,req.role.as_str(),stamp])?;
                event(tx,&req.room,"rejoin",Some(&name),json!({"harness":harness,"role":req.role,"session":null}))?;
            } else {
                name=if let Some(chosen)=&req.name { chosen.clone() } else {
                    let prefix=if harness=="claude_code" { "claude" } else { harness };
                    (1..).map(|n|format!("{prefix}-{n}")).find(|n|!members.iter().any(|m|m.name==*n)).expect("available name")
                };
                if members.iter().any(|m|m.name==name) { return Err(BusError::Refused(format!("member name {name:?} has already been used"))); }
                tx.execute("INSERT INTO members(room,name,harness,role,joined_at,last_command_at) VALUES(?1,?2,?3,?4,?5,?5)",params![req.room,name,harness,req.role.as_str(),stamp])?;
                event(tx,&req.room,"join",Some(&name),json!({"harness":harness,"role":req.role}))?;
            }
            if let Some(binding)=&binding {
                if harness!=binding.harness.as_str() { return Err(BusError::Refused("a member may bind only its own harness session".into())); }
                if members.iter().any(|m|m.name!=name && m.left_at.is_none() && m.session.as_ref().is_some_and(|s|s.harness==binding.harness && s.id==binding.id)) { return Err(BusError::Refused("session is already bound to another member of this room".into())); }
                tx.execute("UPDATE members SET session_harness=?3,session_id=?4,session_cwd=?5 WHERE room=?1 AND name=?2",params![req.room,name,binding.harness.as_str(),binding.id,binding.cwd])?;
                event(tx,&req.room,"bind",Some(&name),json!({"session":binding}))?;
            }
            active_member(tx,&req.room,&name)
        })
    }
    pub fn leave(&mut self, room: &str, member: &str) -> Result<(), BusError> {
        self.write(|tx| {
            require_room(tx, room, true)?;
            active_member(tx, room, member)?;
            tx.execute(
                "UPDATE members SET left_at=?3,last_command_at=?3 WHERE room=?1 AND name=?2",
                params![room, member, now()],
            )?;
            event(tx, room, "leave", Some(member), json!({}))
        })
    }
    pub fn candidates(
        harness: HarnessId,
        cwd: Option<&str>,
        roots: &StoreRoots,
    ) -> Result<Vec<SessionInfo>, BusError> {
        let mut rows = reader::for_harness(harness)
            .and_then(|r| r.list_all(roots))
            .map_err(|e| BusError::Refused(format!("cannot list {harness} sessions: {e}")))?;
        // Pi's listing has no updated field. Enrich candidates locally without changing the
        // existing reader/list contract; modification time is a hint, never identity evidence.
        for row in &mut rows {
            if row.updated.is_none() {
                row.updated = std::fs::metadata(&row.path)
                    .and_then(|m| m.modified())
                    .ok()
                    .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339());
            }
        }
        if let Some(cwd) = cwd {
            rows.retain(|r| r.cwd.as_deref().is_some_and(|c| same_directory(c, cwd)));
        }
        rows.sort_by(|a, b| b.updated.cmp(&a.updated).then(a.id.cmp(&b.id)));
        Ok(rows)
    }
    pub fn bind(
        &mut self,
        room: &str,
        member: &str,
        harness: HarnessId,
        session_id: &str,
        roots: &StoreRoots,
    ) -> Result<BoundSession, BusError> {
        let binding = Self::resolve_binding(harness, session_id, roots)?;
        self.write(|tx| {
            require_room(tx,room,true)?; let own=active_member(tx,room,member)?;
            if own.harness!=harness.as_str() { return Err(BusError::Refused("a member may bind only its own harness session".into())); }
            if members_in(tx,room)?.iter().any(|m|m.name!=member && m.left_at.is_none() && m.session.as_ref().is_some_and(|s|s.harness==harness && s.id==binding.id)) { return Err(BusError::Refused("session is already bound to another member of this room".into())); }
            tx.execute("UPDATE members SET session_harness=?3,session_id=?4,session_cwd=?5,last_command_at=?6 WHERE room=?1 AND name=?2",params![room,member,harness.as_str(),binding.id,binding.cwd,now()])?;
            tx.execute("DELETE FROM observe_cursors WHERE room=?1 AND target=?2",params![room,member])?;
            event(tx,room,"bind",Some(member),json!({"session":binding}))?; Ok(binding.clone())
        })
    }
    fn resolve_binding(
        harness: HarnessId,
        session_id: &str,
        roots: &StoreRoots,
    ) -> Result<BoundSession, BusError> {
        if session_id.is_empty() {
            return Err(BusError::Refused(
                "an explicit session ID is required".into(),
            ));
        }
        let candidates = Self::candidates(harness, None, roots)?;
        let exact = candidates.iter().find(|c| c.id == session_id);
        let matches: Vec<_> = candidates
            .iter()
            .filter(|c| c.id.starts_with(session_id))
            .collect();
        let found=exact.or_else(||(matches.len()==1).then(||matches[0])).ok_or_else(||BusError::Refused(format!("session {session_id:?} is unknown or ambiguous; select an explicit ID from the {harness} candidates")))?;
        if let Some(reason) = &found.unreadable {
            return Err(BusError::Refused(format!(
                "session cannot be bound: {reason}"
            )));
        }
        let binding = BoundSession {
            harness,
            id: found.id.clone(),
            cwd: found.cwd.clone(),
        };
        Ok(binding)
    }
}
fn same_directory(a: &str, b: &str) -> bool {
    let a = a.replace('\\', "/");
    let b = b.replace('\\', "/");
    if cfg!(windows) {
        a.trim_end_matches('/')
            .eq_ignore_ascii_case(b.trim_end_matches('/'))
    } else {
        a.trim_end_matches('/') == b.trim_end_matches('/')
    }
}
