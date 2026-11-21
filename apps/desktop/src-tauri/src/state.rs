//! What the app keeps between commands: the installations it lists and what each one listed, the
//! session the reader has open, and the current hop or import, including its outcome until dismissed.

use crate::error::{CmdError, CmdResult};
use crate::locations::{self, Location};
use crate::view::ReceiptView;
use mukade::model::Session;
use mukade::reader::SessionInfo;
use mukade::roots::StoreRoots;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

pub struct Cached {
    pub installation: String,
    pub id: String,
    pub session: Arc<Session>,
}

pub struct Inner {
    pub standard: StoreRoots,
    locations: Mutex<Vec<Location>>,
    /// What each installation's reader listed last, by key: the hop, the resume and the export
    /// need a session's folder, parent and readability without listing the store again.
    listed: Mutex<HashMap<String, Vec<SessionInfo>>>,
    /// Held for a whole scan-and-list, so two listings never interleave their results.
    inventory: Mutex<()>,
    cached: Mutex<Option<Cached>>,
    writes: Mutex<Writes>,
    write_finished: Condvar,
}

#[derive(Clone)]
pub struct AppState(Arc<Inner>);

impl AppState {
    pub fn new(standard: StoreRoots) -> Self {
        let locations = locations::standard_locations(&standard);
        Self(Arc::new(Inner {
            standard,
            locations: Mutex::new(locations),
            listed: Mutex::new(HashMap::new()),
            inventory: Mutex::new(()),
            cached: Mutex::new(None),
            writes: Mutex::new(Writes::default()),
            write_finished: Condvar::new(),
        }))
    }

    pub fn standard(&self) -> &StoreRoots {
        &self.0.standard
    }

    fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
        mutex
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn set_locations(&self, locations: Vec<Location>) {
        *Self::locked(&self.0.locations) = locations;
    }

    pub fn locations(&self) -> Vec<Location> {
        Self::locked(&self.0.locations).clone()
    }

    pub fn location(&self, key: &str) -> CmdResult<Location> {
        Self::locked(&self.0.locations)
            .iter()
            .find(|location| location.key() == key)
            .cloned()
            .ok_or_else(|| CmdError::not_found(format!("no installation {key}: reload the list")))
    }

    /// One scan-and-list at a time; the guard ends it.
    pub fn begin_inventory(&self) -> MutexGuard<'_, ()> {
        Self::locked(&self.0.inventory)
    }

    pub fn set_listed(&self, key: &str, sessions: Vec<SessionInfo>) {
        Self::locked(&self.0.listed).insert(key.to_string(), sessions);
    }

    pub fn listed(&self, key: &str) -> Option<Vec<SessionInfo>> {
        Self::locked(&self.0.listed).get(key).cloned()
    }

    /// One listed session of `key`.
    pub fn listed_session(&self, key: &str, id: &str) -> CmdResult<SessionInfo> {
        Self::locked(&self.0.listed)
            .get(key)
            .and_then(|sessions| sessions.iter().find(|info| info.id == id))
            .cloned()
            .ok_or_else(|| {
                CmdError::not_found(format!("{id} is not listed in {key}: reload the list"))
            })
    }

    pub fn cache_session(&self, installation: &str, id: &str, session: Session) -> Arc<Session> {
        let session = Arc::new(session);
        *Self::locked(&self.0.cached) = Some(Cached {
            installation: installation.to_string(),
            id: id.to_string(),
            session: session.clone(),
        });
        session
    }

    pub fn cached_session(&self, installation: &str, id: &str) -> Option<Arc<Session>> {
        Self::locked(&self.0.cached)
            .as_ref()
            .filter(|cached| cached.installation == installation && cached.id == id)
            .map(|cached| cached.session.clone())
    }

    /// Reserve the slot before scheduling disk work. Retransmissions attach to the same
    /// operation even when it finished before the replacement window connected.
    pub fn start_write(
        &self,
        kind: WriteKind,
        schedule: impl FnOnce(u64),
    ) -> CmdResult<WriteOperation> {
        let mut writes = Self::locked(&self.0.writes);
        if writes.exiting {
            return Err(CmdError::refused("Mukade is closing"));
        }
        if let Some(current) = &writes.current {
            return Ok(current.clone());
        }
        writes.sequence += 1;
        let operation = WriteOperation {
            id: writes.sequence,
            kind,
            outcome: WriteOutcome::Running,
        };
        writes.current = Some(operation.clone());
        drop(writes);
        schedule(operation.id);
        Ok(operation)
    }

    pub fn finish_write(&self, id: u64, result: CmdResult<ReceiptView>) {
        let mut writes = Self::locked(&self.0.writes);
        if let Some(operation) = &mut writes.current {
            if operation.id == id && matches!(operation.outcome, WriteOutcome::Running) {
                operation.outcome = match result {
                    Ok(receipt) => WriteOutcome::Completed {
                        receipt: Box::new(receipt),
                    },
                    Err(error) => WriteOutcome::Failed { error },
                };
                self.0.write_finished.notify_all();
            }
        }
    }

    pub fn write_status(&self) -> Option<WriteOperation> {
        Self::locked(&self.0.writes).current.clone()
    }

    pub fn acknowledge_write(&self, id: u64) {
        let mut writes = Self::locked(&self.0.writes);
        if writes
            .current
            .as_ref()
            .is_some_and(|op| op.id == id && !matches!(op.outcome, WriteOutcome::Running))
        {
            writes.current = None;
        }
    }

    pub fn writes_running(&self) -> usize {
        usize::from(Self::locked(&self.0.writes).running())
    }

    /// Used off the UI thread after window recovery has failed.
    pub fn drain_writes_for_exit(&self) {
        let mut writes = Self::locked(&self.0.writes);
        writes.exiting = true;
        drop(
            self.0
                .write_finished
                .wait_while(writes, |writes| writes.running())
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        );
    }
}

#[derive(Default)]
struct Writes {
    sequence: u64,
    current: Option<WriteOperation>,
    exiting: bool,
}

impl Writes {
    fn running(&self) -> bool {
        self.current
            .as_ref()
            .is_some_and(|op| matches!(op.outcome, WriteOutcome::Running))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WriteKind {
    Hop,
    Import,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WriteOperation {
    pub id: u64,
    pub kind: WriteKind,
    #[serde(flatten)]
    pub outcome: WriteOutcome,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum WriteOutcome {
    Running,
    Completed { receipt: Box<ReceiptView> },
    Failed { error: CmdError },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_installation_is_found_by_its_key_and_an_unknown_one_is_refused() {
        let state = AppState::new(StoreRoots::default());
        let first = state.locations()[0].key();
        assert!(state.location(&first).is_ok());
        let error = state.location("nope").unwrap_err();
        assert_eq!(error.kind, "not_found");
    }

    #[test]
    fn a_reconnected_renderer_attaches_until_the_result_is_dismissed() {
        let state = AppState::new(StoreRoots::default());
        let write = state.start_write(WriteKind::Import, |_| {}).unwrap();
        state.acknowledge_write(write.id);
        assert_eq!(state.writes_running(), 1);
        let another = state.clone();
        assert_eq!(
            another
                .start_write(WriteKind::Hop, |_| panic!("second writer"))
                .unwrap(),
            write
        );

        state.finish_write(write.id, Err(CmdError::failed("disk full")));
        let finished = another.write_status().unwrap();
        assert_eq!(
            serde_json::to_value(&finished).unwrap(),
            serde_json::json!({
                "id": write.id, "kind": "import", "status": "failed",
                "error": {"kind": "failed", "message": "disk full"}
            })
        );
        assert_eq!(
            another
                .start_write(WriteKind::Import, |_| panic!("lost outcome"))
                .unwrap(),
            finished
        );
        assert_eq!(state.writes_running(), 0);
        another.acknowledge_write(write.id);
        assert!(state.write_status().is_none());
        let next = state.start_write(WriteKind::Hop, |_| {}).unwrap();
        assert_ne!(write.id, next.id);
        state.acknowledge_write(write.id);
        assert_eq!(state.write_status(), Some(next));
    }

    #[test]
    fn recovery_exit_keeps_the_worker_alive_and_closes_admission() {
        let state = AppState::new(StoreRoots::default());
        let pending = state.start_write(WriteKind::Import, |_| {}).unwrap();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                state.drain_writes_for_exit();
                assert_eq!(state.writes_running(), 0);
            });
            // Observe the admission decision under the same lock as a submit.
            while !AppState::locked(&state.0.writes).exiting {
                std::thread::yield_now();
            }
            let late = state.start_write(WriteKind::Hop, |_| {});
            state.finish_write(pending.id, Err(CmdError::failed("stopped")));
            assert!(late.is_err());
        });
        state.acknowledge_write(pending.id);
        assert!(
            state
                .start_write(WriteKind::Hop, |_| panic!("writer after drain"))
                .is_err()
        );
    }

    #[test]
    fn the_open_session_is_cached_for_the_installation_and_id_it_was_read_for() {
        use crate::scratch::Scratch;
        let state = AppState::new(StoreRoots::default());
        let scratch = Scratch::new("cache");
        let roots = crate::testing::claude_store(&scratch);
        let location = crate::locations::standard_locations(&roots).remove(0);
        let session = crate::sessions::read(&location, crate::testing::CLAUDE_FIXTURE).unwrap();
        state.cache_session("claude_code", "x", session);
        assert!(state.cached_session("claude_code", "x").is_some());
        assert!(state.cached_session("claude_code", "y").is_none());
        assert!(state.cached_session("codex", "x").is_none());
    }
}
