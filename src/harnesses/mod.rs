//! One adapter per harness. Every decision that depends on which harness a session comes from or
//! goes to is a method of [`Harness`]; the adapter in `<harness>.rs` is the one place it is made.
//! The readers, writers and detectors stay in their own folders; the adapter refers to them.

mod claude_code;
mod codex;
mod opencode;
mod pi;

use crate::model::{HarnessId, Session};
use crate::reader::{Reader, SessionInfo};
use crate::roots::StoreRoots;
use crate::writer::{WriteOpts, Writer, Written, plan::Plan};
use std::path::Path;

/// What mukade knows about one harness.
pub trait Harness: Send + Sync {
    fn id(&self) -> HarnessId;

    /// The reader of this harness's sessions.
    fn reader(&self) -> &'static dyn Reader;

    /// The writer of new sessions in this harness's store. `None` is a harness mukade only reads.
    fn writer(&self) -> Option<&'static dyn Writer>;

    /// Read one session from its native file (or, for OpenCode, its interchange document).
    fn read_file(&self, path: &Path) -> anyhow::Result<Session>;

    /// This harness's store in `roots`.
    fn store_root<'a>(&self, roots: &'a StoreRoots) -> Option<&'a Path>;

    /// Point this harness at an explicit store.
    fn set_store_root(&self, roots: &mut StoreRoots, root: &Path);

    /// Point a process of this harness at its store in `roots`, through the variable it reads.
    fn apply_store_env(&self, roots: &StoreRoots, command: &mut std::process::Command);
}

/// The adapter of a harness.
pub fn get(id: HarnessId) -> &'static dyn Harness {
    match id {
        HarnessId::ClaudeCode => &claude_code::ClaudeCode,
        HarnessId::Codex => &codex::Codex,
        HarnessId::OpenCode => &opencode::OpenCode,
        HarnessId::Pi => &pi::Pi,
    }
}

/// Every adapter, in [`HarnessId::ALL`] order.
pub fn all() -> [&'static dyn Harness; 4] {
    HarnessId::ALL.map(get)
}

/// Set `variable` to the store root, when there is one; otherwise the process inherits it.
fn set_store_env(command: &mut std::process::Command, variable: &str, root: Option<&Path>) {
    if let Some(root) = root {
        command.env(variable, root);
    }
}

// The adapters hand out `&'static` readers and writers; `reader::for_harness` and
// `writer::for_harness` box them. A reference forwards every method, defaults included.

impl<R: Reader + ?Sized> Reader for &R {
    fn harness(&self) -> HarnessId {
        (**self).harness()
    }

    fn list(&self, roots: &StoreRoots) -> anyhow::Result<Vec<SessionInfo>> {
        (**self).list(roots)
    }

    fn list_all(&self, roots: &StoreRoots) -> anyhow::Result<Vec<SessionInfo>> {
        (**self).list_all(roots)
    }

    fn read(&self, id: &str, roots: &StoreRoots) -> anyhow::Result<Session> {
        (**self).read(id, roots)
    }
}

impl<W: Writer + ?Sized> Writer for &W {
    fn harness(&self) -> HarnessId {
        (**self).harness()
    }

    fn offered(&self) -> crate::tools::ToolSet {
        (**self).offered()
    }

    fn write(&self, session: &Session, opts: &WriteOpts) -> anyhow::Result<Written> {
        (**self).write(session, opts)
    }

    fn write_planned(
        &self,
        session: &Session,
        plan: &Plan,
        opts: &WriteOpts,
    ) -> anyhow::Result<Written> {
        (**self).write_planned(session, plan, opts)
    }
}
