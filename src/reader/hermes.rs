//! The Hermes Agent reader (Hermes v0.22.5 and later, `state.db` schema 32 and later).
//!
//! Facts this reader depends on (docs/harnesses/hermes-agent.md):
//!
//! - the store is `HERMES_HOME/state.db`, SQLite in WAL mode; it is opened read-only, never
//!   `immutable`, because a gateway may be writing;
//! - a session's conversation is its `messages` rows in `id` order (never by time), the rows with
//!   `active = 1`; `active = 0` rows are history the harness archived (an in-place compaction) or
//!   undid (a rewind), kept here as [`MetaKind::OffBranch`];
//! - `content` is text, or `"\x00json:"` + JSON for a list of parts, and a cell may be NULL, a number
//!   or a BLOB; `tool_calls` is the OpenAI array and `arguments` is a JSON string;
//! - a compaction summary is a stored row with `_compressed_summary = 1`; the default compaction is in
//!   place under the same session id, a legacy one rotates the id and ends the old session with
//!   `end_reason = 'compression'`;
//! - a subagent (`delegate_task`) is a session with `model_config._delegate_from`, a `/branch` one
//!   with `model_config._branched_from`; its parent's rows are copied into the branch;
//! - `hermes sessions export` writes one JSON object per session: the row, with its `messages`
//!   decoded the way this reader decodes them. [`read_file`] reads that file.
//!
//! The schema number is not a shape contract (columns are added under the same number), so the gate
//! is the number from 32 on plus the columns this reader needs.

use super::json::{attachments, blocks_text, result_content, str_at, title_line};
use super::{Lookup, Reader, SessionInfo};
use crate::model::{
    CanonicalTool, ChatReasoning, Entry, HarnessId, Kind, Meta, MetaKind, ModelRef, Opaque,
    ParentKind, Protocol, Reasoning, ResultContent, Session, StepEnd, ToolCall, ToolResult,
    Trigger,
};
use crate::roots::StoreRoots;
use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags, types::ValueRef};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// The Hermes Agent reader.
#[derive(Debug, Clone, Copy, Default)]
pub struct Hermes;

/// The oldest `schema_version` read: v0.22.5 declares 32, and 33 (v0.23.1) is data-only.
const MIN_SCHEMA: i64 = 32;

/// What `messages` rows are decoded from, in the order the export keeps them.
const CONTENT_JSON_PREFIX: &str = "\u{0}json:";

/// Columns a message row is stored in that the harness's own export leaves out.
const NOT_EXPORTED: [&str; 2] = ["display_identity", "display_order"];

/// Message columns that hold JSON text.
const JSON_COLUMNS: [&str; 5] = [
    "tool_calls",
    "display_metadata",
    "reasoning_details",
    "codex_reasoning_items",
    "codex_message_items",
];

/// The columns a session row must have for this reader to say what it is.
const REQUIRED_SESSION_COLUMNS: [&str; 6] = [
    "id",
    "source",
    "parent_session_id",
    "model_config",
    "started_at",
    "end_reason",
];

/// The columns a message row must have.
const REQUIRED_MESSAGE_COLUMNS: [&str; 8] = [
    "id",
    "session_id",
    "role",
    "content",
    "tool_call_id",
    "tool_calls",
    "timestamp",
    "active",
];

/// The session columns a listing reads, where the store has them.
const LIST_COLUMNS: [&str; 13] = [
    "id",
    "source",
    "created_source",
    "model_config",
    "parent_session_id",
    "started_at",
    "ended_at",
    "end_reason",
    "cwd",
    "title",
    "last_activity_at",
    "archived",
    "auto_archived",
];

// ---------------------------------------------------------------------------------------------
// export file

/// Read a `hermes sessions export` file: one session, as one JSON line.
pub fn read_file(path: &Path) -> Result<Session> {
    let mut head = [0u8; 16];
    let read = std::fs::File::open(path)
        .and_then(|mut file| std::io::Read::read(&mut file, &mut head))
        .with_context(|| format!("read {}", path.display()))?;
    if head[..read].starts_with(b"SQLite format 3") {
        anyhow::bail!(
            "{} is a SQLite database: point HERMES_HOME at its folder and name the session by id",
            path.display()
        );
    }
    let jsonl = super::json::read_jsonl(path)?;
    let source_sha256 = jsonl.sha256.wait();
    let [doc] = jsonl.rows.as_slice() else {
        let ids: Vec<String> = jsonl
            .rows
            .iter()
            .filter_map(|row| str_at(row, "/id"))
            .collect();
        anyhow::bail!(
            "{} holds {} sessions ({}): read one session per file",
            path.display(),
            jsonl.rows.len(),
            ids.join(", ")
        );
    };
    let mut session = parse(doc, Some(path), "unknown", None)?;
    session.meta.source_sha256 = Some(source_sha256);
    super::json::keep_unfinished(&mut session, jsonl.unfinished);
    Ok(session)
}

// ---------------------------------------------------------------------------------------------
// session document → Session

/// How a session came from another one, read from its own row.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Lineage {
    Root,
    /// `/branch`: a copy of the parent's conversation, continued on its own.
    Fork(String),
    /// `delegate_task`: a subagent. The parent is `None` for an orphaned one.
    Spawn(Option<String>),
    /// A legacy rotating compaction: the same conversation under a new id.
    Continuation(String),
}

/// The lineage a session row states. `parent_end` is the `end_reason` of its `parent_session_id`
/// row, where the caller has that row.
fn lineage(session: &Value, parent_end: Option<&str>) -> Lineage {
    let config = json_value(session.get("model_config"));
    let marker = |key: &str| {
        str_at(&config, &format!("/{key}")).filter(|v| !v.is_empty() && v != "__orphaned__")
    };
    let parent = str_at(session, "/parent_session_id").filter(|p| !p.is_empty());
    let marked_delegate = str_at(&config, "/_delegate_from").is_some_and(|v| !v.is_empty());
    if marked_delegate || str_at(session, "/source").as_deref() == Some("subagent") {
        return Lineage::Spawn(marker("_delegate_from").or(parent));
    }
    if let Some(source) = marker("_branched_from") {
        return Lineage::Fork(source);
    }
    match (parent, parent_end) {
        (Some(parent), Some("branched")) => Lineage::Fork(parent),
        (Some(parent), Some("compression")) => Lineage::Continuation(parent),
        _ => Lineage::Root,
    }
}

fn parent_of(lineage: &Lineage) -> (Option<String>, Option<ParentKind>) {
    match lineage {
        Lineage::Fork(parent) => (Some(parent.clone()), Some(ParentKind::fork())),
        Lineage::Spawn(Some(parent)) => (Some(parent.clone()), Some(ParentKind::spawn())),
        _ => (None, None),
    }
}

/// A JSON column, whether the document holds it parsed (the export) or as text (the store).
fn json_value(value: Option<&Value>) -> Value {
    match value {
        Some(Value::String(text)) => serde_json::from_str(text).unwrap_or(Value::Null),
        Some(other) => other.clone(),
        None => Value::Null,
    }
}

fn seconds(value: Option<&Value>) -> Option<f64> {
    value.and_then(Value::as_f64).filter(|s| s.is_finite())
}

fn rfc3339(seconds: f64) -> Option<String> {
    chrono::DateTime::from_timestamp_millis((seconds * 1000.0).round() as i64)
        .map(|time| time.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
}

/// The wire protocol the session's model spoke, from the runtime the gateway recorded.
fn session_protocol(session: &Value) -> Option<Protocol> {
    let config = json_value(session.get("model_config"));
    match str_at(&config, "/gateway_runtime/api_mode")?.as_str() {
        "chat_completions" => Some(Protocol::OpenaiChat),
        "codex_responses" => Some(Protocol::OpenaiResponses),
        "anthropic_messages" => Some(Protocol::AnthropicMessages),
        other => Some(Protocol::Other(other.to_string())),
    }
}

fn session_model(session: &Value) -> Option<ModelRef> {
    let id = str_at(session, "/model").filter(|m| !m.is_empty())?;
    let config = json_value(session.get("model_config"));
    Some(ModelRef {
        provider: str_at(&config, "/gateway_runtime/provider")
            .or_else(|| str_at(session, "/billing_provider"))
            .filter(|p| !p.is_empty()),
        id,
    })
}

/// Read a session document: the session row with its `messages`.
pub fn parse(
    doc: &Value,
    source_path: Option<&Path>,
    harness_version: &str,
    parent_end: Option<&str>,
) -> Result<Session> {
    let Some(messages) = doc.get("messages").and_then(Value::as_array) else {
        anyhow::bail!("not a Hermes session document: no messages array");
    };
    let model = session_model(doc);
    let mut builder = Builder {
        model: model.clone(),
        protocol: session_protocol(doc),
        ..Builder::default()
    };
    for (index, row) in messages.iter().enumerate() {
        builder.push(index, row, messages.get(index + 1..).unwrap_or_default());
    }
    builder.flush_results();
    let (parent, parent_kind) = parent_of(&lineage(doc, parent_end));
    let last_message = messages
        .iter()
        .rev()
        .find_map(|row| seconds(row.get("timestamp")));
    Ok(Session {
        meta: Meta {
            harness: HarnessId::Hermes,
            harness_version: harness_version.to_string(),
            id: str_at(doc, "/id").unwrap_or_else(|| "unknown".into()),
            cwd: str_at(doc, "/cwd").filter(|c| !c.is_empty()),
            title: str_at(doc, "/title").filter(|t| !t.is_empty()),
            created: seconds(doc.get("started_at")).and_then(rfc3339),
            updated: seconds(doc.get("last_activity_at"))
                .or_else(|| seconds(doc.get("ended_at")))
                .or(last_message)
                .and_then(rfc3339),
            models: model.into_iter().collect(),
            git_branch: str_at(doc, "/git_branch").filter(|b| !b.is_empty()),
            parent,
            parent_kind,
            source_path: source_path.map(|p| p.to_string_lossy().into_owned()),
            source_sha256: None,
        },
        entries: builder.entries,
    })
}

#[derive(Default)]
struct Builder {
    entries: Vec<Entry>,
    model: Option<ModelRef>,
    protocol: Option<Protocol>,
    /// Tool rows read since the last other row: one `Results` entry, as the harness sent them to
    /// the model together.
    results: Vec<ToolResult>,
    results_id: Option<String>,
    results_raw: Vec<Value>,
}

/// Who produced a user entry's row: a `user` row with no scaffolding `display_kind` is what a
/// person typed; a scaffolding one Hermes injected; anything else is not known.
pub(crate) fn user_origin(row: &Value) -> crate::reader::origin::Origin {
    use crate::reader::origin::Origin;
    if str_at(row, "/role").as_deref() != Some("user") {
        return Origin::Unknown;
    }
    match scaffolding(&str_at(row, "/display_kind").unwrap_or_default()) {
        None => Origin::Typed,
        Some(_) => Origin::Injected("hermes_scaffolding"),
    }
}

/// What a user-role row of this `display_kind` is when it is not something the user typed.
fn scaffolding(display_kind: &str) -> Option<MetaKind> {
    match display_kind {
        "hidden"
        | "internal_notification"
        | "async_delegation_complete"
        | "process_complete"
        | "auto_continue" => Some(MetaKind::Internal),
        "model_switch" | "personality_switch" => Some(MetaKind::ModelChange),
        _ => None,
    }
}

fn id_of(row: &Value, index: usize) -> String {
    match row.get("id") {
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::String(s)) if !s.is_empty() => s.clone(),
        _ => super::json::fallback_id("msg", index),
    }
}

/// The text of a `content` value: the string, or the text parts of a list.
fn text_of(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => blocks_text(parts),
        _ => String::new(),
    }
}

impl Builder {
    fn meta(&mut self, index: usize, row: &Value, kind: MetaKind) {
        self.entries.push(Entry {
            id: id_of(row, index),
            kind: Kind::Meta { kind },
            raw: vec![row.clone()],
        });
    }

    /// One message row. `later` is every row after it, for a compaction's `kept_from`.
    fn push(&mut self, index: usize, row: &Value, later: &[Value]) {
        let role = str_at(row, "/role").unwrap_or_default();
        let active = row.get("active").and_then(Value::as_i64).unwrap_or(1) != 0;
        let live_tool_row = active && role == "tool";
        if !live_tool_row {
            self.flush_results();
        }
        if !active {
            return self.meta(index, row, MetaKind::OffBranch);
        }
        let display_kind = str_at(row, "/display_kind").unwrap_or_default();
        let summary = row
            .get("_compressed_summary")
            .and_then(Value::as_i64)
            .is_some_and(|flag| flag != 0);
        match role.as_str() {
            _ if summary => {
                let kept_from = later
                    .iter()
                    .enumerate()
                    .find(|(_, next)| next.get("active").and_then(Value::as_i64) != Some(0))
                    .map(|(offset, next)| id_of(next, index + 1 + offset));
                self.entries.push(Entry {
                    id: id_of(row, index),
                    kind: Kind::Compaction {
                        summary: summary_text(&text_of(row.get("content"))),
                        kept_from,
                        trigger: Trigger::Unknown,
                        tokens_before: None,
                    },
                    raw: vec![row.clone()],
                });
            }
            "user" => match scaffolding(&display_kind) {
                Some(kind) => self.meta(index, row, kind),
                None => {
                    let blocks = match row.get("content") {
                        Some(Value::Array(parts)) => attachments(parts),
                        _ => Vec::new(),
                    };
                    self.entries.push(Entry {
                        id: id_of(row, index),
                        kind: Kind::User {
                            text: text_of(row.get("content")),
                            attachments: blocks,
                        },
                        raw: vec![row.clone()],
                    });
                }
            },
            "assistant" if display_kind == "failed_turn" => {
                self.meta(index, row, MetaKind::Synthetic)
            }
            "assistant" => self.assistant(index, row),
            "tool" => {
                if self.results_id.is_none() {
                    self.results_id = Some(id_of(row, index));
                }
                self.results_raw.push(row.clone());
                self.results.push(ToolResult {
                    call_id: str_at(row, "/tool_call_id").unwrap_or_default(),
                    content: match row.get("content") {
                        Some(Value::Array(parts)) => result_content(parts),
                        other => ResultContent::Text(text_of(other)),
                    },
                    is_error: false,
                    truncated_by_source: false,
                });
            }
            "system" => self.meta(index, row, MetaKind::SystemPrompt),
            _ => self.meta(index, row, MetaKind::Internal),
        }
    }

    fn flush_results(&mut self) {
        if let Some(id) = self.results_id.take() {
            self.entries.push(Entry {
                id,
                kind: Kind::Results {
                    results: std::mem::take(&mut self.results),
                },
                raw: std::mem::take(&mut self.results_raw),
            });
        }
    }

    fn assistant(&mut self, index: usize, row: &Value) {
        let mut calls = Vec::new();
        for call in json_value(row.get("tool_calls"))
            .as_array()
            .into_iter()
            .flatten()
        {
            let name = str_at(call, "/function/name")
                .or_else(|| str_at(call, "/name"))
                .unwrap_or_default();
            let arguments = call
                .pointer("/function/arguments")
                .or_else(|| call.get("arguments"));
            let mut input = match arguments {
                Some(Value::String(text)) if text.trim().is_empty() => Value::Object(Map::new()),
                Some(Value::String(text)) => {
                    serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.clone()))
                }
                Some(other) => other.clone(),
                None => Value::Object(Map::new()),
            };
            let canonical = match CanonicalTool::from_hermes(&name) {
                CanonicalTool::Grep if str_at(&input, "/target").as_deref() == Some("files") => {
                    CanonicalTool::Glob
                }
                other => other,
            };
            // The canonical timeout is in milliseconds; Hermes's `terminal` counts seconds. The
            // record itself stays verbatim in `raw`.
            if name == "terminal"
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
                id: str_at(call, "/id")
                    .or_else(|| str_at(call, "/call_id"))
                    .unwrap_or_default(),
                canonical: Some(canonical),
                source_name: name,
                input,
            });
        }
        let text = text_of(row.get("content"));
        let end = match str_at(row, "/finish_reason").as_deref() {
            Some("tool_calls") => StepEnd::ToolUse,
            Some("stop") => StepEnd::Completed,
            Some("error") | Some("content_filter") => StepEnd::Error,
            Some(_) => StepEnd::Unknown,
            None if calls.is_empty() => StepEnd::Completed,
            None => StepEnd::ToolUse,
        };
        self.entries.push(Entry {
            id: id_of(row, index),
            kind: Kind::Step {
                model: self.model.clone(),
                reasoning: self.reasoning(row),
                text: if text.is_empty() {
                    Vec::new()
                } else {
                    vec![text]
                },
                calls,
                end,
            },
            raw: vec![row.clone()],
        });
    }

    /// The row's reasoning: the Responses items when it has them, the Chat details next, the
    /// plain text last. Signed and encrypted payloads are never edited.
    fn reasoning(&self, row: &Value) -> Vec<Reasoning> {
        let model_id = self.model.as_ref().map(|m| m.id.clone());
        let items = json_value(row.get("codex_reasoning_items"));
        let responses: Vec<Reasoning> = items
            .as_array()
            .into_iter()
            .flatten()
            .filter(|item| str_at(item, "/type").as_deref() == Some("reasoning"))
            .map(|item| Reasoning {
                text: item
                    .get("summary")
                    .and_then(Value::as_array)
                    .map(|summary| blocks_text(summary))
                    .unwrap_or_default(),
                opaque: Some(Opaque {
                    protocol: Protocol::OpenaiResponses,
                    model: model_id.clone(),
                    signature: None,
                    encrypted: str_at(item, "/encrypted_content"),
                    chat: None,
                }),
            })
            .collect();
        if !responses.is_empty() {
            return responses;
        }
        let plain = ["reasoning", "reasoning_content"]
            .iter()
            .filter_map(|key| str_at(row, &format!("/{key}")))
            .map(|text| text.trim().to_string())
            .find(|text| !text.is_empty())
            .unwrap_or_default();
        let details = json_value(row.get("reasoning_details"));
        let details = details.as_array().filter(|d| !d.is_empty());
        let Some(details) = details else {
            return if plain.is_empty() {
                Vec::new()
            } else {
                vec![Reasoning {
                    text: plain,
                    opaque: None,
                }]
            };
        };
        let text = if plain.is_empty() {
            details
                .iter()
                .filter_map(|d| str_at(d, "/text"))
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            plain
        };
        vec![Reasoning {
            text,
            opaque: Some(Opaque {
                protocol: details_protocol(details)
                    .or_else(|| self.protocol.clone())
                    .unwrap_or(Protocol::Other("unknown".into())),
                model: model_id,
                signature: details.iter().find_map(|d| str_at(d, "/signature")),
                encrypted: details
                    .iter()
                    .find_map(|d| str_at(d, "/data").or_else(|| str_at(d, "/encrypted"))),
                chat: Some(ChatReasoning {
                    provider: self.model.as_ref().and_then(|m| m.provider.clone()),
                    details: details.clone(),
                }),
            }),
        }]
    }
}

/// The protocol the details' `format` names, when they all name the same known one.
fn details_protocol(details: &[Value]) -> Option<Protocol> {
    let mut formats = details.iter().filter_map(|d| str_at(d, "/format"));
    let first = formats.next()?;
    if formats.any(|next| next != first) {
        return Some(Protocol::Other("mixed-reasoning-details".into()));
    }
    match first.as_str() {
        "anthropic-claude-v1" => Some(Protocol::AnthropicMessages),
        "openai-responses-v1" | "openai-native-responses-v1" => Some(Protocol::OpenaiResponses),
        "unknown" => None,
        other => Some(Protocol::Other(other.to_string())),
    }
}

/// The summary a compaction row carries, without Hermes's framing: the prefix that says it is
/// reference only, what was merged in before it, and the end marker.
fn summary_text(content: &str) -> String {
    const MERGED: &str = "[END OF PRIOR CONTEXT — COMPACTION SUMMARY BELOW]";
    const END: &str = "--- END OF CONTEXT SUMMARY";
    const PREFIXES: [&str; 2] = ["avoid repeating it:", "[CONTEXT SUMMARY]:"];
    let mut text = content
        .split_once(MERGED)
        .map_or(content, |(_, after)| after);
    if let Some((before, _)) = text.split_once(END) {
        text = before;
    }
    for prefix in PREFIXES {
        if let Some((_, after)) = text.split_once(prefix) {
            text = after;
            break;
        }
    }
    text.trim().to_string()
}

// ---------------------------------------------------------------------------------------------
// state.db

fn open(path: &Path) -> Result<Connection> {
    let db = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("open {} read-only", path.display()))?;
    db.busy_timeout(std::time::Duration::from_secs(2))?;
    Ok(db)
}

/// Run `work`, once more when it failed the way a reader of a WAL database fails during a
/// checkpoint (`SQLITE_IOERR`, `SQLITE_BUSY`): Hermes's own read-only readers retry the same way.
fn with_retry<T>(work: impl Fn() -> Result<T>) -> Result<T> {
    use rusqlite::ErrorCode::{DatabaseBusy, SystemIoFailure};
    match work() {
        Err(error)
            if error.chain().any(|cause| {
                cause
                    .downcast_ref::<rusqlite::Error>()
                    .and_then(|e| e.sqlite_error_code())
                    .is_some_and(|code| matches!(code, DatabaseBusy | SystemIoFailure))
            }) =>
        {
            std::thread::sleep(std::time::Duration::from_millis(200));
            work()
        }
        other => other,
    }
}

fn table_columns(db: &Connection, table: &str) -> Result<Vec<String>> {
    let mut query = db.prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))?;
    let names = query.query_map([], |row| row.get::<_, String>(0))?;
    Ok(names.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Open the store and check it is one this reader can read: the schema number and the columns.
/// Returns the connection and the schema number.
fn open_checked(path: &Path) -> Result<(Connection, i64)> {
    let db = open(path)?;
    let version: Option<i64> = db
        .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
            row.get(0)
        })
        .with_context(|| format!("{} has no Hermes schema_version", path.display()))?;
    let version = version.unwrap_or(0);
    if version < MIN_SCHEMA {
        anyhow::bail!(
            "{} is Hermes schema {version}; mukade reads schema {MIN_SCHEMA} and later (Hermes Agent \
             0.22.5): run a newer Hermes once to migrate it",
            path.display()
        );
    }
    let mut missing = Vec::new();
    for (table, required) in [
        ("sessions", &REQUIRED_SESSION_COLUMNS[..]),
        ("messages", &REQUIRED_MESSAGE_COLUMNS[..]),
    ] {
        let have = table_columns(&db, table)?;
        missing.extend(
            required
                .iter()
                .filter(|column| !have.iter().any(|h| h == *column))
                .map(|column| format!("{table}.{column}")),
        );
    }
    if !missing.is_empty() {
        anyhow::bail!(
            "{} (schema {version}) lacks {}: this reader needs them",
            path.display(),
            missing.join(", ")
        );
    }
    Ok((db, version))
}

fn cell(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(n) => Value::from(n),
        ValueRef::Real(f) => serde_json::Number::from_f64(f).map_or(Value::Null, Value::Number),
        ValueRef::Text(bytes) | ValueRef::Blob(bytes) => {
            Value::String(String::from_utf8_lossy(bytes).into_owned())
        }
    }
}

/// Every row `sql` returns as an object of its columns.
fn rows(db: &Connection, sql: &str, params: &[&str]) -> Result<Vec<Map<String, Value>>> {
    let mut query = db.prepare(sql)?;
    let names: Vec<String> = query.column_names().iter().map(|n| n.to_string()).collect();
    let mapped = query.query_map(rusqlite::params_from_iter(params), |row| {
        let mut object = Map::new();
        for (index, name) in names.iter().enumerate() {
            object.insert(name.clone(), cell(row.get_ref(index)?));
        }
        Ok(object)
    })?;
    Ok(mapped.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// A message row as the export writes it: `content` decoded, the JSON columns parsed, the columns
/// the export leaves out removed.
fn decode_message(mut row: Map<String, Value>) -> Value {
    for column in NOT_EXPORTED {
        row.remove(column);
    }
    // The export keeps the flag only when it is set.
    if row.get("_compressed_summary").and_then(Value::as_i64) == Some(0) {
        row.remove("_compressed_summary");
    }
    if let Some(Value::String(text)) = row.get("content")
        && let Some(json) = text.strip_prefix(CONTENT_JSON_PREFIX)
        && let Ok(parsed) = serde_json::from_str::<Value>(json)
    {
        row.insert("content".into(), parsed);
    }
    for column in JSON_COLUMNS {
        if let Some(Value::String(text)) = row.get(column)
            && let Ok(parsed) = serde_json::from_str::<Value>(text)
        {
            row.insert(column.into(), parsed);
        }
    }
    Value::Object(row)
}

fn session_row(db: &Connection, id: &str) -> Result<Option<Value>> {
    let found = rows(db, "SELECT * FROM sessions WHERE id = ?1", &[id])?;
    Ok(found.into_iter().next().map(Value::Object))
}

fn messages_of(db: &Connection, id: &str) -> Result<Vec<Value>> {
    Ok(rows(
        db,
        "SELECT * FROM messages WHERE session_id = ?1 ORDER BY id",
        &[id],
    )?
    .into_iter()
    .map(decode_message)
    .collect())
}

/// The id a user typed: an exact one, or the only one it is the start of.
fn resolve(db: &Connection, typed: &str) -> Result<String> {
    if session_row(db, typed)?.is_some() {
        return Ok(typed.to_string());
    }
    let ids = rows(db, "SELECT id FROM sessions ORDER BY id", &[])?;
    let matching: Vec<&str> = ids
        .iter()
        .filter_map(|row| row.get("id").and_then(Value::as_str))
        .filter(|id| id.starts_with(typed))
        .collect();
    match matching.as_slice() {
        [one] => Ok((*one).to_string()),
        [] => Err(Lookup::NotFound(format!("no Hermes session {typed:?}")).into()),
        several => Err(Lookup::Ambiguous(format!(
            "{typed:?} starts {} Hermes sessions ({})",
            several.len(),
            several
                .iter()
                .take(5)
                .copied()
                .collect::<Vec<_>>()
                .join(", ")
        ))
        .into()),
    }
}

/// Read session `id` from the `state.db` at `path`, the place a listing names.
pub(crate) fn read_bound(path: &Path, id: &str) -> Result<Session> {
    with_retry(|| read_session(path, id))
}

fn read_session(path: &Path, typed: &str) -> Result<Session> {
    let (db, schema) = open_checked(path)?;
    let id = resolve(&db, typed)?;
    let tip = session_row(&db, &id)?.expect("a resolved id has its row");
    let parent_end = |row: &Value| -> Result<(Option<Value>, Option<String>)> {
        let Some(parent) = str_at(row, "/parent_session_id").filter(|p| !p.is_empty()) else {
            return Ok((None, None));
        };
        let parent = session_row(&db, &parent)?;
        let end = parent.as_ref().and_then(|p| str_at(p, "/end_reason"));
        Ok((parent, end))
    };
    // A legacy rotating compaction continues the parent's conversation under a new id: the
    // session is the whole chain, root first.
    let (mut parent, mut end) = parent_end(&tip)?;
    let first_end = end.clone();
    let mut chain = vec![tip.clone()];
    let mut seen: HashSet<String> = HashSet::from([id.clone()]);
    while let Some(row) = parent.take() {
        let child = chain.last().expect("a chain has a tip");
        if !matches!(lineage(child, end.as_deref()), Lineage::Continuation(_))
            || !seen.insert(str_at(&row, "/id").unwrap_or_default())
        {
            break;
        }
        let (grandparent, grandparent_end) = parent_end(&row)?;
        chain.push(row);
        parent = grandparent;
        end = grandparent_end;
    }
    let mut doc = match tip {
        Value::Object(map) => map,
        _ => unreachable!("a session row is an object"),
    };
    let mut messages = Vec::new();
    for row in chain.iter().rev() {
        messages.extend(messages_of(&db, &str_at(row, "/id").unwrap_or_default())?);
    }
    if let Some(root) = chain.last()
        && let Some(started) = root.get("started_at")
    {
        doc.insert("started_at".into(), started.clone());
    }
    doc.insert("messages".into(), Value::Array(messages));
    let mut session = parse(
        &Value::Object(doc),
        Some(path),
        &format!("schema {schema}"),
        first_end.as_deref(),
    )?;
    session.meta.source_path = Some(path.to_string_lossy().into_owned());
    Ok(session)
}

/// The sources Hermes's pickers (`sessions list`, the TUI and Desktop lists) leave out: finite
/// runs (`oneshot`), third-party integrations (`tool`) and workers (`kanban`)
/// (`INTERNAL_LISTING_SOURCES`, v0.23.1).
const INTERNAL_LISTING_SOURCES: [&str; 3] = ["kanban", "tool", "oneshot"];

/// One session of a listing, with what sorts it.
struct Listed {
    info: SessionInfo,
    updated: f64,
    lineage: Lineage,
    /// It ended by a legacy rotating compaction and a continuation took over.
    superseded: bool,
    /// Its `source` is one Hermes's own pickers hide: a finite run, an integration, a worker.
    internal: bool,
}

fn listed(path: &Path) -> Result<Vec<Listed>> {
    let (db, _) = open_checked(path)?;
    let have = table_columns(&db, "sessions")?;
    let wanted: Vec<&str> = LIST_COLUMNS
        .iter()
        .copied()
        .filter(|column| have.iter().any(|h| h == column))
        .collect();
    let sessions = rows(
        &db,
        &format!("SELECT {} FROM sessions", wanted.join(", ")),
        &[],
    )?;
    let ends: HashMap<String, String> = sessions
        .iter()
        .filter_map(|row| {
            Some((
                row.get("id")?.as_str()?.to_string(),
                row.get("end_reason")?.as_str()?.to_string(),
            ))
        })
        .collect();
    let parents: Vec<Option<String>> = sessions
        .iter()
        .map(|row| {
            row.get("parent_session_id")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect();
    let mut out: Vec<Listed> = Vec::new();
    let mut continued: HashSet<String> = HashSet::new();
    for (row, parent) in sessions.into_iter().zip(parents) {
        let session = Value::Object(row);
        let parent_end = parent
            .as_ref()
            .and_then(|p| ends.get(p))
            .map(String::as_str);
        let lineage = lineage(&session, parent_end);
        if let Lineage::Continuation(parent) = &lineage {
            continued.insert(parent.clone());
        }
        let started = seconds(session.get("started_at"));
        let updated = seconds(session.get("last_activity_at"))
            .or_else(|| seconds(session.get("ended_at")))
            .or(started);
        let id = str_at(&session, "/id").unwrap_or_default();
        let title = str_at(&session, "/title")
            .filter(|t| !t.is_empty())
            .or_else(|| first_prompt(&db, &id).and_then(|text| title_line(Some(&text))));
        let (parent, parent_kind) = parent_of(&lineage);
        let flag = |key: &str| session.get(key).and_then(Value::as_i64).unwrap_or(0) != 0;
        out.push(Listed {
            info: SessionInfo {
                id,
                title,
                cwd: str_at(&session, "/cwd").filter(|c| !c.is_empty()),
                updated: updated.and_then(rfc3339),
                archived: flag("archived") || flag("auto_archived"),
                path: path.to_string_lossy().into_owned(),
                parent,
                parent_kind,
                ..Default::default()
            },
            updated: updated.unwrap_or(0.0),
            lineage,
            superseded: false,
            internal: str_at(&session, "/source")
                .is_some_and(|source| INTERNAL_LISTING_SOURCES.contains(&source.as_str())),
        });
    }
    for item in &mut out {
        item.superseded = continued.contains(&item.info.id)
            && ends.get(&item.info.id).map(String::as_str) == Some("compression");
    }
    out.sort_by(|a, b| {
        b.updated
            .total_cmp(&a.updated)
            .then_with(|| a.info.id.cmp(&b.info.id))
    });
    Ok(out)
}

/// The first thing the user said in a session the store gave no title.
fn first_prompt(db: &Connection, id: &str) -> Option<String> {
    let found = rows(
        db,
        "SELECT content FROM messages WHERE session_id = ?1 AND role = 'user' AND active = 1 \
         ORDER BY id LIMIT 1",
        &[id],
    )
    .ok()?;
    let message = decode_message(found.into_iter().next()?);
    Some(text_of(message.get("content"))).filter(|text| !text.is_empty())
}

/// What a listing shows. `roots_only` leaves out subagent sessions, the old halves of a rotated
/// compaction and the sessions of a source Hermes's own pickers hide, which a picker never shows;
/// a branch is a session of its own and stays.
fn list_in(roots: &StoreRoots, roots_only: bool) -> Result<Vec<SessionInfo>> {
    let db = state_db(roots, "list")?;
    let all = with_retry(|| listed(&db))?;
    Ok(all
        .into_iter()
        .filter(|item| {
            !roots_only
                || (!item.superseded
                    && !item.internal
                    && !matches!(item.lineage, Lineage::Spawn(_)))
        })
        .map(|item| item.info)
        .collect())
}

/// The sessions the store holds, and how many of them came from another session (a branch, a
/// subagent).
pub fn store_counts(db: &Path) -> Result<(usize, usize)> {
    let all = with_retry(|| listed(db))?;
    let children = all
        .iter()
        .filter(|item| matches!(item.lineage, Lineage::Fork(_) | Lineage::Spawn(_)))
        .count();
    Ok((all.len(), children))
}

/// `state.db` in the store `roots` names, or the refusal that no store is set.
fn state_db(roots: &StoreRoots, what: &str) -> Result<PathBuf> {
    roots.hermes_state_db().ok_or_else(|| {
        Lookup::NoStore(format!("HERMES_HOME is not set: no Hermes store to {what}")).into()
    })
}

impl Reader for Hermes {
    fn harness(&self) -> HarnessId {
        HarnessId::Hermes
    }

    fn list(&self, roots: &StoreRoots) -> Result<Vec<SessionInfo>> {
        list_in(roots, true)
    }

    fn list_all(&self, roots: &StoreRoots) -> Result<Vec<SessionInfo>> {
        list_in(roots, false)
    }

    fn read(&self, id: &str, roots: &StoreRoots) -> Result<Session> {
        let db = state_db(roots, "read")?;
        with_retry(|| read_session(&db, id))
    }
}
