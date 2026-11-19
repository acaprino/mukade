//! The DeepSeek Harness (`dsh` 0.2.2) writer: session format 4 (docs/harnesses/deepseek-harness.md).
//!
//! Shape it must produce:
//!
//! - `sessions/<project key>/<encoded id>/session.v4.jsonl.zstd`: the header line alone in frame 1,
//!   then the events, each frame with the content checksum on. dsh refuses a session whose folder
//!   is not the one its id and `cwd` name, so the folder names are dsh's own (`projectKey`,
//!   `encodeSegment`). A root that holds plain `session.v4.jsonl` logs is written plain: dsh opens
//!   a root in one encoding only;
//! - the header carries `cwd` only when it is an absolute path on this machine, else the session is
//!   filed under `_no-cwd`;
//! - events number `seq` from 0 with no gap; the five surface events (`system`, `user`,
//!   `assistant`, `tool/result`, `developer`) carry `surfaceOp: "append"`;
//! - a turn is `turn/start`, then steps (`step/start` ... `step/end`) holding the messages, then
//!   `turn/end`; every call of an assistant message is also logged by a `tool/call` event, which is
//!   how dsh knows the call started, and its result cites that event in `sourceEventSeqs`;
//! - an assistant message has no `replayState`: dsh reads it as another provider's message and
//!   converts it itself;
//! - node 0 of the surface is a `system/message` with no content: dsh renders its own system prompt
//!   on the next step and replaces that node.

use super::plan::{self, PlannedBlock, PlannedResult, PlannedStep};
use super::{WriteOpts, Writer, Written, write_bytes};
use crate::model::{HarnessId, Session};
use crate::policy::Clock;
use crate::reader::zstd_frames;
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// The DeepSeek Harness writer.
#[derive(Debug, Clone, Copy, Default)]
pub struct DeepSeekHarness;

/// How many events one frame holds: dsh writes a frame per durable batch, and a batch is small.
const EVENTS_PER_FRAME: usize = 256;

impl Writer for DeepSeekHarness {
    fn harness(&self) -> HarnessId {
        HarnessId::DeepSeekHarness
    }

    fn write_planned(
        &self,
        session: &Session,
        plan: &plan::Plan,
        opts: &WriteOpts,
    ) -> Result<Written> {
        crate::harnesses::destination(HarnessId::DeepSeekHarness)?.validate_write(opts)?;
        let sessions = opts
            .roots
            .dsh_sessions()
            .context("DSH_HOME is not set: nowhere to write the DeepSeek Harness session")?;
        let cwd = opts.cwd.clone().or_else(|| session.meta.cwd.clone());
        if let Some(cwd) = &cwd {
            anyhow::ensure!(
                is_absolute(cwd),
                "{cwd:?} is not an absolute path on this machine: dsh refuses a session header \
                 with a relative cwd (give --cwd)"
            );
        }
        let id = opts
            .dest_id
            .clone()
            .unwrap_or_else(|| format!("session-{}", uuid::Uuid::new_v4()));
        let mut clock = Clock::new(opts.now.as_deref());
        let created = clock.base_millis();
        let header = header(&id, created, cwd.as_deref());
        let events = events(plan, session, opts, &mut clock);

        let dir = project_dir(&sessions, cwd.as_deref()).join(encode_segment(&id));
        let compression = root_compression(&sessions);
        let file = dir.join(compression.file_name());
        let bytes = compression.encode(&header, &events);
        write_bytes(&file, &bytes)?;

        Ok(Written {
            records: Vec::new(),
            id: id.clone(),
            location: file.to_string_lossy().into_owned(),
            resume: crate::catalog::resume(
                HarnessId::DeepSeekHarness,
                opts.exe.as_deref().unwrap_or("dsh"),
                &id,
                Some(&file),
            ),
        })
    }
}

// ---------------------------------------------------------------------------------------------
// folders

/// Whether `path` is absolute where dsh runs (Node's `isAbsolute`): on Windows a leading separator
/// or a drive and a separator, elsewhere a leading `/`.
fn is_absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    if cfg!(windows) {
        matches!(bytes.first(), Some(b'/' | b'\\'))
            || (bytes.len() >= 3
                && bytes[0].is_ascii_alphabetic()
                && bytes[1] == b':'
                && matches!(bytes[2], b'/' | b'\\'))
    } else {
        bytes.first() == Some(&b'/')
    }
}

/// Whether a UTF-16 unit is one dsh writes as it is in a path segment.
fn is_safe_unit(unit: u16) -> bool {
    u8::try_from(unit)
        .is_ok_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

/// dsh's `encodeSegment`: every UTF-16 unit but `[A-Za-z0-9._-]` (and `~` itself) becomes `~` and
/// four uppercase hex digits; `.` and `..` are escaped whole.
pub fn encode_segment(raw: &str) -> String {
    match raw {
        "." => return "~002E".into(),
        ".." => return "~002E~002E".into(),
        _ => {}
    }
    raw.encode_utf16()
        .map(|unit| {
            if is_safe_unit(unit) {
                char::from(unit as u8).to_string()
            } else {
                format!("~{unit:04X}")
            }
        })
        .collect()
}

/// dsh's `projectKey`: a run of `/`, `\` and `:` is one `-`, the other units are escaped as in
/// [`encode_segment`], leading `-` are dropped, an empty key is `root`, at most 251 characters,
/// between `--`.
pub fn project_key(cwd: &str) -> String {
    let mut readable = String::new();
    let mut in_separator = false;
    for unit in cwd.encode_utf16() {
        if matches!(unit, 0x2F | 0x5C | 0x3A) {
            if !in_separator {
                readable.push('-');
            }
            in_separator = true;
        } else {
            if is_safe_unit(unit) {
                readable.push(char::from(unit as u8));
            } else {
                readable.push_str(&format!("~{unit:04X}"));
            }
            in_separator = false;
        }
    }
    let trimmed = readable.trim_start_matches('-');
    let key = if trimmed.is_empty() { "root" } else { trimmed };
    // The key is ASCII: a character is a UTF-16 unit.
    format!("--{}--", &key[..key.len().min(251)])
}

/// The folder a session of `cwd` is filed in.
fn project_dir(sessions: &Path, cwd: Option<&str>) -> PathBuf {
    match cwd {
        Some(cwd) => sessions.join(project_key(cwd)),
        None => sessions.join("_no-cwd"),
    }
}

// ---------------------------------------------------------------------------------------------
// the container

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Compression {
    Zstd,
    None,
}

impl Compression {
    fn file_name(self) -> &'static str {
        match self {
            Compression::Zstd => "session.v4.jsonl.zstd",
            Compression::None => "session.v4.jsonl",
        }
    }

    fn encode(self, header: &Value, events: &[Value]) -> Vec<u8> {
        let line = |row: &Value| format!("{row}\n");
        match self {
            Compression::Zstd => {
                let mut bytes = zstd_frames::encode_frame(line(header).as_bytes());
                for batch in events.chunks(EVENTS_PER_FRAME) {
                    let text: String = batch.iter().map(line).collect();
                    bytes.extend(zstd_frames::encode_frame(text.as_bytes()));
                }
                bytes
            }
            Compression::None => std::iter::once(header)
                .chain(events)
                .map(line)
                .collect::<String>()
                .into_bytes(),
        }
    }
}

/// How the root `sessions` is encoded: plain when it holds plain logs and no zstd one, else
/// zstd, dsh's default. A root is opened in one encoding only.
fn root_compression(sessions: &Path) -> Compression {
    let (mut plain, mut zstd) = (false, false);
    for project in std::fs::read_dir(sessions).into_iter().flatten().flatten() {
        for folder in std::fs::read_dir(project.path())
            .into_iter()
            .flatten()
            .flatten()
        {
            plain |= folder.path().join("session.v4.jsonl").is_file();
            zstd |= folder.path().join("session.v4.jsonl.zstd").is_file();
        }
    }
    if plain && !zstd {
        Compression::None
    } else {
        Compression::Zstd
    }
}

// ---------------------------------------------------------------------------------------------
// records

fn header(id: &str, created: i64, cwd: Option<&str>) -> Value {
    let mut header = json!({"type": "session", "version": 4, "id": id, "createdAt": created});
    if let Some(cwd) = cwd {
        header["cwd"] = json!(cwd);
    }
    header["isSeeded"] = json!(false);
    header["delegationDepth"] = json!(0);
    header
}

/// The events under construction: numbered from 0, stamped by the clock.
struct Log<'a> {
    events: Vec<Value>,
    clock: &'a mut Clock,
    turn: u64,
    step: u64,
    open_turn: bool,
    open_step: bool,
    /// The open step already holds its assistant message: the next one is a new step.
    step_answered: bool,
    system_written: bool,
}

fn message_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

impl Log<'_> {
    fn push(&mut self, kind: &str, data: Value, extra: &[(&str, Value)]) -> u64 {
        let seq = self.events.len() as u64;
        let mut event =
            json!({"type": kind, "seq": seq, "time": self.clock.next_millis(), "data": data});
        for (key, value) in extra {
            event[*key] = value.clone();
        }
        self.events.push(event);
        seq
    }

    fn append(&mut self, kind: &str, data: Value) -> u64 {
        self.push(kind, data, &[("surfaceOp", json!("append"))])
    }

    fn begin_step(&mut self) {
        self.step += 1;
        self.push(
            "step/start",
            json!({"turn": self.turn, "step": self.step}),
            &[],
        );
        self.open_step = true;
        self.step_answered = false;
    }

    fn end_step(&mut self) {
        if self.open_step {
            self.push(
                "step/end",
                json!({"turn": self.turn, "step": self.step}),
                &[],
            );
            self.open_step = false;
        }
    }

    fn begin_turn(&mut self) {
        self.turn += 1;
        self.step = 0;
        self.push("turn/start", json!({"turn": self.turn}), &[]);
        self.open_turn = true;
        self.begin_step();
        if !self.system_written {
            self.system_written = true;
            self.append(
                "system/message",
                json!({"turn": self.turn, "step": self.step, "message": {
                    "role": "system", "content": [], "source": {"kind": "system-prompt"},
                    "id": message_id()}}),
            );
        }
    }

    fn end_turn(&mut self) {
        self.end_step();
        if self.open_turn {
            self.push(
                "turn/end",
                json!({"turn": self.turn, "reason": {"kind": "completed"}}),
                &[],
            );
            self.open_turn = false;
        }
    }

    fn user(&mut self, parts: Vec<Value>) -> u64 {
        self.append(
            "user/message",
            json!({"role": "user", "content": parts, "source": {"kind": "user"}, "id": message_id()}),
        )
    }
}

fn text_part(text: &str) -> Value {
    json!({"type": "text", "text": text})
}

/// An attachment as dsh holds it: its own `image` and `file` records, kept as they were. Another
/// harness's image has no counterpart in dsh's attachment store, and the plan has dropped it.
fn attachment_part(raw: &Value) -> Option<Value> {
    let kind = raw.get("type").and_then(Value::as_str)?;
    (matches!(kind, "image" | "file") && raw.get("attachment").is_some_and(Value::is_string))
        .then(|| raw.clone())
}

/// A tool result's content as dsh's parts: text, and the attachment parts dsh itself wrote.
fn result_parts(content: &crate::model::ResultContent) -> Vec<Value> {
    use crate::model::ResultContent;
    match content {
        ResultContent::Text(text) => vec![text_part(text)],
        ResultContent::Blocks(blocks) => blocks
            .iter()
            .filter(|block| {
                block.get("type").and_then(Value::as_str) == Some("text")
                    || attachment_part(block).is_some()
            })
            .cloned()
            .collect(),
    }
}

fn events(plan: &plan::Plan, session: &Session, opts: &WriteOpts, clock: &mut Clock) -> Vec<Value> {
    let mut log = Log {
        events: Vec::new(),
        clock,
        turn: 0,
        step: 0,
        open_turn: false,
        open_step: false,
        step_answered: false,
        system_written: false,
    };
    let mut titled = false;
    let mut title = |log: &mut Log, first_user: u64| {
        if titled {
            return;
        }
        titled = true;
        if let Some(text) = opts.title.as_deref().filter(|t| !t.trim().is_empty()) {
            log.push(
                "session/title",
                json!({"title": text, "messageSeqs": [first_user], "source": {"kind": "fallback"}}),
                &[],
            );
        }
    };
    let mut i = 0;
    while i < plan.blocks.len() {
        match &plan.blocks[i] {
            PlannedBlock::User { text, attachments } => {
                log.end_turn();
                log.begin_turn();
                let mut parts = Vec::new();
                if !text.is_empty() {
                    parts.push(text_part(text));
                }
                parts.extend(attachments.iter().filter_map(|a| attachment_part(&a.raw)));
                let seq = log.user(parts);
                title(&mut log, seq);
            }
            // A summary is carried as the user's message, as every file destination does (L4).
            PlannedBlock::Compaction { summary } => {
                log.end_turn();
                log.begin_turn();
                let seq = log.user(vec![text_part(summary)]);
                title(&mut log, seq);
            }
            PlannedBlock::Step(step) => {
                if !log.open_turn {
                    log.begin_turn();
                } else if log.step_answered {
                    log.end_step();
                    log.begin_step();
                }
                let calls = assistant(&mut log, step, session, opts);
                // Results are written beside the step they answer, in the call order.
                if let Some(PlannedBlock::Results { results }) = plan.blocks.get(i + 1) {
                    for result in results {
                        match result {
                            PlannedResult::Native {
                                call_id,
                                content,
                                is_error,
                            } => {
                                let Some(call_seq) = calls
                                    .iter()
                                    .find(|(id, _)| id == call_id)
                                    .map(|(_, seq)| *seq)
                                else {
                                    continue;
                                };
                                let (turn, step) = (log.turn, log.step);
                                log.push(
                                    "tool/result",
                                    json!({"turn": turn, "step": step, "message": {
                                        "role": "tool", "toolCallId": call_id,
                                        "source": {"kind": "tool", "callId": call_id},
                                        "content": result_parts(content), "isError": is_error,
                                        "id": message_id()}}),
                                    &[
                                        ("surfaceOp", json!("append")),
                                        ("sourceEventSeqs", json!([call_seq])),
                                    ],
                                );
                            }
                            // A call that travelled as text answers with text, in the same place.
                            PlannedResult::Text(text) => {
                                log.user(vec![text_part(text)]);
                            }
                        }
                    }
                    i += 1;
                }
            }
            // Results are written beside the step they answer; a stray one has no call left.
            PlannedBlock::Results { .. } => {}
        }
        i += 1;
    }
    log.end_turn();
    log.events
}

/// One assistant message and its `tool/call` events; the calls' event numbers by call id.
fn assistant(
    log: &mut Log,
    step: &PlannedStep,
    session: &Session,
    opts: &WriteOpts,
) -> Vec<(String, u64)> {
    let mut content: Vec<Value> = Vec::new();
    for reasoning in step.native_reasoning() {
        if !reasoning.text.is_empty() {
            content.push(json!({"type": "reasoning", "text": reasoning.text}));
        }
    }
    for text in &step.text {
        content.push(text_part(text));
    }
    for call in &step.calls {
        content.push(
            json!({"type": "tool-call", "id": call.id, "name": call.name,
            "arguments": call.input.to_string()}),
        );
    }
    let provider = plan::provider_id(step, session, opts).unwrap_or_else(|| "unknown".into());
    let model = plan::model_id(step, session, opts).unwrap_or_else(|| "unknown".into());
    let (turn, number) = (log.turn, log.step);
    log.append(
        "assistant/message",
        json!({"turn": turn, "step": number, "message": {
            "role": "assistant", "content": content,
            "source": {"kind": "model", "provider": provider, "model": model},
            "id": message_id()}, "stream": []}),
    );
    log.step_answered = true;
    step.calls
        .iter()
        .map(|call| {
            let seq = log.push(
                "tool/call",
                json!({"turn": turn, "step": number, "callId": call.id, "name": call.name,
                    "arguments": call.input.to_string()}),
                &[],
            );
            (call.id.clone(), seq)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_and_project_keys_are_dshs_own() {
        assert_eq!(
            encode_segment("session-6de20419-8d24"),
            "session-6de20419-8d24"
        );
        assert_eq!(encode_segment("a b~c"), "a~0020b~007Ec");
        assert_eq!(encode_segment("."), "~002E");
        assert_eq!(encode_segment(".."), "~002E~002E");
        assert_eq!(encode_segment("é"), "~00E9");
        // A character outside the BMP is two UTF-16 units.
        assert_eq!(encode_segment("😀"), "~D83D~DE00");
        assert_eq!(project_key("/home/u/p"), "--home-u-p--");
        assert_eq!(
            project_key(r"D:\Projects\foo bar"),
            "--D-Projects-foo~0020bar--"
        );
        assert_eq!(project_key("//a//b"), "--a-b--");
        assert_eq!(project_key("/"), "--root--");
        assert_eq!(project_key("/a~b"), "--a~007Eb--");
        assert_eq!(
            project_key(&format!("/{}", "x".repeat(300))),
            format!("--{}--", "x".repeat(251))
        );
    }

    #[test]
    fn a_cwd_is_absolute_where_dsh_runs() {
        assert!(is_absolute("/work/demo"));
        assert_eq!(is_absolute(r"D:\work"), cfg!(windows));
        assert_eq!(is_absolute("D:/work"), cfg!(windows));
        assert!(!is_absolute("work/demo"));
        assert!(!is_absolute(""));
        assert!(!is_absolute("D:"));
    }
}
