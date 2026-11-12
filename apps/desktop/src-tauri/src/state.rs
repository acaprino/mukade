//! What the app keeps between commands: the installations it lists and what each one listed, the
//! session the reader has open, and how many writes are running (the window refuses to close while
//! one does).

use crate::error::{CmdError, CmdResult};
use crate::locations::{self, Location};
use mukade::model::Session;
use mukade::reader::SessionInfo;
use mukade::roots::StoreRoots;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

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
    writes: AtomicUsize,
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
            writes: AtomicUsize::new(0),
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

    /// A write (a hop or an import) is starting; the guard ends it.
    pub fn begin_write(&self) -> WriteGuard {
        self.0.writes.fetch_add(1, Ordering::SeqCst);
        WriteGuard(self.0.clone())
    }

    pub fn writes_running(&self) -> usize {
        self.0.writes.load(Ordering::SeqCst)
    }
}

pub struct WriteGuard(Arc<Inner>);

impl Drop for WriteGuard {
    fn drop(&mut self) {
        self.0.writes.fetch_sub(1, Ordering::SeqCst);
    }
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
    fn the_write_counter_follows_its_guards() {
        let state = AppState::new(StoreRoots::default());
        assert_eq!(state.writes_running(), 0);
        let first = state.begin_write();
        let second = state.begin_write();
        assert_eq!(state.writes_running(), 2);
        drop(first);
        assert_eq!(state.writes_running(), 1);
        drop(second);
        assert_eq!(state.writes_running(), 0);
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
