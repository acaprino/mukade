//! Small helpers the readers share for working with decoded source records.

use crate::model::{Attachment, ResultContent};
use anyhow::{Context, Result};
use serde_json::Value;

/// Decode a JSON Lines file into records, skipping blank lines. A leading UTF-8 BOM (Pi writes
/// one on Windows) is tolerated.
pub fn rows_from_lines(text: &str, what: &str) -> Result<Vec<Value>> {
    let mut rows = Vec::new();
    for (n, line) in text.trim_start_matches('\u{feff}').lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        rows.push(
            serde_json::from_str::<Value>(line).with_context(|| format!("{what}:{}", n + 1))?,
        );
    }
    Ok(rows)
}

/// A string at a JSON pointer, when present.
pub fn str_at(value: &Value, pointer: &str) -> Option<String> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// The text of a `content` value that is either a string or an array of `{type:"text",text}`.
pub fn text_of_content(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Array(blocks) => {
            let text = blocks_text(blocks);
            if text.is_empty() { None } else { Some(text) }
        }
        _ => None,
    }
}

/// The text of content blocks, joined the way the harness joined them.
pub fn blocks_text(blocks: &[Value]) -> String {
    blocks
        .iter()
        .filter_map(|b| b.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Attachments from a harness's user-content blocks. Their complete records stay in `raw` so a
/// destination can preserve any fields that are not part of the shared model.
pub fn attachments(blocks: &[Value]) -> Vec<Attachment> {
    blocks
        .iter()
        .filter(|block| {
            let kind = block.get("type").and_then(Value::as_str).unwrap_or("");
            !matches!(
                kind,
                "text" | "input_text" | "output_text" | "tool_result" | "tool_use"
            )
        })
        .map(|block| Attachment {
            kind: block
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string(),
            path: block
                .get("path")
                .or_else(|| block.get("file_path"))
                .and_then(Value::as_str)
                .map(str::to_owned),
            raw: block.clone(),
        })
        .collect()
}

/// A tool result's content: plain text when every block is text, the blocks themselves otherwise.
pub fn result_content(blocks: &[Value]) -> ResultContent {
    if blocks
        .iter()
        .any(|b| b.get("type").and_then(Value::as_str) != Some("text"))
    {
        ResultContent::Blocks(blocks.to_vec())
    } else {
        ResultContent::Text(blocks_text(blocks))
    }
}

/// An entry id for a record that does not carry one of its own; unique within the session.
pub fn fallback_id(prefix: &str, index: usize) -> String {
    format!("{prefix}-{index}")
}
