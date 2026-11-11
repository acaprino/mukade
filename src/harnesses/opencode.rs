//! OpenCode: one SQLite database, named by `OPENCODE_DB`.

use super::{Harness, InterchangeDocument, StoreStats};
use crate::hop::HopError;
use crate::model::{HarnessId, Kind, Opaque, Protocol, Session};
use crate::native_compaction::NativeCompaction;
use crate::policy::Clock;
use crate::reader::Reader;
use crate::roots::StoreRoots;
use crate::summary::{self, Reply, SummarizeOpts, SummaryCommand};
use crate::writer::{WriteOpts, Writer, plan::Plan};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Duration;

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

    /// OpenCode speaks whatever its configured provider speaks.
    fn destination_protocol(&self) -> Option<Protocol> {
        None
    }

    /// OpenCode keeps a signature, an encrypted payload and the Chat envelope around them.
    fn payload_has_place(&self, opaque: &Opaque) -> bool {
        opaque.signature.is_some() || opaque.encrypted.is_some()
    }

    /// The receiving provider must understand the envelope OpenCode keeps. Native OpenAI
    /// ciphertext is never relabelled as OpenRouter ciphertext merely because both speak Responses.
    fn provider_reads_envelope(&self, opaque: &Opaque, provider: Option<&str>) -> bool {
        let Some(chat) = &opaque.chat else {
            return true;
        };
        if provider == Some("openrouter")
            && chat.details.iter().any(|d| {
                d.get("format").and_then(serde_json::Value::as_str)
                    == Some("openai-native-responses-v1")
            })
        {
            return false;
        }
        provider.is_some() && provider == chat.provider.as_deref()
            || crate::policy::scalar_chat_payload(opaque)
                && matches!(provider, Some("anthropic" | "openrouter"))
    }

    /// One database, no reserved file: a session id `ses_…`, an agent name, metadata as an object.
    fn validate_write(&self, opts: &WriteOpts) -> anyhow::Result<()> {
        if opts.dest_path.is_some() {
            anyhow::bail!("--dest-path is supported only for file-based destinations");
        }
        if let Some(id) = opts.dest_id.as_deref() {
            crate::writer::opencode::validate_session_id(id)?;
        }
        if let Some(agent) = opts.dest_agent.as_deref() {
            crate::writer::opencode::validate_agent(agent)?;
        }
        if let Some(metadata) = &opts.metadata {
            anyhow::ensure!(
                metadata.is_object(),
                "session metadata must be a JSON object"
            );
        }
        Ok(())
    }

    /// OpenCode ids are prefixed `ses_`.
    fn claims_id(&self, id: &str) -> bool {
        id.starts_with("ses_")
    }

    fn interchange_document(&self) -> Option<&'static dyn InterchangeDocument> {
        Some(&Document)
    }

    /// OpenCode's `model` is `<provider>/<model>`.
    ///
    /// A self-contained installation keeps its configuration beside its store: a database in an
    /// XDG data layout (`<root>/share/opencode/opencode.db`, a portable JanuraCode's) is
    /// configured by `<root>/config/opencode/opencode.json`. When such a file exists it alone
    /// answers, even without a model: another installation's model is not this one's. Otherwise
    /// OpenCode's process configuration applies (`OPENCODE_CONFIG`, then `XDG_CONFIG_HOME`).
    fn configured_model(&self, roots: &StoreRoots) -> Option<(String, Option<String>)> {
        let own = roots
            .opencode_db
            .as_deref()
            .and_then(opencode_config_beside)
            .filter(|path| path.is_file());
        opencode_model(&own.or_else(opencode_process_config)?)
    }

    fn project_id(
        &self,
        exe: &str,
        cwd: &str,
        roots: &StoreRoots,
        cache: Option<&Path>,
    ) -> anyhow::Result<Option<String>> {
        Ok(crate::reader::opencode::project_id_for(exe, cwd, roots, cache))
    }

    fn store_stats(&self, roots: &StoreRoots) -> StoreStats {
        let counted = match roots.opencode_db.as_deref() {
            Some(db) => crate::reader::opencode::store_counts(db),
            None => Err(anyhow::anyhow!("no OpenCode database to count")),
        };
        match counted {
            Ok(counts) => StoreStats {
                sessions: Ok(counts.sessions),
                forks: Some(Ok(counts.forks)),
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
    // Every `run` writes to the store: the fork is deleted once the reply is in.
    fn summary_command(
        &self,
        session: &Session,
        opts: &SummarizeOpts,
        _scratch: &Path,
    ) -> Result<SummaryCommand, HopError> {
        let mut args: Vec<String> = [
            "run",
            "--standalone",
            "--session",
            &session.meta.id,
            "--fork",
            "--format",
            "json",
        ]
        .map(String::from)
        .to_vec();
        // A model is named only with its provider.
        args.extend(summary::model_args(
            "-m",
            opts.model.as_deref(),
            summary::own_model(session),
            |model| match model.provider.as_deref() {
                Some(provider) => vec!["-m".into(), format!("{provider}/{}", model.id)],
                None => Vec::new(),
            },
        ));
        let env = vec![("OPENCODE_CONFIG_CONTENT".into(), DENY_ALL.into())];
        summary::finish_command(session, opts, args, None, env)
    }

    /// The `text` events, joined.
    fn parse_summary_reply(
        &self,
        stdout: &str,
        _output_file: Option<&str>,
    ) -> Result<Reply, String> {
        let mut texts = Vec::new();
        for event in stdout
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
        {
            match event.get("type").and_then(Value::as_str) {
                Some("text") => {
                    if let Some(text) = event.pointer("/part/text").and_then(Value::as_str) {
                        texts.push(text.to_string());
                    }
                }
                // A model that reached for a tool was working, not summarizing.
                Some("tool_use") => {
                    return Err(format!(
                        "the model called a tool ({}) instead of only summarizing",
                        event
                            .pointer("/part/tool")
                            .and_then(Value::as_str)
                            .unwrap_or("?")
                    ));
                }
                Some("error") => {
                    let error = event.get("error").unwrap_or(&event);
                    let message = error
                        .pointer("/data/message")
                        .or_else(|| error.get("message"))
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .unwrap_or_else(|| error.to_string());
                    return Err(format!("OpenCode reported an error: {message}"));
                }
                _ => {}
            }
        }
        Ok(Reply {
            text: texts.join("\n\n"),
            usage: None,
            fork: fork_of(stdout),
        })
    }

    /// The fork exists as soon as OpenCode names it: it goes whatever came of the run, and a
    /// failure says how to remove one that stayed.
    fn after_summary(
        &self,
        command: &SummaryCommand,
        roots: &StoreRoots,
        stdout: &str,
    ) -> Option<String> {
        fork_of(stdout).and_then(|fork| delete_fork(command, roots, &fork))
    }
}

/// OpenCode's `run` has no switch for tools: its inline configuration denies every permission, so
/// the summarizing run cannot edit a file or run a command (live record, "Ephemeral runs").
const DENY_ALL: &str = r#"{"permission":{"*":"deny"}}"#;

/// How long deleting OpenCode's fork may take.
const DELETE_TIMEOUT: Duration = Duration::from_secs(30);

/// The session OpenCode forked for a run: the `sessionID` of the first event it printed, there as
/// soon as the fork exists, before any text.
pub(crate) fn fork_of(stdout: &str) -> Option<String> {
    stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
        .find_map(|event| {
            event
                .get("sessionID")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
}

/// Delete the session OpenCode forked for the summary: `None` once it is gone, else the command
/// that removes it.
fn delete_fork(command: &SummaryCommand, roots: &StoreRoots, fork: &str) -> Option<String> {
    let args = ["session", "delete", fork, "--standalone"];
    let mut process = std::process::Command::new(crate::exe::spawnable(&command.program));
    process.args(args);
    if let Some(cwd) = &command.cwd {
        process.current_dir(cwd);
    }
    roots.apply_to(HarnessId::OpenCode, &mut process);
    let deleted = crate::exe::run_within(&mut process, DELETE_TIMEOUT)
        .ok()
        .and_then(|finished| finished.status)
        .is_some_and(|status| status.success());
    (!deleted).then(|| format!("{} {}", command.program, args.join(" ")))
}

/// The configuration an OpenCode database in an XDG data layout has beside it:
/// `<root>/share/opencode/opencode.db` → `<root>/config/opencode/opencode.json`.
fn opencode_config_beside(database: &Path) -> Option<PathBuf> {
    let opencode = database.parent()?;
    let share = opencode.parent()?;
    let named = |path: &Path, name: &str| path.file_name().is_some_and(|n| n == name);
    (named(opencode, "opencode") && named(share, "share"))
        .then(|| share.parent())
        .flatten()
        .map(|root| root.join("config").join("opencode").join("opencode.json"))
}

/// The configuration this process's OpenCode reads.
fn opencode_process_config() -> Option<PathBuf> {
    std::env::var_os("OPENCODE_CONFIG")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(|dir| PathBuf::from(dir).join("opencode").join("opencode.json"))
        })
}

/// OpenCode's config names its model as `<provider>/<model>`.
fn opencode_model(config: &Path) -> Option<(String, Option<String>)> {
    let document: Value = serde_json::from_str(&std::fs::read_to_string(config).ok()?).ok()?;
    let (provider, model) = document.get("model")?.as_str()?.split_once('/')?;
    Some((provider.to_string(), Some(model.to_string())))
}

#[cfg(test)]
mod tests {
    use super::opencode_config_beside;
    use std::path::PathBuf;

    #[test]
    fn only_an_xdg_data_layout_has_a_configuration_beside_it() {
        let root = PathBuf::from("install");
        assert_eq!(
            opencode_config_beside(&root.join("data/share/opencode/opencode.db")),
            Some(root.join("data/config/opencode/opencode.json"))
        );
        assert_eq!(opencode_config_beside(&root.join("store/opencode.db")), None);
    }
}

/// OpenCode's interchange format: what `opencode session import` takes.
struct Document;

impl InterchangeDocument for Document {
    /// An OpenCode session keeps each call to a tool OpenCode offers under its own name and input.
    fn source_tool_map(
        &self,
        source_harness: HarnessId,
        source: &Session,
    ) -> Vec<(String, String, Value, Value)> {
        if source_harness != HarnessId::OpenCode {
            return Vec::new();
        }
        let offered = crate::tools::offered(HarnessId::OpenCode);
        let mut map = Vec::new();
        for entry in source.conversation() {
            if let Kind::Step { calls, .. } = &entry.kind {
                for call in calls {
                    if offered.contains(&call.source_name) && call.input.is_object() {
                        map.push((
                            call.source_name.clone(),
                            call.source_name.clone(),
                            call.input.clone(),
                            call.input.clone(),
                        ));
                    }
                }
            }
        }
        map
    }

    /// Without a reserved id the document is `ses_preview`.
    fn document(
        &self,
        session: &Session,
        plan: &Plan,
        cwd: &str,
        opts: &WriteOpts,
        clock: &mut Clock,
    ) -> anyhow::Result<(Value, Option<NativeCompaction>)> {
        let id = opts.dest_id.as_deref().unwrap_or("ses_preview");
        crate::writer::opencode::validate_session_id(id)?;
        crate::writer::opencode::document_from_plan_report(session, plan, id, cwd, opts, clock)
    }
}
