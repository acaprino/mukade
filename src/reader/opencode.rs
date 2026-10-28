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
//! Reading OpenCode's SQLite store directly is **not implemented**: this reader takes the JSON the
//! bench and `session export` produce (`docs/verification/2026-10-27-H27.md`), while
//! [`Opencode::list`] and [`Opencode::read`] report the gap instead of guessing at the file.

use super::json::{result_content, str_at};
use super::{Reader, SessionInfo};
use crate::model::{
    CanonicalTool, Entry, HarnessId, Kind, Meta, MetaKind, ModelRef, Opaque, Protocol, Reasoning,
    ResultContent, Session, StepEnd, ToolCall, ToolResult, Trigger,
};
use crate::roots::StoreRoots;
use anyhow::{Context, Result};
use serde_json::Value;
use std::path::Path;

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
                "reasoning" => reasoning.push(Reasoning {
                    text: str_at(block, "/text").unwrap_or_default(),
                    opaque: Some(Opaque {
                        protocol: self
                            .protocol
                            .clone()
                            .unwrap_or(Protocol::Other("unknown".into())),
                        model: model_id.clone(),
                        signature: str_at(block, "/native/signature"),
                        encrypted: str_at(block, "/native/encrypted"),
                    }),
                }),
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
                parent: str_at(&session, "/parent_id"),
                source_path: source_path.map(|p| p.to_string_lossy().into_owned()),
                source_sha256: None,
            },
            entries: self.entries,
        })
    }
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
        Err(sqlite_gap(roots))
    }

    /// OpenCode's own export path (OC2): `opencode session export <id> --standalone` prints the
    /// interchange document, which this reader reads. **`--standalone` is not optional** — without
    /// it the CLI hands the work to the background service of the user's real home (L1).
    fn read(&self, id: &str, roots: &StoreRoots) -> Result<Session> {
        let exe = opencode_binary();
        let argv = export_command(&exe, id);
        let mut command = std::process::Command::new(&argv[0]);
        command.args(&argv[1..]);
        opencode_environment(&mut command, roots);
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

/// Which `opencode` to spawn: the caller's override (`MUKADE_OPENCODE_EXE`), then the binary on
/// `PATH`, then the well-known install locations.
fn opencode_binary() -> String {
    crate::exe::resolve(HarnessId::OpenCode, None)
}

/// Carry the caller's explicit OpenCode store into the subprocess rather than silently using the
/// process environment. XDG variables must still be supplied by the caller when OpenCode needs them.
pub fn opencode_environment(command: &mut std::process::Command, roots: &StoreRoots) {
    if let Some(db) = &roots.opencode_db {
        command.env("OPENCODE_DB", db);
    } else {
        command.env_remove("OPENCODE_DB");
    }
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

/// OpenCode's native store is SQLite. Reading it needs either a SQLite dependency (a decision this
/// crate has not taken) or `opencode session export`; until then the gap is reported, not guessed.
fn sqlite_gap(roots: &StoreRoots) -> anyhow::Error {
    anyhow::anyhow!(
        "OpenCode's native store is SQLite ({}): reading it directly is not implemented; \
         export the session with `opencode session export <id> --standalone` and read that JSON",
        roots
            .opencode_db
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "OPENCODE_DB".into())
    )
}
