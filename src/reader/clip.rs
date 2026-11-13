//! Reading a session of any size (docs/adr/ADR-0004-size-agnostic-session-reads.md).
//!
//! A reader that decodes a whole session before it builds anything holds about five times the
//! file. This module is what the readers share so that no session is too large to read:
//!
//! - [`Clip`] and [`Clipped`]: a read that clips every long string of the source records as they
//!   are decoded, then builds the session from the clipped records;
//! - [`clipped_rows`]: the records of a JSON Lines file, decoded one line at a time when the file
//!   is past the streaming threshold, so the file's text is never held whole;
//! - [`RowReader`]: a record read back, whole, from where the first pass saw it.
//!
//! The clipping is done on the source records, before a reader interprets them, so a session read
//! below the threshold (whole, then clipped) and one read above it (streamed) are the same value.

use crate::model::Session;
use crate::sha256::Sha256;
use anyhow::{Context, Result};
use serde::de::{DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};
use std::io::{BufRead, BufReader};
use std::path::Path;

/// A file this size or smaller is read whole, then clipped; a bigger one is decoded one record at a
/// time. The result is the same; only the memory differs.
pub const STREAM_FROM_BYTES: u64 = 16 * 1024 * 1024;

/// The shortest clip a read accepts. A title is up to 200 characters and a reader recognises some
/// records by the start of a text, so a smaller limit is raised to this one.
pub const MIN_STRING_BYTES: usize = 1024;

/// What a clipped read asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Clip {
    /// Every string longer than this many bytes is cut to a prefix of at most this many, at a
    /// UTF-8 boundary.
    pub max_string_bytes: usize,
    /// The file size past which the records are decoded one at a time
    /// ([`STREAM_FROM_BYTES`] unless a test says otherwise).
    pub stream_from_bytes: u64,
}

impl Clip {
    /// A clip at `max_string_bytes` (at least [`MIN_STRING_BYTES`]), streaming from
    /// [`STREAM_FROM_BYTES`].
    pub fn new(max_string_bytes: usize) -> Self {
        Clip {
            max_string_bytes: max_string_bytes.max(MIN_STRING_BYTES),
            stream_from_bytes: STREAM_FROM_BYTES,
        }
    }

    /// The same clip, streaming from `bytes` instead.
    pub fn streaming_from(self, bytes: u64) -> Self {
        Clip {
            stream_from_bytes: bytes,
            ..self
        }
    }
}

/// What a clipped read cut away. It is how a caller knows a session is clipped; the session's own
/// serialized form carries nothing about it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Clipping {
    /// The limit the read applied.
    pub max_string_bytes: usize,
    /// How many strings were cut.
    pub strings: u64,
    /// How many bytes those strings lost.
    pub bytes_dropped: u64,
    /// Whether the file was decoded one record at a time (it was past the threshold).
    pub streamed: bool,
}

impl Clipping {
    /// Whether any string was cut: the session is not the full read.
    pub fn is_clipped(&self) -> bool {
        self.strings > 0
    }
}

/// A session read with its long strings clipped. It is a different type from [`Session`] on
/// purpose: a clipped session is for showing and searching, and cannot be written to another
/// harness by mistake.
#[derive(Debug, Clone, PartialEq)]
pub struct Clipped {
    /// The session: the same entries, ids, kinds and order as the full read; every string is the
    /// full read's, or a prefix of it.
    pub session: Session,
    pub clipping: Clipping,
}

/// Key paths (from a record's root, arrays not counted) whose subtree a reader parses, so a clip
/// would change what it reads: Codex's tool-call `arguments` and custom tool-call `input`.
pub(crate) type Keep = &'static [&'static [&'static str]];

/// No path is kept.
pub(crate) const KEEP_NONE: Keep = &[];

/// The longest prefix of `text` that is at most `max` bytes and ends on a character boundary.
fn cut_at(text: &str, max: usize) -> usize {
    let mut at = max.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

#[derive(Default)]
struct Tally {
    strings: u64,
    bytes: u64,
}

impl Tally {
    fn clipping(&self, max: usize, streamed: bool) -> Clipping {
        Clipping {
            max_string_bytes: max,
            strings: self.strings,
            bytes_dropped: self.bytes,
            streamed,
        }
    }
}

// --- clipping a value already decoded -------------------------------------------------------

/// Clip every string of `value` (the keys are not strings of the text), but the subtrees `keep`
/// names.
fn walk<'a>(
    value: &'a mut Value,
    max: usize,
    keep: Keep,
    path: &mut Vec<&'a str>,
    tally: &mut Tally,
) {
    match value {
        Value::String(text) => {
            if text.len() > max {
                let at = cut_at(text, max);
                tally.strings += 1;
                tally.bytes += (text.len() - at) as u64;
                text.truncate(at);
            }
        }
        Value::Array(items) => {
            for item in items {
                walk(item, max, keep, path, tally);
            }
        }
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                path.push(key.as_str());
                if !keep.iter().any(|kept| *kept == path.as_slice()) {
                    walk(child, max, keep, path, tally);
                }
                path.pop();
            }
        }
        _ => {}
    }
}

/// Clip `value` in place; the tally is added to `into`.
fn clip_value(value: &mut Value, max: usize, keep: Keep, into: &mut Tally) {
    walk(value, max, keep, &mut Vec::new(), into);
}

/// A session with every string clipped, built from the session itself: for a harness whose reader
/// has no clipped read of its own. The strings are cut after the reader has interpreted them.
pub fn clip_session(session: &Session, max_string_bytes: usize) -> Result<Clipped> {
    let max = max_string_bytes.max(MIN_STRING_BYTES);
    let mut value = serde_json::to_value(session).context("encode the session")?;
    let mut tally = Tally::default();
    clip_value(&mut value, max, KEEP_NONE, &mut tally);
    Ok(Clipped {
        session: serde_json::from_value(value).context("decode the clipped session")?,
        clipping: tally.clipping(max, false),
    })
}

// --- decoding a record with its strings clipped as they come ---------------------------------

struct Ctx {
    max: usize,
    keep: Keep,
    /// The longest kept path: keys deeper than this are not followed.
    cap: usize,
    path: Vec<String>,
    tally: Tally,
}

/// A `DeserializeSeed` that builds a [`Value`] and cuts every long string as the decoder hands it
/// over, so the cut-away part is not copied. `kept` is a subtree that is not clipped.
struct Seed<'a> {
    ctx: &'a mut Ctx,
    kept: bool,
}

impl<'de> DeserializeSeed<'de> for Seed<'_> {
    type Value = Value;

    fn deserialize<D: serde::Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Seed<'_> {
    type Value = Value;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("any valid JSON value")
    }

    fn visit_bool<E>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }

    fn visit_i64<E>(self, v: i64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }

    fn visit_u64<E>(self, v: u64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }

    fn visit_f64<E>(self, v: f64) -> Result<Value, E> {
        Ok(Number::from_f64(v).map_or(Value::Null, Value::Number))
    }

    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_str<E>(self, v: &str) -> Result<Value, E> {
        if self.kept || v.len() <= self.ctx.max {
            return Ok(Value::String(v.to_owned()));
        }
        let at = cut_at(v, self.ctx.max);
        self.ctx.tally.strings += 1;
        self.ctx.tally.bytes += (v.len() - at) as u64;
        Ok(Value::String(v[..at].to_owned()))
    }

    fn visit_string<E>(self, mut v: String) -> Result<Value, E> {
        if !self.kept && v.len() > self.ctx.max {
            let at = cut_at(&v, self.ctx.max);
            self.ctx.tally.strings += 1;
            self.ctx.tally.bytes += (v.len() - at) as u64;
            v.truncate(at);
        }
        Ok(Value::String(v))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut items = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(4096));
        while let Some(item) = seq.next_element_seed(Seed {
            ctx: &mut *self.ctx,
            kept: self.kept,
        })? {
            items.push(item);
        }
        Ok(Value::Array(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut object = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            let tracked = !self.kept && self.ctx.path.len() < self.ctx.cap;
            let mut kept = self.kept;
            if tracked {
                self.ctx.path.push(key.clone());
                kept = self.ctx.keep.iter().any(|path| {
                    path.iter()
                        .copied()
                        .eq(self.ctx.path.iter().map(String::as_str))
                });
            }
            let value = map.next_value_seed(Seed {
                ctx: &mut *self.ctx,
                kept,
            })?;
            if tracked {
                self.ctx.path.pop();
            }
            object.insert(key, value);
        }
        Ok(Value::Object(object))
    }
}

/// One record, decoded with its long strings cut; the tally grows by what was cut.
fn decode_record(text: &str, ctx: &mut Ctx) -> serde_json::Result<Value> {
    let mut decoder = serde_json::Deserializer::from_str(text);
    let value = Seed { ctx, kept: false }.deserialize(&mut decoder)?;
    decoder.end()?;
    Ok(value)
}

fn new_ctx(max: usize, keep: Keep) -> Ctx {
    Ctx {
        max,
        keep,
        cap: keep.iter().map(|path| path.len()).max().unwrap_or(0),
        path: Vec::new(),
        tally: Tally::default(),
    }
}

// --- a JSON Lines file, clipped -------------------------------------------------------------

/// The records of a JSON Lines file, clipped, and what a reader needs besides them.
pub(crate) struct ClippedRows {
    pub rows: Vec<Value>,
    /// Where each record's line starts in the file; empty when the file was read whole.
    pub offsets: Vec<u64>,
    /// A last line that did not parse (a hard kill cut it off), clipped, and where it starts.
    pub unfinished: Option<(String, u64)>,
    /// The SHA-256 of the file's bytes.
    pub sha256: String,
    pub clipping: Clipping,
}

/// What `str::lines` leaves of a line: no `\n`, and no `\r` before it.
fn without_break(line: &str) -> &str {
    let line = line.strip_suffix('\n').unwrap_or(line);
    line.strip_suffix('\r').unwrap_or(line)
}

/// The records of the JSON Lines file at `path` with every string longer than `clip` cut, a last
/// line that does not parse kept apart, and the file's digest. Up to `clip.stream_from_bytes` the
/// file is read whole as the readers always have, then clipped; above it the file is decoded one
/// line at a time and its text is never held whole. Either way the records are the same, and
/// so are the errors: a bad line anywhere but last names its line.
pub(crate) fn clipped_rows(path: &Path, clip: &Clip, keep: Keep) -> Result<ClippedRows> {
    let size = std::fs::metadata(path)
        .with_context(|| format!("read {}", path.display()))?
        .len();
    if size <= clip.stream_from_bytes {
        let file = super::json::read_jsonl(path)?;
        let mut tally = Tally::default();
        let mut rows = file.rows;
        for row in &mut rows {
            clip_value(row, clip.max_string_bytes, keep, &mut tally);
        }
        let unfinished = file.unfinished.map(|mut line| {
            let mut value = Value::String(std::mem::take(&mut line));
            clip_value(&mut value, clip.max_string_bytes, KEEP_NONE, &mut tally);
            (value.as_str().unwrap_or_default().to_owned(), 0)
        });
        return Ok(ClippedRows {
            rows,
            offsets: Vec::new(),
            unfinished,
            sha256: file.sha256.wait(),
            clipping: tally.clipping(clip.max_string_bytes, false),
        });
    }
    decode_streaming(path, clip, keep)
}

/// A line that did not parse, kept until the next non-blank line says whether it was the last.
struct Bad {
    line: usize,
    error: serde_json::Error,
    text: String,
    at: u64,
}

fn decode_streaming(path: &Path, clip: &Clip, keep: Keep) -> Result<ClippedRows> {
    let what = path.display().to_string();
    let file = std::fs::File::open(path).with_context(|| format!("read {what}"))?;
    let mut reader = BufReader::with_capacity(1 << 20, file);
    let mut digest = Sha256::new();
    let mut ctx = new_ctx(clip.max_string_bytes, keep);
    let mut rows = Vec::new();
    let mut offsets = Vec::new();
    let mut bad: Option<Bad> = None;
    let mut line = Vec::new();
    let (mut at, mut number) = (0u64, 0usize);
    loop {
        line.clear();
        let read = reader
            .read_until(b'\n', &mut line)
            .with_context(|| format!("read {what}"))?;
        if read == 0 {
            break;
        }
        digest.update(&line);
        let start = at;
        at += read as u64;
        number += 1;
        let text = std::str::from_utf8(&line)
            .map_err(|_| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "stream did not contain valid UTF-8",
                )
            })
            .with_context(|| format!("read {what}"))?;
        let text = if number == 1 {
            text.trim_start_matches('\u{feff}')
        } else {
            text
        };
        if text.trim().is_empty() {
            continue;
        }
        if let Some(bad) = bad.take() {
            // Something follows the line that did not parse: it was not a cut-off last line.
            return Err(bad.error).with_context(|| format!("{what}:{}", bad.line));
        }
        match decode_record(text, &mut ctx) {
            Ok(row) => {
                rows.push(row);
                offsets.push(start);
            }
            Err(error) => {
                let line_text = without_break(text);
                let cut = cut_at(line_text, clip.max_string_bytes);
                ctx.tally.strings += u64::from(cut < line_text.len());
                ctx.tally.bytes += (line_text.len() - cut) as u64;
                bad = Some(Bad {
                    line: number,
                    error,
                    text: line_text[..cut].to_owned(),
                    at: start,
                });
            }
        }
    }
    Ok(ClippedRows {
        rows,
        offsets,
        unfinished: bad.map(|bad| (bad.text, bad.at)),
        sha256: digest.finalize_hex(),
        clipping: ctx.tally.clipping(clip.max_string_bytes, true),
    })
}

/// Records read back whole from a JSON Lines file, by the offset [`clipped_rows`] saw their line
/// at. Reading them in the order they sit costs no seek.
pub(crate) struct RowReader {
    reader: BufReader<std::fs::File>,
    what: String,
    /// Where the next byte read comes from.
    at: u64,
    line: Vec<u8>,
}

impl RowReader {
    pub(crate) fn open(path: &Path) -> Result<Self> {
        let what = path.display().to_string();
        let file = std::fs::File::open(path).with_context(|| format!("read {what}"))?;
        Ok(RowReader {
            reader: BufReader::with_capacity(1 << 20, file),
            what,
            at: 0,
            line: Vec::new(),
        })
    }

    /// The line that starts at `offset`, without its line break.
    fn line_at(&mut self, offset: u64) -> Result<&str> {
        if offset != self.at {
            let delta = i64::try_from(offset).unwrap_or(i64::MAX)
                - i64::try_from(self.at).unwrap_or(i64::MAX);
            self.reader
                .seek_relative(delta)
                .with_context(|| format!("read {}", self.what))?;
        }
        self.line.clear();
        let read = self
            .reader
            .read_until(b'\n', &mut self.line)
            .with_context(|| format!("read {}", self.what))?;
        self.at = offset + read as u64;
        let text = std::str::from_utf8(&self.line)
            .map_err(|_| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "stream did not contain valid UTF-8",
                )
            })
            .with_context(|| format!("read {}", self.what))?;
        Ok(if offset == 0 {
            text.trim_start_matches('\u{feff}')
        } else {
            text
        })
    }

    /// The record whose line starts at `offset`, whole.
    pub(crate) fn row_at(&mut self, offset: u64) -> Result<Value> {
        let what = self.what.clone();
        let text = self.line_at(offset)?;
        serde_json::from_str(text).with_context(|| format!("{what}: the record at byte {offset}"))
    }

    /// The text of the line that starts at `offset`, whole, as `str::lines` gives it.
    pub(crate) fn text_at(&mut self, offset: u64) -> Result<String> {
        Ok(without_break(self.line_at(offset)?).to_owned())
    }
}

/// What the pass that finds structure and offsets clips at: structure is all it needs.
pub(crate) const STRUCTURE_PASS: Clip = Clip {
    max_string_bytes: MIN_STRING_BYTES,
    stream_from_bytes: 0,
};

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(max: usize) -> Ctx {
        new_ctx(max, KEEP_NONE)
    }

    #[test]
    fn a_string_is_cut_at_a_character_boundary() {
        let mut c = ctx(5);
        // "é" is two bytes: a cut at 5 falls inside the third one.
        let row = decode_record(r#"{"a":"ééé","b":"short","n":[1,2.5,null,true]}"#, &mut c)
            .expect("decodes");
        assert_eq!(row["a"], "éé");
        assert_eq!(row["b"], "short");
        assert_eq!(row["n"], serde_json::json!([1, 2.5, null, true]));
        assert_eq!((c.tally.strings, c.tally.bytes), (1, 2));
    }

    #[test]
    fn decoding_clipped_equals_decoding_whole_then_clipping() {
        let text = r#"{"x":"abcdefghijklmnop","k":{"y":["qrstuvwxyz0123",{"z":"é😀é😀é😀"}]},"e":"a\nbéc\"d"}"#;
        for max in [1usize, 2, 3, 5, 8, 13, 100] {
            let mut streaming = ctx(max);
            let a = decode_record(text, &mut streaming).expect("decodes");
            let mut whole: Value = serde_json::from_str(text).expect("parses");
            let mut tally = Tally::default();
            clip_value(&mut whole, max, KEEP_NONE, &mut tally);
            assert_eq!(a, whole, "max {max}");
            assert_eq!(
                (streaming.tally.strings, streaming.tally.bytes),
                (tally.strings, tally.bytes),
                "max {max}"
            );
        }
    }

    #[test]
    fn a_kept_path_is_left_whole_by_both_ways() {
        const KEEP: Keep = &[&["payload", "arguments"]];
        let text = r#"{"payload":{"arguments":"0123456789","other":"0123456789"},"arguments":"0123456789"}"#;
        let mut streaming = new_ctx(4, KEEP);
        let a = decode_record(text, &mut streaming).expect("decodes");
        let mut whole: Value = serde_json::from_str(text).expect("parses");
        let mut tally = Tally::default();
        clip_value(&mut whole, 4, KEEP, &mut tally);
        assert_eq!(a, whole);
        assert_eq!(a["payload"]["arguments"], "0123456789");
        assert_eq!(a["payload"]["other"], "0123");
        assert_eq!(a["arguments"], "0123");
    }

    #[test]
    fn an_object_keeps_its_key_order() {
        let mut c = ctx(100);
        let row = decode_record(r#"{"b":1,"a":2,"c":3}"#, &mut c).expect("decodes");
        let keys: Vec<&String> = row.as_object().expect("an object").keys().collect();
        assert_eq!(keys, ["b", "a", "c"]);
    }
}
