//! The document form of a conversation: the JSON that moves between the library and a command
//! line, and the shape a hop policy operates on.
//!
//! It is a **projection** of [`Session`]: the conversation, in wire order, with the source metadata
//! beside it. The source records themselves (`Entry::raw`) are not part of it — a document is what
//! a writer needs, not what a reader saw.
//!
//! ```json
//! {"meta": { … the source session's Meta … },
//!  "messages": [
//!    {"role": "user",      "timestamp": "…", "content": [{"type": "text", "text": "…"}]},
//!    {"role": "assistant", "timestamp": "…", "end": "tool_use",
//!     "content": [{"type": "thinking", "thinking": "…", "opaque": { … }},
//!                 {"type": "tool_use", "id": "…", "name": "Read", "input": { … }, "canonical": "read"}]},
//!    {"role": "user",      "timestamp": "…",
//!     "content": [{"type": "tool_result", "tool_use_id": "…", "content": {"text": "…"}, "is_error": false}]}
//!  ]}
//! ```
//!
//! One `Step` is **one** assistant message, with every block of that step in `content`: that is how
//! Claude Code, Pi and OpenCode store a response, and splitting it is what made the probe lose the
//! grouping (§6.2). A `Compaction` is one user message holding the source's own summary (L4).

use crate::model::{
    Attachment, Branch, CanonicalTool, Entry, Kind, Meta, ModelRef, Reasoning, ResultContent,
    Session, StepEnd, ToolCall, ToolResult,
};
use anyhow::{Context, Result};
use serde_json::{Value, json};

/// The library name and version a document states, so a reader knows what wrote it.
pub const LIBRARY: &str = "mukade";

/// The messages of a run of entries: the conversation among them, Meta left out.
fn messages_of<'a>(entries: impl Iterator<Item = &'a Entry>) -> Vec<Value> {
    entries
        .filter(|e| !matches!(e.kind, Kind::Meta { .. }))
        .enumerate()
        .filter_map(|(n, entry)| message_of(entry, n))
        .collect()
}

/// The message one entry makes: one entry, one message, `None` for a Meta entry. `n` is the entry's
/// place among the conversation entries of its run (a compaction states it).
pub fn message_of(entry: &Entry, n: usize) -> Option<Value> {
    let timestamp = timestamp_of(entry);
    Some(match &entry.kind {
        Kind::User { text, attachments } => {
            let mut content = vec![json!({"type": "text", "text": text})];
            for attachment in attachments {
                content.push(json!({
                    "type": "attachment",
                    "kind": attachment.kind,
                    "path": attachment.path,
                    "raw": attachment.raw,
                }));
            }
            json!({"role": "user", "id": entry.id, "timestamp": timestamp, "content": content})
        }
        Kind::Step {
            model,
            reasoning,
            text,
            calls,
            end,
        } => {
            let mut content = Vec::new();
            for r in reasoning {
                content.push(json!({
                    "type": "thinking",
                    "thinking": r.text,
                    "opaque": r.opaque,
                }));
            }
            for t in text {
                content.push(json!({"type": "text", "text": t}));
            }
            for call in calls {
                content.push(json!({
                    "type": "tool_use",
                    "id": call.id,
                    "name": call.source_name,
                    "input": call.input,
                    "canonical": call.canonical,
                }));
            }
            json!({
                "role": "assistant",
                "id": entry.id,
                "timestamp": timestamp,
                "model": model.as_ref().map(|m| m.id.clone()),
                "provider": model.as_ref().and_then(|m| m.provider.clone()),
                "end": end,
                "content": content,
            })
        }
        Kind::Results { results } => {
            let mut content = Vec::new();
            for result in results {
                content.push(json!({
                    "type": "tool_result",
                    "tool_use_id": result.call_id,
                    "content": result.content,
                    "is_error": result.is_error,
                    "truncated_by_source": result.truncated_by_source,
                }));
            }
            json!({"role": "user", "id": entry.id, "timestamp": timestamp, "content": content})
        }
        Kind::Compaction {
            summary,
            kept_from,
            trigger,
            tokens_before,
        } => json!({
            "role": "user",
            "id": entry.id,
            "timestamp": timestamp,
            "content": [{"type": "text", "text": summary}],
            "compaction": {
                "kept_from": kept_from,
                "trigger": trigger,
                "tokens_before": tokens_before,
                "entry": n,
            },
        }),
        Kind::Meta { .. } => return None,
    })
}

/// Project a session into its document form.
pub fn to_document(session: &Session) -> Value {
    let messages = messages_of(session.entries.iter());
    let mut doc = json!({
        "meta": serde_json::to_value(&session.meta).unwrap_or(Value::Null),
        "library": {"name": LIBRARY, "version": env!("CARGO_PKG_VERSION")},
        "messages": messages,
    });
    // The branches the session left, each from the entry it leaves (null: before the first).
    if !session.branches.is_empty() {
        doc["branches"] = Value::Array(
            session
                .branches
                .iter()
                .map(|branch| {
                    json!({"from": branch.from, "messages": messages_of(branch.entries.iter())})
                })
                .collect(),
        );
    }
    doc
}

/// The document of a session written **one message at a time**, to any `io::Write`: the bytes
/// `serde_json::to_writer(out, &to_document(&session))` writes (compact; no caller needs another
/// form: the CLI pretty-prints a document it has whole), without holding the session. One entry is
/// one message, so the writer holds none: each message is built, written and dropped.
///
/// A document states its `meta` first, so it is given up front. [`Reader::read_streamed_with_meta`]
/// hands it over before the first entry; [`DocumentWriter::finish`] takes the final one and refuses
/// to close a document whose opening `meta` was not it.
///
/// ```ignore
/// let mut writer = None;
/// let meta = reader.read_streamed_with_meta(id, &roots,
///     &mut |meta| { writer = Some(DocumentWriter::new(File::create(path)?, meta)?); Ok(()) },
///     &mut |entry| writer.as_mut().unwrap().entry(&entry))?;
/// writer.unwrap().finish(&meta)?;
/// ```
///
/// A caller that changes each message before it is written (removing reasoning, redacting) uses
/// [`DocumentWriter::entry_with`].
///
/// [`Reader::read_streamed_with_meta`]: crate::reader::Reader::read_streamed_with_meta
pub struct DocumentWriter<W: std::io::Write> {
    out: W,
    /// The `meta` the document opened with, as written.
    meta: Value,
    /// Messages written.
    messages: usize,
    /// Conversation entries seen: what a compaction states as its `entry`.
    conversation: usize,
}

impl<W: std::io::Write> DocumentWriter<W> {
    /// Start a document for a session whose meta is `meta`.
    pub fn new(mut out: W, meta: &Meta) -> Result<Self> {
        let meta = serde_json::to_value(meta).unwrap_or(Value::Null);
        out.write_all(b"{\"meta\":")?;
        serde_json::to_writer(&mut out, &meta)?;
        out.write_all(b",\"library\":")?;
        serde_json::to_writer(
            &mut out,
            &json!({"name": LIBRARY, "version": env!("CARGO_PKG_VERSION")}),
        )?;
        out.write_all(b",\"messages\":[")?;
        Ok(DocumentWriter {
            out,
            meta,
            messages: 0,
            conversation: 0,
        })
    }

    /// The message of the next entry, as [`to_document`] makes it. A Meta entry makes none.
    pub fn entry(&mut self, entry: &Entry) -> Result<()> {
        self.entry_with(entry, &mut |_| Ok(()))
    }

    /// [`DocumentWriter::entry`], handing the message to `change` before it is written.
    pub fn entry_with(
        &mut self,
        entry: &Entry,
        change: &mut dyn FnMut(&mut Value) -> Result<()>,
    ) -> Result<()> {
        let Some(mut message) = message_of(entry, self.conversation) else {
            return Ok(());
        };
        self.conversation += 1;
        change(&mut message)?;
        self.message(&message)
    }

    /// Write one message as it is: the lower level of [`DocumentWriter::entry_with`], for a caller
    /// that builds its messages itself.
    pub fn message(&mut self, message: &Value) -> Result<()> {
        if self.messages > 0 {
            self.out.write_all(b",")?;
        }
        serde_json::to_writer(&mut self.out, message)?;
        self.messages += 1;
        Ok(())
    }

    /// Close the document. `meta` is the session's final one: it must be the one the document
    /// opened with, or nothing is closed and the error says so.
    pub fn finish(self, meta: &Meta) -> Result<W> {
        self.finish_with_branches(meta, &[])
    }

    /// [`DocumentWriter::finish`] for a session that carries branches (`read_all_branches`): each
    /// is written whole, as [`to_document`] writes it. A branch is held by its caller already.
    pub fn finish_with_branches(mut self, meta: &Meta, branches: &[Branch]) -> Result<W> {
        let last = serde_json::to_value(meta).unwrap_or(Value::Null);
        anyhow::ensure!(
            last == self.meta,
            "the session's meta changed after the document opened with it: the document is not \
             closed"
        );
        self.out.write_all(b"]")?;
        if !branches.is_empty() {
            let branches: Vec<Value> = branches
                .iter()
                .map(|branch| {
                    json!({"from": branch.from, "messages": messages_of(branch.entries.iter())})
                })
                .collect();
            self.out.write_all(b",\"branches\":")?;
            serde_json::to_writer(&mut self.out, &branches)?;
        }
        self.out.write_all(b"}")?;
        self.out.flush()?;
        Ok(self.out)
    }
}

/// Rebuild a session from its document form.
pub fn from_document(doc: &Value) -> Result<Session> {
    let meta: Meta = serde_json::from_value(doc.get("meta").cloned().unwrap_or(Value::Null))
        .context("the document has no readable `meta`")?;
    let messages = doc
        .get("messages")
        .and_then(Value::as_array)
        .context("the document has no `messages` array")?;
    let entries = entries_of(messages)?;
    let mut branches = Vec::new();
    for (n, branch) in doc
        .get("branches")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let messages = branch
            .get("messages")
            .and_then(Value::as_array)
            .with_context(|| format!("branch {n} has no `messages` array"))?;
        branches.push(Branch {
            from: branch
                .get("from")
                .and_then(Value::as_str)
                .map(str::to_owned),
            entries: entries_of(messages).with_context(|| format!("branch {n}"))?,
        });
    }
    Ok(Session {
        meta,
        entries,
        branches,
    })
}

/// The entries the messages of a document stand for.
fn entries_of(messages: &[Value]) -> Result<Vec<Entry>> {
    let mut entries: Vec<Entry> = Vec::new();
    for (n, message) in messages.iter().enumerate() {
        let role = message
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("user");
        let blocks = message
            .get("content")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        // The source's own entry id, so a compaction boundary still points at its entry.
        let id = message
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| format!("doc-{n}"));

        match role {
            "assistant" => {
                let mut reasoning = Vec::new();
                let mut text = Vec::new();
                let mut calls = Vec::new();
                for block in &blocks {
                    match block.get("type").and_then(Value::as_str).unwrap_or("") {
                        "thinking" => reasoning.push(Reasoning {
                            text: block
                                .get("thinking")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            opaque: serde_json::from_value(
                                block.get("opaque").cloned().unwrap_or(Value::Null),
                            )
                            .ok(),
                        }),
                        "text" => text.push(
                            block
                                .get("text")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                        ),
                        "tool_use" => {
                            let name = block
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string();
                            let canonical = serde_json::from_value(
                                block.get("canonical").cloned().unwrap_or(Value::Null),
                            )
                            .ok()
                            .or_else(|| Some(CanonicalTool::Other(name.clone())));
                            calls.push(ToolCall {
                                id: block
                                    .get("id")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .to_string(),
                                source_name: name,
                                canonical,
                                input: block.get("input").cloned().unwrap_or(Value::Null),
                            });
                        }
                        other => {
                            anyhow::bail!(
                                "message {n}: an assistant block of type {other:?} is not part of a document"
                            )
                        }
                    }
                }
                let end =
                    serde_json::from_value(message.get("end").cloned().unwrap_or(Value::Null))
                        .unwrap_or(if calls.is_empty() {
                            StepEnd::Completed
                        } else {
                            StepEnd::ToolUse
                        });
                let model = message
                    .get("model")
                    .and_then(Value::as_str)
                    .map(|id| ModelRef {
                        provider: message
                            .get("provider")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                        id: id.to_string(),
                    });
                entries.push(Entry {
                    id,
                    kind: Kind::Step {
                        model,
                        reasoning,
                        text,
                        calls,
                        end,
                    },
                    raw: Vec::new(),
                });
            }
            _ => {
                // A user message: results first (they answer the step before it), then the text.
                let mut results = Vec::new();
                let mut text_parts = Vec::new();
                let mut attachments = Vec::new();
                let mut compaction: Option<&Value> = message.get("compaction");
                for block in &blocks {
                    match block.get("type").and_then(Value::as_str).unwrap_or("") {
                        "tool_result" => results.push(ToolResult {
                            call_id: block
                                .get("tool_use_id")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            content: result_content_of(block.get("content"))
                                .with_context(|| format!("message {n}: a tool result's content"))?,
                            is_error: block
                                .get("is_error")
                                .and_then(Value::as_bool)
                                .unwrap_or(false),
                            truncated_by_source: block
                                .get("truncated_by_source")
                                .and_then(Value::as_bool)
                                .unwrap_or(false),
                        }),
                        "text" => text_parts.push(
                            block
                                .get("text")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                        ),
                        "attachment" => attachments.push(Attachment {
                            kind: block
                                .get("kind")
                                .and_then(Value::as_str)
                                .unwrap_or("unknown")
                                .to_string(),
                            path: block.get("path").and_then(Value::as_str).map(str::to_owned),
                            raw: block.get("raw").cloned().unwrap_or(Value::Null),
                        }),
                        other => {
                            anyhow::bail!(
                                "message {n}: a user block of type {other:?} is not part of a document"
                            )
                        }
                    }
                }
                let has_text =
                    compaction.is_some() || !text_parts.is_empty() || !attachments.is_empty();
                if !results.is_empty() {
                    entries.push(Entry {
                        id: if has_text {
                            format!("{id}-results")
                        } else {
                            id.clone()
                        },
                        kind: Kind::Results { results },
                        raw: Vec::new(),
                    });
                }
                if let Some(marker) = compaction.take() {
                    let summary = text_parts.join("\n");
                    entries.push(Entry {
                        id,
                        kind: Kind::Compaction {
                            summary,
                            kept_from: marker
                                .get("kept_from")
                                .and_then(Value::as_str)
                                .map(str::to_owned),
                            trigger: serde_json::from_value(
                                marker.get("trigger").cloned().unwrap_or(Value::Null),
                            )
                            .unwrap_or_default(),
                            tokens_before: marker.get("tokens_before").and_then(Value::as_u64),
                        },
                        raw: Vec::new(),
                    });
                } else if !text_parts.is_empty() || !attachments.is_empty() {
                    entries.push(Entry {
                        id,
                        kind: Kind::User {
                            text: text_parts.join("\n"),
                            attachments,
                        },
                        raw: Vec::new(),
                    });
                }
            }
        }
    }
    Ok(entries)
}

/// A tool result's content: this library's `{text}` / `{blocks}` form, or Claude's own plain
/// string or array of blocks.
fn result_content_of(content: Option<&Value>) -> Result<ResultContent> {
    match content {
        None | Some(Value::Null) => Ok(ResultContent::Text(String::new())),
        Some(Value::String(text)) => Ok(ResultContent::Text(text.clone())),
        Some(Value::Array(blocks)) => Ok(crate::reader::json::result_content(blocks)),
        Some(other) => serde_json::from_value(other.clone())
            .with_context(|| format!("not a result content: {other}")),
    }
}

fn timestamp_of(entry: &Entry) -> Option<String> {
    entry.raw.iter().find_map(|r| {
        r.get("timestamp")
            .and_then(Value::as_str)
            .map(str::to_owned)
    })
}
