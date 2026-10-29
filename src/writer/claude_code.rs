//! The Claude Code 2.1.311 writer.
//!
//! Shape it must produce (docs/session-interchange-spec.md §2.1, L2):
//!
//! - one row per content block, **all sharing `message.id`**;
//! - the rows chained by `parentUuid` in file order, except that **each `tool_result` row's parent
//!   is its own `tool_use` row** — chaining every row on the previous one makes Claude Code
//!   substitute `[Tool result missing due to internal error]` (§6.2);
//! - `<CLAUDE_CONFIG_DIR>\projects\<cwd, non-alphanumerics → ->\<uuid>.jsonl`; no index is needed,
//!   a session file written outside Claude Code resumes by id [V].

use super::plan::{self, PlannedBlock, PlannedResult, PlannedStep};
use super::{WriteOpts, Writer, Written, write_jsonl};
use crate::model::{HarnessId, Reasoning, Session};
use crate::policy::{Clock, ReasoningPlan};
use crate::roots::claude_project_dir_name;
use anyhow::{Context, Result};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::path::Path;

/// The Claude Code writer.
#[derive(Debug, Clone, Copy, Default)]
pub struct ClaudeCode;

impl Writer for ClaudeCode {
    fn harness(&self) -> HarnessId {
        HarnessId::ClaudeCode
    }

    fn write_planned(
        &self,
        session: &Session,
        plan: &plan::Plan,
        opts: &WriteOpts,
    ) -> Result<Written> {
        super::validate_options(HarnessId::ClaudeCode, opts)?;
        let root =
            opts.roots.claude_config_dir.clone().context(
                "CLAUDE_CONFIG_DIR is not set: nowhere to write the Claude Code session",
            )?;
        let cwd = opts
            .cwd
            .clone()
            .or_else(|| session.meta.cwd.clone())
            .context("a working directory is required")?;
        let id = opts.dest_id.clone().unwrap_or_else(crate::policy::new_session_id);
        let dir = root
            .join("projects")
            .join(claude_project_dir_name(Path::new(&cwd)));
        std::fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
        let file = opts.dest_path.clone().unwrap_or_else(|| dir.join(format!("{id}.jsonl")));

        let mut clock = Clock::new(opts.now.as_deref());
        let rows = rows(plan, session, &id, &cwd, opts, &mut clock);
        write_jsonl(&file, &rows)?;

        Ok(Written {
            id: id.clone(),
            location: file.to_string_lossy().into_owned(),
            resume: vec![
                opts.exe.clone().unwrap_or_else(|| "claude".into()),
                "-p".into(),
                "--resume".into(),
                id,
            ],
        })
    }
}

struct RowWriter<'a> {
    out: Vec<Value>,
    parent: Option<String>,
    /// call id → the uuid of the row carrying its `tool_use` block (L2).
    calls_owner: HashMap<String, String>,
    session_id: &'a str,
    cwd: &'a str,
    opts: &'a WriteOpts,
    clock: &'a mut Clock,
}

impl RowWriter<'_> {
    fn push(&mut self, row: Value) {
        let uuid = row.get("uuid").and_then(Value::as_str).map(str::to_owned);
        self.out.push(row);
        if let Some(uuid) = uuid {
            self.parent = Some(uuid);
        }
    }

    fn user_row(&mut self, content: Value) -> Value {
        json!({
            "parentUuid": self.parent.clone(),
            "isSidechain": false,
            "type": "user",
            "message": {"role": "user", "content": content},
            "uuid": new_uuid(),
            "timestamp": self.clock.next_iso(),
            "sessionId": self.session_id,
            "cwd": self.cwd,
        })
    }

    /// One assistant row: exactly one content block, sharing the step's `message.id`.
    fn assistant_row(
        &mut self,
        block: Value,
        message_id: &str,
        model: Option<&str>,
        stop: &str,
        parent: Option<String>,
    ) -> Value {
        let mut message = Map::new();
        message.insert("id".into(), Value::String(message_id.to_string()));
        message.insert("type".into(), Value::String("message".into()));
        message.insert("role".into(), Value::String("assistant".into()));
        if let Some(model) = model {
            message.insert("model".into(), Value::String(model.to_string()));
        }
        message.insert("content".into(), Value::Array(vec![block]));
        message.insert("stop_reason".into(), Value::String(stop.to_string()));
        message.insert("stop_sequence".into(), Value::Null);
        let mut row = json!({
            "parentUuid": parent.or_else(|| self.parent.clone()),
            "isSidechain": false,
            "type": "assistant",
            "message": Value::Object(message),
            "uuid": new_uuid(),
            "timestamp": self.clock.next_iso(),
            "sessionId": self.session_id,
            "cwd": self.cwd,
        });
        if let (Some(map), Some(version)) = (row.as_object_mut(), self.opts.dest_version.as_ref()) {
            map.insert("version".into(), Value::String(version.clone()));
        }
        row
    }
}

fn rows(
    plan: &plan::Plan,
    session: &Session,
    session_id: &str,
    cwd: &str,
    opts: &WriteOpts,
    clock: &mut Clock,
) -> Vec<Value> {
    let mut w = RowWriter {
        out: Vec::new(),
        parent: None,
        calls_owner: HashMap::new(),
        session_id,
        cwd,
        opts,
        clock,
    };

    for block in &plan.blocks {
        match block {
            PlannedBlock::User { text, attachments } => {
                let mut content = Vec::new();
                if !text.is_empty() {
                    content.push(json!({"type": "text", "text": text}));
                }
                content.extend(
                    attachments
                        .iter()
                        .filter_map(|a| (!a.raw.is_null()).then_some(a.raw.clone())),
                );
                let row = if content.len() == 1
                    && content[0].get("type").and_then(Value::as_str) == Some("text")
                {
                    w.user_row(Value::String(text.clone()))
                } else {
                    w.user_row(Value::Array(content))
                };
                let row = with_version(row, opts);
                w.push(row);
            }
            PlannedBlock::Compaction { summary: text } => {
                let row = w.user_row(Value::String(text.clone()));
                let row = with_version(row, opts);
                w.push(row);
            }
            PlannedBlock::Step(step) => {
                w.calls_owner.clear();
                let message_id = format!("msg_{}", uuid::Uuid::new_v4().simple());
                let model = plan::model_id(step, session, opts);
                let stop = if step.calls.is_empty() {
                    "end_turn"
                } else {
                    "tool_use"
                };
                for block in step_blocks(step) {
                    let is_call = block.get("type").and_then(Value::as_str) == Some("tool_use");
                    let row =
                        w.assistant_row(block.clone(), &message_id, model.as_deref(), stop, None);
                    let uuid = row
                        .get("uuid")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .unwrap_or_default();
                    if is_call && let Some(id) = block.get("id").and_then(Value::as_str) {
                        w.calls_owner.insert(id.to_string(), uuid);
                    }
                    w.push(row);
                }
            }
            PlannedBlock::Results { results } => {
                let mut text_blocks: Vec<Value> = Vec::new();
                for result in results {
                    match result {
                        PlannedResult::Native {
                            call_id,
                            content,
                            is_error,
                        } => {
                            // L2: the parent is this result's own tool_use row.
                            let parent = w.calls_owner.get(call_id).cloned();
                            let row =
                                w.assistant_result_row(call_id, content.text(), *is_error, parent);
                            w.push(row);
                        }
                        PlannedResult::Text(t) => {
                            text_blocks.push(json!({"type": "text", "text": t}))
                        }
                    }
                }
                if !text_blocks.is_empty() {
                    let row = w.user_row(Value::Array(text_blocks));
                    let row = with_version(row, opts);
                    w.push(row);
                }
            }
        }
    }
    // The session's name, as Claude Code's own /rename writes it: /resume lists it under this.
    if let Some(title) = opts
        .title
        .as_ref()
        .or(session.meta.title.as_ref())
        .filter(|t| !t.trim().is_empty())
    {
        w.out
            .push(json!({"type": "custom-title", "customTitle": title, "sessionId": session_id}));
    }
    // Claude Code's /resume picker titles a session by the `lastPrompt` of its last `last-prompt`
    // row, else by its first user prompt; a native session ends with one.
    if let Some(leaf) = w.parent.clone() {
        let mut row = json!({"type": "last-prompt", "leafUuid": leaf, "sessionId": session_id});
        if let Some(prompt) = last_prompt(plan) {
            row["lastPrompt"] = Value::String(prompt);
        }
        w.out.push(row);
    }
    w.out
}

/// The user prompt the conversation ended on, as the picker shows it: one line, at most 200
/// characters and an ellipsis.
fn last_prompt(plan: &plan::Plan) -> Option<String> {
    let text = plan.blocks.iter().rev().find_map(|block| match block {
        PlannedBlock::User { text, .. } if !text.trim().is_empty() => Some(text.as_str()),
        _ => None,
    })?;
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    Some(if line.chars().count() > 200 {
        format!("{}…", line.chars().take(200).collect::<String>().trim_end())
    } else {
        line
    })
}

impl RowWriter<'_> {
    fn assistant_result_row(
        &mut self,
        call_id: &str,
        text: String,
        is_error: bool,
        parent: Option<String>,
    ) -> Value {
        let mut row = self.user_row(Value::Array(vec![json!({
            "tool_use_id": call_id,
            "type": "tool_result",
            "content": text,
            "is_error": is_error,
        })]));
        if let Some(parent) = parent
            && let Some(map) = row.as_object_mut()
        {
            map.insert("parentUuid".into(), Value::String(parent));
        }
        if let (Some(map), Some(version)) = (row.as_object_mut(), self.opts.dest_version.as_ref()) {
            map.insert("version".into(), Value::String(version.clone()));
        }
        row
    }
}

fn with_version(row: Value, opts: &WriteOpts) -> Value {
    let mut row = row;
    if let (Some(map), Some(version)) = (row.as_object_mut(), opts.dest_version.as_ref()) {
        map.insert("version".into(), Value::String(version.clone()));
    }
    row
}

fn step_blocks(step: &PlannedStep) -> Vec<Value> {
    let mut blocks = Vec::new();
    for reasoning in &step.reasoning {
        if let ReasoningPlan::Native(Reasoning { text, opaque }) = reasoning {
            let signature = opaque.as_ref().and_then(|o| o.signature.clone());
            let encrypted = opaque.as_ref().and_then(|o| o.encrypted.clone());
            // A redacted block has no signature: its payload is `data`, and it goes back as it came.
            blocks.push(match (signature, encrypted) {
                (None, Some(data)) => json!({"type": "redacted_thinking", "data": data}),
                (signature, _) => {
                    json!({"type": "thinking", "thinking": text, "signature": signature})
                }
            });
        }
    }
    for text in &step.text {
        blocks.push(json!({"type": "text", "text": text}));
    }
    for call in &step.calls {
        blocks.push(
            json!({"type": "tool_use", "id": call.id, "name": call.name, "input": call.input}),
        );
    }
    blocks
}

fn new_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}
