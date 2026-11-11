//! Claude Code: sessions under `CLAUDE_CONFIG_DIR/projects/`.

use super::{Harness, InterchangeDocument, StoreStats};
use crate::hop::HopError;
use crate::model::{HarnessId, Opaque, Protocol, Session};
use crate::reader::Reader;
use crate::roots::StoreRoots;
use crate::summary::{self, Reply, SummarizeOpts, SummaryCommand};
use crate::writer::{WriteOpts, Writer};
use serde_json::{Value, json};
use std::path::Path;

pub(super) struct ClaudeCode;

impl Harness for ClaudeCode {
    fn id(&self) -> HarnessId {
        HarnessId::ClaudeCode
    }

    fn reader(&self) -> &'static dyn Reader {
        &crate::reader::claude_code::ClaudeCode
    }

    fn writer(&self) -> Option<&'static dyn Writer> {
        Some(&crate::writer::claude_code::ClaudeCode)
    }

    fn read_file(&self, path: &Path) -> anyhow::Result<Session> {
        crate::reader::claude_code::read_file(path)
    }

    fn store_root<'a>(&self, roots: &'a StoreRoots) -> Option<&'a Path> {
        roots.claude_config_dir.as_deref()
    }

    fn set_store_root(&self, roots: &mut StoreRoots, root: &Path) {
        roots.claude_config_dir = Some(root.to_path_buf());
    }

    fn apply_store_env(&self, roots: &StoreRoots, command: &mut std::process::Command) {
        let variable = crate::catalog::store_env(self.id());
        super::set_store_env(command, variable, self.store_root(roots));
    }

    fn destination_protocol(&self) -> Option<Protocol> {
        Some(Protocol::AnthropicMessages)
    }

    /// A signature is written as `thinking`, an encrypted payload as `redacted_thinking`. A Chat
    /// envelope is not kept: only one with a scalar conversion fits.
    fn payload_has_place(&self, opaque: &Opaque) -> bool {
        crate::policy::scalar_chat_payload(opaque)
            && (opaque.signature.is_some() || opaque.encrypted.is_some())
    }

    fn validate_write(&self, opts: &WriteOpts) -> anyhow::Result<()> {
        super::validate_file_write(self, opts)
    }

    fn interchange_document(&self) -> Option<&'static dyn InterchangeDocument> {
        None
    }

    fn store_stats(&self, roots: &StoreRoots) -> StoreStats {
        super::listed_stats(self, roots)
    }

    // `--tools` takes several values: before `--resume`, so it cannot swallow the prompt.
    fn summary_command(
        &self,
        session: &Session,
        opts: &SummarizeOpts,
        _scratch: &Path,
    ) -> Result<SummaryCommand, HopError> {
        let mut args: Vec<String> = [
            "-p",
            "--tools",
            "",
            "--resume",
            &session.meta.id,
            "--no-session-persistence",
            "--output-format",
            "json",
        ]
        .map(String::from)
        .to_vec();
        args.extend(summary::model_args(
            "--model",
            opts.model.as_deref(),
            summary::own_model(session),
            |model| vec!["--model".into(), model.id.clone()],
        ));
        summary::finish_command(session, opts, args, None, Vec::new())
    }

    /// The JSON `result`, read from the last line that parses.
    fn parse_summary_reply(
        &self,
        stdout: &str,
        _output_file: Option<&str>,
    ) -> Result<Reply, String> {
        let result: Value = stdout
            .lines()
            .rev()
            .find_map(|line| serde_json::from_str(line.trim()).ok())
            .or_else(|| serde_json::from_str(stdout.trim()).ok())
            .ok_or_else(|| "Claude Code printed no JSON result".to_string())?;
        let subtype = result.get("subtype").and_then(Value::as_str).unwrap_or("");
        let text = result.get("result").and_then(Value::as_str).unwrap_or("");
        if result.get("is_error").and_then(Value::as_bool) == Some(true) || subtype != "success" {
            return Err(format!(
                "Claude Code answered with an error ({}): {}",
                if subtype.is_empty() {
                    "no subtype"
                } else {
                    subtype
                },
                text.trim()
            ));
        }
        let usage = json!({
            "total_cost_usd": result.get("total_cost_usd"),
            "usage": result.get("usage"),
            "modelUsage": result.get("modelUsage"),
        });
        Ok(Reply {
            text: text.to_string(),
            usage: Some(usage.to_string()),
            fork: None,
        })
    }
}
