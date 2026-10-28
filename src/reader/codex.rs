//! The Codex CLI 0.164.0 reader.
//!
//! Facts this reader depends on (docs/session-interchange-spec.md §2.2):
//!
//! - a rollout is JSON Lines of `{timestamp, ordinal, type, payload}`, ordered by `ordinal`;
//! - the conversation lives in `response_item` records only; `event_msg`, `turn_context`,
//!   `world_state` and `token_usage_record` are events about it, not conversation;
//! - reasoning is `{"type":"reasoning","summary":[…],"encrypted_content":…}` — the encrypted
//!   payload is stored, the plain `content` is not;
//! - parallel calls are consecutive `function_call` records, answered by `function_call_output`
//!   records paired by `call_id`;
//! - `compacted` replaces the history: `replacement_history` is the new base, and the assistant
//!   record that *produced* the summary is the compaction's own turn, not conversation;
//! - a graceful interrupt writes a user message carrying `<turn_aborted>` plus an
//!   `event_msg/turn_aborted`; a hard kill leaves the user message and nothing else (L9);
//! - Codex 0.165 may keep one thread over several files (`history_mode: "paginated"`): a later
//!   page, named `rollout-<ts>-<thread>_<page>.jsonl`, starts with a `session_meta` whose
//!   `history_base` says it continues the thread from the first `end_ordinal_exclusive` records
//!   (`end_byte_offset` bytes) of an earlier file, itself a page when the thread was redone more
//!   than once; ordinals run on across the files. Codex's own state names the latest page as the
//!   thread's rollout; each earlier file's records past its cut are a branch the thread left.

use super::json::{blocks_text, fallback_id, str_at};
use super::{Reader, SessionInfo};
use crate::model::{
    CanonicalTool, Entry, HarnessId, Kind, Meta, MetaKind, ModelRef, Opaque, Protocol, Reasoning,
    ResultContent, Session, StepEnd, ToolCall, ToolResult, Trigger,
};
use crate::roots::StoreRoots;
use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The Codex CLI reader.
#[derive(Debug, Clone, Copy, Default)]
pub struct Codex;

/// Which records of a paginated thread a read carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Branches {
    /// The latest version, what Codex resumes: each earlier file up to the point the next one
    /// continues from, then the latest page. What the earlier files hold past those points is
    /// left behind and counted (the default).
    #[default]
    Active,
    /// Everything: every file of the thread whole, the first one first, abandoned turns included.
    All,
}

/// A thread redone more often than this is not followed further.
const MAX_PAGES: usize = 64;

/// Read one Codex rollout file (`<CODEX_HOME>/sessions/YYYY/MM/DD/rollout-<ts>-<uuid>.jsonl`).
/// A page of a paginated thread is read with the part of the earlier files it continues from.
pub fn read_file(path: &Path) -> Result<Session> {
    read_file_with(path, Branches::Active)
}

/// [`read_file`], choosing which branches of a paginated thread to carry.
pub fn read_file_with(path: &Path, branches: Branches) -> Result<Session> {
    let mut session = read_rollout(path, branches)?;
    if session.meta.title.is_none() {
        session.meta.title = indexed_name(path, &session.meta.id);
    }
    Ok(session)
}

/// The thread's name in `<CODEX_HOME>/session_index.jsonl`, the append-only catalogue of
/// `{id, thread_name, updated_at}` the Codex app names its threads from: the newest entry wins
/// (by `updated_at`, then by place in the file). None when the rollout is not under a
/// `sessions/` folder, the catalogue is missing, or it does not name the thread.
fn indexed_name(rollout: &Path, id: &str) -> Option<String> {
    let sessions = rollout
        .ancestors()
        .find(|dir| dir.file_name().and_then(|n| n.to_str()) == Some("sessions"))?;
    let text = std::fs::read_to_string(sessions.parent()?.join("session_index.jsonl")).ok()?;
    text.lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|entry| entry.get("id").and_then(Value::as_str) == Some(id))
        .filter_map(|entry| {
            let name = entry.get("thread_name")?.as_str()?.trim().to_string();
            let updated = entry
                .get("updated_at")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            (!name.is_empty()).then_some((updated, name))
        })
        .enumerate()
        .max_by(|(i, (a, _)), (j, (b, _))| a.cmp(b).then(i.cmp(j)))
        .map(|(_, (_, name))| name)
}

fn read_rollout(path: &Path, branches: Branches) -> Result<Session> {
    let file = super::json::read_jsonl(path)?;
    let Some(base) = history_base(&file.rows) else {
        let _span = crate::timing::span("build entries");
        let mut session = parse_owned(file.rows, Some(path))?;
        session.meta.source_sha256 = Some(file.sha256.wait());
        super::json::keep_unfinished(&mut session, file.unfinished);
        return Ok(session);
    };
    let prefix = base_prefix(path, &base, branches, &mut vec![path.to_path_buf()])?;
    let mut rows = prefix.rows;
    rows.extend(file.rows);
    let _span = crate::timing::span("build entries");
    let mut session = parse_owned(rows, Some(path))?;
    // The fingerprint is that of the bytes read, in the order they were read: the earlier files
    // (their prefixes, or all of them), then the page.
    let mut digest = crate::sha256::Sha256::new();
    digest.update(&prefix.bytes);
    digest.update(&std::fs::read(path).with_context(|| format!("read {}", path.display()))?);
    session.meta.source_sha256 = Some(digest.finalize_hex());
    for (index, row) in prefix.left.into_iter().enumerate() {
        session.entries.push(Entry {
            id: fallback_id("off-branch", index),
            kind: Kind::Meta {
                kind: MetaKind::OffBranch,
            },
            raw: vec![row],
        });
    }
    super::json::keep_unfinished(&mut session, file.unfinished);
    Ok(session)
}

/// Where a page continues its thread from (`session_meta.payload.history_base`).
struct HistoryBase {
    thread: String,
    end_ordinal: u64,
    end_byte: usize,
}

fn history_base(rows: &[Value]) -> Option<HistoryBase> {
    let first = rows.first()?;
    if first.get("type").and_then(Value::as_str) != Some("session_meta") {
        return None;
    }
    let base = first.pointer("/payload/history_base")?;
    Some(HistoryBase {
        thread: base.get("thread_id")?.as_str()?.to_string(),
        end_ordinal: base.get("end_ordinal_exclusive")?.as_u64()?,
        end_byte: usize::try_from(base.get("end_byte_offset")?.as_u64()?).ok()?,
    })
}

/// What a page continues from, through every earlier page down to the thread's first file: the
/// records to read before the page's own, the bytes they came from, the conversation records the
/// earlier files hold past their cuts (the branches the thread left), and how many records the
/// thread's latest version has before the page (equal to `rows.len()` unless `Branches::All`).
struct BasePrefix {
    rows: Vec<Value>,
    bytes: Vec<u8>,
    left: Vec<Value>,
    active_len: usize,
}

/// The base a page continues from: the other rollout of its thread whose first `end_byte` bytes
/// end with record `end_ordinal - 1`, counting the records of the files before it when it is a
/// page itself. `chain` holds the files already in the chain, the page first.
fn base_prefix(
    page: &Path,
    base: &HistoryBase,
    branches: Branches,
    chain: &mut Vec<PathBuf>,
) -> Result<BasePrefix> {
    if chain.len() > MAX_PAGES {
        anyhow::bail!(
            "{}: thread {} has more than {MAX_PAGES} pages; not followed further",
            page.display(),
            base.thread
        );
    }
    let sessions = page
        .ancestors()
        .find(|dir| dir.file_name().and_then(|n| n.to_str()) == Some("sessions"))
        .or_else(|| page.ancestors().nth(4))
        .unwrap_or(Path::new("."));
    let candidates: Vec<PathBuf> = rollout_files(sessions)?
        .into_iter()
        .filter(|file| !chain.contains(file) && rollout_id(file) == base.thread)
        .collect();
    if candidates.is_empty() {
        anyhow::bail!(
            "{}: its history_base continues thread {} from its first {} records, and no other \
             rollout of that thread is under {}",
            page.display(),
            base.thread,
            base.end_ordinal,
            sessions.display()
        );
    }
    // Why an earlier page could not be followed, should no candidate fit.
    let mut earlier_failure: Option<anyhow::Error> = None;
    for candidate in &candidates {
        let bytes =
            std::fs::read(candidate).with_context(|| format!("read {}", candidate.display()))?;
        let Some(prefix) = bytes.get(..base.end_byte) else {
            continue;
        };
        if !(prefix.is_empty() || prefix.ends_with(b"\n")) {
            continue;
        }
        let Ok(text) = std::str::from_utf8(&bytes) else {
            continue;
        };
        let (head, tail) = text.split_at(base.end_byte);
        let (head_rows, _) = super::json::rows_from_lines(head, &candidate.display().to_string())?;
        // An earlier page: its own records start where the files before it leave off.
        let earlier = match history_base(&head_rows) {
            Some(inner) => {
                chain.push(candidate.clone());
                match base_prefix(candidate, &inner, branches, chain) {
                    Ok(earlier) => Some(earlier),
                    Err(error) => {
                        chain.pop();
                        earlier_failure = Some(error);
                        continue;
                    }
                }
            }
            None => None,
        };
        let ordinal = |row: &Value| row.get("ordinal").and_then(Value::as_u64);
        let before = earlier.as_ref().map_or(0, |e| e.active_len);
        let fits = (before + head_rows.len()) as u64 == base.end_ordinal
            && head_rows.last().and_then(ordinal) == base.end_ordinal.checked_sub(1);
        if !fits {
            if earlier.is_some() {
                chain.pop();
            }
            continue;
        }
        let (tail_rows, _) = super::json::rows_from_lines(tail, &candidate.display().to_string())?;
        let mut out = earlier.unwrap_or(BasePrefix {
            rows: Vec::new(),
            bytes: Vec::new(),
            left: Vec::new(),
            active_len: 0,
        });
        out.active_len += head_rows.len();
        out.rows.extend(head_rows);
        match branches {
            Branches::Active => {
                out.bytes.extend_from_slice(prefix);
                out.left.extend(
                    tail_rows
                        .into_iter()
                        .filter(|row| !matches!(class_of(row), Class::Meta(_))),
                );
            }
            Branches::All => {
                out.bytes.extend_from_slice(&bytes);
                out.rows.extend(tail_rows);
            }
        }
        return Ok(out);
    }
    let message = format!(
        "{}: its history_base end_byte_offset {} does not end record {} of any other rollout of \
         thread {} (looked at {})",
        page.display(),
        base.end_byte,
        base.end_ordinal.saturating_sub(1),
        base.thread,
        candidates
            .iter()
            .map(|c| c.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
    Err(match earlier_failure {
        Some(error) => error.context(message),
        None => anyhow::anyhow!(message),
    })
}

/// Read records that were already decoded.
pub fn parse(rows: &[Value], source_path: Option<&Path>) -> Result<Session> {
    parse_owned(rows.to_vec(), source_path)
}

/// [`parse`] for records the caller no longer needs: each one moves into its entry's `raw`
/// instead of being copied there.
pub fn parse_owned(rows: Vec<Value>, source_path: Option<&Path>) -> Result<Session> {
    let mut b = Builder::default();
    for (i, row) in rows.into_iter().enumerate() {
        b.push(i, row);
    }
    b.finish(source_path)
}

/// What `row.get("payload")` is for a record without one.
static NO_PAYLOAD: Value = Value::Null;

// ---------------------------------------------------------------------------------------------

enum Class {
    /// A model output record; records of one response stay in one step.
    StepPart,
    /// A `function_call_output` record.
    Result,
    /// A user turn.
    User,
    /// The `<turn_aborted>` marker.
    Abort,
    /// `compacted`.
    Compacted,
    /// `turn_context`: a turn starts here, so an open step ends here.
    TurnBoundary,
    Meta(MetaKind),
}

fn class_of(row: &Value) -> Class {
    let payload = row.get("payload").unwrap_or(&NO_PAYLOAD);
    let item_type = payload.get("type").and_then(Value::as_str).unwrap_or("");
    match row.get("type").and_then(Value::as_str).unwrap_or("") {
        "compacted" => Class::Compacted,
        "turn_context" => Class::TurnBoundary,
        "event_msg" => match item_type {
            "turn_aborted" => Class::Abort,
            "task_started" | "task_complete" => Class::Meta(MetaKind::Idle),
            "thread_settings_applied" => Class::Meta(MetaKind::ModelChange),
            _ => Class::Meta(MetaKind::Internal),
        },
        "response_item" => match item_type {
            "reasoning" | "function_call" | "custom_tool_call" | "local_shell_call"
            | "web_search_call" => Class::StepPart,
            "function_call_output" | "custom_tool_call_output" => Class::Result,
            "message" => match payload.get("role").and_then(Value::as_str).unwrap_or("") {
                "assistant" => Class::StepPart,
                "developer" | "system" => Class::Meta(MetaKind::SystemPrompt),
                "user" => {
                    let text = payload
                        .get("content")
                        .and_then(super::json::text_of_content)
                        .unwrap_or_default();
                    // Codex's own records start with their tag; a prompt that only mentions one
                    // is the user's words.
                    let text = text.trim_start();
                    if text.starts_with("<turn_aborted>") {
                        Class::Abort
                    } else if text.starts_with("<environment_context>")
                        || text.starts_with("<skills_instructions>")
                        // Codex Desktop 0.165 sends the project's AGENTS.md as a user message.
                        || text.starts_with("# AGENTS.md instructions for ")
                    {
                        Class::Meta(MetaKind::SystemPrompt)
                    } else if text.starts_with("<external_codex_apps_open_page>") {
                        // An event of the Desktop app, recorded as a user message.
                        Class::Meta(MetaKind::Internal)
                    } else {
                        Class::User
                    }
                }
                _ => Class::Meta(MetaKind::Internal),
            },
            _ => Class::Meta(MetaKind::Internal),
        },
        _ => Class::Meta(MetaKind::Internal),
    }
}

fn step_end(finish: Option<&str>, has_calls: bool) -> StepEnd {
    match finish {
        Some("tool_use") | Some("tool-calls") => StepEnd::ToolUse,
        Some("aborted") => StepEnd::Aborted,
        Some(_) => StepEnd::Completed,
        None if has_calls => StepEnd::ToolUse,
        None => StepEnd::Completed,
    }
}

#[derive(Default)]
struct Builder {
    entries: Vec<Entry>,
    meta: MetaBuilder,
    /// The step currently being appended to, and the turn it belongs to.
    open_step: Option<(usize, String)>,
    /// The results entry currently being appended to.
    open_results: Option<usize>,
    model: Option<ModelRef>,
    /// The shell the harness runs commands with, as its own `<environment_context>` states it.
    /// It decides whether `exec_command` reads as the canonical `Bash` or `PowerShell` (L13).
    env_shell: Option<CanonicalTool>,
}

#[derive(Default)]
struct MetaBuilder {
    id: Option<String>,
    cwd: Option<String>,
    version: Option<String>,
    created: Option<String>,
    updated: Option<String>,
    models: Vec<ModelRef>,
    git_branch: Option<String>,
}

impl MetaBuilder {
    fn observe(&mut self, row: &Value) {
        let payload = row.get("payload").unwrap_or(&NO_PAYLOAD);
        if row.get("type").and_then(Value::as_str) == Some("session_meta") {
            if self.id.is_none() {
                // `id` is the rollout's own thread; a subagent's `session_id` names the thread
                // that spawned it. A hand-written rollout may carry only one of the two.
                self.id = str_at(payload, "/id")
                    .filter(|s| !s.is_empty())
                    .or_else(|| str_at(payload, "/session_id"))
                    .filter(|s| !s.is_empty());
            }
            if self.cwd.is_none() {
                self.cwd = str_at(payload, "/cwd");
            }
            if self.version.is_none() {
                self.version = str_at(payload, "/cli_version");
            }
            if self.created.is_none() {
                self.created = str_at(payload, "/timestamp");
            }
            if self.git_branch.is_none() {
                self.git_branch = str_at(payload, "/git/branch");
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
    }
}

impl Builder {
    fn push(&mut self, index: usize, row: Value) {
        self.meta.observe(&row);
        let payload = row.get("payload").unwrap_or(&NO_PAYLOAD);
        // The environment block states the shell commands run with; it arrives before any call.
        if let Some(text) = payload
            .get("content")
            .and_then(super::json::text_of_content)
            && let Some(shell) = shell_declared(&text)
        {
            self.env_shell = Some(shell);
        }
        if row.get("type").and_then(Value::as_str) == Some("event_msg")
            && payload.get("type").and_then(Value::as_str) == Some("thread_settings_applied")
            && let Some(settings) = payload.get("thread_settings")
        {
            let model = ModelRef {
                provider: str_at(settings, "/model_provider_id"),
                id: str_at(settings, "/model").unwrap_or_default(),
            };
            if !model.id.is_empty() && !self.meta.models.iter().any(|m| m.id == model.id) {
                self.meta.models.push(model.clone());
            }
            self.model = Some(model);
        }
        match class_of(&row) {
            Class::StepPart => {
                self.open_results = None;
                self.step_with(index, row);
            }
            Class::Result => {
                self.open_step = None;
                self.results_with(index, row);
            }
            Class::User => {
                self.flush();
                self.entries.push(Entry {
                    id: id_of(payload, index),
                    kind: Kind::User {
                        text: payload
                            .get("content")
                            .and_then(super::json::text_of_content)
                            .unwrap_or_default(),
                        attachments: payload
                            .get("content")
                            .and_then(Value::as_array)
                            .map(|blocks| super::json::attachments(blocks))
                            .unwrap_or_default(),
                    },
                    raw: vec![row],
                });
            }
            Class::Abort => {
                self.flush();
                self.entries.push(Entry {
                    id: id_of(payload, index),
                    kind: Kind::Meta {
                        kind: MetaKind::TurnAborted,
                    },
                    raw: vec![row],
                });
            }
            Class::Compacted => self.compacted(index, row),
            Class::TurnBoundary => {
                // A new turn starts here: the step and any results entry that was still open
                // belong to the turn that just ended.
                self.open_step = None;
                self.open_results = None;
                self.entries.push(Entry {
                    id: id_of(payload, index),
                    kind: Kind::Meta {
                        kind: MetaKind::Internal,
                    },
                    raw: vec![row],
                });
            }
            Class::Meta(kind) => {
                self.entries.push(Entry {
                    id: id_of(payload, index),
                    kind: Kind::Meta { kind },
                    raw: vec![row],
                });
            }
        }
    }

    /// The `compacted` record. The summary the model wrote in the turn it names is folded into
    /// this entry: it is not part of the history the destination replays.
    fn compacted(&mut self, index: usize, row: Value) {
        let payload = row.get("payload").unwrap_or(&NO_PAYLOAD);
        let compaction_turn = str_at(payload, "/latest_token_usage_record/turn_id");
        let fold = match (&self.open_step, &compaction_turn) {
            (Some((i, turn)), Some(t)) if turn == t => Some(*i),
            _ => None,
        };
        let kind = Kind::Compaction {
            summary: str_at(payload, "/message").unwrap_or_default(),
            kept_from: None, // resolved at the end: the first conversation entry after this one
            trigger: Trigger::Unknown, // Codex records no trigger field
            tokens_before: payload
                .pointer("/latest_token_usage_record/usage/input_tokens")
                .and_then(Value::as_u64),
        };
        self.open_step = None;
        self.open_results = None;
        match fold {
            // Replace the summary turn in place, so the entry stays where its first record is.
            Some(i) => {
                let entry = &mut self.entries[i];
                entry.raw.push(row);
                entry.kind = kind;
            }
            None => self.entries.push(Entry {
                id: id_of(payload, index),
                kind,
                raw: vec![row],
            }),
        }
    }

    fn flush(&mut self) {
        self.open_step = None;
        self.open_results = None;
    }

    fn step_with(&mut self, index: usize, row: Value) {
        let payload = row.get("payload").unwrap_or(&NO_PAYLOAD);
        let turn = str_at(
            payload,
            "/internal_chat_message_metadata_passthrough/turn_id",
        )
        .unwrap_or_default();
        // A record of a different turn is a different model response, even when only events sit
        // between them: the reader must not merge two answers into one step.
        if self
            .open_step
            .as_ref()
            .is_some_and(|(_, open_turn)| *open_turn != turn)
        {
            self.open_step = None;
        }
        if self.open_step.is_none() {
            let id = id_of(payload, index);
            self.entries.push(Entry {
                id,
                kind: Kind::Step {
                    model: self.model.clone(),
                    reasoning: Vec::new(),
                    text: Vec::new(),
                    calls: Vec::new(),
                    end: StepEnd::Unknown,
                },
                raw: Vec::new(),
            });
            self.open_step = Some((self.entries.len() - 1, turn));
        }
        let (i, _) = *self.open_step.as_ref().expect("just opened");
        let entry = &mut self.entries[i];
        let model_id = self.model.as_ref().map(|m| m.id.clone());
        let Kind::Step {
            reasoning,
            text,
            calls,
            end,
            ..
        } = &mut entry.kind
        else {
            entry.raw.push(row);
            return;
        };
        let item_type = payload.get("type").and_then(Value::as_str).unwrap_or("");
        match item_type {
            "reasoning" => reasoning.push(Reasoning {
                text: payload
                    .get("summary")
                    .and_then(Value::as_array)
                    .map(|s| blocks_text(s))
                    .unwrap_or_default(),
                opaque: Some(Opaque {
                    protocol: Protocol::OpenaiResponses,
                    model: model_id,
                    signature: None,
                    encrypted: str_at(payload, "/encrypted_content"),
                }),
            }),
            "message" => {
                if let Some(blocks) = payload.get("content").and_then(Value::as_array) {
                    let t = blocks_text(blocks);
                    if !t.is_empty() {
                        text.push(t);
                    }
                }
            }
            "function_call" | "custom_tool_call" | "local_shell_call" | "web_search_call" => {
                // `local_shell_call` and `web_search_call` have no name or arguments: what they
                // did is in `action`, and a web search has no `call_id` (it has no output record).
                let (name, input) = match item_type {
                    "local_shell_call" => ("local_shell".to_string(), action_of(payload)),
                    "web_search_call" => ("web_search".to_string(), action_of(payload)),
                    _ => (
                        str_at(payload, "/name").unwrap_or_default(),
                        arguments_of(payload),
                    ),
                };
                calls.push(ToolCall {
                    id: str_at(payload, "/call_id")
                        .or_else(|| str_at(payload, "/id"))
                        .unwrap_or_else(|| id_of(payload, index)),
                    canonical: Some(canonical_call(&name, &input, self.env_shell.clone())),
                    source_name: name,
                    input,
                });
            }
            _ => {}
        }
        *end = step_end(None, !calls.is_empty());
        entry.raw.push(row);
    }

    fn results_with(&mut self, index: usize, row: Value) {
        let payload = row.get("payload").unwrap_or(&NO_PAYLOAD);
        let result = ToolResult {
            call_id: str_at(payload, "/call_id").unwrap_or_default(),
            content: match payload.get("output") {
                Some(Value::String(s)) => ResultContent::Text(s.clone()),
                // Content items (an image, an MCP result) stay structured.
                Some(Value::Array(items)) => {
                    super::json::result_content(&items.iter().map(output_item).collect::<Vec<_>>())
                }
                Some(other) => ResultContent::Text(other.to_string()),
                None => ResultContent::Text(String::new()),
            },
            is_error: false,
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
                    id: id_of(payload, index),
                    kind: Kind::Results {
                        results: vec![result],
                    },
                    raw: vec![row],
                });
                self.open_results = Some(self.entries.len() - 1);
            }
        }
    }

    fn finish(mut self, source_path: Option<&Path>) -> Result<Session> {
        self.flush();
        for i in 0..self.entries.len() {
            if !matches!(
                self.entries[i].kind,
                Kind::Compaction {
                    kept_from: None,
                    ..
                }
            ) {
                continue;
            }
            let next = self.entries[i + 1..]
                .iter()
                .find(|e| !matches!(e.kind, Kind::Meta { .. }))
                .map(|e| e.id.clone());
            if let Kind::Compaction { kept_from, .. } = &mut self.entries[i].kind {
                *kept_from = next;
            }
        }
        let m = self.meta;
        Ok(Session {
            meta: Meta {
                harness: HarnessId::Codex,
                harness_version: m.version.unwrap_or_else(|| "unknown".into()),
                id: m.id.unwrap_or_else(|| "unknown".into()),
                cwd: m.cwd,
                title: None,
                created: m.created,
                updated: m.updated,
                models: m.models,
                git_branch: m.git_branch,
                parent: None,
                source_path: source_path.map(|p| p.to_string_lossy().into_owned()),
                source_sha256: None,
            },
            entries: self.entries,
        })
    }
}

/// What a `local_shell_call` or `web_search_call` did.
fn action_of(payload: &Value) -> Value {
    payload.get("action").cloned().unwrap_or(Value::Null)
}

/// A tool output content item, with its text as a plain text block.
fn output_item(item: &Value) -> Value {
    match item.get("type").and_then(Value::as_str) {
        Some("input_text" | "output_text") => serde_json::json!({
            "type": "text",
            "text": item.get("text").cloned().unwrap_or(Value::String(String::new())),
        }),
        _ => item.clone(),
    }
}

/// A tool call's arguments: Codex stores them as a JSON *string*.
fn arguments_of(payload: &Value) -> Value {
    match payload.get("arguments") {
        Some(Value::String(s)) => {
            serde_json::from_str(s).unwrap_or_else(|_| Value::String(s.clone()))
        }
        Some(other) => other.clone(),
        None => payload.get("input").cloned().unwrap_or(Value::Null),
    }
}

/// The shell a harness runs commands with, as its own environment block states it [V].
fn shell_declared(text: &str) -> Option<CanonicalTool> {
    let start = text.find("<shell>")? + "<shell>".len();
    let end = text[start..].find("</shell>")? + start;
    match text[start..end].trim().to_ascii_lowercase().as_str() {
        "bash" | "sh" | "zsh" | "posix" | "git-bash" => Some(CanonicalTool::Bash),
        "powershell" | "pwsh" => Some(CanonicalTool::PowerShell),
        _ => None,
    }
}

/// The canonical form of a Codex call. `exec_command` inherits the harness's shell dialect unless
/// the call names one itself; the dialect is what decides whether a hop may keep a shell call or
/// has to carry it as text (L13). When nothing states the shell, the command line is read as the
/// canonical `Bash`, which is what that name means in this vocabulary.
fn canonical_call(name: &str, input: &Value, env_shell: Option<CanonicalTool>) -> CanonicalTool {
    let from_argument = input
        .get("shell")
        .and_then(Value::as_str)
        .map(|s| s.to_ascii_lowercase());
    match (name, from_argument.as_deref()) {
        ("exec_command" | "local_shell", Some("pwsh" | "powershell")) => CanonicalTool::PowerShell,
        ("exec_command" | "local_shell", Some("bash" | "sh" | "zsh" | "posix")) => {
            CanonicalTool::Bash
        }
        ("exec_command" | "local_shell", _) => env_shell.unwrap_or(CanonicalTool::Bash),
        _ => CanonicalTool::from_codex(name),
    }
}

/// The record's own id, or a stable one derived from its position.
fn id_of(payload: &Value, index: usize) -> String {
    str_at(payload, "/id")
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| fallback_id("ord", index))
}

// ---------------------------------------------------------------------------------------------

impl Reader for Codex {
    fn harness(&self) -> HarnessId {
        HarnessId::Codex
    }

    fn list(&self, roots: &StoreRoots) -> Result<Vec<SessionInfo>> {
        let Some(sessions) = roots.codex_sessions() else {
            return Err(super::Lookup::NoStore(
                "CODEX_HOME is not set: no Codex store to list".into(),
            )
            .into());
        };
        // One entry per thread, at its latest file: the active page of a paginated thread.
        let mut latest: HashMap<String, PathBuf> = HashMap::new();
        for file in rollout_files(&sessions)? {
            let id = rollout_id(&file);
            if id.is_empty() {
                continue;
            }
            match latest.get(&id) {
                Some(known) if known.file_name() >= file.file_name() => {}
                _ => {
                    latest.insert(id, file);
                }
            }
        }
        let mut out: Vec<SessionInfo> = latest
            .into_iter()
            .map(|(id, file)| SessionInfo {
                id,
                path: file.to_string_lossy().into_owned(),
                ..Default::default()
            })
            .collect();
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }

    fn read(&self, id: &str, roots: &StoreRoots) -> Result<Session> {
        read_with(id, roots, Branches::Active)
    }
}

/// Read the thread `id` (an unambiguous prefix is enough) from the store, choosing which branches
/// of a paginated thread to carry.
pub fn read_with(id: &str, roots: &StoreRoots, branches: Branches) -> Result<Session> {
    let Some(sessions) = roots.codex_sessions() else {
        return Err(
            super::Lookup::NoStore("CODEX_HOME is not set: no Codex store to read".into()).into(),
        );
    };
    let matches: Vec<PathBuf> = rollout_files(&sessions)?
        .into_iter()
        .filter(|f| rollout_id(f).starts_with(id))
        .collect();
    // The pages of one paginated thread share its id: the latest page is the thread.
    let one_thread = matches
        .first()
        .map(|file| rollout_id(file))
        .is_some_and(|first| matches.iter().all(|f| rollout_id(f) == first));
    if matches.len() > 1 && one_thread {
        let latest = matches
            .iter()
            .max_by(|a, b| a.file_name().cmp(&b.file_name()))
            .expect("not empty");
        return read_file_with(latest, branches);
    }
    match matches.as_slice() {
        [one] => read_file_with(one, branches),
        [] => Err(super::Lookup::NotFound(format!(
            "no Codex rollout with id {id:?} under {}",
            sessions.display()
        ))
        .into()),
        many => Err(super::Lookup::Ambiguous(format!(
            "{id:?} matches {} Codex rollouts",
            many.len()
        ))
        .into()),
    }
}

/// The uuid a rollout file name ends with: `rollout-<ts>-<uuid>.jsonl`, or the thread's uuid for
/// a page of a paginated thread, `rollout-<ts>-<uuid>_<page uuid>.jsonl`. A uuid is five dash
/// separated groups, so the id is the last five of the name's parts.
fn rollout_id(file: &Path) -> String {
    let name = file.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let stem = match name.rsplit_once('_') {
        Some((thread, _page)) if name.starts_with("rollout-") => thread,
        _ => name,
    };
    let parts: Vec<&str> = stem.split('-').collect();
    if stem.starts_with("rollout-") && parts.len() >= 5 {
        parts[parts.len() - 5..].join("-")
    } else {
        stem.to_string()
    }
}

/// Every rollout under `<CODEX_HOME>/sessions/YYYY/MM/DD/`.
fn rollout_files(sessions: &Path) -> Result<Vec<PathBuf>> {
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
