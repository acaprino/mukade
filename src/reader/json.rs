//! Small helpers the readers share for working with decoded source records.

use crate::model::{Attachment, Entry, Kind, MetaKind, ResultContent, Session};
use anyhow::{Context, Result};
use serde_json::Value;

/// Below this size, threads cost more than they save.
const PARALLEL_FROM: usize = 2 * 1024 * 1024;
const MAX_THREADS: usize = 8;

/// Decode a JSON Lines file into records, skipping blank lines. A leading UTF-8 BOM (Pi writes
/// one on Windows) is tolerated. A last line that does not parse is what a hard kill during an
/// append leaves: it is returned apart, as text, instead of failing the whole session; a bad line
/// anywhere else is an error naming its line.
///
/// A large file is cut at line breaks and decoded on several threads. Whatever is not clean
/// (a cut-off last line, a bad line) is decided again by the one-thread path, which defines the
/// result and the error.
pub fn rows_from_lines(text: &str, what: &str) -> Result<(Vec<Value>, Option<String>)> {
    let _span = crate::timing::span("parse json lines");
    let text = text.trim_start_matches('\u{feff}');
    if let Some(clean) = decode_in_parallel(text) {
        return Ok((clean, None));
    }
    decode_in_order(text, what)
}

/// The reference decoding: one thread, line by line.
fn decode_in_order(text: &str, what: &str) -> Result<(Vec<Value>, Option<String>)> {
    let lines: Vec<(usize, &str)> = text
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

/// Every record of a large text, decoded on several threads, or `None` when the text is small or
/// any line does not decode (the caller then takes the one-thread path).
fn decode_in_parallel(text: &str) -> Option<Vec<Value>> {
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(MAX_THREADS);
    if text.len() < PARALLEL_FROM || threads < 2 {
        return None;
    }
    // Cut points just after a line break, so every piece is whole lines (and valid UTF-8).
    let bytes = text.as_bytes();
    let mut bounds = vec![0usize];
    for k in 1..threads {
        let guess = (text.len() / threads * k).max(*bounds.last()?);
        match bytes[guess..].iter().position(|b| *b == b'\n') {
            Some(at) => bounds.push(guess + at + 1),
            None => break,
        }
    }
    bounds.push(text.len());
    bounds.dedup();
    let pieces: Vec<&str> = bounds.windows(2).map(|w| &text[w[0]..w[1]]).collect();
    let decoded: Vec<Option<Vec<Value>>> = std::thread::scope(|scope| {
        let handles: Vec<_> = pieces
            .iter()
            .map(|piece| {
                scope.spawn(move || {
                    piece
                        .lines()
                        .filter(|line| !line.trim().is_empty())
                        .map(|line| serde_json::from_str::<Value>(line).ok())
                        .collect::<Option<Vec<Value>>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("a decoding thread does not panic"))
            .collect()
    });
    let mut rows = Vec::new();
    for piece in decoded {
        rows.extend(piece?);
    }
    Some(rows)
}

/// A JSON Lines file, read and decoded.
pub struct Jsonl {
    pub rows: Vec<Value>,
    /// The exact UTF-8 bytes decoded into `rows`, retained for combined fingerprints.
    pub source_bytes: std::sync::Arc<String>,
    /// The file's last line when a hard kill cut it off mid-write.
    pub unfinished: Option<String>,
    /// The SHA-256 of the file's bytes, still being computed.
    pub sha256: PendingDigest,
}

/// A digest being computed on its own thread.
pub struct PendingDigest(std::thread::JoinHandle<String>);

impl PendingDigest {
    /// The digest, waiting for the thread if it is not done.
    pub fn wait(self) -> String {
        self.0.join().expect("the hashing thread does not panic")
    }
}

/// Read a JSON Lines file. Its digest is computed on a second thread, starting while the lines are
/// decoded and finishing while the caller builds its entries: the fingerprint costs no wall time.
pub fn read_jsonl(path: &std::path::Path) -> Result<Jsonl> {
    let text = {
        let _span = crate::timing::span("read file");
        std::sync::Arc::new(
            std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?,
        )
    };
    let hashed = std::sync::Arc::clone(&text);
    let sha256 = PendingDigest(std::thread::spawn(move || {
        let _span = crate::timing::span("fingerprint (parallel)");
        crate::sha256::sha256_hex(hashed.as_bytes())
    }));
    let (rows, unfinished) = rows_from_lines(&text, &path.display().to_string())?;
    Ok(Jsonl {
        rows,
        source_bytes: text,
        unfinished,
        sha256,
    })
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

    /// A text large enough to be decoded on several threads gives the same records, in the same
    /// order, as the one-thread path, and the same answer for a cut-off last line and for a bad
    /// line in the middle.
    #[test]
    fn a_large_text_decodes_the_same_on_several_threads() {
        let line = |n: usize| format!("{{\"n\":{n},\"pad\":\"{}\"}}\n", "x".repeat(200));
        let text: String = (0..20_000).map(line).collect();
        assert!(text.len() > PARALLEL_FROM);
        let (rows, tail) = rows_from_lines(&text, "big").unwrap();
        assert!(tail.is_none());
        assert_eq!(rows.len(), 20_000);
        assert!(
            rows.iter().enumerate().all(|(i, r)| r["n"] == i),
            "native order"
        );

        let cut = format!("{text}{{\"n\":20000,\"pad\":\"x");
        let (rows, tail) = rows_from_lines(&cut, "big").unwrap();
        assert_eq!(rows.len(), 20_000);
        assert_eq!(tail.as_deref(), Some("{\"n\":20000,\"pad\":\"x"));

        let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
        lines[12_345] = "{broken".into();
        let error = rows_from_lines(&lines.join("\n"), "big.jsonl").unwrap_err();
        assert!(
            format!("{error:#}").contains("big.jsonl:12346"),
            "{error:#}"
        );
    }

    #[test]
    fn only_a_cut_off_last_line_is_kept_apart() {
        let (rows, tail) = rows_from_lines("{\"a\":1}\n{\"b\":", "t").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(tail.as_deref(), Some("{\"b\":"));
        let error = rows_from_lines("{\"a\":1}\n{bad\n{\"c\":3}\n", "t.jsonl").unwrap_err();
        assert!(format!("{error:#}").contains("t.jsonl:2"), "{error:#}");
    }

    /// The capture fingerprints the bytes it parsed, not the file as it is later: a source that
    /// changes while a hop runs cannot make the manifest describe a different session.
    #[test]
    fn a_capture_keeps_the_bytes_that_were_parsed_even_if_the_file_changes() {
        let dir = std::env::temp_dir().join(format!("mukade-json-capture-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.jsonl");
        let original = "{\"type\":\"user\",\"text\":\"before\"}\n";
        std::fs::write(&path, original).unwrap();
        let capture = read_jsonl(&path).unwrap();
        std::fs::write(&path, "{\"type\":\"user\",\"text\":\"after\"}\n").unwrap();
        assert_eq!(capture.source_bytes.as_bytes(), original.as_bytes());
        assert_eq!(
            capture.sha256.wait(),
            crate::sha256::sha256_hex(original.as_bytes())
        );
        assert_eq!(capture.rows[0]["text"], "before");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
