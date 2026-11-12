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
//! - a subagent session is `<project>/<parent-uuid>/subagents/agent-<agentId>.jsonl`, every row
//!   `isSidechain` with its `agentId`, beside an `agent-<agentId>.meta.json`.

use super::media::Image;
use super::plan::{self, PlannedBlock, PlannedResult, PlannedStep};
use super::{ChildOf, WriteOpts, Writer, Written, write_jsonl, write_new_file};
use crate::model::{HarnessId, Reasoning, Session};
use crate::native_compaction::{NativeItem, plan_native_items};
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

    /// A subagent's id is its parent's, a dot, and its agent id: `a` and 16 hex digits.
    fn child_id(&self, root_id: &str) -> String {
        let hex = uuid::Uuid::new_v4().simple().to_string();
        format!("{root_id}.a{}", &hex[..16])
    }

    fn child_ids_hold_root(&self) -> bool {
        true
    }

    /// A tree: every row names its parent, and `last-prompt` names the leaf Claude Code resumes.
    fn keeps_branches(&self) -> plan::BranchKeeping {
        plan::BranchKeeping::Tree
    }

    fn write_planned(
        &self,
        session: &Session,
        plan: &plan::Plan,
        opts: &WriteOpts,
    ) -> Result<Written> {
        crate::harnesses::destination(HarnessId::ClaudeCode)?.validate_write(opts)?;
        if let Some(of) = &opts.subagents.of {
            return write_child(session, plan, opts, of);
        }
        let root =
            opts.roots.claude_config_dir.clone().context(
                "CLAUDE_CONFIG_DIR is not set: nowhere to write the Claude Code session",
            )?;
        let cwd = opts
            .cwd
            .clone()
            .or_else(|| session.meta.cwd.clone())
            .context("a working directory is required")?;
        let id = opts
            .dest_id
            .clone()
            .unwrap_or_else(crate::policy::new_session_id);
        let dir = root
            .join("projects")
            .join(claude_project_dir_name(Path::new(&cwd)));
        std::fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
        let file = opts
            .dest_path
            .clone()
            .unwrap_or_else(|| dir.join(format!("{id}.jsonl")));

        let mut clock = Clock::new(opts.now.as_deref());
        let rows = rows(plan, session, &id, &cwd, opts, &mut clock);
        write_jsonl(&file, &rows)?;

        Ok(Written {
            id: id.clone(),
            location: file.to_string_lossy().into_owned(),
            resume: crate::catalog::resume(
                HarnessId::ClaudeCode,
                opts.exe.as_deref().unwrap_or("claude"),
                &id,
                None,
            ),
        })
    }
}

/// A subagent session: the transcript and its meta.json, in the root session's own folder.
fn write_child(
    session: &Session,
    plan: &plan::Plan,
    opts: &WriteOpts,
    of: &ChildOf,
) -> Result<Written> {
    let id = opts
        .dest_id
        .clone()
        .context("a Claude Code subagent needs the id the hop gave it")?;
    let (root, agent) = id
        .split_once('.')
        .context("a Claude Code subagent's id is its parent's, a dot, and its agent id")?;
    anyhow::ensure!(
        root == of.root_id,
        "the subagent {id} does not belong to the session {}",
        of.root_id
    );
    let cwd = opts
        .cwd
        .clone()
        .or_else(|| session.meta.cwd.clone())
        .context("a working directory is required")?;
    // The folder is `<root id>` beside the root session's file, whatever that file is called: the
    // one Claude Code looks in for that session id.
    let folder = Path::new(&of.root_location)
        .parent()
        .context("the root session's file has no folder")?
        .join(root)
        .join("subagents");
    let file = folder.join(format!("agent-{agent}.jsonl"));
    let meta_file = folder.join(format!("agent-{agent}.meta.json"));

    let mut clock = Clock::new(opts.now.as_deref());
    let mut rows = rows(plan, session, root, &cwd, opts, &mut clock);
    // A subagent's file holds its sidechain rows alone: the title and last-prompt rows belong to a
    // session's own file and carry the root's id.
    rows.retain(|row| !matches!(row["type"].as_str(), Some("custom-title" | "last-prompt")));
    for row in &mut rows {
        if let Some(map) = row.as_object_mut() {
            map.insert("isSidechain".into(), Value::Bool(true));
            map.insert("agentId".into(), Value::String(agent.to_string()));
        }
    }

    let agent_type = of.agent.clone().unwrap_or_else(|| "general-purpose".into());
    let mut meta = json!({
        "agentType": agent_type,
        "description": of
            .description
            .clone()
            .or_else(|| session.meta.title.clone())
            .unwrap_or_else(|| agent_type.clone()),
        "spawnDepth": of.depth,
        "requestShape": "foreground",
        "requestNonInteractive": false,
        "isBuiltIn": false,
        "finished": true,
    });
    if let Some(call) = &of.tool_use_id {
        meta["toolUseId"] = Value::String(call.clone());
    }
    // A child of a child is filed flat in the root's folder; its meta names the agent above it.
    if of.depth > 1
        && let Some((_, above)) = of.parent_id.split_once('.')
    {
        meta["parentAgentId"] = Value::String(above.to_string());
    }
    write_jsonl(&file, &rows)?;
    if let Err(error) = write_new_file(&meta_file, &serde_json::to_string(&meta)?) {
        let _ = std::fs::remove_file(&file);
        return Err(error);
    }
    Ok(Written {
        id: id.clone(),
        location: file.to_string_lossy().into_owned(),
        // A subagent is resumed through its parent.
        resume: crate::catalog::resume(
            HarnessId::ClaudeCode,
            opts.exe.as_deref().unwrap_or("claude"),
            root,
            None,
        ),
    })
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

/// Lay out the blocks of one chain of the plan (0 is the active path), and the branches that
/// leave from them.
fn lay(w: &mut RowWriter, plan: &plan::Plan, session: &Session, opts: &WriteOpts, chain: usize) {
    for (index, block) in plan.chain(chain).iter().enumerate() {
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
        // The branches that leave right after this block: laid out here, each from this block,
        // and then the path goes on from where it was.
        for n in plan.branches_after(Some((chain, index))) {
            lay_branch(w, plan, session, opts, n + 1);
        }
    }
}

/// Lay out branch `chain` from where the writer is, then put the writer back there.
fn lay_branch(
    w: &mut RowWriter,
    plan: &plan::Plan,
    session: &Session,
    opts: &WriteOpts,
    chain: usize,
) {
    let saved = (w.parent.clone(), w.calls_owner.clone());
    lay(w, plan, session, opts, chain);
    (w.parent, w.calls_owner) = saved;
}

/// The rows of a Claude Code session file; the Hermes writer hands them to Hermes's own import.
pub(crate) fn rows(
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

    // A branch that leaves before the first block comes first.
    for n in plan.branches_after(None) {
        lay_branch(&mut w, plan, session, opts, n + 1);
    }
    lay(&mut w, plan, session, opts, 0);

    // §4.4: every generated conversation row stays. Claude's native pair is appended before the
    // picker rows: a compact_boundary whose preserved segment names the retained tail, then the
    // summary row that hangs under it.
    if let Some(budget) = &opts.native_budget {
        let conversation: Vec<&Value> = w
            .out
            .iter()
            .filter(|row| {
                row.get("uuid").is_some()
                    && matches!(row["type"].as_str(), Some("user" | "assistant"))
            })
            .collect();
        let user_texts: Vec<Option<String>> =
            conversation.iter().map(|row| user_text(row)).collect();
        let items: Vec<NativeItem> = conversation
            .iter()
            .zip(&user_texts)
            .map(|(row, text)| NativeItem {
                id: row["uuid"].as_str(),
                json: *row,
                user_text: text.as_deref(),
            })
            .collect();
        if let Some(report) = plan_native_items(&items, budget) {
            let retained: Vec<String> = conversation[report.start..]
                .iter()
                .filter_map(|row| row["uuid"].as_str().map(str::to_string))
                .collect();
            if let (Some(head), Some(tail)) = (retained.first(), retained.last()) {
                let old_leaf = w.parent.clone();
                let boundary_uuid = new_uuid();
                let summary_uuid = new_uuid();
                let boundary = json!({
                    "parentUuid": Value::Null,
                    "logicalParentUuid": old_leaf,
                    "isSidechain": false,
                    "type": "system",
                    "subtype": "compact_boundary",
                    "content": "Conversation compacted",
                    "isMeta": false,
                    "timestamp": w.clock.next_iso(),
                    "uuid": boundary_uuid,
                    "level": "info",
                    "compactMetadata": {
                        "trigger": "manual",
                        "preTokens": report.estimated_tokens,
                        "durationMs": 0,
                        "preservedSegment": {"headUuid": head, "anchorUuid": summary_uuid, "tailUuid": tail},
                        "preservedMessages": {"anchorUuid": summary_uuid, "uuids": retained, "allUuids": retained},
                        "postTokens": report.projected_tokens,
                        "cumulativeDroppedTokens": 0,
                    },
                    "userType": "external",
                    "sessionId": w.session_id,
                    "cwd": w.cwd,
                });
                w.push(with_version(boundary, opts));
                let mut summary = w.user_row(Value::String(report.summary.clone()));
                if let Some(map) = summary.as_object_mut() {
                    map.insert("uuid".into(), Value::String(summary_uuid));
                    map.insert("parentUuid".into(), Value::String(boundary_uuid));
                    map.insert("isCompactSummary".into(), Value::Bool(true));
                    map.insert("isVisibleInTranscriptOnly".into(), Value::Bool(true));
                }
                w.push(with_version(summary, opts));
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

/// The text of a generated Claude user row. A tool_result row is an answer, not a user request.
fn user_text(row: &Value) -> Option<String> {
    if row["type"] != "user" {
        return None;
    }
    match &row["message"]["content"] {
        Value::String(text) => (!text.trim().is_empty()).then(|| text.clone()),
        Value::Array(blocks) => {
            if blocks.iter().any(|block| block["type"] == "tool_result") {
                return None;
            }
            let text = blocks
                .iter()
                .filter(|block| block["type"] == "text")
                .filter_map(|block| block["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n");
            (!text.trim().is_empty()).then_some(text)
        }
        _ => None,
    }
}

fn new_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// An image as Claude Code holds it, in a user turn or a tool result (Anthropic messages).
pub(crate) fn image_block(image: &Image) -> Value {
    json!({
        "type": "image",
        "source": {"type": "base64", "media_type": image.mime, "data": image.data},
    })
}
