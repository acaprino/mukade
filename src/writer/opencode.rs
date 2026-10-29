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
use crate::model::{HarnessId, Opaque, Protocol, Reasoning, Session, ToolResult};
use crate::policy::{Clock, ReasoningPlan};
use crate::tools::content_blocks;
use anyhow::{Context, Result};
use serde_json::{Map, Value, json};
use std::path::PathBuf;
use std::process::Command;

/// How long `opencode session import` may take before it is stopped (a locked database, say).
const IMPORT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// The import document holds the conversation: it is created new and, on Unix, readable by its
/// owner only, wherever the temporary folder is.
fn write_private(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(bytes)
}

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

    fn write_planned(
        &self,
        session: &Session,
        plan: &plan::Plan,
        opts: &WriteOpts,
    ) -> Result<Written> {
        super::validate_options(HarnessId::OpenCode, opts)?;
        let id = match opts.dest_id.as_deref() {
            Some(id) => {
                validate_session_id(id)?;
                id.to_string()
            }
            None => new_session_id(),
        };
        let cwd = opts
            .cwd
            .clone()
            .or_else(|| session.meta.cwd.clone())
            .context("a working directory is required")?;
        // `opencode session import … --directory <cwd>`: a value that starts with `-` would be read
        // as an option.
        if cwd.starts_with('-') {
            anyhow::bail!("the working directory {cwd:?} starts with '-': not passed to opencode");
        }
        let mut clock = Clock::new(opts.now.as_deref());
        let document = document_from_plan(session, plan, &id, &cwd, opts, &mut clock)?;

        // The document is mukade's own temporary file: it goes to `mukade-data/tmp` beside the
        // executable, never into OpenCode's store nor the system's temporary folder. One a killed
        // run left behind is removed here once it is old enough to be nobody's.
        let temp = crate::portable::temp_dir()?;
        crate::portable::sweep_stale(
            &temp,
            "mukade-import-",
            ".json",
            crate::portable::STALE_AFTER,
        );
        let file = temp.join(format!("mukade-import-{id}-{}.json", uuid::Uuid::new_v4().simple()));
        // The guard exists before the file does: a write that fails half-way leaves nothing behind.
        let temporary = ImportDocument(file.clone());
        write_private(
            &file,
            serde_json::to_string(&document)
                .context("serialize the document")?
                .as_bytes(),
        )
        .with_context(|| format!("write {}", file.display()))?;
        // The override wins; otherwise the standard resolution (MUKADE_OPENCODE_EXE, PATH, the
        // well-known install locations). The resume command names the binary actually used.
        let exe = opts
            .exe
            .clone()
            .unwrap_or_else(|| crate::exe::resolve(HarnessId::OpenCode, None));
        let argv = import_command(&exe, &file, &cwd);
        let mut command = Command::new(&argv[0]);
        command.args(&argv[1..]);
        crate::reader::opencode::opencode_environment(&mut command, &opts.roots);
        let output = crate::exe::run_within(&mut command, IMPORT_TIMEOUT).map_err(|error| {
            failed_import(
                &id,
                &opts.roots,
                format!("could not complete the import process: {error}"),
            )
        })?;
        let Some(status) = output.status else {
            return Err(failed_import(
                &id,
                &opts.roots,
                format!("opencode session import did not finish within {IMPORT_TIMEOUT:?}"),
            ));
        };
        if !status.success() {
            return Err(failed_import(
                &id,
                &opts.roots,
                format!(
                    "opencode session import failed ({status}): {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
            ));
        }
        drop(temporary);
        // `Imported session: <id>` (OC2). The id the document asked for is the one that lands.
        let stdout = String::from_utf8_lossy(&output.stdout);
        let imported = stdout.lines().find_map(|l| {
            l.strip_prefix("Imported session:")
                .map(|s| s.trim().to_string())
        });
        let Some(imported) = imported.filter(|value| !value.is_empty()) else {
            return Err(super::WriteOutcomeUnknown {
                id,
                location: opts
                    .roots
                    .opencode_db
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned()),
                reason: format!(
                    "the engine did not confirm an import: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
            }
            .into());
        };
        if (opts.dest_id.is_some() || opts.metadata.is_some()) && imported != id {
            return Err(super::WriteOutcomeUnknown {
                id,
                location: opts
                    .roots
                    .opencode_db
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned()),
                reason: format!("the engine reported a different session ID: {imported}"),
            }
            .into());
        }
        if opts.dest_id.is_some() || opts.metadata.is_some() {
            confirm_import(&id, &opts.roots, opts.metadata.as_ref()).map_err(|error| {
                super::WriteOutcomeUnknown {
                    id: id.clone(),
                    location: opts
                        .roots
                        .opencode_db
                        .as_ref()
                        .map(|path| path.to_string_lossy().into_owned()),
                    reason: format!("could not verify the imported session: {error:#}"),
                }
            })?;
        }

        Ok(Written {
            id: imported.clone(),
            // The store the session now lives in: the import document is gone by now.
            location: match opts.roots.opencode_db.as_ref() {
                Some(db) => db.to_string_lossy().into_owned(),
                None => format!("opencode's configured store, session {imported}"),
            },
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

/// Confirm the requested ID and supplied provenance in the selected destination, without writing.
fn confirm_import(
    id: &str,
    roots: &crate::roots::StoreRoots,
    expected: Option<&Value>,
) -> Result<()> {
    use rusqlite::{Connection, OpenFlags, OptionalExtension};
    let path = roots
        .opencode_db
        .as_ref()
        .context("the destination database was not explicitly selected")?;
    let db = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("open {} read-only", path.display()))?;
    db.busy_timeout(std::time::Duration::from_secs(2))?;
    if let Some(expected) = expected {
        let raw: Option<Option<String>> = db
            .query_row(
                "SELECT metadata FROM session_v2 WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .optional()?;
        let raw = raw.flatten().context("session or metadata not found")?;
        let actual: Value = serde_json::from_str(&raw).context("invalid stored metadata")?;
        let expected = expected
            .as_object()
            .context("session metadata must be a JSON object")?;
        let actual = actual
            .as_object()
            .context("stored metadata is not a JSON object")?;
        anyhow::ensure!(
            expected
                .iter()
                .all(|(key, value)| actual.get(key) == Some(value)),
            "stored metadata does not match the requested provenance"
        );
    } else {
        let found: Option<i32> = db
            .query_row("SELECT 1 FROM session_v2 WHERE id = ?1", [id], |row| {
                row.get(0)
            })
            .optional()?;
        anyhow::ensure!(found.is_some(), "session not found");
    }
    Ok(())
}

/// A started subprocess may still be committing or may have written before it reported failure.
fn failed_import(id: &str, roots: &crate::roots::StoreRoots, reason: String) -> anyhow::Error {
    super::WriteOutcomeUnknown {
        id: id.to_string(),
        location: roots
            .opencode_db
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned()),
        reason,
    }
    .into()
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
    document_from_plan(session, &plan, id, cwd, opts, clock)
}

/// [`document_for`] for a plan that is already made.
pub fn document_from_plan(
    session: &Session,
    plan: &plan::Plan,
    id: &str,
    cwd: &str,
    opts: &WriteOpts,
    clock: &mut Clock,
) -> Result<Value> {
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
                let mut assistant = assistant_message(
                    step,
                    results,
                    &model,
                    opts.dest_agent.as_deref().unwrap_or("build"),
                    clock,
                );
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
                    assistant["content"].as_array_mut().expect("assistant content is an array").push(json!({
                        "type": "text", "text": texts.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("\n\n")
                    }));
                }
                messages.push(assistant);
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
    if let Some(metadata) = &opts.metadata {
        anyhow::ensure!(
            metadata.is_object(),
            "session metadata must be a JSON object"
        );
        info.insert("metadata".into(), metadata.clone());
    }
    info.insert("model".into(), model);
    if let Some(agent) = &opts.dest_agent {
        validate_agent(agent)?;
        info.insert("agent".into(), Value::String(agent.clone()));
    }
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

/// OpenCode's own `native` for a reasoning part: the signature or the encrypted payload.
fn opencode_native(opaque: &Opaque) -> Map<String, Value> {
    let mut native = Map::new();
    if let Some(signature) = opaque.signature.clone() {
        native.insert("signature".into(), Value::String(signature));
    }
    if let Some(encrypted) = opaque.encrypted.clone() {
        native.insert("encrypted".into(), Value::String(encrypted));
    }
    native
}

/// Through OpenRouter, OpenCode keeps and replays a provider's reasoning as OpenRouter's own
/// `reasoning_details` (`native.reasoningDetails`, seen in a live export of 2.1.4, 2026-10-28):
/// signed text is a `reasoning.text` detail with its signature, an encrypted payload a
/// `reasoning.encrypted` detail with its data; `format` names the protocol that made it.
fn openrouter_native(text: &str, opaque: &Opaque, index: usize) -> Map<String, Value> {
    let format = match &opaque.protocol {
        Protocol::AnthropicMessages => "anthropic-claude-v1",
        Protocol::OpenaiResponses => "openai-responses-v1",
        _ => "unknown",
    };
    let detail = match (&opaque.signature, &opaque.encrypted) {
        (Some(signature), _) => json!({"type": "reasoning.text", "text": text,
                                       "signature": signature, "format": format, "index": index}),
        (None, Some(data)) => json!({"type": "reasoning.encrypted", "data": data,
                                     "format": format, "index": index}),
        (None, None) => return Map::new(),
    };
    let mut native = Map::new();
    native.insert("reasoningDetails".into(), Value::Array(vec![detail]));
    native
}

fn assistant_message(
    step: &PlannedStep,
    results: &[PlannedResult],
    model: &Value,
    agent: &str,
    clock: &mut Clock,
) -> Value {
    // Times follow the conversation: the message after the one it answers, each part inside it.
    let created = clock.next_millis();
    // L8: one message per step, with every block of that step in one `content` array.
    let mut content: Vec<Value> = Vec::new();
    let through_openrouter = model.get("providerID").and_then(Value::as_str) == Some("openrouter");
    for (index, reasoning) in step.reasoning.iter().enumerate() {
        if let ReasoningPlan::Native(Reasoning { text, opaque }) = reasoning {
            let mut block = Map::new();
            block.insert("type".into(), Value::String("reasoning".into()));
            block.insert("text".into(), Value::String(text.clone()));
            let native = match opaque {
                Some(opaque) if through_openrouter => openrouter_native(text, opaque, index),
                Some(opaque) => opencode_native(opaque),
                None => Map::new(),
            };
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
        // A failed call needs its `error`, a `Session.StructuredError`: without it OpenCode 2.1.4
        // refuses the whole import ([V] 2026-10-28, `Bad Request`). `tool.execution` is the type
        // OpenCode itself records for a tool that failed.
        if let Some((content, true)) = answer {
            state.insert(
                "error".into(),
                json!({"type": "tool.execution", "message": content.text()}),
            );
        }
        state.insert("metadata".into(), json!({"truncated": false}));
        content.push(json!({
            "type": "tool",
            "id": call.id,
            "name": call.name,
            "executed": false,
            "state": Value::Object(state),
            "time": {"created": clock.next_millis(), "completed": clock.next_millis()},
        }));
    }

    let mut message = Map::new();
    message.insert("id".into(), Value::String(message_id()));
    message.insert("type".into(), Value::String("assistant".into()));
    message.insert("agent".into(), Value::String(agent.to_string()));
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
        json!({"created": created, "completed": clock.next_millis()}),
    );
    message.insert("cost".into(), Value::from(0));
    Value::Object(message)
}

/// OpenCode ids are prefixed; the suffix is opaque.
fn new_session_id() -> String {
    format!("ses_{}", uuid::Uuid::new_v4().simple())
}

/// A caller-provided ID also enters a temporary filename; do not permit paths or flags.
pub fn validate_session_id(id: &str) -> Result<()> {
    let suffix = id.strip_prefix("ses_").unwrap_or("");
    anyhow::ensure!(
        !suffix.is_empty()
            && id.len() <= 128
            && suffix
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'),
        "invalid OpenCode destination session ID"
    );
    Ok(())
}

/// An agent name is a bounded, non-empty identifier, never a control sequence.
pub fn validate_agent(agent: &str) -> Result<()> {
    anyhow::ensure!(
        !agent.trim().is_empty() && agent.len() <= 256 && !agent.chars().any(char::is_control),
        "invalid OpenCode destination agent"
    );
    Ok(())
}

fn message_id() -> String {
    format!("msg_{}", uuid::Uuid::new_v4().simple())
}
