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

mod order;

/// The Claude Code reader.
#[derive(Debug, Clone, Copy, Default)]
pub struct ClaudeCode;

impl ClaudeCode {
    /// Read one session file directly. Every error names the file it came from.
    pub fn read_path(path: &Path) -> Result<Session> {
        read_file(path)
    }
}

/// [`Reader::list`], remembering in `cache` (a folder) what it read from each session file, so a
/// file that has not changed is not read again. Listing a few hundred sessions reads up to half a
/// megabyte of each, which on a cold disk takes seconds; with the cache only the files that
/// changed since the last listing are read. `None` is no cache.
pub fn list_cached(roots: &StoreRoots, cache: Option<&Path>) -> Result<Vec<SessionInfo>> {
    let (Some(config), Some(projects)) =
        (roots.claude_config_dir.as_deref(), roots.claude_projects())
    else {
        return Err(super::Lookup::NoStore(
            "CLAUDE_CONFIG_DIR is not set: no Claude Code store to list".into(),
        )
        .into());
    };
    let files = session_files(&projects)?;
    let mut out = peek_files(&files, &projects, cache);
    add_what_the_transcripts_do_not_say(&mut out, config);
    out.sort_by(|a, b| a.path.cmp(&b.path).then_with(|| a.id.cmp(&b.id)));
    Ok(out)
}

/// A file's size and modification time (nanoseconds since 1970): what tells that it changed.
type Stamp = (u64, u64);

fn stamp(file: &Path) -> Option<Stamp> {
    let meta = std::fs::metadata(file).ok()?;
    let nanos = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some((meta.len(), u64::try_from(nanos).ok()?))
}

/// A file modified this recently is not remembered: a clock that ticks every few milliseconds
/// cannot tell two writes within one tick apart, so "same size, same time" proves nothing about
/// it yet (the rule git applies to its index).
const SETTLED_AFTER: std::time::Duration = std::time::Duration::from_secs(2);

/// What each session file says about its session: from `cache` where the file is as it was,
/// otherwise from the file (the files that changed or are new, read side by side).
fn peek_files(files: &[PathBuf], projects: &Path, cache: Option<&Path>) -> Vec<SessionInfo> {
    use serde_json::{Map, Value};

    let table_name = crate::cache::table_name("claude-list", &projects.to_string_lossy());
    let remembered: Map<String, Value> = cache
        .map(|dir| crate::cache::load_table(dir, &table_name))
        .unwrap_or_default();
    let stamps: Vec<Option<Stamp>> = files.iter().map(|file| stamp(file)).collect();
    let mut peeked: Vec<Option<SessionInfo>> = vec![None; files.len()];
    let mut unread = Vec::new();
    for (index, file) in files.iter().enumerate() {
        let hit = stamps[index].and_then(|stamp| {
            let entry = remembered.get(file.to_string_lossy().as_ref())?;
            from_entry(entry, stamp)
        });
        match hit {
            Some(info) => peeked[index] = Some(info),
            None => unread.push(index),
        }
    }
    for (index, info) in peek_many(files, &unread) {
        peeked[index] = Some(info);
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |age| u64::try_from(age.as_nanos()).unwrap_or(u64::MAX));
    let settled = u64::try_from(SETTLED_AFTER.as_nanos()).unwrap_or(u64::MAX);
    let mut table = Map::new();
    let mut out = Vec::with_capacity(files.len());
    for ((file, stamp), info) in files.iter().zip(stamps).zip(peeked) {
        let mut info = info.unwrap_or_default();
        if let Some((size, modified)) = stamp
            && modified.saturating_add(settled) <= now
        {
            table.insert(
                file.to_string_lossy().into_owned(),
                to_entry(size, modified, &info),
            );
        }
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
    if let Some(dir) = cache
        && table != remembered
    {
        crate::cache::store_table(dir, &table_name, &table);
    }
    out
}

/// What the cache keeps of a session file: its stamp, and the four things a peek found.
fn to_entry(size: u64, modified: u64, info: &SessionInfo) -> serde_json::Value {
    let mut entry = serde_json::Map::new();
    entry.insert("s".into(), size.into());
    entry.insert("m".into(), modified.into());
    for (key, value) in [
        ("i", Some(&info.id)),
        ("t", info.title.as_ref()),
        ("c", info.cwd.as_ref()),
        ("u", info.updated.as_ref()),
    ] {
        if let Some(value) = value.filter(|value| !value.is_empty()) {
            entry.insert(key.into(), value.clone().into());
        }
    }
    entry.into()
}

/// The peek the cache kept for a file, when the file is the one it was kept for.
fn from_entry(entry: &serde_json::Value, (size, modified): Stamp) -> Option<SessionInfo> {
    if entry.get("s")?.as_u64()? != size || entry.get("m")?.as_u64()? != modified {
        return None;
    }
    let text = |key: &str| {
        entry
            .get(key)
            .and_then(|value| value.as_str())
            .map(str::to_owned)
    };
    Some(SessionInfo {
        id: text("i").unwrap_or_default(),
        title: text("t"),
        cwd: text("c"),
        updated: text("u"),
        ..Default::default()
    })
}

/// Peek at the files at `indexes`, side by side when there are many: a cold disk answers several
/// reads at once faster than one after another.
fn peek_many(files: &[PathBuf], indexes: &[usize]) -> Vec<(usize, SessionInfo)> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Mutex, PoisonError};

    /// Below this many files starting threads costs more than it saves.
    const SIDE_BY_SIDE_FROM: usize = 8;
    const MOST_AT_ONCE: usize = 8;
    let workers = std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .min(MOST_AT_ONCE);
    if indexes.len() < SIDE_BY_SIDE_FROM || workers < 2 {
        return indexes
            .iter()
            .map(|&index| (index, peek(&files[index])))
            .collect();
    }
    let next = AtomicUsize::new(0);
    let done = Mutex::new(Vec::with_capacity(indexes.len()));
    std::thread::scope(|scope| {
        for _ in 0..workers.min(indexes.len()) {
            scope.spawn(|| {
                while let Some(&index) = indexes.get(next.fetch_add(1, Ordering::Relaxed)) {
                    let info = peek(&files[index]);
                    done.lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push((index, info));
                }
            });
        }
    });
    done.into_inner().unwrap_or_else(PoisonError::into_inner)
}

/// Read one Claude Code session file (`<CLAUDE_CONFIG_DIR>/projects/<project>/<uuid>.jsonl`).
pub fn read_file(path: &Path) -> Result<Session> {
    let file = super::json::read_jsonl(path)?;
    let _span = crate::timing::span("build entries");
    let mut session = parse_owned(file.rows, Some(path))?;
    session.meta.source_sha256 = Some(file.sha256.wait());
    super::json::keep_unfinished(&mut session, file.unfinished);
    Ok(session)
}

/// Read records that were already decoded (the CLI and the tests both come through here).
pub fn parse(rows: &[Value], source_path: Option<&Path>) -> Result<Session> {
    parse_owned(rows.to_vec(), source_path)
}

/// [`parse`] for records the caller no longer needs: each one moves into its entry's `raw`
/// instead of being copied there.
pub fn parse_owned(rows: Vec<Value>, source_path: Option<&Path>) -> Result<Session> {
    let reconstruction = {
        let _span = crate::timing::span("resumed branch");
        order::reconstruct(&rows)?
    };
    let mut b = Builder::default();
    {
        let _span = crate::timing::span("entries");
        let mut source: Vec<Option<Value>> = rows.into_iter().map(Some).collect();
        for i in reconstruction.order {
            b.push(
                i,
                source[i].take().expect("each source row is used once"),
                reconstruction.on_branch[i],
            );
        }
    }
    let _span = crate::timing::span("finish");
    b.finish(source_path)
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

/// L9: the row Claude Code writes when the user interrupts a turn (`[Request interrupted by
/// user]`, `[Request interrupted by user for tool use]`). The user never typed it.
fn is_interruption_marker(text: &str) -> bool {
    let text = text.trim();
    text.starts_with("[Request interrupted by user") && text.ends_with(']')
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

/// A prompt-type slash command is followed by an `isMeta` row containing the actual user prompt.
fn with_command_context(
    row: &Value,
    mut class: Class,
    text: Option<&str>,
    after_prompt_command: &mut bool,
) -> Class {
    if matches!(class, Class::Meta(MetaKind::Command))
        && *after_prompt_command
        && truthy(row.get("isMeta"))
        && text.is_some_and(|text| !is_command_row(text))
    {
        class = Class::User;
    }
    match &class {
        Class::Meta(MetaKind::Command) => {
            *after_prompt_command = text.is_some_and(is_prompt_command);
        }
        Class::Meta(_) => {}
        _ => *after_prompt_command = false,
    }
    class
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
        .filter(|id| !id.trim().is_empty())
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
    /// The step collecting blocks, including when bookkeeping or results arrived between them.
    open_step: Option<OpenStep>,
    /// The current step's results, including when later blocks of that step are still arriving.
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
    title: TitleBuilder,
    git_branch: Option<String>,
    version: Option<String>,
    created: Option<String>,
    updated: Option<String>,
    models: Vec<ModelRef>,
}

/// The rows that name a session, in the order Claude Code's own picker takes them: its custom
/// title (`/rename`), else the `lastPrompt` of its last `last-prompt` row, else its first user
/// prompt (docs/harnesses/claude-code.md §8, source [9]). A `slug` is Claude's anonymous internal
/// name on a compact_boundary row: the last resort, and still a name where a bare uuid is not.
#[derive(Default)]
struct TitleBuilder {
    custom: Option<String>,
    last_prompt: Option<String>,
    first_prompt: Option<String>,
    slug: Option<String>,
}

impl TitleBuilder {
    /// What one row says about the session's name. A later row supersedes an earlier one, as both
    /// are appended: a rename replaces the name before it, and a turn's prompt the one before it.
    fn observe(&mut self, row: &Value) {
        use super::json::title_line;
        let at = |p: &str| row.pointer(p).and_then(Value::as_str);
        match row.get("type").and_then(Value::as_str).unwrap_or("") {
            "custom-title" => {
                if let Some(title) = title_line(at("/customTitle")) {
                    self.custom = Some(title);
                }
            }
            "last-prompt" => {
                if let Some(prompt) = title_line(at("/lastPrompt")) {
                    self.last_prompt = Some(prompt);
                }
            }
            _ => {}
        }
        if self.slug.is_none() {
            self.slug = title_line(at("/slug"));
        }
        // The user's own first prompt: not a subagent's, not a command or summary row.
        if self.first_prompt.is_none()
            && !truthy(row.get("isSidechain"))
            && matches!(class_of(row), Class::User)
        {
            self.first_prompt = title_line(content_text(row).as_deref());
        }
    }

    /// The name the picker shows.
    fn pick(self) -> Option<String> {
        let Self {
            custom,
            last_prompt,
            first_prompt,
            slug,
        } = self;
        custom.or(last_prompt).or(first_prompt).or(slug)
    }
}

impl MetaBuilder {
    fn observe(&mut self, row: &Value) {
        let str_at = |p: &str| row.pointer(p).and_then(Value::as_str).map(str::to_owned);
        self.title.observe(row);
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
    fn push(&mut self, index: usize, row: Value, on_branch: bool) {
        self.meta.observe(&row);
        if truthy(row.get("isSidechain")) {
            // A subagent's transcript: kept, never conversation (§10 "not exercised").
            self.flush_compaction();
            self.entries.push(Entry {
                id: row_id(&row, index),
                kind: Kind::Meta {
                    kind: MetaKind::Internal,
                },
                raw: vec![row],
            });
            return;
        }
        let class = class_of(&row);
        if !on_branch {
            // An abandoned row cannot change slash-command state or split an active response.
            self.flush_compaction();
            self.entries.push(Entry {
                id: row_id(&row, index),
                kind: Kind::Meta {
                    kind: match class {
                        Class::Meta(kind) => kind,
                        _ => MetaKind::OffBranch,
                    },
                },
                raw: vec![row],
            });
            return;
        }
        let text = content_text(&row);
        let class =
            with_command_context(&row, class, text.as_deref(), &mut self.after_prompt_command);
        // L9: a row of the resumed branch that is the harness's own interruption marker. Only
        // here: an interruption of an abandoned branch says nothing about this one.
        let class = match class {
            Class::User if text.as_deref().is_some_and(is_interruption_marker) => {
                Class::Meta(MetaKind::TurnAborted)
            }
            class => class,
        };
        match class {
            Class::Assistant(message_id) => {
                self.flush_unless_step(&message_id);
                self.unsummarized = None;
                self.step_with(index, row, message_id);
            }
            Class::Results => {
                self.flush_unless_results();
                self.unsummarized = None;
                let id = row_id(&row, index);
                self.results_with(index, row);
                // The user's own text beside the results is a turn of its own, after them. An
                // interruption written there is the harness's marker, not a turn.
                if let Some(text) = text {
                    self.flush();
                    self.entries.push(Entry {
                        id: format!("{id}-text"),
                        kind: if is_interruption_marker(&text) {
                            Kind::Meta {
                                kind: MetaKind::TurnAborted,
                            }
                        } else {
                            Kind::User {
                                text,
                                attachments: Vec::new(),
                            }
                        },
                        raw: Vec::new(),
                    });
                }
            }
            Class::Boundary => {
                self.flush();
                let compact = row.get("compactMetadata").cloned().unwrap_or(Value::Null);
                self.open_compaction = Some(OpenCompaction {
                    boundary: row,
                    trigger: match compact.get("trigger").and_then(Value::as_str) {
                        Some("manual") => Trigger::Manual,
                        Some("auto") => Trigger::Auto,
                        _ => Trigger::Unknown,
                    },
                    tokens_before: compact.get("preTokens").and_then(Value::as_u64),
                    kept_from: compact
                        .pointer("/preservedSegment/headUuid")
                        .and_then(Value::as_str)
                        .or_else(|| {
                            compact
                                .pointer("/preservedMessages/uuids/0")
                                .and_then(Value::as_str)
                        })
                        .map(str::to_owned),
                });
            }
            Class::Summary if self.open_compaction.is_none() && self.unsummarized.is_some() => {
                let at = self.unsummarized.take().expect("checked");
                if let Kind::Compaction { summary, .. } = &mut self.entries[at].kind {
                    *summary = text.unwrap_or_default();
                }
                self.entries.push(Entry {
                    id: row_id(&row, index),
                    kind: Kind::Meta {
                        kind: MetaKind::Internal,
                    },
                    raw: vec![row],
                });
            }
            Class::Summary => {
                if let Some(open) = self.open_compaction.take() {
                    let summary = content_text(&row).unwrap_or_default();
                    self.entries.push(Entry {
                        id: row_id(&open.boundary, index),
                        kind: Kind::Compaction {
                            summary,
                            kept_from: open.kept_from,
                            trigger: open.trigger,
                            tokens_before: open.tokens_before,
                        },
                        raw: vec![open.boundary, row],
                    });
                } else {
                    // A summary with no boundary before it: keep it, do not guess.
                    self.entries.push(Entry {
                        id: row_id(&row, index),
                        kind: Kind::Meta {
                            kind: MetaKind::Internal,
                        },
                        raw: vec![row],
                    });
                }
            }
            Class::User => {
                self.flush();
                self.unsummarized = None;
                let text = text.unwrap_or_default();
                self.entries.push(Entry {
                    id: row_id(&row, index),
                    kind: Kind::User {
                        text,
                        attachments: row
                            .pointer("/message/content")
                            .and_then(Value::as_array)
                            .map(|blocks| super::json::attachments(blocks))
                            .unwrap_or_default(),
                    },
                    raw: vec![row],
                });
            }
            Class::Meta(MetaKind::TurnAborted) => {
                // L9: the reply the interruption cut keeps what it holds and says how it ended.
                // Nothing of that turn continues after the marker.
                let last = self
                    .entries
                    .iter()
                    .rposition(|entry| !matches!(entry.kind, Kind::Meta { .. }));
                if let Some(Kind::Step { end, .. }) =
                    last.map(|index| &mut self.entries[index].kind)
                {
                    *end = StepEnd::Aborted;
                }
                self.flush();
                self.unsummarized = None;
                self.entries.push(Entry {
                    id: row_id(&row, index),
                    kind: Kind::Meta {
                        kind: MetaKind::TurnAborted,
                    },
                    raw: vec![row],
                });
            }
            Class::Meta(kind) => {
                // Bookkeeping is not a conversation boundary: a streaming response or its
                // results may continue after it.
                self.flush_compaction();
                self.entries.push(Entry {
                    id: row_id(&row, index),
                    kind: Kind::Meta { kind },
                    raw: vec![row],
                });
            }
        }
    }

    /// Flush the pending boundary of a compaction block that has no summary row after it.
    fn flush(&mut self) {
        self.flush_step();
        self.open_results = None;
        self.flush_compaction();
    }

    fn flush_compaction(&mut self) {
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
            return;
        }
        self.flush();
    }

    fn flush_unless_results(&mut self) {
        if self.open_compaction.is_some() {
            self.flush();
        }
    }

    fn step_with(&mut self, index: usize, row: Value, message_id: String) {
        if self.open_step.is_none() {
            let id = row_id(&row, index);
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
                                chat: None,
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
                                chat: None,
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
            if let Some(stop) = row.pointer("/message/stop_reason").and_then(Value::as_str) {
                *end = step_end(Some(stop));
            }
        }
        entry.raw.push(row);
    }

    fn results_with(&mut self, index: usize, row: Value) {
        // Read from the record where it is: the tool results' bodies are most of a session.
        let found: Vec<ToolResult> = row
            .pointer("/message/content")
            .and_then(Value::as_array)
            .map(|b| {
                b.iter()
                    .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_result"))
                    .map(tool_result_of)
                    .collect()
            })
            .unwrap_or_default();
        match self.open_results {
            Some(i) => {
                let entry = &mut self.entries[i];
                entry.raw.push(row);
                if let Kind::Results { results } = &mut entry.kind {
                    results.extend(found);
                }
            }
            None => {
                self.entries.push(Entry {
                    id: row_id(&row, index),
                    kind: Kind::Results { results: found },
                    raw: vec![row],
                });
                self.open_results = Some(self.entries.len() - 1);
            }
        }
    }

    fn finish(mut self, source_path: Option<&Path>) -> Result<Session> {
        self.flush();
        // Resolve source UUIDs through merged records and the preserved list. When neither names a
        // conversation entry, fall back to the first conversation entry after the boundary.
        if self
            .entries
            .iter()
            .any(|entry| matches!(entry.kind, Kind::Compaction { .. }))
        {
            let mut aliases = std::collections::HashMap::new();
            for (index, entry) in self.entries.iter().enumerate() {
                aliases.insert(entry.id.clone(), index);
                for row in &entry.raw {
                    if let Some(id) = row.get("uuid").and_then(Value::as_str) {
                        aliases.insert(id.to_owned(), index);
                    }
                }
            }
            let mut next = vec![None; self.entries.len() + 1];
            for index in (0..self.entries.len()).rev() {
                next[index] = if matches!(self.entries[index].kind, Kind::Meta { .. }) {
                    next[index + 1]
                } else {
                    Some(index)
                };
            }
            let resolved: Vec<_> = self
                .entries
                .iter()
                .enumerate()
                .filter_map(|(index, entry)| {
                    let Kind::Compaction { kept_from, .. } = &entry.kind else {
                        return None;
                    };
                    // A preserved list may start with a bookkeeping sibling that is not itself on
                    // the resumed chain. Prefer its first retained conversation row to a position
                    // guessed from where that sibling happened to be written.
                    let listed = entry
                        .raw
                        .iter()
                        .find_map(|row| {
                            row.pointer("/compactMetadata/preservedMessages/uuids")
                                .and_then(Value::as_array)
                        })
                        .and_then(|ids| {
                            ids.iter()
                                .filter_map(Value::as_str)
                                .filter_map(|id| aliases.get(id).copied())
                                .find(|&at| !matches!(self.entries[at].kind, Kind::Meta { .. }))
                        });
                    let position = match kept_from.as_ref().and_then(|id| aliases.get(id)).copied()
                    {
                        Some(at) if matches!(self.entries[at].kind, Kind::Meta { .. }) => {
                            listed.or(next[at])
                        }
                        Some(at) => Some(at),
                        None => listed,
                    }
                    .or(next[index + 1]);
                    Some((index, position.map(|at| self.entries[at].id.clone())))
                })
                .collect();
            for (index, resolved) in resolved {
                if let Kind::Compaction { kept_from, .. } = &mut self.entries[index].kind {
                    *kept_from = resolved;
                }
            }
        }
        let m = self.meta;
        Ok(Session {
            meta: Meta {
                harness: HarnessId::ClaudeCode,
                harness_version: m.version.unwrap_or_else(|| "unknown".into()),
                id: m.id.unwrap_or_else(|| "unknown".into()),
                cwd: m.cwd,
                title: m.title.pick(),
                created: m.created,
                updated: m.updated,
                models: m.models,
                git_branch: m.git_branch,
                parent: None,
                parent_kind: None,
                source_path: source_path.map(|p| p.to_string_lossy().into_owned()),
                source_sha256: None,
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
        list_cached(roots, crate::cache::dir().as_deref())
    }

    fn read(&self, id: &str, roots: &StoreRoots) -> Result<Session> {
        let Some(projects) = roots.claude_projects() else {
            return Err(super::Lookup::NoStore(
                "CLAUDE_CONFIG_DIR is not set: no Claude Code store to read".into(),
            )
            .into());
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
            [] => Err(super::Lookup::NotFound(why_not_found(id, &projects, roots)).into()),
            many => Err(super::Lookup::Ambiguous(format!(
                "{id:?} matches {} Claude Code sessions",
                many.len()
            ))
            .into()),
        }
    }
}

/// What Claude Code records about sessions beside their transcripts, added to `listed`:
///
/// - `history.jsonl` still names sessions whose transcript was deleted: they are listed, marked
///   unreadable, so their projects are still seen;
/// - `sessions/<pid>.json` registers each running session. A registration whose process is running
///   marks its session live, and lists it when no transcript exists yet; one whose process is gone
///   is a leftover and says nothing. Where the store keeps no `sessions/` folder, or a
///   registration belongs to another machine, `live` stays unknown (`None`) rather than `false`.
fn add_what_the_transcripts_do_not_say(listed: &mut Vec<SessionInfo>, config: &Path) {
    use super::claude_history as history;
    use super::claude_live as live;
    use std::collections::{HashMap, HashSet};

    let stem_of = |path: &str| {
        Path::new(path)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(str::to_owned)
    };
    let mut known: HashSet<String> = HashSet::new();
    for info in listed.iter() {
        known.insert(info.id.clone());
        known.extend(stem_of(&info.path));
    }
    let history_file = history::path(config).to_string_lossy().into_owned();
    for session in history::sessions(config) {
        if !known.insert(session.id.clone()) {
            continue;
        }
        listed.push(SessionInfo {
            id: session.id,
            title: session.title,
            cwd: Some(session.project),
            updated: chrono::DateTime::from_timestamp_millis(session.last_ms)
                .map(|time| time.to_rfc3339()),
            path: history_file.clone(),
            unreadable: Some(
                "its transcript is gone; the session survives only as prompts in history.jsonl"
                    .into(),
            ),
            ..Default::default()
        });
    }

    let Some(registrations) = live::registrations(config) else {
        return;
    };
    // session id → Some(true) when a process runs it, None when that cannot be told from here.
    let mut state: HashMap<String, Option<bool>> = HashMap::new();
    for registration in &registrations {
        match live::liveness(registration) {
            live::Liveness::Gone => {}
            live::Liveness::Unknown => {
                state.entry(registration.session_id.clone()).or_insert(None);
            }
            live::Liveness::Running => {
                state.insert(registration.session_id.clone(), Some(true));
                if known.insert(registration.session_id.clone()) {
                    listed.push(SessionInfo {
                        id: registration.session_id.clone(),
                        title: registration.name.clone(),
                        cwd: registration.cwd.clone(),
                        updated: registration
                            .updated_ms
                            .or(registration.started_ms)
                            .and_then(chrono::DateTime::from_timestamp_millis)
                            .map(|time| time.to_rfc3339()),
                        path: registration.file.to_string_lossy().into_owned(),
                        unreadable: Some(
                            "its process is running but has written no transcript yet".into(),
                        ),
                        ..Default::default()
                    });
                }
            }
        }
    }
    for info in listed.iter_mut() {
        let by_stem = stem_of(&info.path).and_then(|stem| state.get(&stem).copied());
        info.live = match state.get(&info.id).copied().or(by_stem) {
            Some(running) => running,
            None => Some(false),
        };
    }
}

/// Why no session file carries `id`: the history or a registration may still know the session.
fn why_not_found(id: &str, projects: &Path, roots: &StoreRoots) -> String {
    let plain = format!(
        "no Claude Code session with id {id:?} under {}",
        projects.display()
    );
    let (Some(config), false) = (roots.claude_config_dir.as_deref(), id.is_empty()) else {
        return plain;
    };
    if let Some(session) = super::claude_history::sessions(config)
        .into_iter()
        .find(|session| session.id.starts_with(id))
    {
        return format!(
            "Claude Code session {} has no transcript any more: it is known only from prompts in {}",
            session.id,
            super::claude_history::path(config).display()
        );
    }
    if super::claude_live::registrations(config)
        .is_some_and(|all| all.iter().any(|r| r.session_id.starts_with(id)))
    {
        return format!("Claude Code session {id:?} has written no transcript yet");
    }
    plain
}

/// What a session file's two ends say about it, without reading the whole file
/// (`super::json::peek_ends`): the head for the id, the directory and the first prompt, the tail
/// for the rows Claude Code appends as the conversation goes on (`custom-title`, `last-prompt`).
fn peek(file: &Path) -> SessionInfo {
    let mut info = SessionInfo::default();
    let mut titles = TitleBuilder::default();
    {
        let mut into = |row: Value| {
            let str_at = |p: &str| row.pointer(p).and_then(Value::as_str).map(str::to_owned);
            if info.id.is_empty() {
                info.id = str_at("/sessionId").unwrap_or_default();
            }
            if info.cwd.is_none() {
                info.cwd = str_at("/cwd");
            }
            if info.updated.is_none() {
                info.updated = str_at("/timestamp");
            }
            titles.observe(&row);
        };
        super::json::peek_ends(file, &mut into);
    }
    info.title = titles.pick();
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
