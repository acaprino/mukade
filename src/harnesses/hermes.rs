//! Hermes Agent: sessions in `HERMES_HOME/state.db`.

use super::{Harness, StoreStats};
use crate::hop::HopError;
use crate::model::{HarnessId, Session};
use crate::reader::Reader;
use crate::roots::StoreRoots;
use crate::summary::{Reply, SummarizeOpts, SummaryCommand};
use std::path::{Path, PathBuf};

pub(super) struct Hermes;

impl Harness for Hermes {
    fn id(&self) -> HarnessId {
        HarnessId::Hermes
    }

    fn reader(&self) -> &'static dyn Reader {
        &crate::reader::hermes::Hermes
    }

    fn read_file(&self, path: &Path) -> anyhow::Result<Session> {
        crate::reader::hermes::read_file(path)
    }

    fn store_root<'a>(&self, roots: &'a StoreRoots) -> Option<&'a Path> {
        roots.hermes_home.as_deref()
    }

    fn store_root_mut<'a>(&self, roots: &'a mut StoreRoots) -> &'a mut Option<PathBuf> {
        &mut roots.hermes_home
    }

    fn apply_store_env(&self, roots: &StoreRoots, command: &mut std::process::Command) {
        let variable = crate::catalog::store_env(self.id());
        super::set_store_env(command, variable, self.store_root(roots));
    }

    /// Hermes records a subagent's and a branch's parent in the child's own row.
    fn store_stats(&self, roots: &StoreRoots) -> StoreStats {
        let counted = match roots.hermes_state_db() {
            Some(db) => crate::reader::hermes::store_counts(&db),
            None => Err(anyhow::anyhow!("no Hermes state.db to count")),
        };
        match counted {
            Ok((sessions, forks)) => StoreStats {
                sessions: Ok(sessions),
                forks: Some(Ok(forks)),
            },
            Err(error) => {
                let error = format!("{error:#}");
                StoreStats {
                    sessions: Err(error.clone()),
                    forks: Some(Err(error)),
                }
            }
        }
    }

    fn summary_command(
        &self,
        _session: &Session,
        _opts: &SummarizeOpts,
        _scratch: &Path,
    ) -> Result<SummaryCommand, HopError> {
        Err(HopError::Unsupported(
            "a summary from a Hermes Agent session is not implemented yet".into(),
        ))
    }

    fn parse_summary_reply(
        &self,
        _stdout: &str,
        _output_file: Option<&str>,
    ) -> Result<Reply, String> {
        Err("a summary from a Hermes Agent session is not implemented yet".into())
    }
}
