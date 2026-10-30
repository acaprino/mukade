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
    /// Whether the source marked this session archived.
    pub archived: bool,
    /// The native file (or store record) the session lives in.
    pub path: String,
    /// The native parent link, when the harness records one. `None` is "not recorded or no
    /// parent", never a guess.
    pub parent: Option<String>,
}

/// One reader per harness. It turns native records into [`Session`], keeping every source record
/// verbatim in the entry's `raw`.
pub trait Reader {
    fn harness(&self) -> HarnessId;

    /// Every root session in the store, in a stable order.
    fn list(&self, roots: &StoreRoots) -> anyhow::Result<Vec<SessionInfo>>;

    /// Every session the store holds, forks included, in a stable order. A harness that has no
    /// cross-session parent link returns the same list as [`Reader::list`].
    fn list_all(&self, roots: &StoreRoots) -> anyhow::Result<Vec<SessionInfo>> {
        self.list(roots)
    }

    /// Read one session by its native id (an unambiguous prefix is accepted where the store's file
    /// names allow it).
    fn read(&self, id: &str, roots: &StoreRoots) -> anyhow::Result<Session>;
}

/// A session a reader could not hand back for a reason that is a refusal, not a file it failed to
/// understand: the store is not configured, the session is not there, or the id names several.
/// A hop refuses these (exit 2); any other read error means the source could not be read (exit 5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lookup {
    NoStore(String),
    NotFound(String),
    Ambiguous(String),
}

impl std::fmt::Display for Lookup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Lookup::NoStore(what) | Lookup::NotFound(what) | Lookup::Ambiguous(what) => {
                f.write_str(what)
            }
        }
    }
}

impl std::error::Error for Lookup {}

/// The reader for a harness.
pub fn for_harness(harness: HarnessId) -> anyhow::Result<Box<dyn Reader>> {
    Ok(match harness {
        HarnessId::ClaudeCode => Box::new(claude_code::ClaudeCode),
        HarnessId::Codex => Box::new(codex::Codex),
        HarnessId::OpenCode => Box::new(opencode::Opencode),
        HarnessId::Pi => Box::new(pi::Pi),
    })
}
