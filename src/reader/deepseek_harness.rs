//! The DeepSeek Harness reader: the session logs under `DSH_HOME/sessions/`.

use super::{Reader, SessionInfo};
use crate::model::{HarnessId, Session};
use crate::roots::StoreRoots;
use anyhow::Result;
use std::path::{Path, PathBuf};

/// The DeepSeek Harness reader.
#[derive(Debug, Clone, Copy, Default)]
pub struct DeepSeekHarness;

/// Read one DeepSeek Harness session log.
pub fn read_file(path: &Path) -> Result<Session> {
    anyhow::bail!(
        "reading DeepSeek Harness sessions is not implemented yet: {}",
        path.display()
    )
}

/// The `sessions/` folder of the store `roots` names, or the refusal that no store is set.
fn sessions(roots: &StoreRoots, what: &str) -> Result<PathBuf> {
    roots.dsh_sessions().ok_or_else(|| {
        super::Lookup::NoStore(format!(
            "DSH_HOME is not set: no DeepSeek Harness store to {what}"
        ))
        .into()
    })
}

impl Reader for DeepSeekHarness {
    fn harness(&self) -> HarnessId {
        HarnessId::DeepSeekHarness
    }

    fn list(&self, roots: &StoreRoots) -> Result<Vec<SessionInfo>> {
        let dir = sessions(roots, "list")?;
        anyhow::bail!(
            "reading DeepSeek Harness sessions is not implemented yet: {}",
            dir.display()
        )
    }

    fn read(&self, id: &str, roots: &StoreRoots) -> Result<Session> {
        let dir = sessions(roots, "read")?;
        anyhow::bail!(
            "reading DeepSeek Harness session {id} is not implemented yet: {}",
            dir.display()
        )
    }
}
