//! The Hermes Agent reader: the sessions in `HERMES_HOME/state.db`.

use super::{Reader, SessionInfo};
use crate::model::{HarnessId, Session};
use crate::roots::StoreRoots;
use anyhow::Result;
use std::path::{Path, PathBuf};

/// The Hermes Agent reader.
#[derive(Debug, Clone, Copy, Default)]
pub struct Hermes;

/// Read one Hermes session from a file.
pub fn read_file(path: &Path) -> Result<Session> {
    anyhow::bail!(
        "reading Hermes Agent sessions is not implemented yet: {}",
        path.display()
    )
}

/// `state.db` in the store `roots` names, or the refusal that no store is set.
fn state_db(roots: &StoreRoots, what: &str) -> Result<PathBuf> {
    roots.hermes_state_db().ok_or_else(|| {
        super::Lookup::NoStore(format!("HERMES_HOME is not set: no Hermes store to {what}")).into()
    })
}

impl Reader for Hermes {
    fn harness(&self) -> HarnessId {
        HarnessId::Hermes
    }

    fn list(&self, roots: &StoreRoots) -> Result<Vec<SessionInfo>> {
        let db = state_db(roots, "list")?;
        anyhow::bail!(
            "reading Hermes Agent sessions is not implemented yet: {}",
            db.display()
        )
    }

    fn read(&self, id: &str, roots: &StoreRoots) -> Result<Session> {
        let db = state_db(roots, "read")?;
        anyhow::bail!(
            "reading Hermes Agent session {id} is not implemented yet: {}",
            db.display()
        )
    }
}
