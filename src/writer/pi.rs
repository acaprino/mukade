//! The Pi 1.2.0 writer.
//!
//! Shape it must produce (docs/session-interchange-spec.md §2.4, L3):
//!
//! - `sessions\<project>\<ISO ts>_<uuid>.jsonl`: a `session` header, then `message` records, each
//!   with its own `id` and a `parentId` chain;
//! - an assistant message holds thinking, text and **all** the calls of one step; Pi inserts
//!   `No result provided` when it finds an assistant tool call not followed by its result, and
//!   sends the real result twice (§6.2) — the probe wrote one message per call, which is the bug
//!   L3 exists for;
//! - one `toolResult` record per result, in the same order as the calls;
//! - `bash.timeout` is in **seconds** (L12), so the canonical milliseconds are converted.

use super::media::Image;
use super::plan::{self, PlannedBlock, PlannedResult, PlannedStep};
use super::{WriteOpts, Writer, Written, write_jsonl};
use crate::model::{HarnessId, Reasoning, Session, StepEnd, ToolResult};
use crate::native_compaction::{NativeItem, plan_native_items};
use crate::policy::{Clock, ReasoningPlan};
use crate::tools::content_blocks;
use anyhow::{Context, Result};
use chrono::DateTime;
use serde_json::{Map, Value, json};

/// The Pi writer.
#[derive(Debug, Clone, Copy, Default)]
pub struct Pi;

impl Writer for Pi {
    fn harness(&self) -> HarnessId {
        HarnessId::Pi
    }

    /// A tree: every entry names its parent, and the last entry is the leaf Pi resumes.
    fn keeps_branches(&self) -> plan::BranchKeeping {
        plan::BranchKeeping::TreeLeafLast
    }

    fn write_planned(
        &self,
        session: &Session,
        plan: &plan::Plan,
        opts: &WriteOpts,
    ) -> Result<Written> {
        crate::harnesses::destination(HarnessId::Pi)?.validate_write(opts)?;
        let root = opts
            .roots
            .pi_agent_dir
            .clone()
            .context("PI_CODING_AGENT_DIR is not set: nowhere to write the Pi session")?;
        let cwd = opts
            .cwd
            .clone()
            .or_else(|| session.meta.cwd.clone())
            .context("a working directory is required")?;
        let id = opts
            .dest_id
            .clone()
            .unwrap_or_else(crate::policy::new_session_id);
        let mut clock = Clock::new(opts.now.as_deref());
        let stamp = file_stamp(clock.base_millis());

        let dir = root.join("sessions").join(project_dir_name(&cwd));
        std::fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
        let file = opts
            .dest_path
            .clone()
            .unwrap_or_else(|| dir.join(format!("{stamp}_{id}.jsonl")));

        let rows = rows(plan, session, &id, &cwd, opts, &mut clock);
        write_jsonl(&file, &rows)?;

        Ok(Written {
            records: Vec::new(),
            id: id.clone(),
            location: file.to_string_lossy().into_owned(),
            // Pi searches the current project first, so the library passes the file path (§5).
            resume: crate::catalog::resume(
                HarnessId::Pi,
                opts.exe.as_deref().unwrap_or("pi"),
                &id,
                Some(&file),
            ),
        })
    }
}

/// Pi's session file stamp, the ISO instant with `:` and `.` turned into `-`:
/// `2026-10-27T16-00-39-689Z`.
fn file_stamp(millis: i64) -> String {
    match DateTime::from_timestamp_millis(millis) {
        Some(dt) => format!(
            "{}-{:03}Z",
            dt.format("%Y-%m-%dT%H-%M-%S"),
            dt.timestamp_subsec_millis()
        ),
        None => "1970-01-01T00-00-00-000Z".into(),
    }
}

/// Pi's own folder for a working directory (`getDefaultSessionDirPath`, Pi 1.2.0): the resolved
/// path, one leading separator dropped, and only `/`, `\` and `:` turned into `-`, between `--`.
fn project_dir_name(cwd: &str) -> String {
    let resolved = std::path::absolute(cwd)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| cwd.to_string());
    let name = resolved
        .strip_prefix(['/', '\\'])
        .unwrap_or(&resolved)
        .replace(['/', '\\', ':'], "-");
    format!("--{name}--")
}

/// Lay out the blocks of one chain of the plan (0 is the active path), and the branches that
/// leave from them.
fn lay(
    out: &mut Vec<Value>,
    parent: &mut Option<String>,
    plan: &plan::Plan,
    session: &Session,
    opts: &WriteOpts,
    clock: &mut Clock,
    chain: usize,
) {
    let blocks = plan.chain(chain);
    let mut i = 0;
    while i < blocks.len() {
        let first = i;
        match &blocks[i] {
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
                let content = if content.len() == 1
                    && content[0].get("type").and_then(Value::as_str) == Some("text")
                {
                    Value::String(text.clone())
                } else {
                    Value::Array(content)
                };
                let message = json!({
                    "role": "user",
                    "content": content,
                    "timestamp": clock.next_millis(),
                });
                push(out, parent, |row_id, parent_id| {
                    message_row(row_id, parent_id, &clock.next_iso(), message)
                });
            }
            PlannedBlock::Compaction { summary: text } => {
                let message = json!({
                    "role": "user",
                    "content": [{"type": "text", "text": text}],
                    "timestamp": clock.next_millis(),
                });
                push(out, parent, |row_id, parent_id| {
                    message_row(row_id, parent_id, &clock.next_iso(), message)
                });
            }
            PlannedBlock::Step(step) => {
                let message = assistant_message(step, session, opts, clock);
                push(out, parent, |row_id, parent_id| {
                    message_row(row_id, parent_id, &clock.next_iso(), message)
                });
                // L3: every call of the step lives in that one message; its results follow as one
                // record each, in the same order.
                if let Some(PlannedBlock::Results { results }) = blocks.get(i + 1) {
                    for result in results {
                        match result {
                            PlannedResult::Native {
                                call_id,
                                content,
                                is_error,
                            } => {
                                let Some(name) = step
                                    .calls
                                    .iter()
                                    .find(|c| c.id == *call_id)
                                    .map(|c| c.name.clone())
                                else {
                                    continue;
                                };
                                let message = json!({
                                    "role": "toolResult",
                                    "toolCallId": call_id,
                                    "toolName": name,
                                    "content": content_blocks(&ToolResult {
                                        call_id: call_id.clone(),
                                        content: content.clone(),
                                        is_error: *is_error,
                                        truncated_by_source: false,
                                    }),
                                    "isError": is_error,
                                    "timestamp": clock.next_millis(),
                                });
                                push(out, parent, |row_id, parent_id| {
                                    message_row(row_id, parent_id, &clock.next_iso(), message)
                                });
                            }
                            // A call that travelled as text answers with text, in the same place.
                            PlannedResult::Text(t) => {
                                let message = json!({
                                    "role": "user",
                                    "content": [{"type": "text", "text": t}],
                                    "timestamp": clock.next_millis(),
                                });
                                push(out, parent, |row_id, parent_id| {
                                    message_row(row_id, parent_id, &clock.next_iso(), message)
                                });
                            }
                        }
                    }
                    i += 1; // the results block was written here, beside its calls
                }
            }
            // Results were written next to the step they answer; a stray one has no call left.
            PlannedBlock::Results { .. } => {}
        }
        // The branches that leave right after a block this step wrote (a step and its results go
        // together): each laid out from there, then the path goes on from where it was.
        for at in first..=i {
            for n in plan.branches_after(Some((chain, at))) {
                lay_branch(out, parent, plan, session, opts, clock, n + 1);
            }
        }
        i += 1;
    }
}

/// Lay out branch `chain` from where the writer is, then put the writer back there.
fn lay_branch(
    out: &mut Vec<Value>,
    parent: &mut Option<String>,
    plan: &plan::Plan,
    session: &Session,
    opts: &WriteOpts,
    clock: &mut Clock,
    chain: usize,
) {
    let saved = parent.clone();
    lay(out, parent, plan, session, opts, clock, chain);
    *parent = saved;
}

fn rows(
    plan: &plan::Plan,
    session: &Session,
    id: &str,
    cwd: &str,
    opts: &WriteOpts,
    clock: &mut Clock,
) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    let mut parent: Option<String> = None;

    let mut header = json!({
        "type": "session",
        "version": 3,
        "id": id,
        "timestamp": clock.next_iso(),
        "cwd": cwd,
    });
    // A fork names its origin's file, as Pi's own `/fork` does.
    if let Some(fork) = &opts.fork_of {
        header["parentSession"] = Value::String(fork.location.clone());
    }
    out.push(header);

    // A branch that leaves before the first entry comes first.
    for n in plan.branches_after(None) {
        lay_branch(&mut out, &mut parent, plan, session, opts, clock, n + 1);
    }
    lay(&mut out, &mut parent, plan, session, opts, clock, 0);

    // §4.4: every generated entry stays in the file; Pi's own compaction entry is appended and points
    // back at the first retained entry.
    if let Some(budget) = &opts.native_budget {
        let message_rows: Vec<&Value> = out
            .iter()
            .skip(1)
            .filter(|row| row["type"] == "message")
            .collect();
        let user_texts: Vec<Option<String>> =
            message_rows.iter().map(|row| user_text(row)).collect();
        let items: Vec<NativeItem> = message_rows
            .iter()
            .zip(&user_texts)
            .map(|(row, text)| NativeItem {
                id: row["id"].as_str(),
                json: *row,
                user_text: text.as_deref(),
            })
            .collect();
        if let Some(report) = plan_native_items(&items, budget) {
            if let Some(first_kept) = report.keep_id {
                out.push(json!({
                    "type": "compaction",
                    "id": short_id(),
                    "parentId": parent,
                    "timestamp": clock.next_iso(),
                    "summary": report.summary,
                    "firstKeptEntryId": first_kept,
                    "tokensBefore": report.estimated_tokens,
                }));
            }
        }
    }
    out
}

/// Append one message row, chaining it on the previous one.
fn push(
    out: &mut Vec<Value>,
    parent: &mut Option<String>,
    build: impl FnOnce(&str, Option<&str>) -> Value,
) {
    let id = short_id();
    out.push(build(&id, parent.as_deref()));
    *parent = Some(id);
}

fn assistant_message(
    step: &PlannedStep,
    session: &Session,
    opts: &WriteOpts,
    clock: &mut Clock,
) -> Value {
    let mut content: Vec<Value> = Vec::new();
    for reasoning in &step.reasoning {
        if let ReasoningPlan::Native(Reasoning { text, opaque }) = reasoning {
            let signature = opaque.as_ref().and_then(|o| o.signature.clone());
            let encrypted = opaque.as_ref().and_then(|o| o.encrypted.clone());
            content.push(match (signature, encrypted) {
                // Anthropic's `redacted_thinking`, in the shape Pi keeps it and sends it back.
                (None, Some(data)) => json!({
                    "type": "thinking",
                    "thinking": "[Reasoning redacted]",
                    "thinkingSignature": data,
                    "redacted": true,
                }),
                (signature, _) => json!({
                    "type": "thinking",
                    "thinking": text,
                    "thinkingSignature": signature,
                }),
            });
        }
    }
    for text in &step.text {
        content.push(json!({"type": "text", "text": text}));
    }
    for call in &step.calls {
        content.push(
            json!({"type": "toolCall", "id": call.id, "name": call.name, "arguments": call.input}),
        );
    }
    let mut message = Map::new();
    message.insert("role".into(), Value::String("assistant".into()));
    message.insert("content".into(), Value::Array(content));
    if let Some(protocol) = opts.dest_protocol.as_ref() {
        message.insert(
            "api".into(),
            Value::String(plan::protocol_name(protocol).to_string()),
        );
    }
    if let Some(provider) = plan::provider_id(step, session, opts) {
        message.insert("provider".into(), Value::String(provider));
    }
    if let Some(model) = plan::model_id(step, session, opts) {
        message.insert("model".into(), Value::String(model));
    }
    // Pi leaves an aborted or failed turn out of the next request, so the step keeps its end.
    message.insert(
        "stopReason".into(),
        Value::String(
            match step.end {
                StepEnd::Aborted => "aborted",
                StepEnd::Error => "error",
                _ if step.calls.is_empty() => "stop",
                _ => "toolUse",
            }
            .to_string(),
        ),
    );
    message.insert("timestamp".into(), Value::from(clock.next_millis()));
    // Pi reads `usage.totalTokens` on every assistant message and refuses the session without it
    // [V: `Cannot read properties of undefined (reading 'totalTokens')`]. The source's own numbers
    // are not part of the model, so the counters are zero — an absence, not an invented figure.
    message.insert("usage".into(), zero_usage());
    Value::Object(message)
}

/// The usage envelope Pi writes on an assistant message.
fn zero_usage() -> Value {
    json!({
        "input": 0,
        "output": 0,
        "cacheRead": 0,
        "cacheWrite": 0,
        "totalTokens": 0,
        "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0},
        "cacheWrite1h": 0,
    })
}

fn message_row(id: &str, parent_id: Option<&str>, timestamp: &str, message: Value) -> Value {
    let mut row = Map::new();
    row.insert("type".into(), Value::String("message".into()));
    row.insert("id".into(), Value::String(id.to_string()));
    row.insert(
        "parentId".into(),
        parent_id
            .map(|p| Value::String(p.to_string()))
            .unwrap_or(Value::Null),
    );
    row.insert("timestamp".into(), Value::String(timestamp.to_string()));
    row.insert("message".into(), message);
    Value::Object(row)
}

/// The text of a Pi user message, whether the writer stored it as a string or as text blocks.
fn user_text(row: &Value) -> Option<String> {
    if row["message"]["role"] != "user" {
        return None;
    }
    match &row["message"]["content"] {
        Value::String(text) => Some(text.clone()),
        Value::Array(blocks) => {
            let text = blocks
                .iter()
                .filter(|block| block["type"] == "text")
                .filter_map(|block| block["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n");
            (!text.is_empty()).then_some(text)
        }
        _ => None,
    }
}

/// Pi stamps every record with a short hex id of its own.
fn short_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..8].to_string()
}

/// An image as Pi holds it, in a user turn or a tool result (pi-ai `ImageContent`).
pub(crate) fn image_block(image: &Image) -> Value {
    json!({"type": "image", "data": image.data, "mimeType": image.mime})
}
