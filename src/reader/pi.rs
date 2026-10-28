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

use super::json::{result_content, rows_from_lines, str_at};
use super::{Reader, SessionInfo};
use crate::model::{
    CanonicalTool, Entry, HarnessId, Kind, Meta, MetaKind, ModelRef, Opaque, Protocol, Reasoning,
    Session, StepEnd, ToolCall, ToolResult, Trigger,
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
    let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let (rows, unfinished) = rows_from_lines(&text, &path.display().to_string())?;
    let mut session = parse(&rows, Some(path))?;
    super::json::keep_unfinished(&mut session, unfinished);
    Ok(session)
}

/// Read records that were already decoded.
pub fn parse(rows: &[Value], source_path: Option<&Path>) -> Result<Session> {
    let on_branch = active_branch(rows);
    let mut b = Builder::default();
    for (i, row) in rows.iter().enumerate() {
        b.push(i, row, on_branch[i]);
    }
    b.finish(source_path)
}

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
    /// The results entry currently being appended to.
    open_results: Option<usize>,
    model: Option<ModelRef>,
    protocol: Option<Protocol>,
}

#[derive(Default)]
struct MetaBuilder {
    id: Option<String>,
    cwd: Option<String>,
    version: Option<String>,
    created: Option<String>,
    updated: Option<String>,
    parent: Option<String>,
    models: Vec<ModelRef>,
}

impl MetaBuilder {
    fn observe(&mut self, row: &Value) {
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
    fn push(&mut self, index: usize, row: &Value, on_branch: bool) {
        self.meta.observe(row);
        if let Some(model_id) = str_at(row, "/modelId") {
            // `model_change`: the provider and model the session was using.
            let model = ModelRef {
                provider: str_at(row, "/provider"),
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
        let class = class_of(row);
        if !on_branch && !matches!(class, Class::Header | Class::Meta(_)) {
            // A record of a branch the conversation left: kept, not resumed.
            self.open_results = None;
            self.entries.push(Entry {
                id: id_of(row, index),
                kind: Kind::Meta {
                    kind: MetaKind::OffBranch,
                },
                raw: vec![row.clone()],
            });
            return;
        }
        match class {
            Class::CustomMessage => {
                self.open_results = None;
                self.entries.push(Entry {
                    id: id_of(row, index),
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
                    raw: vec![row.clone()],
                });
            }
            // Pi shows the model a branch summary as a user message, in its own words.
            Class::BranchSummary => {
                self.open_results = None;
                let summary = str_at(row, "/summary").unwrap_or_default();
                self.entries.push(Entry {
                    id: id_of(row, index),
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
                    raw: vec![row.clone()],
                });
            }
            Class::Header => self.entries.push(Entry {
                id: id_of(row, index),
                kind: Kind::Meta {
                    kind: MetaKind::Internal,
                },
                raw: vec![row.clone()],
            }),
            Class::Meta(kind) => self.entries.push(Entry {
                id: id_of(row, index),
                kind: Kind::Meta { kind },
                raw: vec![row.clone()],
            }),
            Class::Message => match role_of(row) {
                "user" => {
                    self.open_results = None;
                    self.entries.push(Entry {
                        id: id_of(row, index),
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
                        raw: vec![row.clone()],
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
                    let message = row.get("message").cloned().unwrap_or(Value::Null);
                    let excluded = message.get("excludeFromContext") == Some(&Value::Bool(true));
                    self.entries.push(Entry {
                        id: id_of(row, index),
                        kind: if excluded {
                            Kind::Meta {
                                kind: MetaKind::Internal,
                            }
                        } else {
                            Kind::User {
                                text: bash_execution_text(&message),
                                attachments: Vec::new(),
                            }
                        },
                        raw: vec![row.clone()],
                    });
                }
                "system" => self.entries.push(Entry {
                    id: id_of(row, index),
                    kind: Kind::Meta {
                        kind: MetaKind::SystemPrompt,
                    },
                    raw: vec![row.clone()],
                }),
                _ => self.entries.push(Entry {
                    id: id_of(row, index),
                    kind: Kind::Meta {
                        kind: MetaKind::Internal,
                    },
                    raw: vec![row.clone()],
                }),
            },
            Class::Compacted => {
                self.open_results = None;
                self.entries.push(Entry {
                    id: id_of(row, index),
                    kind: Kind::Compaction {
                        summary: str_at(row, "/summary").unwrap_or_default(),
                        kept_from: str_at(row, "/firstKeptEntryId"),
                        trigger: Trigger::Unknown,
                        tokens_before: row.get("tokensBefore").and_then(Value::as_u64),
                    },
                    raw: vec![row.clone()],
                });
            }
        }
    }

    /// One assistant message: thinking, text and **all** of its tool calls together (L3's other
    /// half: what Pi writes back on its own resume).
    fn assistant(&mut self, row: &Value, index: usize) {
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
                "thinking" => reasoning.push(Reasoning {
                    text: str_at(block, "/thinking").unwrap_or_default(),
                    opaque: Some(Opaque {
                        protocol: self
                            .protocol
                            .clone()
                            .unwrap_or(Protocol::Other("unknown".into())),
                        model: model_id.clone(),
                        signature: str_at(block, "/thinkingSignature"),
                        encrypted: None,
                    }),
                }),
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
            str_at(row, "/message/stopReason").as_deref(),
            !calls.is_empty(),
        );
        self.entries.push(Entry {
            id: id_of(row, index),
            kind: Kind::Step {
                model: self.model.clone(),
                reasoning,
                text,
                calls,
                end,
            },
            raw: vec![row.clone()],
        });
    }

    fn tool_result(&mut self, row: &Value, index: usize) {
        let blocks = row
            .pointer("/message/content")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let result = ToolResult {
            call_id: str_at(row, "/message/toolCallId").unwrap_or_default(),
            content: result_content(&blocks),
            is_error: row
                .pointer("/message/isError")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            truncated_by_source: false,
        };
        match self.open_results {
            Some(i) => {
                let entry = &mut self.entries[i];
                entry.raw.push(row.clone());
                if let Kind::Results { results } = &mut entry.kind {
                    results.push(result);
                }
            }
            None => {
                self.entries.push(Entry {
                    id: id_of(row, index),
                    kind: Kind::Results {
                        results: vec![result],
                    },
                    raw: vec![row.clone()],
                });
                self.open_results = Some(self.entries.len() - 1);
            }
        }
    }

    fn finish(self, source_path: Option<&Path>) -> Result<Session> {
        let m = self.meta;
        Ok(Session {
            meta: Meta {
                harness: HarnessId::Pi,
                harness_version: m.version.unwrap_or_else(|| "unknown".into()),
                id: m.id.unwrap_or_else(|| "unknown".into()),
                cwd: m.cwd,
                title: None,
                created: m.created,
                updated: m.updated,
                models: m.models,
                git_branch: None,
                parent: m.parent,
                source_path: source_path.map(|p| p.to_string_lossy().into_owned()),
            },
            entries: self.entries,
        })
    }
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
            anyhow::bail!("PI_CODING_AGENT_DIR is not set: no Pi store to list");
        };
        let mut out = Vec::new();
        for file in session_files(&sessions)? {
            out.push(SessionInfo {
                id: pi_id(&file),
                path: file.to_string_lossy().into_owned(),
                ..Default::default()
            });
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }

    fn read(&self, id: &str, roots: &StoreRoots) -> Result<Session> {
        let Some(sessions) = roots.pi_sessions() else {
            anyhow::bail!("PI_CODING_AGENT_DIR is not set: no Pi store to read");
        };
        let matches: Vec<PathBuf> = session_files(&sessions)?
            .into_iter()
            .filter(|f| {
                let named = f.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                named == id || named.ends_with(&format!("_{id}")) || pi_id(f).starts_with(id)
            })
            .collect();
        match matches.as_slice() {
            [one] => read_file(one),
            [] => anyhow::bail!("no Pi session with id {id:?} under {}", sessions.display()),
            many => anyhow::bail!("{id:?} matches {} Pi sessions", many.len()),
        }
    }
}

/// Pi names a session file `<timestamp>_<uuid>.jsonl`.
fn pi_id(file: &Path) -> String {
    let stem = file.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    stem.rsplit_once('_')
        .map(|(_, id)| id.to_string())
        .unwrap_or_else(|| stem.to_string())
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
