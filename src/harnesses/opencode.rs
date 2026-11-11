//! OpenCode: one SQLite database, named by `OPENCODE_DB`.

use super::{Harness, InterchangeDocument};
use crate::model::{HarnessId, Kind, Opaque, Protocol, Session};
use crate::native_compaction::NativeCompaction;
use crate::policy::Clock;
use crate::reader::Reader;
use crate::roots::StoreRoots;
use crate::writer::{WriteOpts, Writer, plan::Plan};
use serde_json::Value;
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

    fn interchange_document(&self) -> Option<&'static dyn InterchangeDocument> {
        Some(&Document)
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
