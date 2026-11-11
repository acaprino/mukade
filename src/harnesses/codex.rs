//! Codex CLI: rollouts under `CODEX_HOME/sessions/`.

use super::Harness;
use crate::model::{HarnessId, Session};
use crate::reader::Reader;
use crate::roots::StoreRoots;
use crate::writer::Writer;
use std::path::Path;

pub(super) struct Codex;

impl Harness for Codex {
    fn id(&self) -> HarnessId {
        HarnessId::Codex
    }

    fn reader(&self) -> &'static dyn Reader {
        &crate::reader::codex::Codex
    }

    fn writer(&self) -> Option<&'static dyn Writer> {
        Some(&crate::writer::codex::Codex)
    }

    fn read_file(&self, path: &Path) -> anyhow::Result<Session> {
        crate::reader::codex::read_file(path)
    }

    fn store_root<'a>(&self, roots: &'a StoreRoots) -> Option<&'a Path> {
        roots.codex_home.as_deref()
    }

    fn set_store_root(&self, roots: &mut StoreRoots, root: &Path) {
        roots.codex_home = Some(root.to_path_buf());
    }

    fn apply_store_env(&self, roots: &StoreRoots, command: &mut std::process::Command) {
        super::set_store_env(command, "CODEX_HOME", self.store_root(roots));
    }
}
