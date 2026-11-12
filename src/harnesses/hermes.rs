//! Hermes Agent: sessions in `HERMES_HOME/state.db`.

use super::{Destination, Harness, InterchangeDocument, StoreStats};
use crate::hop::HopError;
use crate::model::{CanonicalTool, HarnessId, Opaque, Protocol, Session};
use crate::reader::Reader;
use crate::roots::StoreRoots;
use crate::summary::{Reply, SummarizeOpts, SummaryCommand};
use crate::tools::{Shell, ToolSet, Translation};
use crate::writer::{WriteOpts, Writer, media::Image};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub(super) struct Hermes;

impl Harness for Hermes {
    fn id(&self) -> HarnessId {
        HarnessId::Hermes
    }

    fn reader(&self) -> &'static dyn Reader {
        &crate::reader::hermes::Hermes
    }

    fn destination(&self) -> Option<&dyn Destination> {
        Some(self)
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

/// Hermes is written through its own `sessions import`, which reads a conversation as turns of
/// text: nothing it takes is a tool call, a reasoning payload or an image.
impl Destination for Hermes {
    fn writer(&self) -> &'static dyn Writer {
        &crate::writer::hermes::Hermes
    }

    /// Hermes speaks whatever its configured provider speaks.
    fn protocol(&self) -> Option<Protocol> {
        None
    }

    fn payload_has_place(&self, _opaque: &Opaque) -> bool {
        false
    }

    /// Hermes allocates the id and the project of a session itself, titles it, and has no
    /// agent, metadata or compaction record to take: each is refused, not ignored.
    fn validate_write(&self, opts: &WriteOpts) -> anyhow::Result<()> {
        if opts.dest_id.is_some() {
            anyhow::bail!("--dest-id is not supported toward Hermes: Hermes allocates the session id");
        }
        if opts.dest_path.is_some() {
            anyhow::bail!("--dest-path is not supported toward Hermes: its sessions live in state.db");
        }
        if opts.dest_agent.is_some() || opts.metadata.is_some() {
            anyhow::bail!(
                "--dest-agent and --metadata-file are supported only for an OpenCode destination"
            );
        }
        if opts.native_budget.is_some() {
            anyhow::bail!("--compaction native is not supported toward Hermes");
        }
        Ok(())
    }

    fn interchange_document(&self) -> Option<&'static dyn InterchangeDocument> {
        None
    }

    /// No tool: Hermes's import keeps a tool call as `[ran tool: <name>]` and drops its result,
    /// so every call travels as text, with its arguments and its result.
    fn offered(&self) -> ToolSet {
        ToolSet { names: &[] }
    }

    fn translate(
        &self,
        _canonical: &CanonicalTool,
        _source_name: &str,
        _input: &Value,
        _dest_shell: Option<Shell>,
    ) -> Translation {
        Translation::Text {
            why: "Hermes's import keeps no tool call",
        }
    }

    /// An image has no place in what Hermes's import keeps of a turn (it writes `[image]`).
    fn image_in_result(&self, _image: &Image) -> Value {
        Value::Null
    }

    fn flattens_result_blocks(&self) -> bool {
        true
    }
}
