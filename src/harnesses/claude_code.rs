//! Claude Code: sessions under `CLAUDE_CONFIG_DIR/projects/`.

use super::{Harness, InterchangeDocument, StoreStats};
use crate::hop::HopError;
use crate::model::{CanonicalTool, HarnessId, Opaque, Protocol, Session};
use crate::reader::Reader;
use crate::roots::StoreRoots;
use crate::summary::{self, Reply, SummarizeOpts, SummaryCommand};
use crate::tools::{self, EditCall, GrepCall, Shell, ToolSet, Translation};
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

    fn offered(&self) -> ToolSet {
        ToolSet {
            names: &[
                "Agent",
                "Bash",
                "CronCreate",
                "CronDelete",
                "CronList",
                "Edit",
                "EnterWorktree",
                "ExitWorktree",
                "Glob",
                "Grep",
                "ListAgents",
                "Monitor",
                "NotebookEdit",
                "Read",
                "ReportFindings",
                "ScheduleWakeup",
                "SendMessage",
                "Skill",
                "TaskStop",
                "WebFetch",
                "WebSearch",
                "Workflow",
                "Write",
            ],
        }
    }

    /// Claude offers both shells and runs either, whatever the session's shell.
    fn translate(
        &self,
        canonical: &CanonicalTool,
        source_name: &str,
        input: &Value,
        _dest_shell: Option<Shell>,
    ) -> Translation {
        match canonical {
            // A PowerShell call needs the PowerShell tool.
            CanonicalTool::Bash => shell(input, "Bash"),
            CanonicalTool::PowerShell => shell(input, "PowerShell"),
            CanonicalTool::Read => read(input),
            CanonicalTool::Write => write(input),
            CanonicalTool::Edit | CanonicalTool::MultiEdit => edit(canonical, input),
            CanonicalTool::Grep => grep(source_name, input),
            CanonicalTool::Glob => match tools::glob_input(input) {
                Ok(input) => Translation::Call {
                    name: "Glob",
                    input,
                },
                Err(text) => text,
            },
            CanonicalTool::Ls => Translation::Text {
                why: "no ls tool offered",
            },
            CanonicalTool::WebFetch => web_fetch(input),
            CanonicalTool::WebSearch => web_search(input),
            _ => tools::OTHER_CAPABILITY,
        }
    }

    fn accepted_keys(&self, canonical: &CanonicalTool) -> &'static [&'static str] {
        match canonical {
            CanonicalTool::Read => &[
                "file_path",
                "path",
                "offset",
                "limit",
                "pages",
                "allow_large",
            ],
            CanonicalTool::Grep => &[
                "pattern",
                "path",
                "glob",
                "include",
                "type",
                "output_mode",
                "-i",
                "ignoreCase",
                "caseSensitive",
                "-n",
                "-A",
                "-B",
                "-C",
                "context",
                "head_limit",
                "limit",
                "offset",
                "multiline",
            ],
            CanonicalTool::WebFetch => &["url", "prompt"],
            CanonicalTool::WebSearch => &["query", "allowed_domains", "blocked_domains"],
            _ => tools::common_keys(canonical),
        }
    }

    /// The writer writes a result's text: its other blocks are dropped and counted.
    fn flattens_result_blocks(&self) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------------------------
// tools toward Claude Code

fn shell(input: &Value, tool: &'static str) -> Translation {
    let Some(command) = tools::command_of(input) else {
        return Translation::Text { why: "no command" };
    };
    let mut pairs: Vec<(&str, Value)> = vec![("command", command)];
    if let Some(t) = tools::arg(input, "timeout") {
        pairs.push(("timeout", t.clone()));
    }
    if let Some(d) = tools::either(input, &["description", "justification"]) {
        pairs.push(("description", d));
    }
    if tools::in_background(input) {
        pairs.push(("run_in_background", Value::Bool(true)));
    }
    if let Some(unsandboxed) = tools::arg(input, "dangerouslyDisableSandbox") {
        pairs.push(("dangerouslyDisableSandbox", unsandboxed.clone()));
    }
    Translation::Call {
        name: tool,
        input: tools::obj_pairs(pairs),
    }
}

/// Claude's own `Read`: `file_path`, not `path`.
fn read(input: &Value) -> Translation {
    let Some(path) = tools::path_of(input) else {
        return Translation::Text { why: "no path" };
    };
    let mut pairs: Vec<(&str, Value)> = vec![("file_path", path)];
    for key in ["offset", "limit", "pages", "allow_large"] {
        if let Some(v) = tools::arg(input, key) {
            pairs.push((key, v.clone()));
        }
    }
    Translation::Call {
        name: "Read",
        input: tools::obj_pairs(pairs),
    }
}

fn write(input: &Value) -> Translation {
    match tools::path_of(input) {
        Some(path) => Translation::Call {
            name: "Write",
            input: tools::obj([
                ("file_path", path),
                ("content", tools::required_content(input)),
            ]),
        },
        None => Translation::Text { why: "no path" },
    }
}

fn edit(canonical: &CanonicalTool, input: &Value) -> Translation {
    let (path, old, new, replace_all) = match tools::edit_call(canonical, input) {
        Ok(EditCall::One {
            path,
            old,
            new,
            replace_all,
        }) => (path, old, new, replace_all),
        // Claude 2.1.311 has no MultiEdit, and one result cannot be split.
        Ok(EditCall::Several { .. }) => {
            return Translation::Text {
                why: "no multi-edit equivalent",
            };
        }
        Err(text) => return text,
    };
    let mut pairs: Vec<(&str, Value)> = vec![
        ("file_path", path),
        ("old_string", old),
        ("new_string", new),
    ];
    if replace_all {
        pairs.push(("replace_all", Value::Bool(true)));
    }
    Translation::Call {
        name: "Edit",
        input: tools::obj_pairs(pairs),
    }
}

fn grep(source_name: &str, input: &Value) -> Translation {
    // A Claude source keeps its own call: every argument is Claude's.
    if source_name == "Grep" {
        return Translation::Call {
            name: "Grep",
            input: input.clone(),
        };
    }
    let GrepCall {
        mut pairs,
        filter,
        limit,
        context,
        ignore_case,
        literal,
    } = match tools::grep_call(input) {
        Ok(grep) => grep,
        Err(text) => return text,
    };
    // OpenCode and Pi print the matching lines with their numbers.
    if literal.is_some() {
        return Translation::Text {
            why: "Claude's Grep has no literal search",
        };
    }
    if let Some(filter) = filter {
        pairs.push(("glob", filter));
    }
    if let Some(i) = ignore_case {
        pairs.push(("-i", Value::Bool(i)));
    }
    if let Some(context) = context {
        pairs.push(("-C", context));
    }
    if let Some(limit) = limit {
        pairs.push(("head_limit", limit));
    }
    pairs.push(("output_mode", Value::String("content".into())));
    pairs.push(("-n", Value::Bool(true)));
    Translation::Call {
        name: "Grep",
        input: tools::obj_pairs(pairs),
    }
}

/// Claude's WebFetch needs the prompt it answers.
fn web_fetch(input: &Value) -> Translation {
    let url = match tools::url_of(input) {
        Ok(url) => url,
        Err(text) => return text,
    };
    match tools::required(input, "prompt") {
        Some(prompt) => Translation::Call {
            name: "WebFetch",
            input: tools::obj([("url", url), ("prompt", prompt)]),
        },
        None => Translation::Text { why: "no prompt" },
    }
}

fn web_search(input: &Value) -> Translation {
    let query = match tools::query_of(input) {
        Ok(query) => query,
        Err(text) => return text,
    };
    let mut pairs: Vec<(&str, Value)> = vec![("query", query)];
    for key in ["allowed_domains", "blocked_domains"] {
        if let Some(v) = tools::arg(input, key) {
            pairs.push((key, v.clone()));
        }
    }
    Translation::Call {
        name: "WebSearch",
        input: tools::obj_pairs(pairs),
    }
}
