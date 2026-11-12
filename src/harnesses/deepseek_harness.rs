//! DeepSeek Harness (`dsh`): session logs under `DSH_HOME/sessions/`.

use super::{Harness, StoreStats};
use crate::hop::HopError;
use crate::model::{HarnessId, Session};
use crate::reader::Reader;
use crate::roots::StoreRoots;
use crate::summary::{Reply, SummarizeOpts, SummaryCommand};
use std::path::{Path, PathBuf};

pub(super) struct DeepSeekHarness;

impl Harness for DeepSeekHarness {
    fn id(&self) -> HarnessId {
        HarnessId::DeepSeekHarness
    }

    fn reader(&self) -> &'static dyn Reader {
        &crate::reader::deepseek_harness::DeepSeekHarness
    }

    fn read_file(&self, path: &Path) -> anyhow::Result<Session> {
        crate::reader::deepseek_harness::read_file(path)
    }

    fn store_root<'a>(&self, roots: &'a StoreRoots) -> Option<&'a Path> {
        roots.dsh_home.as_deref()
    }

    fn store_root_mut<'a>(&self, roots: &'a mut StoreRoots) -> &'a mut Option<PathBuf> {
        &mut roots.dsh_home
    }

    fn apply_store_env(&self, roots: &StoreRoots, command: &mut std::process::Command) {
        let variable = crate::catalog::store_env(self.id());
        super::set_store_env(command, variable, self.store_root(roots));
    }

    /// A subagent's or a fork's log names its parent in its own header.
    fn store_stats(&self, roots: &StoreRoots) -> StoreStats {
        let listed = super::listed_stats(self, roots);
        let forks = Some(match &listed.sessions {
            Ok(_) => Err("not counted yet".to_string()),
            Err(error) => Err(error.clone()),
        });
        StoreStats { forks, ..listed }
    }

    fn summary_command(
        &self,
        _session: &Session,
        _opts: &SummarizeOpts,
        _scratch: &Path,
    ) -> Result<SummaryCommand, HopError> {
        Err(HopError::Unsupported(
            "a summary from a DeepSeek Harness session is not implemented yet".into(),
        ))
    }

    fn parse_summary_reply(
        &self,
        _stdout: &str,
        _output_file: Option<&str>,
    ) -> Result<Reply, String> {
        Err("a summary from a DeepSeek Harness session is not implemented yet".into())
    }
}
