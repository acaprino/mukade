//! The Pi 1.2.0 reader.
//!
//! Facts this reader depends on (docs/session-interchange-spec.md §2.4):
//!
//! - the file is JSON Lines: a `session` header, then `model_change`, `thinking_level_change`,
//!   a stored **system message**, and the conversation, each record carrying its own `id` and
//!   `parentId`;
//! - an assistant message holds thinking with `thinkingSignature`, text, and **both parallel
//!   `toolCall`s in one message**; results are one `toolResult` record each;
//! - a compaction is `{type:"compaction",summary,firstKeptEntryId,tokensBefore,…}`;
//! - a killed turn leaves only the user entry, so the last entry of a killed session is a user
//!   turn with no reply (L9).

use super::clip::{Clip, Clipped};
use super::json::{result_content, str_at};
use super::{Reader, SessionInfo};
use crate::model::{
    CanonicalTool, Entry, HarnessId, Kind, Meta, MetaKind, ModelRef, Opaque, ParentKind, Protocol,
    Reasoning, Session, StepEnd, ToolCall, ToolResult, Trigger,
};
use crate::roots::StoreRoots;
use anyhow::{Context, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// The Pi reader.
#[derive(Debug, Clone, Copy, Default)]
pub struct Pi;

/// Read one Pi session file (`<PI_CODING_AGENT_DIR>/sessions/<project>/<ts>_<uuid>.jsonl`).
pub fn read_file(path: &Path) -> Result<Session> {
    read_file_with(path, false)
}

/// [`read_file`] with the branches the conversation left (`/tree`) carried as branches
/// (`parentId` tree) instead of left behind as off-branch records.
pub fn read_file_all_branches(path: &Path) -> Result<Session> {
    read_file_with(path, true)
}

fn read_file_with(path: &Path, branches: bool) -> Result<Session> {
    let mut session = read_one(path)?;
    if branches {
        // Pi chains every message, results included, one after another: nothing attaches.
        super::branches::lift(
            &mut session,
            "id",
            "parentId",
            |_, _| false,
            |rows| {
                let mut b = Builder::default();
                for (i, row) in rows.into_iter().enumerate() {
                    b.push(i, row, true);
                }
                Ok(b.finish(None)?.entries)
            },
        )?;
    }
    Ok(session)
}

fn read_one(path: &Path) -> Result<Session> {
    let file = super::json::read_jsonl(path)?;
    // The digest is computed on its own thread while the records are built.
    read_one_from(path, file.rows, file.unfinished, || file.sha256.wait())
}

/// The session in `path` from its decoded records (clipped, or whole), the last line that did not
/// parse, and the file's digest.
fn read_one_from(
    path: &Path,
    rows: Vec<Value>,
    unfinished: Option<String>,
    sha256: impl FnOnce() -> String,
) -> Result<Session> {
    let _span = crate::timing::span("build entries");
    let mut session = parse_owned(rows, Some(path))?;
    session.meta.source_sha256 = Some(sha256());
    super::json::keep_unfinished(&mut session, unfinished);
    Ok(session)
}

/// [`read_file`] that never refuses for size: see [`Reader::read_clipped`].
pub fn read_file_clipped(path: &Path, clip: &Clip) -> Result<Clipped> {
    let clipped = super::clip::clipped_rows(path, clip, super::clip::KEEP_NONE)?;
    let session = read_one_from(
        path,
        clipped.rows,
        clipped.unfinished.map(|(text, _)| text),
        || clipped.sha256,
    )?;
    Ok(Clipped {
        session,
        clipping: clipped.clipping,
    })
}

/// [`read_file`], its entries handed to `on_entry` one at a time (see [`Reader::read_streamed`]).
/// A first pass decodes the file one line at a time with every string clipped and keeps each
/// record's byte offset; it gives which records are on the resumed branch. A second pass builds
/// the entries with the same builder from the full records read back by offset, and hands each one
/// over as soon as no later record can change it. A session with `context_edit` records, which
/// rewrite earlier records' text, is read whole.
pub fn read_file_streamed(
    path: &Path,
    on_entry: &mut dyn FnMut(Entry) -> Result<()>,
) -> Result<Meta> {
    read_file_streamed_with_meta(path, &mut |_| Ok(()), on_entry)
}

/// [`read_file_streamed`] that hands the session's [`Meta`] over **first**, once the clipped pass has
/// it (see [`Reader::read_streamed_with_meta`]), and returns the final one.
pub fn read_file_streamed_with_meta(
    path: &Path,
    on_meta: &mut dyn FnMut(&Meta) -> Result<()>,
    on_entry: &mut dyn FnMut(Entry) -> Result<()>,
) -> Result<Meta> {
    let first =
        super::clip::clipped_rows(path, &super::clip::STRUCTURE_PASS, super::clip::KEEP_NONE)?;
    let sha256 = first.sha256;
    let (offsets, unfinished) = (first.offsets, first.unfinished);
    let (clipped, plan) = parse_planned(first.rows, Some(path))?;
    if plan.edited {
        let session = read_one(path)?;
        on_meta(&session.meta)?;
        for entry in session.entries {
            on_entry(entry)?;
        }
        return Ok(session.meta);
    }
    {
        // Built from clipped records: the same facts, but for one longer than the clip.
        let mut early = clipped.meta.clone();
        early.source_sha256 = Some(sha256.clone());
        on_meta(&early)?;
    }
    let mut b = Builder::default();
    let mut rows = super::clip::RowReader::open(path)?;
    let mut delivered = 0usize;
    let mut hand_over = |entries: Vec<Entry>| -> Result<()> {
        for entry in entries {
            let Some(shape) = clipped.entries.get(delivered) else {
                anyhow::bail!("the streamed read built more entries than the clipped one");
            };
            anyhow::ensure!(
                shape.id == entry.id,
                "the streamed read disagrees with the clipped one at entry {delivered}: \
                 {:?} against {:?}",
                entry.id,
                shape.id
            );
            delivered += 1;
            on_entry(entry)?;
        }
        Ok(())
    };
    for (i, &offset) in offsets.iter().enumerate() {
        b.push(i, rows.row_at(offset)?, plan.on_branch[i]);
        hand_over(b.take_final(false))?;
    }
    hand_over(b.take_final(true))?;
    anyhow::ensure!(
        delivered == clipped.entries.len(),
        "the streamed read built {delivered} entries, the clipped one {}",
        clipped.entries.len()
    );
    let mut meta = b.finish(Some(path))?.meta;
    if let Some((_, at)) = unfinished {
        on_entry(super::json::unfinished_entry(
            rows.text_at(at)?,
            clipped.entries.len(),
        ))?;
    }
    meta.source_sha256 = Some(sha256);
    Ok(meta)
}

/// Read records that were already decoded.
pub fn parse(rows: &[Value], source_path: Option<&Path>) -> Result<Session> {
    parse_owned(rows.to_vec(), source_path)
}

/// [`parse`] for records the caller no longer needs: each one moves into its entry's `raw`
/// instead of being copied there.
pub fn parse_owned(rows: Vec<Value>, source_path: Option<&Path>) -> Result<Session> {
    Ok(parse_planned(rows, source_path)?.0)
}

/// How a session's records were read: what a second pass over the same records needs.
struct Plan {
    on_branch: Vec<bool>,
    /// Some record's text was replaced by a `context_edit`.
    edited: bool,
}

/// [`parse_owned`], and the plan it followed.
fn parse_planned(mut rows: Vec<Value>, source_path: Option<&Path>) -> Result<(Session, Plan)> {
    let on_branch = active_branch(&rows);
    let mut originals = apply_context_edits(&mut rows, &on_branch);
    let edited = !originals.is_empty();
    let mut b = Builder::default();
    for (i, row) in rows.into_iter().enumerate() {
        b.push(i, row, on_branch[i]);
    }
    let mut session = b.finish(source_path)?;
    // The entries were built from the edited view of a record; the record itself is what is kept.
    if !originals.is_empty() {
        for entry in &mut session.entries {
            for raw in &mut entry.raw {
                let original = raw
                    .get("id")
                    .and_then(Value::as_str)
                    .and_then(|id| originals.remove(id));
                if let Some(original) = original {
                    *raw = original;
                }
            }
        }
    }
    Ok((session, Plan { on_branch, edited }))
}

/// What the model is shown after Pi's `context_edit` entries (`buildSessionProjection`, Pi
/// 1.2.0): for each target on the resumed path, the latest edit applies. A message keeps its role
/// and takes the replacement's content (a string becomes one text block for an assistant message
/// or a tool result); a null replacement leaves it out. A record that an edit changes is replaced
/// in place by its edited view; the record as written is returned, by id. A session with no edits
/// is not copied at all.
fn apply_context_edits(
    rows: &mut [Value],
    on_branch: &[bool],
) -> std::collections::HashMap<String, Value> {
    let mut edits: std::collections::HashMap<String, Value> = std::collections::HashMap::new();
    for (row, on) in rows.iter().zip(on_branch) {
        if *on
            && row.get("type").and_then(Value::as_str) == Some("context_edit")
            && let (Some(target), Some(replacement)) = (
                row.get("targetId").and_then(Value::as_str),
                row.get("replacement"),
            )
        {
            edits.insert(target.to_string(), replacement.clone());
        }
    }
    let mut originals = std::collections::HashMap::new();
    if edits.is_empty() {
        return originals;
    }
    for (row, on) in rows.iter_mut().zip(on_branch) {
        let Some(id) = row.get("id").and_then(Value::as_str).filter(|_| *on) else {
            continue;
        };
        let Some(replacement) = edits.get(id) else {
            continue;
        };
        let Some(view) = edited_view(row, replacement) else {
            continue;
        };
        let id = id.to_string();
        originals.insert(id, std::mem::replace(row, view));
    }
    originals
}

/// One record as its edit shows it, or `None` when the edit does not apply to this kind of record.
fn edited_view(row: &Value, replacement: &Value) -> Option<Value> {
    let kind = row.get("type").and_then(Value::as_str)?;
    let role = row.pointer("/message/role").and_then(Value::as_str);
    let editable = match kind {
        "message" => matches!(role, Some("user" | "assistant" | "toolResult")),
        "custom_message" => true,
        _ => false,
    };
    if !editable {
        return None;
    }
    // Left out: not a message any more, so the reader keeps it as an internal record.
    if replacement.is_null() {
        let mut view = row.clone();
        view["type"] = Value::String("context_removed".into());
        return Some(view);
    }
    let content = replacement.get("content")?.clone();
    let content = match (&content, role) {
        (Value::String(text), Some("assistant" | "toolResult")) => {
            serde_json::json!([{"type": "text", "text": text}])
        }
        _ => content,
    };
    let mut view = row.clone();
    match kind {
        "message" => view["message"]["content"] = content,
        _ => view["content"] = content,
    }
    Some(view)
}

/// What `row.get("message")` is for a `bashExecution` record without one.
static NO_MESSAGE: Value = Value::Null;

/// Pi's own text for a `bashExecution` message (`bashExecutionToText`, pi-coding-agent 1.2.0).
fn bash_execution_text(message: &Value) -> String {
    let mut text = format!(
        "Ran `{}`\n",
        str_at(message, "/command").unwrap_or_default()
    );
    match str_at(message, "/output").filter(|output| !output.is_empty()) {
        Some(output) => text.push_str(&format!("```\n{output}\n```")),
        None => text.push_str("(no output)"),
    }
    let exit_code = message.get("exitCode").and_then(Value::as_i64);
    if message.get("cancelled") == Some(&Value::Bool(true)) {
        text.push_str("\n\n(command cancelled)");
    } else if let Some(code) = exit_code.filter(|code| *code != 0) {
        text.push_str(&format!("\n\nCommand exited with code {code}"));
    }
    if message.get("truncated") == Some(&Value::Bool(true))
        && let Some(path) = str_at(message, "/fullOutputPath")
    {
        text.push_str(&format!("\n\n[Output truncated. Full output: {path}]"));
    }
    text
}

/// Pi's own text around a branch summary when it shows one to the model
/// (`BRANCH_SUMMARY_PREFIX` / `BRANCH_SUMMARY_SUFFIX` in pi-coding-agent 1.2.0).
const BRANCH_SUMMARY_PREFIX: &str =
    "The following is a summary of a branch that this conversation came back from:\n\n<summary>\n";
const BRANCH_SUMMARY_SUFFIX: &str = "</summary>";

/// Which records belong to the path Pi resumes. Navigating back in Pi's tree starts a new branch
/// from an earlier entry and leaves the abandoned one in the file; Pi builds the context from the
/// last entry back through `parentId` (`buildSessionPath`). The header and records outside the
/// tree (no `id`) are left as they are.
fn active_branch(rows: &[Value]) -> Vec<bool> {
    let in_tree = |row: &Value| {
        row.get("id").and_then(Value::as_str).is_some()
            && !matches!(
                row.get("type").and_then(Value::as_str),
                Some("session" | "session_start")
            )
    };
    let Some(leaf) = rows.iter().rposition(&in_tree) else {
        return vec![true; rows.len()];
    };
    let by_id: std::collections::HashMap<&str, usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| in_tree(row))
        .filter_map(|(i, row)| row.get("id").and_then(Value::as_str).map(|id| (id, i)))
        .collect();
    let mut active = vec![false; rows.len()];
    let mut at = Some(leaf);
    while let Some(i) = at {
        if active[i] {
            break;
        }
        active[i] = true;
        at = rows[i]
            .get("parentId")
            .and_then(Value::as_str)
            .and_then(|id| by_id.get(id).copied());
    }
    rows.iter()
        .zip(active)
        .map(|(row, on)| on || !in_tree(row))
        .collect()
}

// ---------------------------------------------------------------------------------------------

/// What a Pi record is.
enum Class {
    /// A conversation message; the role decides which.
    Message,
    /// `compaction`.
    Compacted,
    /// `branch_summary`: what the branch the conversation came back from did.
    BranchSummary,
    /// `custom_message`: an extension's message, which Pi gives the model as a user message.
    CustomMessage,
    Meta(MetaKind),
    /// The `session` header and anything else with no conversation meaning.
    Header,
}

fn class_of(row: &Value) -> Class {
    match row.get("type").and_then(Value::as_str).unwrap_or("") {
        "message" => Class::Message,
        "branch_summary" => Class::BranchSummary,
        "compaction" => Class::Compacted,
        "model_change" | "thinking_level_change" => Class::Meta(MetaKind::ModelChange),
        "custom_message" => Class::CustomMessage,
        "session" | "session_start" => Class::Header,
        _ => Class::Header,
    }
}

fn role_of(row: &Value) -> &str {
    row.pointer("/message/role")
        .and_then(Value::as_str)
        .unwrap_or("")
}

fn step_end(stop: Option<&str>, has_calls: bool) -> StepEnd {
    match stop {
        Some("toolUse") | Some("tool_use") | Some("tool-calls") => StepEnd::ToolUse,
        Some("aborted") => StepEnd::Aborted,
        Some("error") => StepEnd::Error,
        Some(_) => StepEnd::Completed,
        None if has_calls => StepEnd::ToolUse,
        None => StepEnd::Unknown,
    }
}

#[derive(Default)]
struct Builder {
    entries: Vec<Entry>,
    meta: MetaBuilder,
    /// Entries before this one were handed over by [`Builder::take_final`] and are placeholders.
    taken: usize,
    /// The results entry currently being appended to.
    open_results: Option<usize>,
    model: Option<ModelRef>,
    protocol: Option<Protocol>,
}

#[derive(Default)]
struct MetaBuilder {
    id: Option<String>,
    cwd: Option<String>,
    title: TitleBuilder,
    version: Option<String>,
    created: Option<String>,
    updated: Option<String>,
    parent: Option<String>,
    models: Vec<ModelRef>,
}

impl MetaBuilder {
    fn observe(&mut self, row: &Value) {
        self.title.observe(row);
        if row.get("type").and_then(Value::as_str) == Some("session") {
            if self.id.is_none() {
                self.id = str_at(row, "/id").filter(|s| !s.is_empty());
            }
            if self.cwd.is_none() {
                self.cwd = str_at(row, "/cwd");
            }
            if self.parent.is_none() {
                self.parent = str_at(row, "/parentSession");
            }
            if self.created.is_none() {
                self.created = str_at(row, "/timestamp");
            }
        }
        if let Some(ts) = str_at(row, "/timestamp") {
            if self.updated.as_deref().is_none_or(|u| ts.as_str() > u) {
                self.updated = Some(ts.clone());
            }
            if self.created.as_deref().is_none_or(|c| ts.as_str() < c) {
                self.created = Some(ts);
            }
        }
        if let Some(model) = str_at(row, "/message/model")
            && !self.models.iter().any(|m| m.id == model)
        {
            self.models.push(ModelRef {
                provider: None,
                id: model,
            });
        }
    }
}

impl Builder {
    fn push(&mut self, index: usize, row: Value, on_branch: bool) {
        self.meta.observe(&row);
        if let Some(model_id) = str_at(&row, "/modelId") {
            // `model_change`: the provider and model the session was using.
            let model = ModelRef {
                provider: str_at(&row, "/provider"),
                id: model_id,
            };
            if !model.id.is_empty() && !self.meta.models.iter().any(|m| m.id == model.id) {
                self.meta.models.push(model.clone());
            }
            self.model = Some(model);
        }
        if let Some(api) = row.pointer("/message/api").and_then(Value::as_str) {
            self.protocol = Some(Protocol::from_provider(api));
        }
        let class = class_of(&row);
        if !on_branch && !matches!(class, Class::Header | Class::Meta(_)) {
            // A record of a branch the conversation left: kept, not resumed.
            self.open_results = None;
            self.entries.push(Entry {
                id: id_of(&row, index),
                kind: Kind::Meta {
                    kind: MetaKind::OffBranch,
                },
                raw: vec![row],
            });
            return;
        }
        match class {
            Class::CustomMessage => {
                self.open_results = None;
                self.entries.push(Entry {
                    id: id_of(&row, index),
                    kind: Kind::User {
                        text: row
                            .get("content")
                            .and_then(super::json::text_of_content)
                            .unwrap_or_default(),
                        attachments: row
                            .get("content")
                            .and_then(Value::as_array)
                            .map(|blocks| super::json::attachments(blocks))
                            .unwrap_or_default(),
                    },
                    raw: vec![row],
                });
            }
            // Pi shows the model a branch summary as a user message, in its own words.
            Class::BranchSummary => {
                self.open_results = None;
                let summary = str_at(&row, "/summary").unwrap_or_default();
                self.entries.push(Entry {
                    id: id_of(&row, index),
                    kind: if summary.is_empty() {
                        Kind::Meta {
                            kind: MetaKind::Internal,
                        }
                    } else {
                        Kind::User {
                            text: format!(
                                "{BRANCH_SUMMARY_PREFIX}{summary}{BRANCH_SUMMARY_SUFFIX}"
                            ),
                            attachments: Vec::new(),
                        }
                    },
                    raw: vec![row],
                });
            }
            Class::Header => self.entries.push(Entry {
                id: id_of(&row, index),
                kind: Kind::Meta {
                    kind: MetaKind::Internal,
                },
                raw: vec![row],
            }),
            Class::Meta(kind) => self.entries.push(Entry {
                id: id_of(&row, index),
                kind: Kind::Meta { kind },
                raw: vec![row],
            }),
            Class::Message => match role_of(&row) {
                "user" => {
                    self.open_results = None;
                    self.entries.push(Entry {
                        id: id_of(&row, index),
                        kind: Kind::User {
                            text: row
                                .pointer("/message/content")
                                .and_then(super::json::text_of_content)
                                .unwrap_or_default(),
                            attachments: row
                                .pointer("/message/content")
                                .and_then(Value::as_array)
                                .map(|blocks| super::json::attachments(blocks))
                                .unwrap_or_default(),
                        },
                        raw: vec![row],
                    });
                }
                "assistant" => {
                    self.open_results = None;
                    self.assistant(row, index);
                }
                "toolResult" => self.tool_result(row, index),
                // `!cmd`: Pi gives the model the command and its output as a user message, unless
                // `!!` kept it out of the context.
                "bashExecution" => {
                    self.open_results = None;
                    let message = row.get("message").unwrap_or(&NO_MESSAGE);
                    let excluded = message.get("excludeFromContext") == Some(&Value::Bool(true));
                    self.entries.push(Entry {
                        id: id_of(&row, index),
                        kind: if excluded {
                            Kind::Meta {
                                kind: MetaKind::Internal,
                            }
                        } else {
                            Kind::User {
                                text: bash_execution_text(message),
                                attachments: Vec::new(),
                            }
                        },
                        raw: vec![row],
                    });
                }
                "system" => self.entries.push(Entry {
                    id: id_of(&row, index),
                    kind: Kind::Meta {
                        kind: MetaKind::SystemPrompt,
                    },
                    raw: vec![row],
                }),
                _ => self.entries.push(Entry {
                    id: id_of(&row, index),
                    kind: Kind::Meta {
                        kind: MetaKind::Internal,
                    },
                    raw: vec![row],
                }),
            },
            Class::Compacted => {
                self.open_results = None;
                self.entries.push(Entry {
                    id: id_of(&row, index),
                    kind: Kind::Compaction {
                        summary: str_at(&row, "/summary").unwrap_or_default(),
                        kept_from: str_at(&row, "/firstKeptEntryId"),
                        trigger: Trigger::Unknown,
                        tokens_before: row.get("tokensBefore").and_then(Value::as_u64),
                    },
                    raw: vec![row],
                });
            }
        }
    }

    /// One assistant message: thinking, text and **all** of its tool calls together (L3's other
    /// half: what Pi writes back on its own resume).
    fn assistant(&mut self, row: Value, index: usize) {
        let model_id = self.model.as_ref().map(|m| m.id.clone());
        let mut reasoning = Vec::new();
        let mut text = Vec::new();
        let mut calls = Vec::new();
        for block in row
            .pointer("/message/content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            match block.get("type").and_then(Value::as_str).unwrap_or("") {
                "thinking" => {
                    // Anthropic's `redacted_thinking`, as Pi keeps it: a placeholder text, the
                    // encrypted data in `thinkingSignature`, `redacted: true`.
                    let redacted = block.get("redacted").and_then(Value::as_bool) == Some(true);
                    let payload = str_at(block, "/thinkingSignature");
                    reasoning.push(Reasoning {
                        text: if redacted {
                            String::new()
                        } else {
                            str_at(block, "/thinking").unwrap_or_default()
                        },
                        opaque: Some(Opaque {
                            chat: None,
                            protocol: self
                                .protocol
                                .clone()
                                .unwrap_or(Protocol::Other("unknown".into())),
                            model: model_id.clone(),
                            signature: if redacted { None } else { payload.clone() },
                            encrypted: if redacted { payload } else { None },
                        }),
                    })
                }
                "text" => {
                    let t = str_at(block, "/text").unwrap_or_default();
                    if !t.is_empty() {
                        text.push(t);
                    }
                }
                "toolCall" => {
                    let name = str_at(block, "/name").unwrap_or_default();
                    let mut input = block.get("arguments").cloned().unwrap_or(Value::Null);
                    // L12: Pi counts the timeout in seconds; the canonical unit is milliseconds,
                    // so the input is normalised here and converted back by the Pi writer. The
                    // record itself stays verbatim in `raw`.
                    if name == "bash"
                        && let Some(seconds) = input.get("timeout").and_then(Value::as_f64)
                        && seconds >= 0.0
                        && let Some(map) = input.as_object_mut()
                    {
                        map.insert(
                            "timeout".into(),
                            Value::from((seconds * 1000.0).round() as u64),
                        );
                    }
                    calls.push(ToolCall {
                        id: str_at(block, "/id").unwrap_or_default(),
                        canonical: Some(CanonicalTool::from_pi(&name)),
                        source_name: name,
                        input,
                    });
                }
                _ => {}
            }
        }
        let end = step_end(
            str_at(&row, "/message/stopReason").as_deref(),
            !calls.is_empty(),
        );
        self.entries.push(Entry {
            id: id_of(&row, index),
            kind: Kind::Step {
                model: self.model.clone(),
                reasoning,
                text,
                calls,
                end,
            },
            raw: vec![row],
        });
    }

    fn tool_result(&mut self, row: Value, index: usize) {
        let result = ToolResult {
            call_id: str_at(&row, "/message/toolCallId").unwrap_or_default(),
            content: result_content(
                row.pointer("/message/content")
                    .and_then(Value::as_array)
                    .map_or(&[][..], Vec::as_slice),
            ),
            is_error: row
                .pointer("/message/isError")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            truncated_by_source: false,
        };
        match self.open_results {
            Some(i) => {
                let entry = &mut self.entries[i];
                entry.raw.push(row);
                if let Kind::Results { results } = &mut entry.kind {
                    results.push(result);
                }
            }
            None => {
                self.entries.push(Entry {
                    id: id_of(&row, index),
                    kind: Kind::Results {
                        results: vec![result],
                    },
                    raw: vec![row],
                });
                self.open_results = Some(self.entries.len() - 1);
            }
        }
    }

    /// The entries no later record can change, taken out of the builder in order: only the results
    /// entry being collected (`open_results`) is ever changed after it is pushed. Each taken entry
    /// leaves a placeholder, so the index `open_results` holds stays valid. `all` takes the rest.
    fn take_final(&mut self, all: bool) -> Vec<Entry> {
        let len = self.entries.len();
        let held = match (all, self.open_results) {
            (false, Some(open)) => open.min(len),
            _ => len,
        }
        .max(self.taken);
        let out = self.entries[self.taken..held]
            .iter_mut()
            .map(|entry| {
                std::mem::replace(
                    entry,
                    Entry {
                        id: String::new(),
                        kind: Kind::Meta {
                            kind: MetaKind::Internal,
                        },
                        raw: Vec::new(),
                    },
                )
            })
            .collect();
        self.taken = held;
        out
    }

    fn finish(self, source_path: Option<&Path>) -> Result<Session> {
        let m = self.meta;
        Ok(Session {
            meta: Meta {
                harness: HarnessId::Pi,
                harness_version: m.version.unwrap_or_else(|| "unknown".into()),
                id: m.id.unwrap_or_else(|| "unknown".into()),
                cwd: m.cwd,
                title: m.title.pick(),
                created: m.created,
                updated: m.updated,
                models: m.models,
                git_branch: None,
                // Pi's `parentSession` is the session this one was forked from.
                parent_kind: m.parent.as_ref().map(|_| ParentKind::fork()),
                parent: m.parent.map(|path| parent_id(&path)),
                source_path: source_path.map(|p| p.to_string_lossy().into_owned()),
                source_sha256: None,
            },
            entries: self.entries,
            branches: Vec::new(),
        })
    }
}

/// The session a header's `parentSession` names. Pi writes the origin's file path there; the
/// session is the `id` of that file's own header, so a parent is an id in every harness. A path
/// that is no file here (the session was moved, or made on another machine), or whose first line
/// is no session header, stays as written.
fn parent_id(parent: &str) -> String {
    use std::io::BufRead;
    let resolved = std::fs::File::open(parent).ok().and_then(|file| {
        let mut line = String::new();
        std::io::BufReader::new(file).read_line(&mut line).ok()?;
        let header: Value =
            serde_json::from_str(line.trim_start_matches('\u{feff}').trim()).ok()?;
        if header.get("type").and_then(Value::as_str) != Some("session") {
            return None;
        }
        str_at(&header, "/id").filter(|id| !id.is_empty())
    });
    resolved.unwrap_or_else(|| parent.to_string())
}

/// Pi stamps every record with its own id.
fn id_of(row: &Value, index: usize) -> String {
    str_at(row, "/id")
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| super::json::fallback_id("entry", index))
}

// ---------------------------------------------------------------------------------------------

impl Reader for Pi {
    fn harness(&self) -> HarnessId {
        HarnessId::Pi
    }

    fn list(&self, roots: &StoreRoots) -> Result<Vec<SessionInfo>> {
        let Some(sessions) = roots.pi_sessions() else {
            return Err(super::Lookup::NoStore(
                "PI_CODING_AGENT_DIR is not set: no Pi store to list".into(),
            )
            .into());
        };
        let mut out = Vec::new();
        for file in session_files(&sessions)? {
            let mut info = peek(&file);
            info.id = pi_id(&file);
            info.path = file.to_string_lossy().into_owned();
            out.push(info);
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }

    fn read_all_branches(&self, id: &str, roots: &StoreRoots) -> Option<Result<Session>> {
        Some(read_in(id, roots, true))
    }

    fn read_file_all_branches(&self, path: &Path) -> Option<Result<Session>> {
        Some(read_file_all_branches(path))
    }

    fn reads_all_branches(&self) -> bool {
        true
    }

    fn read(&self, id: &str, roots: &StoreRoots) -> Result<Session> {
        read_in(id, roots, false)
    }

    fn overview(&self, id: &str, roots: &StoreRoots) -> Result<SessionInfo> {
        let Some(sessions) = roots.pi_sessions() else {
            return Err(super::Lookup::NoStore(
                "PI_CODING_AGENT_DIR is not set: no Pi store to list".into(),
            )
            .into());
        };
        let listed = session_files(&sessions)?
            .into_iter()
            .filter(|file| pi_id(file) == id)
            .map(|file| {
                let mut info = peek(&file);
                info.id = pi_id(&file);
                info.path = file.to_string_lossy().into_owned();
                info
            })
            .collect();
        super::pick_overview(listed, id, HarnessId::Pi)
    }

    fn read_clipped(
        &self,
        id: &str,
        roots: &StoreRoots,
        max_string_bytes: usize,
    ) -> Result<Clipped> {
        read_file_clipped(&locate(id, roots)?, &Clip::new(max_string_bytes))
    }

    fn read_streamed_with_meta(
        &self,
        id: &str,
        roots: &StoreRoots,
        on_meta: &mut dyn FnMut(&Meta) -> Result<()>,
        on_entry: &mut dyn FnMut(Entry) -> Result<()>,
    ) -> Result<Meta> {
        read_file_streamed_with_meta(&locate(id, roots)?, on_meta, on_entry)
    }
}

/// [`Reader::read`], with the branches asked for or not.
fn read_in(id: &str, roots: &StoreRoots, branches: bool) -> Result<Session> {
    read_file_with(&locate(id, roots)?, branches)
}

/// The session file `id` names: the whole file stem, the id after its timestamp, or the start of
/// that id.
fn locate(id: &str, roots: &StoreRoots) -> Result<PathBuf> {
    {
        let Some(sessions) = roots.pi_sessions() else {
            return Err(super::Lookup::NoStore(
                "PI_CODING_AGENT_DIR is not set: no Pi store to read".into(),
            )
            .into());
        };
        let matches: Vec<PathBuf> = session_files(&sessions)?
            .into_iter()
            .filter(|f| {
                let named = f.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                named == id || named.ends_with(&format!("_{id}")) || pi_id(f).starts_with(id)
            })
            .collect();
        match matches.as_slice() {
            [one] => Ok(one.clone()),
            [] => Err(super::Lookup::NotFound(format!(
                "no Pi session with id {id:?} under {}",
                sessions.display()
            ))
            .into()),
            many => Err(super::Lookup::Ambiguous(format!(
                "{id:?} matches {} Pi sessions",
                many.len()
            ))
            .into()),
        }
    }
}

/// What a session file's two ends say about it (`super::json::peek_ends`): the header's directory
/// and parent, and the name Pi shows the session under. Missing or damaged metadata never hides the
/// session, and the transcript is not decoded — a record longer than the shared limit is skipped.
fn peek(file: &Path) -> SessionInfo {
    let mut info = SessionInfo::default();
    let mut titles = TitleBuilder::default();
    let mut latest: Option<String> = None;
    {
        let mut into = |row: Value| {
            if row.get("type").and_then(Value::as_str) == Some("session") {
                if info.cwd.is_none() {
                    info.cwd = str_at(&row, "/cwd");
                }
                if info.parent.is_none() {
                    info.parent = str_at(&row, "/parentSession")
                        .filter(|p| !p.is_empty())
                        .map(|path| parent_id(&path));
                    info.parent_kind = info.parent.as_ref().map(|_| ParentKind::fork());
                }
            }
            titles.observe(&row);
            // Every Pi record carries its own ISO time; the newest one seen is the last activity.
            if let Some(time) = str_at(&row, "/timestamp") {
                if latest.as_deref().is_none_or(|known| time.as_str() > known) {
                    latest = Some(time);
                }
            }
        };
        super::json::peek_ends(file, &mut into);
    }
    info.title = titles.pick();
    info.updated = latest;
    info
}

/// The name Pi shows a session under: the display name of its last `session_info` row, which
/// `--name` and a rename write (docs/harnesses/pi.md, "the display name"), else its first user
/// message. A session with neither — a header and bookkeeping only — has no name to show.
#[derive(Default)]
struct TitleBuilder {
    name: Option<String>,
    first_prompt: Option<String>,
}

impl TitleBuilder {
    /// What one record says about the session's name. A later `session_info` supersedes an earlier
    /// one, as a rename appends its own.
    fn observe(&mut self, row: &Value) {
        use super::json::title_line;
        match row.get("type").and_then(Value::as_str).unwrap_or("") {
            "session_info" => {
                if let Some(name) = title_line(str_at(row, "/name").as_deref()) {
                    self.name = Some(name);
                }
            }
            "message" if self.first_prompt.is_none() && role_of(row) == "user" => {
                self.first_prompt = title_line(
                    row.pointer("/message/content")
                        .and_then(super::json::text_of_content)
                        .as_deref(),
                );
            }
            _ => {}
        }
    }

    /// The name to list the session under.
    fn pick(self) -> Option<String> {
        self.name.or(self.first_prompt)
    }
}

/// Pi names a session file `<timestamp>_<uuid>.jsonl`.
fn pi_id(file: &Path) -> String {
    let stem = file.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    stem.rsplit_once('_')
        .map(|(_, id)| id.to_string())
        .unwrap_or_else(|| stem.to_string())
}

/// One read-only pass over Pi's store: how many session files it holds and how many of them are
/// forks. Both numbers come from the same directory walk; only each file's first line is read.
pub struct StoreSummary {
    pub sessions: usize,
    /// The fork count, or the first header that could not be read or understood. A bad header
    /// never hides the sessions that were counted.
    pub forks: Result<usize, anyhow::Error>,
}

/// Count sessions and forks in one traversal of `<agent-dir>/sessions`.
pub fn store_summary(agent_dir: &Path) -> Result<StoreSummary> {
    let mut sessions = 0;
    let mut forks = 0;
    let mut header_error = None;
    for file in session_files(&agent_dir.join("sessions"))? {
        sessions += 1;
        if header_error.is_some() {
            continue;
        }
        match names_a_parent(&file) {
            Ok(true) => forks += 1,
            Ok(false) => {}
            Err(error) => header_error = Some(error),
        }
    }
    Ok(StoreSummary {
        sessions,
        forks: match header_error {
            Some(error) => Err(error),
            None => Ok(forks),
        },
    })
}

/// How many of Pi's sessions are forks: a forked session's header names its `parentSession` (the
/// fork commands, and mukade's own summary fork, both write it).
pub fn forks_in(agent_dir: &Path) -> Result<usize> {
    store_summary(agent_dir)?.forks
}

/// Read one session's first line and report whether it names a parent session.
fn names_a_parent(file: &Path) -> Result<bool> {
    use std::io::BufRead;
    let handle = std::fs::File::open(file)
        .with_context(|| format!("open Pi session header {}", file.display()))?;
    let mut header = String::new();
    std::io::BufReader::new(handle)
        .read_line(&mut header)
        .with_context(|| format!("read Pi session header {}", file.display()))?;
    let row: Value = serde_json::from_str(header.trim())
        .with_context(|| format!("parse Pi session header {}", file.display()))?;
    anyhow::ensure!(
        row.get("type").and_then(Value::as_str) == Some("session"),
        "missing Pi session header in {}",
        file.display()
    );
    Ok(row
        .get("parentSession")
        .and_then(Value::as_str)
        .is_some_and(|parent| !parent.is_empty()))
}

/// Every session file under Pi's `sessions/` directory, whatever project folder it sits in.
fn session_files(sessions: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut dirs = vec![sessions.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(d) => d,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e).with_context(|| format!("read {}", dir.display())),
        };
        for entry in entries {
            let entry = entry?;
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                dirs.push(entry.path());
            } else if entry.path().extension().and_then(|e| e.to_str()) == Some("jsonl") {
                out.push(entry.path());
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A streaming build hands its entries over as they settle: only a results entry still
    /// collecting its tool results is held back.
    #[test]
    fn a_streaming_build_does_not_hold_the_session() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "fixtures/pi/source/2026-10-27T16-00-39-689Z_01a17e17-9f07-743d-8072-8b3b13d65ab5.jsonl",
        );
        let text = std::fs::read_to_string(path).expect("the fixture");
        let rows: Vec<Value> = text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).expect("a row"))
            .collect();
        let (session, plan) = parse_planned(rows.clone(), None).expect("the session");
        let mut b = Builder::default();
        let (mut delivered, mut peak) = (0usize, 0usize);
        for (i, row) in rows.into_iter().enumerate() {
            b.push(i, row, plan.on_branch[i]);
            delivered += b.take_final(false).len();
            peak = peak.max(b.entries.len() - b.taken);
        }
        delivered += b.take_final(true).len();
        assert_eq!(delivered, session.entries.len());
        assert!(
            peak * 4 < delivered,
            "held {peak} of {delivered} entries at once"
        );
    }
}
