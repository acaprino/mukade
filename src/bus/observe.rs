use super::{
    BoundSession, Bus, BusError, Role,
    room::{active_member, require_room},
    store::now,
    view::{self, ViewItem, ViewKind},
};
use crate::{reader, roots::StoreRoots};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs::OpenOptions, time::Duration};

#[derive(Debug, Clone)]
pub struct ObserveOpts {
    pub budget_chars: usize,
    pub latest: bool,
    pub last_turns: Option<usize>,
}
impl Default for ObserveOpts {
    fn default() -> Self {
        Self {
            budget_chars: 8000,
            latest: false,
            last_turns: None,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservedKind {
    New,
    Updated,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shown {
    pub item: ViewItem,
    pub kind: ObservedKind,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Observation {
    pub shown: Vec<Shown>,
    pub remaining: usize,
    pub left_out: usize,
    pub notices: Vec<String>,
}

fn allowed(
    c: &rusqlite::Connection,
    room: &str,
    observer: &str,
    target: &str,
) -> Result<BoundSession, BusError> {
    require_room(c, room, true)?;
    let member = active_member(c, room, observer)?;
    if member.role != Role::Observer {
        return Err(BusError::Refused(
            "only a member with role observer may observe".into(),
        ));
    }
    if observer == target {
        return Err(BusError::Refused(
            "an observer must target another member".into(),
        ));
    }
    active_member(c,room,target)?.session.ok_or_else(||BusError::Refused(format!("member {target:?} has no bound session; the instance or user must explicitly bind its native session ID")))
}
impl Bus {
    /// In-process consumption: returning this batch advances its cursor. Use observe_with
    /// to confirm local consumption first. Neither method establishes remote client receipt.
    pub fn observe(
        &mut self,
        room: &str,
        observer: &str,
        target: &str,
        roots: &StoreRoots,
        opts: &ObserveOpts,
    ) -> Result<Observation, BusError> {
        self.observe_with(room, observer, target, roots, opts, |_| Ok(()))
    }
    /// Confirm local delivery before committing. A crash can repeat a batch. Network callers
    /// need a separate stage/acknowledge contract; flushing a socket is not client receipt.
    pub fn observe_with(
        &mut self,
        room: &str,
        observer: &str,
        target: &str,
        roots: &StoreRoots,
        opts: &ObserveOpts,
        deliver: impl FnOnce(&Observation) -> Result<(), BusError>,
    ) -> Result<Observation, BusError> {
        if opts.budget_chars == 0 || opts.last_turns == Some(0) {
            return Err(BusError::Refused(
                "budget and last-turns must be positive".into(),
            ));
        }
        let binding = allowed(&self.conn, room, observer, target)?;
        let lock_id = crate::sha256::sha256_hex(format!("{room}\0{observer}").as_bytes());
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(
                self.path()
                    .parent()
                    .expect("bus folder")
                    .join(format!("observe-{lock_id}.lock")),
            )?;
        lock.try_lock().map_err(|e| match e {
            std::fs::TryLockError::WouldBlock => {
                BusError::Refused(format!("member {observer:?} already has an active reader"))
            }
            std::fs::TryLockError::Error(e) => BusError::Failed(format!(
                "cannot lock observer in {}: {e}",
                self.path().display()
            )),
        })?;
        let native = reader::for_harness(binding.harness)
            .map_err(|_| BusError::Failed("unsupported session reader".into()))?;
        let mut session = None;
        for attempt in 0..3 {
            match native.read(&binding.id, roots) {
                Ok(s) => {
                    session = Some(s);
                    break;
                }
                Err(_) if attempt < 2 => std::thread::sleep(Duration::from_millis(200)),
                Err(_) => {}
            }
        }
        let session=session.ok_or_else(||BusError::Failed(format!("cannot read {} session {} after 3 attempts; the transcript may be missing, unreadable or invalid",binding.harness,binding.id)))?;
        let items = view::build(&session);
        let cursor:Option<(String,String,String,u32)>=self.conn.query_row("SELECT binding,seen_json,order_json,projection_version FROM observe_cursors WHERE room=?1 AND observer=?2 AND target=?3",params![room,observer,target],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
        let binding_key = format!("{}:{}", binding.harness, binding.id);
        let first = cursor.is_none();
        let mut seen: BTreeMap<String, String> = BTreeMap::new();
        let mut output = Observation::default();
        if let Some((old_binding, old_seen, old_order, version)) = cursor {
            let previous: BTreeMap<String, String> = serde_json::from_str(&old_seen)?;
            let order: Vec<String> = serde_json::from_str(&old_order)?;
            let retained: Vec<String> = items
                .iter()
                .filter(|v| previous.contains_key(&v.id))
                .map(|v| v.id.clone())
                .collect();
            if old_binding != binding_key || retained != order {
                output.notices.push(
                    "the conversation changed under the cursor; restarting from the active branch"
                        .into(),
                );
            } else {
                seen = previous;
                if version == 1 {
                    for item in &items {
                        if seen
                            .get(&item.id)
                            .is_some_and(|hash| *hash == view::legacy_fingerprint(item))
                        {
                            seen.insert(item.id.clone(), item.fingerprint.clone());
                        }
                    }
                }
            }
        }
        if first && let Some(n) = opts.last_turns {
            let prompts: Vec<usize> = items
                .iter()
                .enumerate()
                .filter(|(_, v)| v.kind == ViewKind::Prompt)
                .map(|(i, _)| i)
                .collect();
            let start = prompts
                .get(prompts.len().saturating_sub(n))
                .copied()
                .unwrap_or(0);
            for v in &items[..start] {
                seen.insert(v.id.clone(), v.fingerprint.clone());
            }
            output.left_out = start;
        }
        let pending: Vec<_> = items
            .iter()
            .filter_map(|item| match seen.get(&item.id) {
                Some(f) if *f == item.fingerprint => None,
                Some(_) => Some(Shown {
                    item: item.clone(),
                    kind: ObservedKind::Updated,
                }),
                None => Some(Shown {
                    item: item.clone(),
                    kind: ObservedKind::New,
                }),
            })
            .collect();
        if !first
            && pending.is_empty()
            && let Ok(candidates) = Bus::candidates(binding.harness, binding.cwd.as_deref(), roots)
        {
            let updated = candidates
                .iter()
                .find(|c| c.id == binding.id)
                .and_then(|c| c.updated.as_ref());
            if candidates
                .iter()
                .any(|c| c.id != binding.id && c.updated.as_ref() > updated)
            {
                output.notices.push("this session has stopped growing; a newer one exists in the same folder (binding unchanged)".into());
            }
        }
        let mut used = 0usize;
        if opts.latest {
            let mut start = pending.len();
            for (i, shown) in pending.iter().enumerate().rev() {
                let size = shown.item.text.chars().count();
                if used > 0 && used.saturating_add(size) > opts.budget_chars {
                    break;
                }
                used = used.saturating_add(size);
                start = i;
            }
            output.left_out += start;
            output.shown = pending[start..].to_vec();
            for s in &pending {
                seen.insert(s.item.id.clone(), s.item.fingerprint.clone());
            }
        } else {
            for shown in &pending {
                let size = shown.item.text.chars().count();
                if !output.shown.is_empty() && used.saturating_add(size) > opts.budget_chars {
                    break;
                }
                used = used.saturating_add(size);
                output.shown.push(shown.clone());
                seen.insert(shown.item.id.clone(), shown.item.fingerprint.clone());
            }
            output.remaining = pending.len() - output.shown.len();
        }
        let order: Vec<_> = items
            .iter()
            .filter(|v| seen.contains_key(&v.id))
            .map(|v| v.id.clone())
            .collect();
        let last = order.last();
        let last_fingerprint = last.and_then(|id| seen.get(id));
        // No database transaction is held during source I/O or output. The observer lock covers
        // the batch; recheck membership and binding before persisting in case another command
        // changed them while the source was being read or the output was being written.
        if allowed(&self.conn, room, observer, target)? != binding {
            return Err(BusError::Refused(
                "binding changed while observing; retry".into(),
            ));
        }
        deliver(&output)?;
        self.write(|tx| {
            if allowed(tx,room,observer,target)?!=binding { return Err(BusError::Refused("binding changed before cursor commit; batch may be repeated".into())); }
            tx.execute("INSERT INTO observe_cursors(room,observer,target,position,last_entry_id,last_fingerprint,updated_at,binding,seen_json,order_json) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10) ON CONFLICT(room,observer,target) DO UPDATE SET position=excluded.position,last_entry_id=excluded.last_entry_id,last_fingerprint=excluded.last_fingerprint,updated_at=excluded.updated_at,binding=excluded.binding,seen_json=excluded.seen_json,order_json=excluded.order_json",params![room,observer,target,order.len() as i64,last,last_fingerprint,now(),binding_key,serde_json::to_string(&seen)?,serde_json::to_string(&order)?])?;
            tx.execute("UPDATE members SET last_command_at=?3 WHERE room=?1 AND name=?2",params![room,observer,now()])?;
            tx.execute("UPDATE observe_cursors SET projection_version=?4 WHERE room=?1 AND observer=?2 AND target=?3",params![room,observer,target,view::PROJECTION_VERSION])?;
            if !output.shown.is_empty() || output.left_out>0 {
                tx.execute("INSERT INTO observations(room,observer,target,at,entries_shown,entries_left_out) VALUES(?1,?2,?3,?4,?5,?6)",params![room,observer,target,now(),output.shown.len() as i64,output.left_out as i64])?;
            }
            Ok(())
        })?;
        Ok(output)
    }
}
