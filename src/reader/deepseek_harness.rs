//! The DeepSeek Harness reader (`dsh`, session format 4): the session logs under
//! `DSH_HOME/sessions/` (docs/harnesses/deepseek-harness.md).
//!
//! Facts this reader depends on:
//!
//! - a session is a folder `sessions/<project key>/<id>/` holding `session.v4.jsonl.zstd` (or
//!   `session.v4.jsonl` when the root is configured without compression); older generations
//!   (`session.v3.jsonl`) may sit beside it and are not read;
//! - the zstd file is independent frames with the content checksum on, the header line alone in
//!   frame 1, one frame per durable batch after it; a crash cuts the last frame ([`zstd_frames`]);
//! - line 1 is the header (`type: "session"`, `version`, `id`, `cwd`, `parentSession`, `origin`,
//!   `isSeeded`); every other line is an event with a 0-based `seq`, a `time` and a `data` body;
//! - what the model sees is the *surface*: the `system`, `developer`, `user`, `assistant` and `tool`
//!   events, each applied with its `surfaceOp`. `append` adds a node; `replace` puts the event in
//!   place of the nodes from `startSeq` through `endSeq` (a compaction's checkpoint, a pruned tool
//!   result). The reader walks the surface; nodes that were replaced stay as
//!   [`MetaKind::OffBranch`];
//! - only a `user/message` whose `source.kind` is `user` is something the user typed. The kinds
//!   `agent-message`, `subagent-settled` and `team-message` carry what another agent sent, and are
//!   user turns where they have text (as Codex's `agent_message` is); the rest (~40 kinds: time,
//!   runtime context, hooks, skills) is context the harness injected, kept as markers;
//! - `tool/call` repeats the `tool-call` part of the assistant message before it;
//! - an event type the reader does not know is skipped only when the event says `ignorable`;
//!   otherwise the session is refused, because skipping it could make the transcript wrong.
//!
//! A tail cut off by a crash is dropped and kept as a [`MetaKind::Unfinished`] entry that says so.

use super::json::{
    attachments, blocks_text, fallback_id, keep_unfinished, result_content, rows_from_lines,
    str_at, title_line,
};
use super::{Lookup, Reader, SessionInfo, zstd_frames};
use crate::model::{
    CanonicalTool, Entry, HarnessId, Kind, Meta, MetaKind, ModelRef, ParentKind, Reasoning,
    ResultContent, Session, StepEnd, ToolCall, ToolResult, Trigger,
};
use crate::roots::StoreRoots;
use anyhow::{Context, Result};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The DeepSeek Harness reader.
#[derive(Debug, Clone, Copy, Default)]
pub struct DeepSeekHarness;

/// The session format this reader reads.
const FORMAT: i64 = 4;

/// The events that make up the surface. Each carries a `surfaceOp`.
const SURFACE: [&str; 5] = [
    "system/message",
    "developer/message",
    "user/message",
    "assistant/message",
    "tool/result",
];

/// Every event type dsh 0.2.2-rc.1 knows (`KNOWN_SESSION_EVENT_TYPES`), and `team/mode`, which rc.2
/// adds. Another type is read only when the event is marked `ignorable`.
const KNOWN: [&str; 63] = [
    "agent-preset/selected",
    "agent/inbox/spliced",
    "approval/asked",
    "approval/decided",
    "approval/policy",
    "assistant/attempt",
    "assistant/message",
    "command/done",
    "command/run",
    "compaction/end",
    "compaction/prune",
    "compaction/start",
    "compaction/summary",
    "deliverables/presented",
    "developer/message",
    "feedback/message-delete",
    "feedback/message-put",
    "feedback/record",
    "goal/change",
    "hook/invoked",
    "hook/result",
    "image/offload",
    "job/registered",
    "job/settled",
    "llm/retry",
    "llm/retry-started",
    "model/selection",
    "permission/preset",
    "plan/mode",
    "request/context",
    "request/header",
    "sandbox/mode",
    "schedule/change",
    "session-log-deepseek/delivery-accepted",
    "session/end-seed",
    "session/title",
    "session/title-llm-request",
    "step/end",
    "step/start",
    "subagent/catalog",
    "subagent/descriptor",
    "subagent/model-selection-policy",
    "system/message",
    "team/member",
    "team/message/delivered",
    "team/message/queued",
    "team/mode",
    "team/task",
    "todo/write",
    "tool-workflow/agent-end",
    "tool-workflow/agent-start",
    "tool-workflow/run-end",
    "tool-workflow/run-start",
    "tool/call",
    "tool/ptc-dispatch",
    "tool/ptc-dispatch-start",
    "tool/result",
    "turn/end",
    "turn/start",
    "user/message",
    "web/deepseek-search-llm-request",
    "working-directory/change",
    "workspace/changes",
];

// ---------------------------------------------------------------------------------------------
// the file

/// The text of a log file: the decoded frames (or the plain lines), and what a crash left of the
/// tail, said in words.
struct Text {
    text: String,
    torn: Option<String>,
}

fn decode_text(bytes: &[u8], path: &Path) -> Result<Text> {
    if !zstd_frames::is_zstd(bytes) {
        let text = std::str::from_utf8(bytes).with_context(|| {
            format!(
                "{} is neither a Zstandard file nor UTF-8 text",
                path.display()
            )
        })?;
        return Ok(Text {
            text: text.to_string(),
            torn: None,
        });
    }
    let decoded = zstd_frames::decode(bytes)
        .with_context(|| format!("decode the DeepSeek Harness log {}", path.display()))?;
    let torn = decoded.torn.map(|at| {
        format!(
            "a Zstandard frame cut off by a crash: {} bytes from byte {at}, not read",
            bytes.len() - at
        )
    });
    let joined = decoded.frames.concat();
    let text = String::from_utf8(joined)
        .with_context(|| format!("{} holds a frame that is not UTF-8 text", path.display()))?;
    Ok(Text { text, torn })
}

/// Read one DeepSeek Harness session log, zstd or plain.
pub fn read_file(path: &Path) -> Result<Session> {
    let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let sha256 = crate::sha256::sha256_hex(&bytes);
    let Text { text, torn } = decode_text(&bytes, path)?;
    let (rows, unfinished) = rows_from_lines(&text, &path.display().to_string())?;
    let mut session = parse(&rows, Some(path))?;
    session.meta.source_sha256 = Some(sha256);
    keep_unfinished(&mut session, torn.or(unfinished));
    Ok(session)
}

// ---------------------------------------------------------------------------------------------
// log → Session

fn kind_of(event: &Value) -> &str {
    event
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
}

fn rfc3339(millis: i64) -> Option<String> {
    chrono::DateTime::from_timestamp_millis(millis)
        .map(|time| time.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
}

/// The header's relation to its parent: a subagent the parent started (which inherits the parent's
/// conversation when it was seeded), or a session another one is a copy of.
fn lineage(header: &Value) -> (Option<String>, Option<ParentKind>) {
    let Some(parent) = str_at(header, "/parentSession").filter(|p| !p.is_empty()) else {
        return (None, None);
    };
    let seeded = header.get("isSeeded").and_then(Value::as_bool) == Some(true);
    let kind = if str_at(header, "/origin").as_deref() == Some("subagent") {
        ParentKind::Spawn {
            call_id: None,
            agent: str_at(header, "/agentPreset").filter(|a| !a.is_empty()),
            inherits_context: seeded,
        }
    } else {
        ParentKind::fork()
    };
    (Some(parent), Some(kind))
}

/// The header of a log: its first record, checked against the format this reader reads.
fn header_of(rows: &[Value]) -> Result<&Value> {
    let header = rows
        .first()
        .filter(|row| kind_of(row) == "session")
        .context("not a DeepSeek Harness session log: the first line is not a session header")?;
    match header.get("version").and_then(Value::as_i64) {
        Some(FORMAT) => Ok(header),
        Some(other) => anyhow::bail!(
            "DeepSeek Harness session format {other} is not read: this reader knows format {FORMAT}"
        ),
        None => anyhow::bail!("the session header names no format version"),
    }
}

/// How the surface is built from the events: the nodes in surface order (indexes into the events),
/// and for each node that replaced others, the nodes it took the place of.
struct Surface {
    order: Vec<usize>,
    shadows: HashMap<usize, Vec<usize>>,
}

fn surface(events: &[&Value], seqs: &[u64]) -> Result<Surface> {
    let mut built = Surface {
        order: Vec::new(),
        shadows: HashMap::new(),
    };
    for (index, event) in events.iter().enumerate() {
        if !SURFACE.contains(&kind_of(event)) {
            continue;
        }
        match event.get("surfaceOp") {
            None | Some(Value::Null) => built.order.push(index),
            Some(Value::String(op)) if op == "append" => built.order.push(index),
            Some(Value::Object(op)) if op.get("op").and_then(Value::as_str) == Some("replace") => {
                let (Some(start), Some(end)) = (
                    op.get("startSeq").and_then(Value::as_u64),
                    op.get("endSeq").and_then(Value::as_u64),
                ) else {
                    anyhow::bail!("event {} replaces a range without naming it", seqs[index]);
                };
                let removed: Vec<usize> = built
                    .order
                    .iter()
                    .copied()
                    .filter(|&node| (start..=end).contains(&seqs[node]))
                    .collect();
                let at = removed
                    .first()
                    .and_then(|first| built.order.iter().position(|node| node == first))
                    .unwrap_or(built.order.len());
                built.order.retain(|node| !removed.contains(node));
                built.order.insert(at, index);
                built.shadows.insert(index, removed);
            }
            Some(_) => anyhow::bail!(
                "event {} has a surfaceOp this reader does not know",
                seqs[index]
            ),
        }
    }
    Ok(built)
}

/// The nodes `node` replaced, and the ones those replaced before: oldest first.
fn shadowed_by(shadows: &HashMap<usize, Vec<usize>>, node: usize, into: &mut Vec<usize>) {
    for &under in shadows.get(&node).map(Vec::as_slice).unwrap_or_default() {
        shadowed_by(shadows, under, into);
        into.push(under);
    }
}

/// What a non-surface event is, as a marker.
fn meta_kind(event_type: &str) -> MetaKind {
    match event_type {
        "model/selection" => MetaKind::ModelChange,
        "turn/end" => MetaKind::Idle,
        "command/run" | "command/done" => MetaKind::Command,
        _ => MetaKind::Internal,
    }
}

/// A `tool-call` part's arguments: the JSON the model wrote, or the text when it is not JSON.
fn arguments_of(part: &Value) -> Value {
    match part.get("arguments") {
        Some(Value::String(text)) if text.trim().is_empty() => Value::Object(Map::new()),
        Some(Value::String(text)) => {
            serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.clone()))
        }
        Some(other) => other.clone(),
        None => Value::Object(Map::new()),
    }
}

/// The input of a call in the canonical spelling: dsh's shell timeout is `timeoutMs`.
fn canonical_input(name: &str, input: Value) -> Value {
    let mut input = input;
    if matches!(name, "bash" | "pwsh")
        && let Value::Object(map) = &mut input
        && !map.contains_key("timeout")
        && let Some(timeout) = map.remove("timeoutMs")
    {
        map.insert("timeout".into(), timeout);
    }
    input
}

struct Builder<'a> {
    events: &'a [&'a Value],
    seqs: Vec<u64>,
    /// `tool/call` events by the call they log, attached to the step that made the call.
    logged_calls: HashMap<String, usize>,
    /// `compaction/summary` events by `compactionId`.
    summaries: HashMap<String, usize>,
    entries: Vec<Entry>,
    /// The `Results` entry the next tool result joins: parallel calls are answered together.
    open_results: Option<usize>,
    /// The steps read so far: the turn each is in, and its entry.
    steps: Vec<(u64, usize)>,
    models: Vec<ModelRef>,
    /// Events already kept in an entry's `raw` (a step's `tool/call`s, a checkpoint's summary), not
    /// to be kept again as markers.
    taken: Vec<bool>,
}

impl Builder<'_> {
    fn id(&self, index: usize) -> String {
        self.seqs[index].to_string()
    }

    fn marker(&mut self, index: usize, kind: MetaKind) {
        self.entries.push(Entry {
            id: self.id(index),
            kind: Kind::Meta { kind },
            raw: vec![self.events[index].clone()],
        });
    }

    fn note_model(&mut self, model: ModelRef) {
        if !self.models.contains(&model) {
            self.models.push(model);
        }
    }

    /// The events a node's trailing markers: everything between it and the next surface node.
    fn trailing(&mut self, owned: &[usize]) {
        for &index in owned {
            if self.taken[index] {
                continue;
            }
            let event_type = kind_of(self.events[index]);
            if event_type == "model/selection"
                && let Some(model) = model_selected(self.events[index])
            {
                self.note_model(model);
            }
            self.marker(index, meta_kind(event_type));
        }
    }

    fn user(&mut self, index: usize) {
        let event = self.events[index];
        let data = event.get("data").cloned().unwrap_or(Value::Null);
        let source_kind = str_at(&data, "/source/kind").unwrap_or_default();
        let parts = data
            .get("content")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        match source_kind.as_str() {
            // What the user typed, or what another agent of the tree sent this one (a child's
            // report, a parent's task) where its text is readable: a user turn, as the Codex
            // reader reads `agent_message`.
            kind if kind == "user"
                || (matches!(kind, "agent-message" | "subagent-settled" | "team-message")
                    && !blocks_text(&parts).trim().is_empty()) =>
            {
                self.open_results = None;
                self.entries.push(Entry {
                    id: self.id(index),
                    kind: Kind::User {
                        text: blocks_text(&parts),
                        attachments: attachments(&parts),
                    },
                    raw: vec![event.clone()],
                });
            }
            "compact-checkpoint" => {
                self.open_results = None;
                let summary_event = str_at(&data, "/source/compactionId")
                    .and_then(|id| self.summaries.get(&id).copied())
                    .map(|at| self.events[at]);
                let summary = summary_event
                    .and_then(|summary| summary.pointer("/data/summary"))
                    .and_then(Value::as_array)
                    .map(|parts| blocks_text(parts))
                    .filter(|text| !text.is_empty())
                    .unwrap_or_else(|| blocks_text(&parts));
                let tokens_before = summary_event
                    .and_then(|summary| summary.pointer("/data/shadowedTokenCount"))
                    .and_then(Value::as_u64);
                let trigger = if data.pointer("/source/sourceCommandId").is_some() {
                    Trigger::Manual
                } else {
                    Trigger::Unknown
                };
                let mut raw = vec![event.clone()];
                raw.extend(summary_event.cloned());
                self.entries.push(Entry {
                    id: self.id(index),
                    kind: Kind::Compaction {
                        summary,
                        kept_from: None,
                        trigger,
                        tokens_before,
                    },
                    raw,
                });
            }
            _ => self.marker(index, MetaKind::Internal),
        }
    }

    fn assistant(&mut self, index: usize) {
        self.open_results = None;
        let event = self.events[index];
        let message = event
            .pointer("/data/message")
            .cloned()
            .unwrap_or(Value::Null);
        let model = str_at(&message, "/source/model").map(|id| ModelRef {
            provider: str_at(&message, "/source/provider"),
            id,
        });
        if let Some(model) = &model {
            self.note_model(model.clone());
        }
        let mut raw = vec![event.clone()];
        let (mut reasoning, mut text, mut calls) = (Vec::new(), Vec::new(), Vec::new());
        for part in message
            .get("content")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            match part.get("type").and_then(Value::as_str) {
                Some("text") => text.extend(str_at(part, "/text").filter(|t| !t.is_empty())),
                Some("reasoning") => reasoning.push(Reasoning {
                    text: str_at(part, "/text").unwrap_or_default(),
                    opaque: None,
                }),
                Some("tool-call") => {
                    let id = str_at(part, "/id").unwrap_or_default();
                    let name = str_at(part, "/name").unwrap_or_default();
                    if let Some(&logged) = self.logged_calls.get(&id) {
                        self.taken[logged] = true;
                        raw.push(self.events[logged].clone());
                    }
                    calls.push(ToolCall {
                        canonical: Some(CanonicalTool::from_dsh(&name)),
                        input: canonical_input(&name, arguments_of(part)),
                        id,
                        source_name: name,
                    });
                }
                _ => {}
            }
        }
        let end = if event.pointer("/data/interrupted").and_then(Value::as_bool) == Some(true) {
            StepEnd::Aborted
        } else if calls.is_empty() {
            StepEnd::Completed
        } else {
            StepEnd::ToolUse
        };
        if let Some(turn) = event.pointer("/data/turn").and_then(Value::as_u64) {
            self.steps.push((turn, self.entries.len()));
        }
        self.entries.push(Entry {
            id: self.id(index),
            kind: Kind::Step {
                model,
                reasoning,
                text,
                calls,
                end,
            },
            raw,
        });
    }

    fn result(&mut self, index: usize) {
        let event = self.events[index];
        let message = event
            .pointer("/data/message")
            .cloned()
            .unwrap_or(Value::Null);
        let parts = message
            .get("content")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let result = ToolResult {
            call_id: str_at(&message, "/toolCallId")
                .or_else(|| str_at(&message, "/source/callId"))
                .unwrap_or_default(),
            content: if parts.is_empty() {
                ResultContent::Text(String::new())
            } else {
                result_content(&parts)
            },
            is_error: message.get("isError").and_then(Value::as_bool) == Some(true)
                || event.pointer("/data/error").is_some_and(|e| !e.is_null()),
            truncated_by_source: false,
        };
        if let Some(open) = self.open_results
            && let Kind::Results { results } = &mut self.entries[open].kind
        {
            results.push(result);
            self.entries[open].raw.push(event.clone());
            return;
        }
        self.open_results = Some(self.entries.len());
        self.entries.push(Entry {
            id: self.id(index),
            kind: Kind::Results {
                results: vec![result],
            },
            raw: vec![event.clone()],
        });
    }

    /// A surface node that still holds its place.
    fn live(&mut self, index: usize) {
        match kind_of(self.events[index]) {
            "system/message" => self.marker(index, MetaKind::SystemPrompt),
            "developer/message" => self.marker(index, MetaKind::Internal),
            "user/message" => self.user(index),
            "assistant/message" => self.assistant(index),
            _ => self.result(index),
        }
    }
}

fn model_selected(event: &Value) -> Option<ModelRef> {
    Some(ModelRef {
        provider: str_at(event, "/data/provider"),
        id: str_at(event, "/data/model")?,
    })
}

/// Read a log: the header and its events, as `read_file` splits them.
pub fn parse(rows: &[Value], source_path: Option<&Path>) -> Result<Session> {
    let header = header_of(rows)?;
    let events: Vec<&Value> = rows[1..].iter().collect();
    let mut seqs = Vec::with_capacity(events.len());
    for (index, event) in events.iter().enumerate() {
        let event_type = kind_of(event);
        if !KNOWN.contains(&event_type)
            && event.get("ignorable").and_then(Value::as_bool) != Some(true)
        {
            anyhow::bail!(
                "event {} has the type {event_type:?}, which this reader does not know and the log \
                 does not mark ignorable: the session is not read, because skipping the event could \
                 make the transcript wrong",
                event
                    .get("seq")
                    .and_then(Value::as_u64)
                    .unwrap_or(index as u64)
            );
        }
        seqs.push(
            event
                .get("seq")
                .and_then(Value::as_u64)
                .unwrap_or(index as u64),
        );
    }
    let surface = surface(&events, &seqs)?;

    // The events that sit after a surface node in the file belong to it, and move with it.
    let mut owned: HashMap<Option<usize>, Vec<usize>> = HashMap::new();
    let mut owner = None;
    for (index, event) in events.iter().enumerate() {
        if SURFACE.contains(&kind_of(event)) {
            owner = Some(index);
        } else {
            owned.entry(owner).or_default().push(index);
        }
    }

    let mut logged_calls = HashMap::new();
    let mut summaries = HashMap::new();
    for (index, event) in events.iter().enumerate() {
        match kind_of(event) {
            "tool/call" => {
                if let Some(id) = str_at(event, "/data/callId") {
                    logged_calls.entry(id).or_insert(index);
                }
            }
            "compaction/summary" => {
                if let Some(id) = str_at(event, "/data/compactionId") {
                    summaries.insert(id, index);
                }
            }
            _ => {}
        }
    }
    // The summary a checkpoint cites is kept in the checkpoint's own raw, not a second time.
    let mut taken = vec![false; events.len()];
    for event in &events {
        if kind_of(event) == "user/message"
            && str_at(event, "/data/source/kind").as_deref() == Some("compact-checkpoint")
            && let Some(id) = str_at(event, "/data/source/compactionId")
            && let Some(&at) = summaries.get(&id)
        {
            taken[at] = true;
        }
    }
    let mut builder = Builder {
        events: &events,
        seqs,
        logged_calls,
        summaries,
        entries: Vec::new(),
        open_results: None,
        steps: Vec::new(),
        models: Vec::new(),
        taken,
    };

    // Events before the first surface node, then every live node with the nodes it replaced.
    let leading = owned.remove(&None).unwrap_or_default();
    builder.trailing(&leading);
    for &node in &surface.order {
        let mut shadowed = Vec::new();
        shadowed_by(&surface.shadows, node, &mut shadowed);
        for under in shadowed {
            builder.marker(under, MetaKind::OffBranch);
            let trailing = owned.remove(&Some(under)).unwrap_or_default();
            builder.trailing(&trailing);
        }
        builder.live(node);
        let trailing = owned.remove(&Some(node)).unwrap_or_default();
        builder.trailing(&trailing);
    }
    let Builder {
        mut entries,
        steps,
        models,
        ..
    } = builder;

    // A turn that ended in error or was aborted says so on its last step.
    for event in &events {
        if kind_of(event) != "turn/end" {
            continue;
        }
        let Some(turn) = event.pointer("/data/turn").and_then(Value::as_u64) else {
            continue;
        };
        let ended = match str_at(event, "/data/reason/kind").as_deref() {
            Some("error") => StepEnd::Error,
            Some("aborted" | "interrupted") => StepEnd::Aborted,
            _ => continue,
        };
        if let Some(&(_, at)) = steps.iter().rev().find(|(t, _)| *t == turn)
            && let Kind::Step { end, .. } = &mut entries[at].kind
            && *end == StepEnd::Completed
        {
            *end = ended;
        }
    }
    // A compaction kept what follows it: its first conversation entry.
    for at in 0..entries.len() {
        if matches!(entries[at].kind, Kind::Compaction { .. }) {
            let kept = entries[at + 1..]
                .iter()
                .find(|e| !matches!(e.kind, Kind::Meta { .. } | Kind::Compaction { .. }))
                .map(|e| e.id.clone());
            if let Kind::Compaction { kept_from, .. } = &mut entries[at].kind {
                *kept_from = kept;
            }
        }
    }

    let title = events
        .iter()
        .rev()
        .filter(|event| kind_of(event) == "session/title")
        .find_map(|event| str_at(event, "/data/title").filter(|title| !title.trim().is_empty()));
    let last_time = events
        .iter()
        .filter_map(|event| event.get("time").and_then(Value::as_i64))
        .max();
    let (parent, parent_kind) = lineage(header);
    Ok(Session {
        meta: Meta {
            harness: HarnessId::DeepSeekHarness,
            harness_version: format!("format {FORMAT}"),
            id: str_at(header, "/id").unwrap_or_else(|| fallback_id("session", 0)),
            cwd: str_at(header, "/cwd").filter(|cwd| !cwd.is_empty()),
            title,
            created: header
                .get("createdAt")
                .and_then(Value::as_i64)
                .and_then(rfc3339),
            updated: last_time.and_then(rfc3339),
            models,
            git_branch: None,
            parent,
            parent_kind,
            source_path: source_path.map(|path| path.to_string_lossy().into_owned()),
            source_sha256: None,
        },
        entries,
    })
}

// ---------------------------------------------------------------------------------------------
// the store

/// One session folder's log, as the listing found it.
struct Log {
    /// The log to read: the format-4 file, or the older generation that is all the folder holds.
    file: PathBuf,
    /// The folder's own name, which is the session id encoded.
    folder: String,
    /// The format of the log when it is not the one this reader reads.
    other_format: Option<String>,
}

/// `encodeSegment` undone: `~` and four hex digits stand for a UTF-16 unit.
fn decode_segment(folder: &str) -> String {
    let mut units: Vec<u16> = Vec::new();
    let mut rest = folder;
    while let Some(c) = rest.chars().next() {
        if c == '~'
            && let Some(hex) = rest.get(1..5)
            && let Ok(unit) = u16::from_str_radix(hex, 16)
        {
            units.push(unit);
            rest = &rest[5..];
            continue;
        }
        let mut buffer = [0u16; 2];
        units.extend_from_slice(c.encode_utf16(&mut buffer));
        rest = &rest[c.len_utf8()..];
    }
    String::from_utf16_lossy(&units)
}

/// Every session folder under `sessions/<project>/`: the format-4 log, else the generation that is
/// there. Files directly in a project folder are the obsolete flat layout and are not sessions.
fn logs(sessions: &Path) -> Result<Vec<Log>> {
    let mut found = Vec::new();
    let projects = match std::fs::read_dir(sessions) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(found),
        Err(error) => return Err(error).with_context(|| format!("read {}", sessions.display())),
    };
    for project in projects {
        let project = project?;
        if !project.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let Ok(folders) = std::fs::read_dir(project.path()) else {
            continue;
        };
        for folder in folders.flatten() {
            if !folder.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let dir = folder.path();
            let name = folder.file_name().to_string_lossy().into_owned();
            let current = ["session.v4.jsonl.zstd", "session.v4.jsonl"]
                .into_iter()
                .map(|file| dir.join(file))
                .find(|file| file.is_file());
            if let Some(file) = current {
                found.push(Log {
                    file,
                    folder: name,
                    other_format: None,
                });
                continue;
            }
            // Only an older generation: `session.jsonl` (v0) or `session.v<N>.jsonl[.zstd]`.
            let older = std::fs::read_dir(&dir).ok().and_then(|files| {
                files.flatten().find_map(|file| {
                    let file_name = file.file_name().to_string_lossy().into_owned();
                    let stem = file_name.strip_suffix(".zstd").unwrap_or(&file_name);
                    let generation = stem.strip_prefix("session")?.strip_suffix(".jsonl")?;
                    let version = match generation {
                        "" => "0".to_string(),
                        other => other.strip_prefix(".v")?.to_string(),
                    };
                    Some((file.path(), version))
                })
            });
            if let Some((file, version)) = older {
                found.push(Log {
                    file,
                    folder: name,
                    other_format: Some(version),
                });
            }
        }
    }
    found.sort_by(|a, b| a.file.cmp(&b.file));
    Ok(found)
}

/// What a log's lines say about it without building the session: the header, the last title and
/// the first thing the user typed. Only the lines that can name one are decoded.
fn peek(text: &str) -> (Option<Value>, Option<String>, Option<String>) {
    let mut lines = text.lines();
    let header = lines
        .next()
        .and_then(|line| serde_json::from_str::<Value>(line.trim_start_matches('\u{feff}')).ok());
    let (mut title, mut first_prompt) = (None, None);
    for line in lines {
        if line.contains("\"type\":\"session/title\"")
            && let Ok(event) = serde_json::from_str::<Value>(line)
            && let Some(found) = str_at(&event, "/data/title").filter(|t| !t.trim().is_empty())
        {
            title = Some(found);
        } else if first_prompt.is_none()
            && line.contains("\"type\":\"user/message\"")
            && line.contains("\"source\":{\"kind\":\"user\"")
            && let Ok(event) = serde_json::from_str::<Value>(line)
            && str_at(&event, "/data/source/kind").as_deref() == Some("user")
            && let Some(parts) = event.pointer("/data/content").and_then(Value::as_array)
        {
            first_prompt = Some(blocks_text(parts)).filter(|text| !text.is_empty());
        }
    }
    (header, title, first_prompt)
}

fn listed(log: &Log) -> SessionInfo {
    let mut info = SessionInfo {
        id: decode_segment(&log.folder),
        path: log.file.to_string_lossy().into_owned(),
        updated: super::json::modified_rfc3339(&log.file),
        ..SessionInfo::default()
    };
    if let Some(version) = &log.other_format {
        info.unreadable = Some(format!(
            "the folder holds only a format {version} log; this reader knows format {FORMAT}"
        ));
        return info;
    }
    let text = std::fs::read(&log.file)
        .with_context(|| format!("read {}", log.file.display()))
        .and_then(|bytes| decode_text(&bytes, &log.file));
    let text = match text {
        Ok(text) => text.text,
        Err(error) => {
            info.unreadable = Some(format!("{error:#}"));
            return info;
        }
    };
    let (header, title, first_prompt) = peek(&text);
    let Some(header) = header.filter(|h| kind_of(h) == "session") else {
        info.unreadable = Some("the log has no session header".into());
        return info;
    };
    if let Some(version) = header.get("version").and_then(Value::as_i64)
        && version != FORMAT
    {
        info.unreadable = Some(format!(
            "the log is format {version}; this reader knows format {FORMAT}"
        ));
    }
    if let Some(id) = str_at(&header, "/id").filter(|id| !id.is_empty()) {
        info.id = id;
    }
    info.cwd = str_at(&header, "/cwd").filter(|cwd| !cwd.is_empty());
    (info.parent, info.parent_kind) = lineage(&header);
    info.title = title_line(title.as_deref()).or_else(|| title_line(first_prompt.as_deref()));
    info
}

fn sessions(roots: &StoreRoots, what: &str) -> Result<PathBuf> {
    roots.dsh_sessions().ok_or_else(|| {
        Lookup::NoStore(format!(
            "DSH_HOME is not set: no DeepSeek Harness store to {what}"
        ))
        .into()
    })
}

/// A log's size and modification time (nanoseconds since 1970): what tells that it changed.
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

/// A log written this recently may still be growing, so what was read of it is not remembered.
const SETTLED_AFTER: std::time::Duration = std::time::Duration::from_secs(2);

/// What the cache keeps of a log: its stamp, and what a listing read from it.
fn to_entry((size, modified): Stamp, info: &SessionInfo) -> Value {
    let mut entry = Map::new();
    entry.insert("s".into(), size.into());
    entry.insert("m".into(), modified.into());
    for (key, value) in [
        ("i", Some(&info.id)),
        ("t", info.title.as_ref()),
        ("c", info.cwd.as_ref()),
        ("p", info.parent.as_ref()),
        ("u", info.unreadable.as_ref()),
    ] {
        if let Some(value) = value {
            entry.insert(key.into(), value.clone().into());
        }
    }
    if let Some(kind) = info
        .parent_kind
        .as_ref()
        .and_then(|kind| serde_json::to_value(kind).ok())
    {
        entry.insert("k".into(), kind);
    }
    entry.into()
}

/// What the cache kept for `log`, when it was kept for this very file.
fn from_entry(entry: &Value, (size, modified): Stamp, log: &Log) -> Option<SessionInfo> {
    if entry.get("s")?.as_u64()? != size || entry.get("m")?.as_u64()? != modified {
        return None;
    }
    let text = |key: &str| entry.get(key).and_then(Value::as_str).map(str::to_owned);
    Some(SessionInfo {
        id: text("i")?,
        title: text("t"),
        cwd: text("c"),
        updated: super::json::modified_rfc3339(&log.file),
        path: log.file.to_string_lossy().into_owned(),
        parent: text("p"),
        parent_kind: entry
            .get("k")
            .and_then(|kind| serde_json::from_value(kind.clone()).ok()),
        unreadable: text("u"),
        ..SessionInfo::default()
    })
}

/// Every session of the store, with each log's answer taken from `cache` (a folder) where the log
/// is as it was, else from the log: a listing decodes a log only to find its title and its first
/// prompt, and a log that has not changed is not decoded again. `None` is no cache.
pub fn list_all_cached(roots: &StoreRoots, cache: Option<&Path>) -> Result<Vec<SessionInfo>> {
    let dir = sessions(roots, "list")?;
    listed_all(&dir, cache)
}

fn listed_all(dir: &Path, cache: Option<&Path>) -> Result<Vec<SessionInfo>> {
    let found = logs(dir)?;
    let name = crate::cache::table_name("dsh-list", &dir.to_string_lossy());
    let remembered: Map<String, Value> = cache
        .map(|folder| crate::cache::load_table(folder, &name))
        .unwrap_or_default();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |age| u64::try_from(age.as_nanos()).unwrap_or(u64::MAX));
    let settled = u64::try_from(SETTLED_AFTER.as_nanos()).unwrap_or(u64::MAX);
    let mut table = Map::new();
    let mut out = Vec::with_capacity(found.len());
    for log in &found {
        let key = log.file.to_string_lossy().into_owned();
        let stamp = stamp(&log.file);
        let info = stamp
            .and_then(|stamp| from_entry(remembered.get(&key)?, stamp, log))
            .unwrap_or_else(|| listed(log));
        if let Some(stamp) = stamp
            && stamp.1.saturating_add(settled) <= now
        {
            table.insert(key, to_entry(stamp, &info));
        }
        out.push(info);
    }
    if let Some(folder) = cache
        && table != remembered
    {
        crate::cache::store_table(folder, &name, &table);
    }
    Ok(out)
}

/// Every session of the store, newest first. `roots_only` leaves out the subagents a session
/// started, which a picker does not show; a fork of a session is a session of its own.
fn list_in(roots: &StoreRoots, roots_only: bool) -> Result<Vec<SessionInfo>> {
    let mut out: Vec<SessionInfo> = list_all_cached(roots, crate::cache::dir().as_deref())?
        .into_iter()
        .filter(|info| !roots_only || !matches!(info.parent_kind, Some(ParentKind::Spawn { .. })))
        .collect();
    out.sort_by(|a, b| b.updated.cmp(&a.updated).then_with(|| a.id.cmp(&b.id)));
    Ok(out)
}

/// The sessions the store holds, and how many of them another session started or copied.
pub fn store_counts(sessions: &Path) -> Result<(usize, usize)> {
    let all = listed_all(sessions, crate::cache::dir().as_deref())?;
    let children = all.iter().filter(|info| info.parent_kind.is_some()).count();
    Ok((all.len(), children))
}

impl Reader for DeepSeekHarness {
    fn harness(&self) -> HarnessId {
        HarnessId::DeepSeekHarness
    }

    fn list(&self, roots: &StoreRoots) -> Result<Vec<SessionInfo>> {
        list_in(roots, true)
    }

    fn list_all(&self, roots: &StoreRoots) -> Result<Vec<SessionInfo>> {
        list_in(roots, false)
    }

    /// The id is matched against the folder names, which dsh requires to be the id encoded: only
    /// the log asked for is opened, never the others.
    fn read(&self, id: &str, roots: &StoreRoots) -> Result<Session> {
        let dir = sessions(roots, "read")?;
        let named: Vec<(String, Log)> = logs(&dir)?
            .into_iter()
            .map(|log| (decode_segment(&log.folder), log))
            .collect();
        // The id as dsh writes it, or what follows `session-`: the UUID a user copies.
        let starts = |name: &str| {
            name.starts_with(id)
                || name
                    .strip_prefix("session-")
                    .is_some_and(|rest| rest.starts_with(id))
        };
        let exact: Vec<&(String, Log)> = named.iter().filter(|(name, _)| name == id).collect();
        let found: Vec<&(String, Log)> = if exact.is_empty() {
            named.iter().filter(|(name, _)| starts(name)).collect()
        } else {
            exact
        };
        match found.as_slice() {
            [(name, log)] => {
                if let Some(version) = &log.other_format {
                    anyhow::bail!(
                        "DeepSeek Harness session {name} cannot be read: the folder holds only a \
                         format {version} log; this reader knows format {FORMAT}"
                    );
                }
                read_file(&log.file)
            }
            [] => Err(Lookup::NotFound(format!("no DeepSeek Harness session {id:?}")).into()),
            several => Err(Lookup::Ambiguous(format!(
                "{id:?} starts {} DeepSeek Harness sessions ({})",
                several.len(),
                several
                    .iter()
                    .take(5)
                    .map(|(name, _)| name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
            .into()),
        }
    }
}

/// Who produced a user entry's event: a `user/message` whose `source.kind` is `user` is what a
/// person typed; another agent's message (`agent-message`, `subagent-settled`, `team-message`) or
/// injected context is not; anything else is not known.
pub(crate) fn user_origin(event: &Value) -> crate::reader::origin::Origin {
    use crate::reader::origin::Origin;
    if str_at(event, "/type").as_deref() != Some("user/message") {
        return Origin::Unknown;
    }
    match str_at(event, "/data/source/kind").as_deref() {
        Some("user") => Origin::Typed,
        Some("agent-message" | "subagent-settled" | "team-message") => {
            Origin::Injected("dsh_agent_message")
        }
        Some(_) => Origin::Injected("dsh_injected_context"),
        None => Origin::Unknown,
    }
}
