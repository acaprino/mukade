//! The Claude Code 2.1.311 reader.
//!
//! Facts this reader depends on (docs/session-interchange-spec.md §2.1, all seen in the
//! bench):
//!
//! - one API response is **one row per content block**, all sharing `message.id` and chained by
//!   `parentUuid`; the reader merges them back into one [`Kind::Step`];
//! - thinking blocks carry a **signature**;
//! - each `tool_result` row's parent is its **own** `tool_use` row and results are written in
//!   completion order, so results are paired by `tool_use_id` and never by walking parents (L5);
//! - the `/compact` block is `system/compact_boundary` followed by a user row with
//!   `isCompactSummary`, and `compactMetadata.preservedSegment` says what was kept (L4);
//! - anything transcript-only or harness-internal (`isMeta`, `<local-command-*>`,
//!   `<command-name>`, attachments, request/cost/telemetry rows) is [`MetaKind`], not
//!   conversation (L5);
//! - a process killed mid-turn leaves the user row and nothing else (L9).

use crate::model::{
    CanonicalTool, Entry, HarnessId, Kind, Meta, MetaKind, ModelRef, Opaque, Protocol, Reasoning,
    ResultContent, Session, StepEnd, ToolCall, ToolResult, Trigger,
};
use crate::reader::{Reader, SessionInfo};
use crate::roots::StoreRoots;
use anyhow::{Context, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// The Claude Code reader.
#[derive(Debug, Clone, Copy, Default)]
pub struct ClaudeCode;

impl ClaudeCode {
    /// Read one session file directly. Every error names the file it came from.
    pub fn read_path(path: &Path) -> Result<Session> {
        read_file(path)
    }
}

/// Read one Claude Code session file (`<CLAUDE_CONFIG_DIR>/projects/<project>/<uuid>.jsonl`).
pub fn read_file(path: &Path) -> Result<Session> {
    let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let (rows, unfinished) = super::json::rows_from_lines(&text, &path.display().to_string())?;
    let mut session = parse(&rows, Some(path))?;
    super::json::keep_unfinished(&mut session, unfinished);
    Ok(session)
}

/// Read records that were already decoded (the CLI and the tests both come through here).
pub fn parse(rows: &[Value], source_path: Option<&Path>) -> Result<Session> {
    let on_branch = active_branch(rows);
    let mut b = Builder::default();
    for (i, row) in rows.iter().enumerate() {
        b.push(i, row, on_branch[i]);
    }
    b.finish(source_path)
}

/// Which rows belong to the branch Claude Code resumes. A rewind or an edited prompt starts a new
/// branch from an earlier row and leaves the abandoned one in the file; Claude Code resumes the
/// branch of the last row, walking `parentUuid` back from it (and `logicalParentUuid` across a
/// compaction boundary, whose `parentUuid` is null). Walking parents alone loses a parallel call:
/// its `tool_use` row shares the `message.id` of a row on the chain, and its `tool_result` row hangs
/// off that `tool_use` row, so both are taken by those ids. Rows outside the tree (no `uuid`) and
/// subagent rows are left as they are.
fn active_branch(rows: &[Value]) -> Vec<bool> {
    let text = |row: &Value, key: &str| row.get(key).and_then(Value::as_str).map(str::to_owned);
    let in_tree = |row: &Value| {
        row.get("uuid").and_then(Value::as_str).is_some() && !truthy(row.get("isSidechain"))
    };
    let Some(leaf) = rows.iter().rposition(&in_tree) else {
        return vec![true; rows.len()];
    };
    let by_uuid: std::collections::HashMap<String, usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| in_tree(row))
        .filter_map(|(i, row)| text(row, "uuid").map(|uuid| (uuid, i)))
        .collect();

    let mut active = vec![false; rows.len()];
    let mut at = Some(leaf);
    while let Some(i) = at {
        if active[i] {
            break;
        }
        active[i] = true;
        let parent = text(&rows[i], "parentUuid").or_else(|| text(&rows[i], "logicalParentUuid"));
        at = parent.and_then(|uuid| by_uuid.get(&uuid).copied());
    }

    let message_ids: std::collections::HashSet<String> = rows
        .iter()
        .zip(&active)
        .filter(|(_, on)| **on)
        .filter_map(|(row, _)| {
            row.pointer("/message/id")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect();
    for (i, row) in rows.iter().enumerate() {
        if in_tree(row)
            && row.get("type").and_then(Value::as_str) == Some("assistant")
            && row
                .pointer("/message/id")
                .and_then(Value::as_str)
                .is_some_and(|id| message_ids.contains(id))
        {
            active[i] = true;
        }
    }
    let call_ids: std::collections::HashSet<&str> = rows
        .iter()
        .zip(&active)
        .filter(|(_, on)| **on)
        .filter_map(|(row, _)| row.pointer("/message/content").and_then(Value::as_array))
        .flatten()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("tool_use"))
        .filter_map(|block| block.get("id").and_then(Value::as_str))
        .collect();
    for (i, row) in rows.iter().enumerate() {
        let answers_a_call = row
            .pointer("/message/content")
            .and_then(Value::as_array)
            .is_some_and(|blocks| {
                blocks.iter().any(|block| {
                    block
                        .get("tool_use_id")
                        .and_then(Value::as_str)
                        .is_some_and(|id| call_ids.contains(id))
                })
            });
        if in_tree(row) && answers_a_call {
            active[i] = true;
        }
    }

    rows.iter()
        .zip(active)
        .map(|(row, on)| on || !in_tree(row))
        .collect()
}

// ---------------------------------------------------------------------------------------------

/// What a row is, for grouping purposes.
enum Class {
    /// An assistant row; rows sharing a `message.id` are one step.
    Assistant(String),
    /// A user row carrying tool results.
    Results,
    /// `system/compact_boundary`: the start of the compaction block.
    Boundary,
    /// The `isCompactSummary` user row: the compaction block's summary.
    Summary,
    /// A user turn.
    User,
    /// Anything that is kept but is not conversation.
    Meta(MetaKind),
}

fn class_of(row: &Value) -> Class {
    match row.get("type").and_then(Value::as_str).unwrap_or("") {
        // L9: Claude Code's own stand-in reply ("No response requested.", an API error), written
        // when it reloads a killed session or a request fails. The model never said it.
        "assistant"
            if row.pointer("/message/model").and_then(Value::as_str) == Some("<synthetic>") =>
        {
            Class::Meta(MetaKind::Synthetic)
        }
        "assistant" => {
            let mid = row
                .pointer("/message/id")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(|| format!("assistant-{}", row_id(row, 0)));
            Class::Assistant(mid)
        }
        "system" if row.get("subtype").and_then(Value::as_str) == Some("compact_boundary") => {
            Class::Boundary
        }
        "system" => Class::Meta(MetaKind::SystemPrompt),
        "attachment" => Class::Meta(MetaKind::Attachment),
        "user" => {
            if truthy(row.get("isCompactSummary")) {
                Class::Summary
            } else if truthy(row.get("isMeta")) || truthy(row.get("isVisibleInTranscriptOnly")) {
                Class::Meta(MetaKind::Command)
            } else if has_tool_result(row) {
                // Results first: a row may carry the user's text beside them (see `push`).
                Class::Results
            } else if content_text(row).is_some_and(|text| is_command_row(&text)) {
                Class::Meta(MetaKind::Command)
            } else {
                Class::User
            }
        }
        _ => Class::Meta(MetaKind::Internal),
    }
}

/// A command row is one Claude Code wrote for a slash command: it starts with the tag. A prompt
/// that only mentions one is the user's own words.
fn is_command_row(text: &str) -> bool {
    let text = text.trim_start();
    text.starts_with("<local-command-")
        || text.starts_with("<command-name>")
        || text.starts_with("<command-message>")
}

/// A prompt-type slash command (`/review src/lib.rs`): its `<command-name>` row, which an
/// `isMeta` row with the expanded prompt follows. `/compact` and the other local commands answer
/// with `<local-command-stdout>` instead.
fn is_prompt_command(text: &str) -> bool {
    let text = text.trim_start();
    text.starts_with("<command-name>") || text.starts_with("<command-message>")
}

fn truthy(v: Option<&Value>) -> bool {
    matches!(v, Some(Value::Bool(true)))
}

/// The user text of a row, when it is a plain string or made of text blocks.
fn content_text(row: &Value) -> Option<String> {
    match row.pointer("/message/content") {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Array(blocks)) => {
            let parts: Vec<&str> = blocks
                .iter()
                .filter_map(|b| b.get("text").and_then(Value::as_str))
                .collect();
            if parts.is_empty() {
                None
            } else {
                Some(parts.join("\n"))
            }
        }
        _ => None,
    }
}

fn has_tool_result(row: &Value) -> bool {
    row.pointer("/message/content")
        .and_then(Value::as_array)
        .is_some_and(|blocks| {
            blocks
                .iter()
                .any(|b| b.get("type").and_then(Value::as_str) == Some("tool_result"))
        })
}

/// The row's own id: Claude stamps a `uuid` on every conversation row.
fn row_id(row: &Value, index: usize) -> String {
    row.get("uuid")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("row-{index}"))
}

fn tool_result_of(block: &Value) -> ToolResult {
    let content = match block.get("content") {
        Some(Value::String(s)) => ResultContent::Text(s.clone()),
        Some(Value::Array(items)) => ResultContent::Blocks(items.clone()),
        Some(other) => ResultContent::Text(other.to_string()),
        None => ResultContent::Text(String::new()),
    };
    ToolResult {
        call_id: block
            .get("tool_use_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        content,
        is_error: truthy(block.get("is_error")),
        truncated_by_source: false,
    }
}

fn step_end(stop_reason: Option<&str>) -> StepEnd {
    match stop_reason {
        Some("tool_use") => StepEnd::ToolUse,
        Some("end_turn") | Some("stop_sequence") | Some("max_tokens") | Some("length") => {
            StepEnd::Completed
        }
        Some("refusal") => StepEnd::Error,
        _ => StepEnd::Unknown,
    }
}

/// A compaction block being assembled: the boundary row, then the summary row.
struct OpenCompaction {
    boundary: Value,
    trigger: Trigger,
    tokens_before: Option<u64>,
    kept_from: Option<String>,
}

#[derive(Default)]
struct Builder {
    entries: Vec<Entry>,
    meta: MetaBuilder,
    /// The step currently being appended to, if the last entry is one.
    open_step: Option<OpenStep>,
    /// The results entry currently being appended to, if the last entry is one.
    open_results: Option<usize>,
    /// A boundary row waiting for its summary row.
    open_compaction: Option<OpenCompaction>,
    /// A compaction written without its summary because another row came between the boundary
    /// and the summary row; the summary row, when it comes, fills it in.
    unsummarized: Option<usize>,
    /// The last command row was a prompt-type slash command: the `isMeta` row after it is the
    /// expanded prompt the model received.
    after_prompt_command: bool,
}

struct OpenStep {
    index: usize,
    message_id: String,
}

#[derive(Default)]
struct MetaBuilder {
    id: Option<String>,
    cwd: Option<String>,
    title: Option<String>,
    git_branch: Option<String>,
    version: Option<String>,
    created: Option<String>,
    updated: Option<String>,
    models: Vec<ModelRef>,
}

impl MetaBuilder {
    fn observe(&mut self, row: &Value) {
        let str_at = |p: &str| row.pointer(p).and_then(Value::as_str).map(str::to_owned);
        if self.id.is_none() {
            self.id = str_at("/sessionId");
        }
        if self.cwd.is_none() {
            self.cwd = str_at("/cwd");
        }
        if self.git_branch.is_none() {
            self.git_branch = str_at("/gitBranch");
        }
        if self.version.is_none() {
            self.version = str_at("/version");
        }
        if self.title.is_none() {
            self.title = str_at("/slug");
        }
        if let Some(ts) = str_at("/timestamp") {
            if self.created.as_deref().is_none_or(|c| ts.as_str() < c) {
                self.created = Some(ts.clone());
            }
            if self.updated.as_deref().is_none_or(|u| ts.as_str() > u) {
                self.updated = Some(ts);
            }
        }
        if let Some(model) = str_at("/message/model")
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
    fn push(&mut self, index: usize, row: &Value, on_branch: bool) {
        self.meta.observe(row);
        if truthy(row.get("isSidechain")) {
            // A subagent's transcript: kept, never conversation (§10 "not exercised").
            self.flush();
            self.entries.push(Entry {
                id: row_id(row, index),
                kind: Kind::Meta {
                    kind: MetaKind::Internal,
                },
                raw: vec![row.clone()],
            });
            return;
        }
        let mut class = class_of(row);
        let text = content_text(row);
        if matches!(class, Class::Meta(MetaKind::Command))
            && self.after_prompt_command
            && truthy(row.get("isMeta"))
            && text.as_deref().is_some_and(|text| !is_command_row(text))
        {
            class = Class::User;
        }
        match &class {
            Class::Meta(MetaKind::Command) => {
                self.after_prompt_command = text.as_deref().is_some_and(is_prompt_command);
            }
            Class::Meta(_) => {}
            _ => self.after_prompt_command = false,
        }
        if !on_branch && !matches!(class, Class::Meta(_)) {
            // A row of a branch the conversation left: kept, not resumed.
            self.flush();
            self.entries.push(Entry {
                id: row_id(row, index),
                kind: Kind::Meta {
                    kind: MetaKind::OffBranch,
                },
                raw: vec![row.clone()],
            });
            return;
        }
        match class {
            Class::Assistant(message_id) => {
                self.flush_unless_step(&message_id);
                self.unsummarized = None;
                self.step_with(index, row, message_id);
            }
            Class::Results => {
                self.flush_unless_results();
                self.unsummarized = None;
                self.results_with(index, row);
                // The user's own text beside the results is a turn of its own, after them.
                if let Some(text) = text {
                    self.flush();
                    self.entries.push(Entry {
                        id: format!("{}-text", row_id(row, index)),
                        kind: Kind::User {
                            text,
                            attachments: Vec::new(),
                        },
                        raw: Vec::new(),
                    });
                }
            }
            Class::Boundary => {
                self.flush();
                let compact = row.get("compactMetadata").cloned().unwrap_or(Value::Null);
                self.open_compaction = Some(OpenCompaction {
                    boundary: row.clone(),
                    trigger: match compact.get("trigger").and_then(Value::as_str) {
                        Some("manual") => Trigger::Manual,
                        Some("auto") => Trigger::Auto,
                        _ => Trigger::Unknown,
                    },
                    tokens_before: compact.get("preTokens").and_then(Value::as_u64),
                    kept_from: compact
                        .pointer("/preservedSegment/headUuid")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                });
            }
            Class::Summary if self.open_compaction.is_none() && self.unsummarized.is_some() => {
                let at = self.unsummarized.take().expect("checked");
                if let Kind::Compaction { summary, .. } = &mut self.entries[at].kind {
                    *summary = text.unwrap_or_default();
                }
                self.entries.push(Entry {
                    id: row_id(row, index),
                    kind: Kind::Meta {
                        kind: MetaKind::Internal,
                    },
                    raw: vec![row.clone()],
                });
            }
            Class::Summary => {
                if let Some(open) = self.open_compaction.take() {
                    let summary = content_text(row).unwrap_or_default();
                    self.entries.push(Entry {
                        id: row_id(&open.boundary, index),
                        kind: Kind::Compaction {
                            summary,
                            kept_from: open.kept_from,
                            trigger: open.trigger,
                            tokens_before: open.tokens_before,
                        },
                        raw: vec![open.boundary, row.clone()],
                    });
                } else {
                    // A summary with no boundary before it: keep it, do not guess.
                    self.entries.push(Entry {
                        id: row_id(row, index),
                        kind: Kind::Meta {
                            kind: MetaKind::Internal,
                        },
                        raw: vec![row.clone()],
                    });
                }
            }
            Class::User => {
                self.flush();
                self.unsummarized = None;
                let text = text.unwrap_or_default();
                self.entries.push(Entry {
                    id: row_id(row, index),
                    kind: Kind::User {
                        text,
                        attachments: row
                            .pointer("/message/content")
                            .and_then(Value::as_array)
                            .map(|blocks| super::json::attachments(blocks))
                            .unwrap_or_default(),
                    },
                    raw: vec![row.clone()],
                });
            }
            Class::Meta(kind) => {
                self.flush();
                self.entries.push(Entry {
                    id: row_id(row, index),
                    kind: Kind::Meta { kind },
                    raw: vec![row.clone()],
                });
            }
        }
    }

    /// Flush the pending boundary of a compaction block that has no summary row after it.
    fn flush(&mut self) {
        self.flush_step();
        self.open_results = None;
        if let Some(open) = self.open_compaction.take() {
            self.unsummarized = Some(self.entries.len());
            self.entries.push(Entry {
                id: row_id(&open.boundary, 0),
                kind: Kind::Compaction {
                    summary: String::new(),
                    kept_from: open.kept_from,
                    trigger: open.trigger,
                    tokens_before: open.tokens_before,
                },
                raw: vec![open.boundary],
            });
        }
    }

    fn flush_step(&mut self) {
        self.open_step = None;
    }

    fn flush_unless_step(&mut self, message_id: &str) {
        if self
            .open_step
            .as_ref()
            .is_some_and(|s| s.message_id == message_id)
        {
            self.open_results = None;
            return;
        }
        self.flush();
    }

    fn flush_unless_results(&mut self) {
        self.flush_step();
        if self.open_compaction.is_some() {
            self.flush();
        }
    }

    fn step_with(&mut self, index: usize, row: &Value, message_id: String) {
        if self.open_step.is_none() {
            let id = row_id(row, index);
            self.entries.push(Entry {
                id,
                kind: Kind::Step {
                    model: None,
                    reasoning: Vec::new(),
                    text: Vec::new(),
                    calls: Vec::new(),
                    end: StepEnd::Unknown,
                },
                raw: Vec::new(),
            });
            self.open_step = Some(OpenStep {
                index: self.entries.len() - 1,
                message_id,
            });
        }
        let i = self.open_step.as_ref().expect("just opened").index;
        let entry = &mut self.entries[i];
        entry.raw.push(row.clone());
        let model_id = row
            .pointer("/message/model")
            .and_then(Value::as_str)
            .map(str::to_owned);
        if let Kind::Step {
            model,
            reasoning,
            text,
            calls,
            end,
        } = &mut entry.kind
        {
            if model.is_none() {
                *model = model_id.clone().map(|id| ModelRef { provider: None, id });
            }
            if let Some(blocks) = row.pointer("/message/content").and_then(Value::as_array) {
                for b in blocks {
                    match b.get("type").and_then(Value::as_str) {
                        Some("thinking") => reasoning.push(Reasoning {
                            text: b
                                .get("thinking")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            opaque: Some(Opaque {
                                protocol: Protocol::AnthropicMessages,
                                model: model_id.clone(),
                                signature: b
                                    .get("signature")
                                    .and_then(Value::as_str)
                                    .map(str::to_owned),
                                encrypted: None,
                            }),
                        }),
                        Some("redacted_thinking") => reasoning.push(Reasoning {
                            text: String::new(),
                            opaque: Some(Opaque {
                                protocol: Protocol::AnthropicMessages,
                                model: model_id.clone(),
                                signature: None,
                                encrypted: b.get("data").and_then(Value::as_str).map(str::to_owned),
                            }),
                        }),
                        Some("text") => text.push(
                            b.get("text")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                        ),
                        Some("tool_use") => {
                            let name = b
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string();
                            calls.push(ToolCall {
                                id: b
                                    .get("id")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .to_string(),
                                canonical: Some(CanonicalTool::from_claude(&name)),
                                source_name: name,
                                input: b.get("input").cloned().unwrap_or(Value::Null),
                            });
                        }
                        _ => {}
                    }
                }
            }
            *end = step_end(row.pointer("/message/stop_reason").and_then(Value::as_str));
        }
    }

    fn results_with(&mut self, index: usize, row: &Value) {
        let blocks: Vec<Value> = row
            .pointer("/message/content")
            .and_then(Value::as_array)
            .map(|b| {
                b.iter()
                    .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_result"))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        match self.open_results {
            Some(i) => {
                let entry = &mut self.entries[i];
                entry.raw.push(row.clone());
                if let Kind::Results { results } = &mut entry.kind {
                    results.extend(blocks.iter().map(tool_result_of));
                }
            }
            None => {
                self.entries.push(Entry {
                    id: row_id(row, index),
                    kind: Kind::Results {
                        results: blocks.iter().map(tool_result_of).collect(),
                    },
                    raw: vec![row.clone()],
                });
                self.open_results = Some(self.entries.len() - 1);
            }
        }
    }

    fn finish(mut self, source_path: Option<&Path>) -> Result<Session> {
        self.flush();
        // A compaction names the entry it kept by the source's own id. Resolve it; when the id is
        // not an entry (Claude points at the *logical* leaf), fall back to the first conversation
        // entry that follows the boundary.
        let ids: Vec<String> = self.entries.iter().map(|e| e.id.clone()).collect();
        for i in 0..self.entries.len() {
            let Kind::Compaction { kept_from, .. } = &self.entries[i].kind else {
                continue;
            };
            let kept = kept_from.clone();
            let resolved = match &kept {
                Some(id) if ids.contains(id) => Some(id.clone()),
                _ => self.entries[i + 1..]
                    .iter()
                    .find(|e| !matches!(e.kind, Kind::Meta { .. }))
                    .map(|e| e.id.clone()),
            };
            if let Kind::Compaction { kept_from, .. } = &mut self.entries[i].kind {
                *kept_from = resolved;
            }
        }
        let m = self.meta;
        Ok(Session {
            meta: Meta {
                harness: HarnessId::ClaudeCode,
                harness_version: m.version.unwrap_or_else(|| "unknown".into()),
                id: m.id.unwrap_or_else(|| "unknown".into()),
                cwd: m.cwd,
                title: m.title,
                created: m.created,
                updated: m.updated,
                models: m.models,
                git_branch: m.git_branch,
                parent: None,
                source_path: source_path.map(|p| p.to_string_lossy().into_owned()),
            },
            entries: self.entries,
        })
    }
}

// ---------------------------------------------------------------------------------------------

impl Reader for ClaudeCode {
    fn harness(&self) -> HarnessId {
        HarnessId::ClaudeCode
    }

    fn list(&self, roots: &StoreRoots) -> Result<Vec<SessionInfo>> {
        let Some(projects) = roots.claude_projects() else {
            anyhow::bail!("CLAUDE_CONFIG_DIR is not set: no Claude Code store to list");
        };
        let mut out = Vec::new();
        for file in session_files(&projects)? {
            let mut info = peek(&file);
            if info.id.is_empty() {
                info.id = file
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(str::to_owned)
                    .unwrap_or_else(|| file.to_string_lossy().into_owned());
            }
            info.path = file.to_string_lossy().into_owned();
            out.push(info);
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }

    fn read(&self, id: &str, roots: &StoreRoots) -> Result<Session> {
        let Some(projects) = roots.claude_projects() else {
            anyhow::bail!("CLAUDE_CONFIG_DIR is not set: no Claude Code store to read");
        };
        let matches: Vec<PathBuf> = session_files(&projects)?
            .into_iter()
            .filter(|f| {
                f.file_stem()
                    .and_then(|s| s.to_str())
                    .is_some_and(|name| name.starts_with(id))
            })
            .collect();
        match matches.as_slice() {
            [one] => read_file(one),
            [] => anyhow::bail!(
                "no Claude Code session with id {id:?} under {}",
                projects.display()
            ),
            many => anyhow::bail!("{id:?} matches {} Claude Code sessions", many.len()),
        }
    }
}

/// What a session file's first records say about it, without reading the whole file.
fn peek(file: &Path) -> SessionInfo {
    use std::io::BufRead;
    let mut info = SessionInfo::default();
    let Ok(f) = std::fs::File::open(file) else {
        return info;
    };
    for line in std::io::BufReader::new(f)
        .lines()
        .take(20)
        .map_while(Result::ok)
    {
        let Ok(row) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let str_at = |p: &str| row.pointer(p).and_then(Value::as_str).map(str::to_owned);
        if info.id.is_empty() {
            info.id = str_at("/sessionId").unwrap_or_default();
        }
        if info.cwd.is_none() {
            info.cwd = str_at("/cwd");
        }
        if info.title.is_none() {
            info.title = str_at("/slug");
        }
        if info.updated.is_none() {
            info.updated = str_at("/timestamp");
        }
        if !info.id.is_empty() && info.cwd.is_some() {
            break;
        }
    }
    info
}

/// Every session file under a Claude Code `projects/` directory.
fn session_files(projects: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(projects) {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(e).with_context(|| format!("read {}", projects.display())),
    };
    for project in entries {
        let project = project.with_context(|| format!("read {}", projects.display()))?;
        if !project.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        for file in std::fs::read_dir(project.path())
            .with_context(|| format!("read {}", project.path().display()))?
        {
            let file = file?;
            if file.path().extension().and_then(|e| e.to_str()) == Some("jsonl") {
                out.push(file.path());
            }
        }
    }
    Ok(out)
}
