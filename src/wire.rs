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
    Attachment, CanonicalTool, Entry, Kind, Meta, ModelRef, Reasoning, ResultContent, Session,
    StepEnd, ToolCall, ToolResult,
};
use anyhow::{Context, Result};
use serde_json::{Value, json};

/// The library name and version a document states, so a reader knows what wrote it.
pub const LIBRARY: &str = "mukade";

/// Project a session into its document form.
pub fn to_document(session: &Session) -> Value {
    let mut messages = Vec::new();
    for (n, entry) in session.conversation().enumerate() {
        let timestamp = timestamp_of(entry);
        match &entry.kind {
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
                messages.push(json!({"role": "user", "timestamp": timestamp, "content": content}));
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
                messages.push(json!({
                    "role": "assistant",
                    "timestamp": timestamp,
                    "model": model.as_ref().map(|m| m.id.clone()),
                    "end": end,
                    "content": content,
                }));
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
                messages.push(json!({"role": "user", "timestamp": timestamp, "content": content}));
            }
            Kind::Compaction {
                summary,
                kept_from,
                trigger,
                tokens_before,
            } => messages.push(json!({
                "role": "user",
                "timestamp": timestamp,
                "content": [{"type": "text", "text": summary}],
                "compaction": {
                    "kept_from": kept_from,
                    "trigger": trigger,
                    "tokens_before": tokens_before,
                    "entry": n,
                },
            })),
            Kind::Meta { .. } => {}
        }
    }
    json!({
        "meta": serde_json::to_value(&session.meta).unwrap_or(Value::Null),
        "library": {"name": LIBRARY, "version": env!("CARGO_PKG_VERSION")},
        "messages": messages,
    })
}

/// Rebuild a session from its document form.
pub fn from_document(doc: &Value) -> Result<Session> {
    let meta: Meta = serde_json::from_value(doc.get("meta").cloned().unwrap_or(Value::Null))
        .context("the document has no readable `meta`")?;
    let messages = doc
        .get("messages")
        .and_then(Value::as_array)
        .context("the document has no `messages` array")?;

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
        let id = format!("doc-{n}");

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
                        _ => {}
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
                        provider: None,
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
                            content: serde_json::from_value(
                                block.get("content").cloned().unwrap_or(Value::Null),
                            )
                            .unwrap_or_else(|_| ResultContent::Text(String::new())),
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
                        _ => {}
                    }
                }
                if !results.is_empty() {
                    entries.push(Entry {
                        id: format!("{id}-results"),
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
    Ok(Session { meta, entries })
}

fn timestamp_of(entry: &Entry) -> Option<String> {
    entry.raw.iter().find_map(|r| {
        r.get("timestamp")
            .and_then(Value::as_str)
            .map(str::to_owned)
    })
}
