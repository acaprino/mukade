//! Pi: sessions under `PI_CODING_AGENT_DIR/sessions/`.

use super::Harness;
use crate::model::{HarnessId, Session};
use crate::reader::Reader;
use crate::roots::StoreRoots;
use crate::writer::Writer;
use std::path::Path;

pub(super) struct Pi;

impl Harness for Pi {
    fn id(&self) -> HarnessId {
        HarnessId::Pi
    }

    fn reader(&self) -> &'static dyn Reader {
        &crate::reader::pi::Pi
    }

    fn writer(&self) -> Option<&'static dyn Writer> {
        Some(&crate::writer::pi::Pi)
    }

    fn read_file(&self, path: &Path) -> anyhow::Result<Session> {
        crate::reader::pi::read_file(path)
    }

    fn store_root<'a>(&self, roots: &'a StoreRoots) -> Option<&'a Path> {
        roots.pi_agent_dir.as_deref()
    }

    fn set_store_root(&self, roots: &mut StoreRoots, root: &Path) {
        roots.pi_agent_dir = Some(root.to_path_buf());
    }

    fn apply_store_env(&self, roots: &StoreRoots, command: &mut std::process::Command) {
        super::set_store_env(command, "PI_CODING_AGENT_DIR", self.store_root(roots));
    }
}
