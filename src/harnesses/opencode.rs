//! OpenCode: one SQLite database, named by `OPENCODE_DB`.

use super::Harness;
use crate::model::{HarnessId, Session};
use crate::reader::Reader;
use crate::roots::StoreRoots;
use crate::writer::Writer;
use std::path::Path;

pub(super) struct OpenCode;

impl Harness for OpenCode {
    fn id(&self) -> HarnessId {
        HarnessId::OpenCode
    }

    fn reader(&self) -> &'static dyn Reader {
        &crate::reader::opencode::Opencode
    }

    fn writer(&self) -> Option<&'static dyn Writer> {
        Some(&crate::writer::opencode::Opencode)
    }

    fn read_file(&self, path: &Path) -> anyhow::Result<Session> {
        crate::reader::opencode::read_file(path)
    }

    fn store_root<'a>(&self, roots: &'a StoreRoots) -> Option<&'a Path> {
        roots.opencode_db.as_deref()
    }

    /// The store is the database file: an existing folder means its `opencode.db`.
    fn set_store_root(&self, roots: &mut StoreRoots, root: &Path) {
        roots.opencode_db = Some(if root.is_dir() {
            root.join("opencode.db")
        } else {
            root.to_path_buf()
        });
    }

    /// The database is passed, or the variable removed so the process does not inherit another
    /// store (L1). XDG variables must still be supplied by the caller when OpenCode needs them.
    fn apply_store_env(&self, roots: &StoreRoots, command: &mut std::process::Command) {
        let variable = crate::catalog::store_env(self.id());
        match self.store_root(roots) {
            Some(db) => command.env(variable, db),
            None => command.env_remove(variable),
        };
    }
}
