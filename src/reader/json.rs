//! Small helpers the readers share for working with decoded source records.

use crate::model::{Attachment, Entry, Kind, MetaKind, ResultContent, Session};
use anyhow::{Context, Result};
use serde_json::Value;

/// Decode a JSON Lines file into records, skipping blank lines. A leading UTF-8 BOM (Pi writes
/// one on Windows) is tolerated. A last line that does not parse is what a hard kill during an
/// append leaves: it is returned apart, as text, instead of failing the whole session; a bad line
/// anywhere else is an error.
pub fn rows_from_lines(text: &str, what: &str) -> Result<(Vec<Value>, Option<String>)> {
    let lines: Vec<(usize, &str)> = text
        .trim_start_matches('\u{feff}')
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .collect();
    let mut rows = Vec::new();
    for (k, (n, line)) in lines.iter().enumerate() {
        match serde_json::from_str::<Value>(line) {
            Ok(row) => rows.push(row),
            Err(_) if k + 1 == lines.len() => return Ok((rows, Some((*line).to_string()))),
            Err(error) => return Err(error).with_context(|| format!("{what}:{}", n + 1)),
        }
    }
    Ok((rows, None))
}

/// Keep a cut-off last line at the end of a session, as text.
pub fn keep_unfinished(session: &mut Session, line: Option<String>) {
    if let Some(line) = line {
        session.entries.push(Entry {
            id: fallback_id("unfinished", session.entries.len()),
            kind: Kind::Meta {
                kind: MetaKind::Unfinished,
            },
            raw: vec![Value::String(line)],
        });
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_lines_and_a_byte_order_mark_are_not_records() {
        let (rows, tail) = rows_from_lines("\u{feff}{\"a\":1}\n\n  \n{\"b\":2}\n", "t").unwrap();
        assert_eq!(rows.len(), 2);
        assert!(tail.is_none());
    }

    #[test]
    fn only_a_cut_off_last_line_is_kept_apart() {
        let (rows, tail) = rows_from_lines("{\"a\":1}\n{\"b\":", "t").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(tail.as_deref(), Some("{\"b\":"));
        let error = rows_from_lines("{\"a\":1}\n{bad\n{\"c\":3}\n", "t.jsonl").unwrap_err();
        assert!(format!("{error:#}").contains("t.jsonl:2"), "{error:#}");
    }
}
