//! The contract every harness's store layout adapter fulfils, and the list of adapters.

use super::folder::Folder;
use super::{claude_code, codex, opencode, pi};
use crate::model::HarnessId;
use std::path::{Path, PathBuf};

/// How one harness lays out its store on disk. An adapter only reads, and recognizes a store by
/// concrete files: a folder name alone is never evidence.
pub(crate) trait StoreLayout: Sync {
    fn harness(&self) -> HarnessId;

    /// What shows that `folder` is this harness's store; empty when it is not.
    fn evidence(&self, folder: &Folder) -> Vec<String>;

    /// What shows that `file` is this harness's whole store; empty when it is not. Only a harness
    /// whose store is a single file (OpenCode's database) recognizes one.
    fn file_evidence(&self, _file: &Path) -> Vec<String> {
        Vec::new()
    }

    /// The store as the harness's own variable names it, for a folder [`Self::evidence`]
    /// recognized.
    fn store(&self, folder: &Path) -> PathBuf {
        folder.to_path_buf()
    }

    /// The store's folders that hold only its sessions: a scan does not descend into them.
    fn session_folders(&self) -> &'static [&'static str] {
        &[]
    }
}

/// Every adapter, in the order detection reports harnesses.
pub(crate) fn layouts() -> [&'static dyn StoreLayout; 4] {
    [
        &claude_code::ClaudeCodeLayout,
        &codex::CodexLayout,
        &opencode::OpenCodeLayout,
        &pi::PiLayout,
    ]
}
