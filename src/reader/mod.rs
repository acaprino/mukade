//! Readers: native session store → [`Session`] (docs/session-interchange-spec.md §8.1).

pub mod claude_code;
pub mod codex;
pub mod json;
pub mod opencode;
pub mod pi;

use crate::model::{HarnessId, Session};
use crate::roots::StoreRoots;

/// What a reader knows about a session before reading it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionInfo {
    pub id: String,
    pub title: Option<String>,
    pub cwd: Option<String>,
    pub updated: Option<String>,
    /// The native file (or store record) the session lives in.
    pub path: String,
}

/// One reader per harness. It turns native records into [`Session`], keeping every source record
/// verbatim in the entry's `raw`.
pub trait Reader {
    fn harness(&self) -> HarnessId;

    /// Every session in the store, in a stable order.
    fn list(&self, roots: &StoreRoots) -> anyhow::Result<Vec<SessionInfo>>;

    /// Read one session by its native id (an unambiguous prefix is accepted where the store's file
    /// names allow it).
    fn read(&self, id: &str, roots: &StoreRoots) -> anyhow::Result<Session>;
}

/// The reader for a harness.
pub fn for_harness(harness: HarnessId) -> anyhow::Result<Box<dyn Reader>> {
    Ok(match harness {
        HarnessId::ClaudeCode => Box::new(claude_code::ClaudeCode),
        HarnessId::Codex => Box::new(codex::Codex),
        HarnessId::OpenCode => Box::new(opencode::Opencode),
        HarnessId::Pi => Box::new(pi::Pi),
    })
}
