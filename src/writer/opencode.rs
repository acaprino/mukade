//! The OpenCode 2.1.4 writer.
//!
//! OpenCode has its **own import path**, so the writer uses it instead of writing the store
//! (docs/session-interchange-spec.md §0, §1.2): it builds the interchange document
//! `{info, messages[]}` (OC2) and hands it to `opencode session import <file> --directory <D>
//! --standalone`.
//!
//! - **L8**: one assistant message per step, with `content: [reasoning, text, tool, tool]` — the
//!   probe emitted one message per block, which OpenCode then split into separate messages and
//!   sent reasoning without its signature as text;
//! - the call and its result live in the **same** tool part (`state{status,input,content}`), which
//!   is how OpenCode stores them (§2.3);
//! - **L1**: every `opencode` command carries `--standalone`; without it OpenCode 2.1.4 hands the
//!   work to the background service of the user's real home.

use super::plan::{self, PlannedBlock, PlannedResult, PlannedStep};
use super::{WriteOpts, Writer, Written};
use crate::model::{HarnessId, Reasoning, Session, ToolResult};
use crate::policy::{Clock, ReasoningPlan};
use crate::tools::content_blocks;
use anyhow::{Context, Result};
use serde_json::{Map, Value, json};
use std::path::PathBuf;
use std::process::Command;

struct ImportDocument(PathBuf);

impl Drop for ImportDocument {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// The OpenCode writer.
#[derive(Debug, Clone, Copy, Default)]
pub struct Opencode;

impl Writer for Opencode {
    fn harness(&self) -> HarnessId {
        HarnessId::OpenCode
    }

    fn write(&self, session: &Session, opts: &WriteOpts) -> Result<Written> {
        let id = new_session_id();
        let cwd = opts
            .cwd
            .clone()
            .or_else(|| session.meta.cwd.clone())
            .context("a working directory is required")?;
        let mut clock = Clock::new(opts.now.as_deref());
        let document = document_for(session, &id, &cwd, opts, &mut clock)?;

        // The document goes beside the destination's store, never over it.
        let root = opts
            .roots
            .opencode_db
            .as_ref()
            .and_then(|p| p.parent().map(PathBuf::from))
            .unwrap_or_else(std::env::temp_dir);
        std::fs::create_dir_all(&root).with_context(|| format!("create {}", root.display()))?;
        let file = root.join(format!("mukade-import-{id}.json"));
        std::fs::write(
            &file,
            serde_json::to_string(&document).context("serialize the document")?,
        )
        .with_context(|| format!("write {}", file.display()))?;

        let temporary = ImportDocument(file.clone());
        // The override wins; otherwise the standard resolution (MUKADE_OPENCODE_EXE, PATH, the
        // well-known install locations). The resume command names the binary actually used.
        let exe = opts.exe.clone().unwrap_or_else(|| crate::exe::resolve(HarnessId::OpenCode, None));
        let argv = import_command(&exe, &file, &cwd);
        let mut command = Command::new(&argv[0]);
        command.args(&argv[1..]);
        crate::reader::opencode::opencode_environment(&mut command, &opts.roots);
        let output = command
            .output()
            .with_context(|| format!("run {}", argv.join(" ")))?;
        if !output.status.success() {
            anyhow::bail!(
                "opencode session import failed ({}): {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        drop(temporary);
        // `Imported session: <id>` (OC2). The id the document asked for is the one that lands.
        let stdout = String::from_utf8_lossy(&output.stdout);
        let imported = stdout
            .lines()
            .find_map(|l| {
                l.strip_prefix("Imported session:")
                    .map(|s| s.trim().to_string())
            })
            .unwrap_or_else(|| id.clone());

        Ok(Written {
            id: imported.clone(),
            location: file.to_string_lossy().into_owned(),
            resume: vec![
                exe,
                "run".into(),
                "--standalone".into(),
                "--session".into(),
                imported,
            ],
        })
    }
}

/// L1: the exact argv every spawned `opencode` command carries.
pub fn import_command(exe: &str, file: &std::path::Path, directory: &str) -> Vec<String> {
    vec![
        exe.to_string(),
        "session".into(),
        "import".into(),
        file.to_string_lossy().into_owned(),
        "--directory".into(),
        directory.to_string(),
        "--standalone".into(),
    ]
}

/// The `{info, messages[]}` document `session import` consumes (OC2).
///
/// `info` must be a **complete** `Session.Info`: `projectID`, `cost` and `tokens` are required even
/// though the exporting side only fills them in for its own store — a document without them is
/// refused with `Bad Request` (probed 2026-10-28 against 2.1.4; each field alone is not enough,
/// all three together are).
pub fn document_for(
    session: &Session,
    id: &str,
    cwd: &str,
    opts: &WriteOpts,
    clock: &mut Clock,
) -> Result<Value> {
    let plan = plan::plan(session, HarnessId::OpenCode, opts);
    let model = model_of(session, opts)?;
    let project_id = opts
        .project_id
        .clone()
        .context("OpenCode needs the destination's project id (`--project-id`): its interchange format refuses a document without `info.projectID`")?;
    let mut messages: Vec<Value> = Vec::new();
    let mut i = 0;
    while i < plan.blocks.len() {
        match &plan.blocks[i] {
            PlannedBlock::User { text, attachments } => {
                messages.push(json!({
                    "id": message_id(),
                    "type": "user",
                    "time": {"created": clock.next_millis()},
                    "text": text,
                    "files": attachments.iter().filter_map(|a| (!a.raw.is_null()).then_some(a.raw.clone())).collect::<Vec<_>>(),
                }));
            }
            PlannedBlock::Compaction { summary: text } => {
                messages.push(json!({
                    "id": message_id(),
                    "type": "user",
                    "time": {"created": clock.next_millis()},
                    "text": text,
                    "files": [],
                }));
            }
            PlannedBlock::Step(step) => {
                let results: &[PlannedResult] = match plan.blocks.get(i + 1) {
                    Some(PlannedBlock::Results { results }) => results,
                    _ => &[],
                };
                messages.push(assistant_message(step, results, &model, clock));
                // A call that travelled as text has no tool part to answer it: its result follows
                // as text, in the same place the result was.
                let texts: Vec<Value> = results
                    .iter()
                    .filter_map(|r| match r {
                        PlannedResult::Text(t) => Some(Value::String(t.clone())),
                        _ => None,
                    })
                    .collect();
                if !texts.is_empty() {
                    messages.push(json!({
                        "id": message_id(),
                        "type": "user",
                        "time": {"created": clock.next_millis()},
                        "text": texts.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("\n\n"),
                        "files": [],
                    }));
                }
                if !results.is_empty() {
                    i += 1; // the results were folded into the tool parts above
                }
            }
            // Folded into the step above; a stray one has no call left to answer.
            PlannedBlock::Results { .. } => {}
        }
        i += 1;
    }

    let mut info = Map::new();
    info.insert("id".into(), Value::String(id.to_string()));
    info.insert("projectID".into(), Value::String(project_id));
    info.insert("location".into(), json!({"directory": cwd}));
    info.insert(
        "time".into(),
        json!({"created": clock.base_millis(), "updated": clock.base_millis()}),
    );
    info.insert("cost".into(), Value::from(0));
    info.insert(
        "tokens".into(),
        json!({"input": 0, "output": 0, "reasoning": 0, "cache": {"read": 0, "write": 0}}),
    );
    if let Some(title) = opts.title.as_ref().or(session.meta.title.as_ref()) {
        info.insert("title".into(), Value::String(title.clone()));
    }
    info.insert("model".into(), model);
    Ok(json!({"info": Value::Object(info), "messages": messages}))
}

/// OpenCode's interchange format **requires** a complete model reference — `{id, providerID}` — on
/// every assistant message, and a reference present on `info` must be complete too: `{"id": "…"}`
/// alone is refused with `Bad Request` (probed 2026-10-28 against 2.1.4, and the same document
/// without `providerID` never imports).
fn model_of(session: &Session, opts: &WriteOpts) -> Result<Value> {
    let id = opts
        .model
        .clone()
        .or_else(|| session.meta.models.first().map(|m| m.id.clone()))
        .context("no model id: state the destination's model")?;
    let provider = opts
        .model_provider
        .clone()
        .or_else(|| session.meta.models.first().and_then(|m| m.provider.clone()))
        .context("OpenCode needs the destination's provider id (`--model-provider`): its interchange format refuses a model reference without one")?;
    Ok(json!({"id": id, "providerID": provider}))
}

fn assistant_message(
    step: &PlannedStep,
    results: &[PlannedResult],
    model: &Value,
    clock: &mut Clock,
) -> Value {
    // L8: one message per step, with every block of that step in one `content` array.
    let mut content: Vec<Value> = Vec::new();
    for reasoning in &step.reasoning {
        if let ReasoningPlan::Native(Reasoning { text, opaque }) = reasoning {
            let mut block = Map::new();
            block.insert("type".into(), Value::String("reasoning".into()));
            block.insert("text".into(), Value::String(text.clone()));
            let mut native = Map::new();
            if let Some(signature) = opaque.as_ref().and_then(|o| o.signature.clone()) {
                native.insert("signature".into(), Value::String(signature));
            }
            if let Some(encrypted) = opaque.as_ref().and_then(|o| o.encrypted.clone()) {
                native.insert("encrypted".into(), Value::String(encrypted));
            }
            if !native.is_empty() {
                block.insert("native".into(), Value::Object(native));
            }
            content.push(Value::Object(block));
        }
    }
    for text in &step.text {
        content.push(json!({"type": "text", "text": text}));
    }
    for call in &step.calls {
        // The call and its result live in one part (§2.3).
        let answer = results.iter().find_map(|r| match r {
            PlannedResult::Native {
                call_id,
                content,
                is_error,
            } if *call_id == call.id => Some((content, *is_error)),
            _ => None,
        });
        let mut state = Map::new();
        state.insert("input".into(), call.input.clone());
        state.insert(
            "status".into(),
            Value::String(
                if answer.is_some_and(|(_, error)| error) {
                    "error"
                } else {
                    "completed"
                }
                .into(),
            ),
        );
        state.insert(
            "content".into(),
            match answer {
                Some((content, _)) => Value::Array(content_blocks(&ToolResult {
                    call_id: call.id.clone(),
                    content: content.clone(),
                    is_error: false,
                    truncated_by_source: false,
                })),
                None => Value::Array(Vec::new()),
            },
        );
        state.insert("metadata".into(), json!({"truncated": false}));
        content.push(json!({
            "type": "tool",
            "id": call.id,
            "name": call.name,
            "executed": false,
            "state": Value::Object(state),
            "time": {"created": clock.next_millis(), "completed": clock.base_millis()},
        }));
    }

    let mut message = Map::new();
    message.insert("id".into(), Value::String(message_id()));
    message.insert("type".into(), Value::String("assistant".into()));
    message.insert("agent".into(), Value::String("build".into()));
    message.insert("model".into(), model.clone());
    message.insert("content".into(), Value::Array(content));
    message.insert(
        "finish".into(),
        Value::String(
            if step.calls.is_empty() {
                "stop"
            } else {
                "tool-calls"
            }
            .into(),
        ),
    );
    message.insert(
        "rawFinish".into(),
        Value::String(
            if step.calls.is_empty() {
                "end_turn"
            } else {
                "tool_use"
            }
            .into(),
        ),
    );
    message.insert(
        "time".into(),
        json!({"created": clock.base_millis(), "completed": clock.base_millis()}),
    );
    message.insert("cost".into(), Value::from(0));
    Value::Object(message)
}

/// OpenCode ids are prefixed; the suffix is opaque.
fn new_session_id() -> String {
    format!("ses_{}", uuid::Uuid::new_v4().simple())
}

fn message_id() -> String {
    format!("msg_{}", uuid::Uuid::new_v4().simple())
}
