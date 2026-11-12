//! The OpenCode 2.1.4 reader.
//!
//! Facts this reader depends on (docs/session-interchange-spec.md §2.3):
//!
//! - the store is `session_message` rows ordered by `seq`; the session header is a `session_v2` row;
//! - one assistant row is **one model step**, with `content: [reasoning, text, tool, tool]`; both
//!   parallel calls and their results live in that single row (`state{status,input,content}`);
//! - reasoning is `{"type":"reasoning","text":…,"native":{"signature":…}}`;
//! - `{type:"idle","outcome":"succeeded"}` marks the end of a turn and is not conversation;
//! - a compaction is `{type:"compaction",metadata{keepID},status,reason,summary}`;
//! - an interrupted turn is an assistant row with no `finish` and an empty text block: the harness
//!   was killed, and the export omits that row (L9).
//!
//! Listing opens an explicitly selected SQLite store read-only; reading a session uses the native
//! `session export` command or a previously exported document.

use super::json::{result_content, str_at};
use super::{Reader, SessionInfo};
use crate::model::{
    CanonicalTool, ChatReasoning, Entry, HarnessId, Kind, Meta, MetaKind, ModelRef, Opaque,
    ParentKind, Protocol, Reasoning, ResultContent, Session, StepEnd, ToolCall, ToolResult,
    Trigger,
};
use crate::roots::StoreRoots;
use anyhow::{Context, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// The OpenCode reader.
#[derive(Debug, Clone, Copy, Default)]
pub struct Opencode;

/// Read a JSON document that holds OpenCode session rows: a `session_message` dump, a
/// `session export` document (`{info, messages}`), or a bare array of messages.
pub fn read_file(path: &Path) -> Result<Session> {
    let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let doc: Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .with_context(|| format!("parse {}", path.display()))?;
    let mut session = parse(&doc, Some(path))?;
    session.meta.source_sha256 = Some(crate::sha256::sha256_hex(text.as_bytes()));
    Ok(session)
}

/// Read a decoded OpenCode document.
pub fn parse(doc: &Value, source_path: Option<&Path>) -> Result<Session> {
    // Each message is kept twice: the flattened form the reader interprets, and the record
    // exactly as the store wrote it (`Entry::raw`).
    let mut messages: Vec<(Value, Value)> = match doc {
        Value::Array(rows) => rows.iter().map(|r| (r.clone(), r.clone())).collect(),
        Value::Object(_) => {
            if let Some(rows) = doc.get("session_message").and_then(Value::as_array) {
                rows.iter().map(|r| (normalize_row(r), r.clone())).collect()
            } else if let Some(rows) = doc.get("messages").and_then(Value::as_array) {
                rows.iter().map(|r| (r.clone(), r.clone())).collect()
            } else {
                anyhow::bail!("no session_message or messages array");
            }
        }
        _ => anyhow::bail!("not an OpenCode session document"),
    };
    let session = match doc {
        Value::Object(_) => doc
            .get("session_v2")
            .and_then(Value::as_array)
            .and_then(|v| v.first())
            .or_else(|| doc.get("info"))
            .cloned()
            .unwrap_or(Value::Null),
        _ => Value::Null,
    };

    // Native order is `seq`, not time.
    if messages.iter().all(|(m, _)| m.get("seq").is_some()) {
        messages.sort_by_key(|(m, _)| m.get("seq").and_then(Value::as_u64).unwrap_or(0));
    }

    let mut b = Builder::default();
    for (i, (m, raw)) in messages.iter().enumerate() {
        b.push(i, m, raw);
    }
    b.finish(session, source_path)
}

/// A `session_message` row carries its payload under `data`; a `session export` message is already
/// flattened. Normalise both into the flattened form.
fn normalize_row(row: &Value) -> Value {
    match (row.get("data"), row.get("type"), row.get("id")) {
        (Some(data), Some(kind), Some(id)) => {
            let mut out = data.as_object().cloned().unwrap_or_default();
            out.insert("id".into(), id.clone());
            out.insert("type".into(), kind.clone());
            if let Some(seq) = row.get("seq") {
                out.insert("seq".into(), seq.clone());
            }
            Value::Object(out)
        }
        _ => row.clone(),
    }
}

fn step_end(finish: Option<&str>) -> StepEnd {
    match finish {
        Some("tool-calls") | Some("tool_use") => StepEnd::ToolUse,
        Some("aborted") => StepEnd::Aborted,
        Some("stop") | Some("end_turn") => StepEnd::Completed,
        Some("error") | Some("refusal") => StepEnd::Error,
        Some(_) => StepEnd::Completed,
        // No finish at all: the process was killed mid-turn (§2.3, L9).
        None => StepEnd::Killed,
    }
}

#[derive(Default)]
struct Builder {
    entries: Vec<Entry>,
    model: Option<ModelRef>,
    /// Every model the session used, in first-seen order.
    models: Vec<ModelRef>,
    /// The protocol of the model that produced the current row, when the store states it.
    protocol: Option<Protocol>,
}

impl Builder {
    fn push(&mut self, index: usize, m: &Value, raw: &Value) {
        if let Some(model) = m.get("model") {
            let reference = ModelRef {
                provider: str_at(model, "/providerID"),
                id: str_at(model, "/id").unwrap_or_default(),
            };
            if !reference.id.is_empty() {
                if !self.models.contains(&reference) {
                    self.models.push(reference.clone());
                }
                self.model = Some(reference);
            }
            self.protocol = str_at(model, "/providerID").map(|p| Protocol::from_provider(&p));
        }
        match m.get("type").and_then(Value::as_str).unwrap_or("") {
            "user" => self.entries.push(Entry {
                id: id_of(m, index),
                kind: Kind::User {
                    text: str_at(m, "/text").unwrap_or_default(),
                    attachments: m
                        .get("files")
                        .and_then(Value::as_array)
                        .map(|files| super::json::attachments(files))
                        .unwrap_or_default(),
                },
                raw: vec![raw.clone()],
            }),
            "assistant" => self.assistant(index, m, raw),
            // A subagent's report, which the parent's model received as a message of its own.
            "synthetic"
                if str_at(m, "/metadata/source").as_deref() == Some("subagent")
                    && str_at(m, "/text").is_some_and(|text| !text.is_empty()) =>
            {
                self.entries.push(Entry {
                    id: id_of(m, index),
                    kind: Kind::User {
                        text: str_at(m, "/text").unwrap_or_default(),
                        attachments: Vec::new(),
                    },
                    raw: vec![raw.clone()],
                })
            }
            // A compaction OpenCode did not complete (`failed`: the summary did not match its
            // template) was never applied; it is not the source's compaction.
            "compaction" if str_at(m, "/status").is_some_and(|status| status != "completed") => {
                self.entries.push(Entry {
                    id: id_of(m, index),
                    kind: Kind::Meta {
                        kind: MetaKind::Internal,
                    },
                    raw: vec![raw.clone()],
                })
            }
            "compaction" => self.entries.push(Entry {
                id: id_of(m, index),
                kind: Kind::Compaction {
                    summary: str_at(m, "/summary").unwrap_or_default(),
                    kept_from: str_at(m, "/metadata/keepID"),
                    trigger: match str_at(m, "/reason").as_deref() {
                        Some("manual") => Trigger::Manual,
                        Some("auto") => Trigger::Auto,
                        _ => Trigger::Unknown,
                    },
                    tokens_before: m.pointer("/tokens/input").and_then(Value::as_u64),
                },
                raw: vec![raw.clone()],
            }),
            "idle" => self.entries.push(Entry {
                id: id_of(m, index),
                kind: Kind::Meta {
                    kind: MetaKind::Idle,
                },
                raw: vec![raw.clone()],
            }),
            _ => self.entries.push(Entry {
                id: id_of(m, index),
                kind: Kind::Meta {
                    kind: MetaKind::Internal,
                },
                raw: vec![raw.clone()],
            }),
        }
    }

    /// One assistant row is one step. Its `tool` blocks hold both the call and its result, so the
    /// step is followed by a derived `Results` entry (the row itself is the step's `raw`).
    fn assistant(&mut self, index: usize, m: &Value, raw: &Value) {
        let mut reasoning = Vec::new();
        let mut text = Vec::new();
        let mut calls = Vec::new();
        let mut results = Vec::new();
        let model_id = self.model.as_ref().map(|x| x.id.clone());
        for block in m
            .get("content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            match block.get("type").and_then(Value::as_str).unwrap_or("") {
                "reasoning" => {
                    // Gateway and OpenRouter both use this array. A summary can precede the
                    // encrypted entry, and one part can own several opaque entries.
                    let details = block
                        .pointer("/native/reasoningDetails")
                        .and_then(Value::as_array)
                        .filter(|items| !items.is_empty());
                    let protocol = details
                        .and_then(|items| detail_protocol(items))
                        .or_else(|| self.protocol.clone())
                        .unwrap_or(Protocol::Other("unknown".into()));
                    let (signature, encrypted) = match details {
                        Some(items) => (
                            items.iter().find_map(|d| str_at(d, "/signature")),
                            items.iter().find_map(|d| {
                                str_at(d, "/data").or_else(|| str_at(d, "/encrypted"))
                            }),
                        ),
                        None => (
                            str_at(block, "/native/signature"),
                            str_at(block, "/native/encrypted"),
                        ),
                    };
                    let chat = details.map(|items| ChatReasoning {
                        provider: self.model.as_ref().and_then(|m| m.provider.clone()),
                        details: items.clone(),
                    });
                    reasoning.push(Reasoning {
                        text: str_at(block, "/text").unwrap_or_default(),
                        opaque: Some(Opaque {
                            protocol,
                            model: model_id.clone(),
                            signature,
                            encrypted,
                            chat,
                        }),
                    })
                }
                "text" => {
                    let t = str_at(block, "/text").unwrap_or_default();
                    if !t.is_empty() {
                        text.push(t);
                    }
                }
                "tool" => {
                    let name = str_at(block, "/name").unwrap_or_default();
                    calls.push(ToolCall {
                        id: str_at(block, "/id").unwrap_or_default(),
                        canonical: Some(CanonicalTool::from_opencode(&name)),
                        source_name: name,
                        input: block
                            .pointer("/state/input")
                            .cloned()
                            .unwrap_or(Value::Null),
                    });
                    let status = str_at(block, "/state/status").unwrap_or_default();
                    // A tool still `pending` or `running` when OpenCode stopped has no result
                    // (L9): the call stays unanswered.
                    if status != "completed" && status != "error" {
                        continue;
                    }
                    let blocks = block
                        .pointer("/state/content")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                    // A failed call's message is in `error` (`Session.StructuredError`).
                    let content = match str_at(block, "/state/error/message") {
                        Some(message) if blocks.is_empty() => ResultContent::Text(message),
                        _ => result_content(&blocks),
                    };
                    results.push(ToolResult {
                        call_id: str_at(block, "/id").unwrap_or_default(),
                        content,
                        is_error: status == "error",
                        truncated_by_source: block
                            .pointer("/state/metadata/truncated")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    });
                }
                _ => {}
            }
        }
        let end = step_end(str_at(m, "/finish").as_deref());
        self.entries.push(Entry {
            id: id_of(m, index),
            kind: Kind::Step {
                model: self.model.clone(),
                reasoning,
                text,
                calls,
                end,
            },
            raw: vec![raw.clone()],
        });
        if !results.is_empty() {
            self.entries.push(Entry {
                id: format!("{}-results", id_of(m, index)),
                kind: Kind::Results { results },
                // Derived: this entry was read from the same row as the step above.
                raw: Vec::new(),
            });
        }
    }

    fn finish(self, session: Value, source_path: Option<&Path>) -> Result<Session> {
        let models = self.models.clone();
        let epoch = |ptr: &str| {
            session
                .pointer(ptr)
                .and_then(Value::as_i64)
                .and_then(chrono::DateTime::from_timestamp_millis)
                .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        };
        let (parent, parent_kind) = lineage(&session).unzip();
        Ok(Session {
            meta: Meta {
                harness: HarnessId::OpenCode,
                harness_version: str_at(&session, "/version").unwrap_or_else(|| "unknown".into()),
                id: str_at(&session, "/id").unwrap_or_else(|| "unknown".into()),
                cwd: str_at(&session, "/directory")
                    .or_else(|| str_at(&session, "/location/directory")),
                title: str_at(&session, "/title"),
                created: epoch("/time_created").or_else(|| epoch("/time/created")),
                updated: epoch("/time_updated").or_else(|| epoch("/time/updated")),
                models,
                git_branch: None,
                parent,
                parent_kind,
                source_path: source_path.map(|p| p.to_string_lossy().into_owned()),
                source_sha256: None,
            },
            entries: self.entries,
            branches: Vec::new(),
        })
    }
}

/// The session this one came from, and how. A child names the session that spawned it:
/// `parentID` in an export (`info`), `parent_id` in a store row. A fork names its source in
/// `fork.sessionID` (export) or `fork_session_id` (row), which OpenCode reads as a fork only with
/// its boundary beside it.
fn lineage(session: &Value) -> Option<(String, ParentKind)> {
    let spawner = str_at(session, "/parentID").or_else(|| str_at(session, "/parent_id"));
    if let Some(parent) = spawner.filter(|p| !p.is_empty()) {
        return Some((parent, ParentKind::spawn()));
    }
    let bounded = |id: &str, boundary: &str| {
        let source = str_at(session, id).filter(|p| !p.is_empty())?;
        let boundary = session.pointer(boundary).filter(|b| !b.is_null())?;
        Some((source, boundary_message(boundary)))
    };
    bounded("/fork/sessionID", "/fork/boundary")
        .or_else(|| bounded("/fork_session_id", "/fork_boundary"))
        .map(|(source, at)| (source, ParentKind::Fork { at }))
}

/// The message a fork's boundary names: `{type: "before" | "through", messageID}`, as an object or
/// as the JSON text a store row keeps.
fn boundary_message(boundary: &Value) -> Option<String> {
    let parsed;
    let boundary = match boundary {
        Value::String(text) => {
            parsed = serde_json::from_str::<Value>(text).ok()?;
            &parsed
        }
        other => other,
    };
    str_at(boundary, "/messageID").filter(|id| !id.is_empty())
}

fn id_of(m: &Value, index: usize) -> String {
    str_at(m, "/id")
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| super::json::fallback_id("msg", index))
}

// ---------------------------------------------------------------------------------------------

impl Reader for Opencode {
    fn harness(&self) -> HarnessId {
        HarnessId::OpenCode
    }

    fn list(&self, roots: &StoreRoots) -> Result<Vec<SessionInfo>> {
        list_in(roots, true)
    }

    fn list_all(&self, roots: &StoreRoots) -> Result<Vec<SessionInfo>> {
        list_in(roots, false)
    }

    /// OpenCode's own export path (OC2): `opencode session export <id> --standalone` prints the
    /// interchange document, which this reader reads. **`--standalone` is not optional** — without
    /// it the CLI hands the work to the background service of the user's real home (L1).
    fn read(&self, id: &str, roots: &StoreRoots) -> Result<Session> {
        if let Some(why) = not_exportable_here(id, roots) {
            return Err(super::Lookup::NotFound(why).into());
        }
        let exe = opencode_binary();
        let argv = export_command(&exe, id);
        let mut command = std::process::Command::new(&argv[0]);
        command.args(&argv[1..]);
        roots.apply_to(HarnessId::OpenCode, &mut command);
        let output = crate::exe::run_within(&mut command, EXPORT_TIMEOUT)
            .with_context(|| format!("run {}", argv.join(" ")))?;
        let Some(status) = output.status else {
            anyhow::bail!("opencode session export {id} did not finish within {EXPORT_TIMEOUT:?}");
        };
        if !status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let message = format!("opencode session export {id} failed ({status}): {stderr}");
            // OpenCode's own word for an id it does not hold.
            if stderr.to_ascii_lowercase().contains("not found") {
                return Err(super::Lookup::NotFound(message).into());
            }
            anyhow::bail!(message);
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let doc: Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))
            .context("opencode session export did not print an interchange document")?;
        let mut session = parse(&doc, None)?;
        session.meta.source_sha256 = Some(crate::sha256::sha256_hex(text.as_bytes()));
        Ok(session)
    }
}

/// List OpenCode sessions. `roots_only` keeps the CLI's `list` contract (one row per root
/// session); the explorer asks for every row, forks included.
///
/// The selected database is listed first; when it is named like one of OpenCode's own
/// (`opencode.db`, `opencode-<channel>.db`) the other such files beside it are listed too, because
/// OpenCode starts a new file for a new channel and leaves the old one behind with its sessions.
/// Those sessions are marked unreadable (see [`databases`]); a sibling that cannot be read is
/// skipped, while the selected one is an error as before.
fn list_in(roots: &StoreRoots, roots_only: bool) -> Result<Vec<SessionInfo>> {
    let selected = roots.opencode_db.as_ref().ok_or_else(|| {
        super::Lookup::NoStore("OPENCODE_DB is not set: no OpenCode store to list".into())
    })?;
    // (time_updated, row): the order across files is by time, whatever file a session is in.
    let mut found: Vec<(i64, SessionInfo)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (index, path) in databases(selected).into_iter().enumerate() {
        let rows = match sessions_in(&path, roots_only) {
            Ok(rows) => rows,
            Err(error) if index == 0 => return Err(error),
            Err(_) => continue,
        };
        for (updated, mut info) in rows {
            if !seen.insert(info.id.clone()) {
                continue;
            }
            if index > 0 && info.unreadable.is_none() {
                info.unreadable = Some(format!(
                    "it is in {}, not in the selected {}; select that file (--root) to read it",
                    file_name(&path),
                    file_name(selected)
                ));
            }
            found.push((updated, info));
        }
    }
    found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.id.cmp(&b.1.id)));
    Ok(found.into_iter().map(|(_, info)| info).collect())
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// Whether `path` is named like one of OpenCode's own databases: `opencode.db`, or a channel
/// build's `opencode-<channel>.db`.
fn is_opencode_database(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_ascii_lowercase)
        .is_some_and(|name| name.starts_with("opencode") && name.ends_with(".db"))
}

/// The database files of one OpenCode installation: the selected file first, then, when it is
/// named like OpenCode's own, the other such files in its folder in name order. A file the caller
/// named otherwise (a test store, a copy) stands alone: whatever lies beside it is not its.
fn databases(selected: &Path) -> Vec<PathBuf> {
    let mut out = vec![selected.to_path_buf()];
    if !is_opencode_database(selected) {
        return out;
    }
    let folder = selected
        .parent()
        .filter(|folder| !folder.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut siblings: Vec<PathBuf> = std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .map(|entry| entry.path())
        .filter(|path| is_opencode_database(path) && path.file_name() != selected.file_name())
        .collect();
    siblings.sort();
    out.extend(siblings);
    out
}

fn open_read_only(path: &Path) -> Result<rusqlite::Connection> {
    use rusqlite::{Connection, OpenFlags};
    let db = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("open {} read-only", path.display()))?;
    db.busy_timeout(std::time::Duration::from_secs(2))?;
    Ok(db)
}

fn has_column(db: &rusqlite::Connection, table: &str, column: &str) -> bool {
    db.prepare(&format!(
        "SELECT 1 FROM pragma_table_info('{table}') WHERE name = ?1"
    ))
    .and_then(|mut query| query.exists([column]))
    .unwrap_or(false)
}

fn has_table(db: &rusqlite::Connection, table: &str) -> bool {
    db.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [table],
        |_| Ok(()),
    )
    .is_ok()
}

/// What OpenCode 2.x does with a row still in the pre-2.0 `session` table.
const LEGACY_ROW: &str =
    "legacy-schema row: OpenCode has not moved it to session_v2 and cannot export it";

/// The sessions of one database file, each with its `time_updated`. `session_v2` is the truth;
/// rows only the legacy `session` table has are added, marked unreadable.
fn sessions_in(path: &Path, roots_only: bool) -> Result<Vec<(i64, SessionInfo)>> {
    let db = open_read_only(path)?;
    let (v2, legacy) = (has_table(&db, "session_v2"), has_table(&db, "session"));
    if !v2 && !legacy {
        anyhow::bail!(
            "list sessions in {}: it has neither a session_v2 nor a session table",
            path.display()
        );
    }
    let mut out = Vec::new();
    if v2 {
        out.extend(
            session_rows(&db, path, "session_v2", roots_only, None)
                .with_context(|| format!("list sessions in {}", path.display()))?,
        );
    }
    if legacy {
        let rows = session_rows(&db, path, "session", roots_only, v2.then_some("session_v2"));
        // Next to a readable session_v2 a legacy table this reader does not understand is
        // nothing to fail over; on its own it is the store, and the error is the answer.
        let rows = if v2 {
            rows.unwrap_or_default()
        } else {
            rows.with_context(|| format!("list sessions in {}", path.display()))?
        };
        out.extend(rows.into_iter().map(|(updated, mut info)| {
            info.unreadable = Some(LEGACY_ROW.to_string());
            (updated, info)
        }));
    }
    Ok(out)
}

/// The rows of `table` (`session_v2`, or the legacy `session`), newest first. `except` names a
/// table whose ids are left out.
fn session_rows(
    db: &rusqlite::Connection,
    path: &Path,
    table: &str,
    roots_only: bool,
    except: Option<&str>,
) -> Result<Vec<(i64, SessionInfo)>> {
    let mut conditions = Vec::new();
    if roots_only {
        conditions.push("parent_id IS NULL".to_string());
    }
    if let Some(other) = except {
        conditions.push(format!("id NOT IN (SELECT id FROM {other})"));
    }
    let filter = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {} ", conditions.join(" AND "))
    };
    // A fork names its source in `fork_session_id`, read as one with its `fork_boundary`; a table
    // without the two columns records no fork.
    let forks = has_column(db, table, "fork_session_id") && has_column(db, table, "fork_boundary");
    let fork_columns = if forks {
        "fork_session_id, fork_boundary"
    } else {
        "NULL, NULL"
    };
    let sql = format!(
        "SELECT id, title, directory, time_updated, time_archived, parent_id, {fork_columns} \
         FROM {table} {filter}ORDER BY time_updated DESC, id ASC"
    );
    let mut query = db.prepare(&sql)?;
    let rows = query.query_map([], |row| {
        let updated: Option<i64> = row.get(3)?;
        let spawner: Option<String> = row.get::<_, Option<String>>(5)?.filter(|p| !p.is_empty());
        let fork_source: Option<String> =
            row.get::<_, Option<String>>(6)?.filter(|p| !p.is_empty());
        let boundary: Option<String> = match row.get_ref(7)? {
            rusqlite::types::ValueRef::Null => None,
            rusqlite::types::ValueRef::Text(text) => {
                Some(String::from_utf8_lossy(text).into_owned())
            }
            _ => Some(String::new()),
        };
        let (parent, parent_kind) = match (spawner, fork_source, boundary) {
            (Some(parent), _, _) => (Some(parent), Some(ParentKind::spawn())),
            (None, Some(source), Some(boundary)) => (
                Some(source),
                Some(ParentKind::Fork {
                    at: boundary_message(&Value::String(boundary)),
                }),
            ),
            _ => (None, None),
        };
        Ok((
            updated.unwrap_or(0),
            SessionInfo {
                id: row.get(0)?,
                title: row.get(1)?,
                cwd: row.get(2)?,
                updated: updated
                    .and_then(chrono::DateTime::from_timestamp_millis)
                    .map(|time| time.to_rfc3339()),
                archived: row.get::<_, Option<i64>>(4)?.is_some(),
                path: path.to_string_lossy().into_owned(),
                parent,
                parent_kind,
                ..Default::default()
            },
        ))
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .context("read OpenCode session list")
}

/// Why `id` cannot be exported through the selected database, when another database file or the
/// legacy table is where the session is. `None` when the selected `session_v2` has it, or nobody
/// does (then the export says what it says). OpenCode is never started against a file that is not
/// the selected one: opening a database makes OpenCode migrate it.
fn not_exportable_here(id: &str, roots: &StoreRoots) -> Option<String> {
    let selected = roots.opencode_db.as_deref()?;
    let holds = |path: &Path, table: &str| {
        open_read_only(path).ok().is_some_and(|db| {
            has_table(&db, table)
                && db
                    .query_row(
                        &format!("SELECT 1 FROM {table} WHERE id = ?1"),
                        [id],
                        |_| Ok(()),
                    )
                    .is_ok()
        })
    };
    if holds(selected, "session_v2") {
        return None;
    }
    if holds(selected, "session") {
        return Some(format!(
            "OpenCode session {id} is a {LEGACY_ROW} in {}",
            file_name(selected)
        ));
    }
    databases(selected).into_iter().skip(1).find_map(|other| {
        if holds(&other, "session_v2") {
            Some(format!(
                "OpenCode session {id} is in {}, not in the selected {}; select that file (--root or OPENCODE_DB) to read it",
                file_name(&other),
                file_name(selected)
            ))
        } else if holds(&other, "session") {
            Some(format!(
                "OpenCode session {id} is a {LEGACY_ROW} in {}",
                file_name(&other)
            ))
        } else {
            None
        }
    })
}

/// Session and fork counts read from OpenCode's own store. The database is opened read-only;
/// `total_sessions` includes child sessions, unlike [`Reader::list`], which lists roots only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreCounts {
    pub sessions: usize,
    pub forks: usize,
}

pub fn store_counts(db: &Path) -> Result<StoreCounts> {
    use rusqlite::{Connection, OpenFlags};
    let connection = Connection::open_with_flags(
        db,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("open {} read-only", db.display()))?;
    connection.busy_timeout(std::time::Duration::from_secs(2))?;
    let (sessions, forks): (i64, i64) = connection
        .query_row(
            "SELECT COUNT(*), SUM(CASE WHEN fork_session_id IS NOT NULL THEN 1 ELSE 0 END) \
             FROM session_v2",
            [],
            |row| Ok((row.get(0)?, row.get::<_, Option<i64>>(1)?.unwrap_or(0))),
        )
        .with_context(|| format!("count sessions in {}", db.display()))?;
    Ok(StoreCounts {
        sessions: usize::try_from(sessions).unwrap_or(0),
        forks: usize::try_from(forks).unwrap_or(0),
    })
}

/// Which `opencode` to spawn: the caller's override (`MUKADE_OPENCODE_EXE`), then the binary on
/// `PATH`, then the well-known install locations.
fn opencode_binary() -> String {
    crate::exe::resolve(HarnessId::OpenCode, None)
}

/// How long `opencode session export` may take before it is stopped.
const EXPORT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// L1: the exact argv every spawned `opencode` command carries.
pub fn export_command(exe: &str, id: &str) -> Vec<String> {
    vec![
        exe.to_string(),
        "session".into(),
        "export".into(),
        id.to_string(),
        "--standalone".into(),
    ]
}

/// The project the destination already files sessions of `directory` under. The destination is
/// asked from **inside that directory**: its CLI scopes the session list to the project it is run
/// in. The answer is cached for this binary, this database and this directory, because finding it
/// costs a process start per session it has to look at.
///
/// This is the one destination probe the UI cannot reasonably reimplement: it belongs to the
/// harness's own store, not to the TUI. `cache` is the caller's `mukade-data/cache` folder.
pub fn project_id_for(
    exe: &str,
    cwd: &str,
    roots: &StoreRoots,
    cache: Option<&Path>,
) -> Option<String> {
    let program = std::path::PathBuf::from(crate::exe::spawnable(exe));
    let key = crate::cache::identity(&program).map(|identity| {
        let database = roots
            .opencode_db
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        format!("project|{identity}|{database}|{}", normalise_directory(cwd))
    });
    if let (Some(dir), Some(key)) = (cache, &key)
        && let Some(known) = crate::cache::get(dir, key)
    {
        return Some(known);
    }
    let found = ask_project(exe, cwd, roots)?;
    if let (Some(dir), Some(key)) = (cache, &key) {
        crate::cache::put(dir, key, &found);
    }
    Some(found)
}

fn ask_project(exe: &str, cwd: &str, roots: &StoreRoots) -> Option<String> {
    let directory = Path::new(cwd);
    // Asked of the store the session will be written to, not the caller's own.
    let store = Some((HarnessId::OpenCode, roots));
    let list = run_probe(exe, &["session", "list", "--standalone"], directory, store)
        .map(|out| String::from_utf8_lossy(&out).into_owned())?;
    for id in list
        .lines()
        .filter_map(|line| line.split('\t').next())
        .filter(|id| id.starts_with("ses_"))
        .take(5)
    {
        let Some(doc) = run_json(
            exe,
            &["session", "export", id, "--standalone"],
            directory,
            store,
        ) else {
            continue;
        };
        let found = doc
            .pointer("/info/location/directory")
            .and_then(Value::as_str);
        if found.is_some_and(|found| same_directory(found, cwd)) {
            return doc
                .pointer("/info/projectID")
                .and_then(Value::as_str)
                .map(str::to_owned);
        }
    }
    None
}

fn normalise_directory(directory: &str) -> String {
    directory
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

fn same_directory(a: &str, b: &str) -> bool {
    normalise_directory(a) == normalise_directory(b)
}

/// How long a question to the destination's binary (its version, its sessions) may take.
const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

fn run_probe(
    exe: &str,
    args: &[&str],
    cwd: &Path,
    store: Option<(HarnessId, &StoreRoots)>,
) -> Option<Vec<u8>> {
    let mut command = std::process::Command::new(crate::exe::spawnable(exe));
    command.args(args).current_dir(cwd);
    if let Some((harness, roots)) = store {
        roots.apply_to(harness, &mut command);
    }
    let output = crate::exe::run_within(&mut command, PROBE_TIMEOUT).ok()?;
    output
        .status
        .is_some_and(|status| status.success())
        .then_some(output.stdout)
}

fn run_json(
    exe: &str,
    args: &[&str],
    cwd: &Path,
    store: Option<(HarnessId, &StoreRoots)>,
) -> Option<Value> {
    serde_json::from_slice(&run_probe(exe, args, cwd, store)?).ok()
}

/// A mixed or unknown format must not borrow the protocol of its first detail.
fn detail_protocol(details: &[Value]) -> Option<Protocol> {
    let mut formats = details
        .iter()
        .filter_map(|d| d.get("format").and_then(Value::as_str));
    let format = formats.next()?;
    // Native and reseller OpenAI proof share a family, not a replay namespace.
    if formats.any(|next| next != format) {
        return Some(Protocol::Other("mixed-reasoning-details".into()));
    }
    Some(match format {
        "anthropic-claude-v1" => Protocol::AnthropicMessages,
        "openai-responses-v1" | "openai-native-responses-v1" => Protocol::OpenaiResponses,
        other => Protocol::Other(other.to_owned()),
    })
}
