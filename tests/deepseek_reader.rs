//! Conformance tests for the DeepSeek Harness reader (`dsh`, session format 4).
//!
//! The fixture (`fixtures/deepseek_harness/source`) is a synthetic session, written once as plain
//! JSONL and once as a multi-frame Zstandard file with the content checksum on, compressed by a
//! real encoder. Every other case builds its log here, event by event.

use mukade::harnesses;
use mukade::model::{
    CanonicalTool, HarnessId, Kind, MetaKind, ParentKind, ResultContent, Session, StepEnd, Trigger,
};
use mukade::reader::{Lookup, Reader, deepseek_harness as dsh, zstd_frames};
use mukade::roots::StoreRoots;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

mod support;

fn fixture() -> PathBuf {
    support::source_path(HarnessId::DeepSeekHarness)
}

fn plain_fixture() -> PathBuf {
    fixture().with_file_name("session.v4.jsonl")
}

fn kinds(session: &Session) -> Vec<&'static str> {
    session
        .conversation()
        .map(|e| support::kind_name(&e.kind))
        .collect()
}

fn meta_kinds(session: &Session) -> Vec<MetaKind> {
    session
        .entries
        .iter()
        .filter_map(|e| match &e.kind {
            Kind::Meta { kind } => Some(*kind),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// building logs

/// A log under construction: the header, and events numbered as dsh numbers them.
struct Log {
    header: Value,
    events: Vec<Value>,
}

impl Log {
    fn new(id: &str) -> Log {
        Log {
            header: json!({"type": "session", "version": 4, "id": id, "createdAt": 1_789_000_000_000i64,
                "cwd": "/work/demo", "isSeeded": false, "delegationDepth": 0}),
            events: Vec::new(),
        }
    }

    fn with_header(mut self, extra: Value) -> Log {
        for (key, value) in extra.as_object().unwrap() {
            self.header[key] = value.clone();
        }
        self
    }

    /// Add an event; `extra` is merged into the envelope (`surfaceOp`, `ignorable`, …).
    fn push(&mut self, kind: &str, data: Value, extra: Value) -> u64 {
        let seq = self.events.len() as u64;
        let mut event = json!({"type": kind, "seq": seq, "time": 1_789_000_000_000i64 + seq as i64, "data": data});
        if let Some(extra) = extra.as_object() {
            for (key, value) in extra {
                event[key] = value.clone();
            }
        }
        self.events.push(event);
        seq
    }

    fn append(&mut self, kind: &str, data: Value) -> u64 {
        self.push(kind, data, json!({"surfaceOp": "append"}))
    }

    fn user(&mut self, text: &str) -> u64 {
        self.append(
            "user/message",
            json!({"role": "user", "content": [{"type": "text", "text": text}], "source": {"kind": "user"}}),
        )
    }

    fn user_of(&mut self, kind: &str, text: &str) -> u64 {
        self.append(
            "user/message",
            json!({"role": "user", "content": [{"type": "text", "text": text}], "source": {"kind": kind}}),
        )
    }

    fn assistant(&mut self, turn: u64, parts: Value) -> u64 {
        self.append(
            "assistant/message",
            json!({"turn": turn, "step": 1, "message": {"role": "assistant", "content": parts,
                "source": {"kind": "model", "provider": "openrouter", "model": "m"}}, "stream": []}),
        )
    }

    fn call(name: &str, id: &str, arguments: &str) -> Value {
        json!({"type": "tool-call", "id": id, "name": name, "arguments": arguments})
    }

    fn logged(&mut self, name: &str, id: &str, arguments: &str) -> u64 {
        self.push(
            "tool/call",
            json!({"turn": 1, "step": 1, "callId": id, "name": name, "arguments": arguments}),
            json!({}),
        )
    }

    fn result(&mut self, id: &str, text: &str) -> u64 {
        self.append(
            "tool/result",
            json!({"turn": 1, "step": 1, "message": {"role": "tool", "toolCallId": id,
                "source": {"kind": "tool", "callId": id}, "content": [{"type": "text", "text": text}], "isError": false}}),
        )
    }

    fn lines(&self) -> Vec<String> {
        std::iter::once(&self.header)
            .chain(&self.events)
            .map(|row| format!("{row}\n"))
            .collect()
    }

    fn plain(&self) -> Vec<u8> {
        self.lines().concat().into_bytes()
    }

    /// The Zstandard file dsh writes: the header alone in frame 1, then one frame per event.
    fn zstd(&self) -> Vec<u8> {
        self.lines()
            .iter()
            .flat_map(|line| stored_frame(line.as_bytes()))
            .collect()
    }
}

/// A frame of stored blocks with the content checksum: valid Zstandard, written without an
/// encoder so a test can say exactly where a byte is.
fn stored_frame(content: &[u8]) -> Vec<u8> {
    let mut out = 0xFD2F_B528u32.to_le_bytes().to_vec();
    out.push(0b1010_0100);
    out.extend((content.len() as u32).to_le_bytes());
    let header = 1u32 | (content.len() as u32) << 3;
    out.extend(&header.to_le_bytes()[..3]);
    out.extend(content);
    out.extend((zstd_frames::xxh64(content, 0) as u32).to_le_bytes());
    out
}

fn write_to(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

/// Read a log through a file in a scratch folder.
fn read_log(log: &Log, tag: &str) -> anyhow::Result<Session> {
    let scratch = support::scratch(HarnessId::DeepSeekHarness, tag);
    let path = write_to(&scratch, "session.v4.jsonl", &log.plain());
    dsh::read_file(&path)
}

fn session_of(log: &Log, tag: &str) -> Session {
    read_log(log, tag).unwrap_or_else(|error| panic!("{tag}: {error:#}"))
}

/// A store with `sessions/<project>/<folder>/session.v4.jsonl.zstd` for each log.
fn store(tag: &str, logs: &[(&str, &str, &Log)]) -> StoreRoots {
    let scratch = support::scratch(HarnessId::DeepSeekHarness, tag);
    let roots = support::roots(&scratch);
    let sessions = roots.dsh_sessions().unwrap();
    for (project, folder, log) in logs {
        write_to(
            &sessions.join(project).join(folder),
            "session.v4.jsonl.zstd",
            &log.zstd(),
        );
    }
    roots
}

fn simple(id: &str, prompt: &str) -> Log {
    let mut log = Log::new(id);
    log.user(prompt);
    log.assistant(1, json!([{"type": "text", "text": "ok"}]));
    log
}

// ---------------------------------------------------------------------------------------------
// the fixture

#[test]
fn the_fixture_reads_the_same_from_zstd_and_plain() {
    let zstd = dsh::read_file(&fixture()).expect("the zstd fixture reads");
    let plain = dsh::read_file(&plain_fixture()).expect("the plain fixture reads");
    assert_eq!(zstd.entries, plain.entries);
    assert_eq!(zstd.meta.id, "session-0e5d1a2b-0000-4000-8000-000000000001");
    assert_eq!(
        (zstd.meta.harness, zstd.meta.harness_version.as_str()),
        (HarnessId::DeepSeekHarness, "format 4")
    );
    assert_eq!(zstd.meta.cwd.as_deref(), Some("/work/demo"));
    assert_eq!(zstd.meta.title.as_deref(), Some("Layout and README typo"));
    assert_eq!(
        zstd.meta.created.as_deref(),
        Some("2026-09-10T00:26:39.000Z")
    );
    assert_eq!(zstd.meta.models.len(), 1);
    assert_eq!(zstd.meta.models[0].id, "cohere/north-mini-code:free");
    assert_eq!(zstd.meta.models[0].provider.as_deref(), Some("openrouter"));
    assert_eq!(zstd.meta.parent, None);
    assert!(zstd.meta.source_sha256.is_some());
    assert_ne!(zstd.meta.source_sha256, plain.meta.source_sha256);
}

#[test]
fn the_conversation_is_the_surface_after_the_checkpoint_and_the_prune() {
    let session = dsh::read_file(&fixture()).unwrap();
    // The first turn is behind the compaction's checkpoint: what is left is the checkpoint, the
    // first turn's later steps (one result pruned), then turn 2.
    assert_eq!(
        kinds(&session),
        [
            "compaction",
            "step",
            "results",
            "step",
            "user",
            "step",
            "results",
            "step"
        ]
    );
    let prompts: Vec<&str> = session
        .entries
        .iter()
        .filter_map(|e| match &e.kind {
            Kind::User { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(prompts, ["Thanks. Run the tests."]);
    // The replaced nodes stay, as off-branch markers: the first prompt and context, the first
    // step, its two results, and the edit result the prune replaced.
    let off_branch = meta_kinds(&session)
        .into_iter()
        .filter(|k| *k == MetaKind::OffBranch)
        .count();
    assert_eq!(off_branch, 6);
    let first = session
        .entries
        .iter()
        .find(|e| e.id == "4")
        .expect("the original prompt is kept");
    assert_eq!(
        first.kind,
        Kind::Meta {
            kind: MetaKind::OffBranch
        }
    );
}

#[test]
fn a_checkpoint_is_the_compaction_with_the_summary_the_log_holds() {
    let session = dsh::read_file(&fixture()).unwrap();
    let compaction = session
        .entries
        .iter()
        .find(|e| matches!(e.kind, Kind::Compaction { .. }))
        .expect("a compaction entry");
    let Kind::Compaction {
        summary,
        kept_from,
        trigger,
        tokens_before,
    } = &compaction.kind
    else {
        unreachable!()
    };
    assert_eq!(
        summary,
        "The user asked for the project layout and a README typo fix; both are done."
    );
    assert_eq!(kept_from.as_deref(), Some("15"), "the first step after it");
    assert_eq!(*trigger, Trigger::Manual, "it names a command");
    assert_eq!(*tokens_before, Some(1234));
    // The checkpoint's own event and the summary event it cites.
    assert_eq!(compaction.raw.len(), 2);
    assert_eq!(compaction.raw[1]["type"], "compaction/summary");
}

#[test]
fn a_pruned_result_is_read_once_in_the_place_of_the_original() {
    let session = dsh::read_file(&fixture()).unwrap();
    let results: Vec<(&str, String)> = session
        .entries
        .iter()
        .filter_map(|e| match &e.kind {
            Kind::Results { results } => Some(results),
            _ => None,
        })
        .flatten()
        .map(|r| (r.call_id.as_str(), r.content.text()))
        .collect();
    assert_eq!(
        results,
        [
            ("call-edit", "[pruned]".to_string()),
            ("call-t", "test result: ok. 3 passed".to_string())
        ]
    );
}

#[test]
fn every_event_of_the_log_is_kept_verbatim_once() {
    let session = dsh::read_file(&fixture()).unwrap();
    let text = std::fs::read_to_string(plain_fixture()).unwrap();
    let events: Vec<Value> = text
        .lines()
        .skip(1)
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let mut kept: Vec<&Value> = session.entries.iter().flat_map(|e| &e.raw).collect();
    kept.sort_by_key(|row| row["seq"].as_u64());
    assert_eq!(kept.len(), events.len());
    for (kept, event) in kept.iter().zip(&events) {
        assert_eq!(*kept, event);
    }
}

#[test]
fn steps_keep_their_reasoning_text_calls_and_logged_call_events() {
    let log = {
        let mut log = Log::new("session-a");
        log.user("go");
        log.assistant(
            1,
            json!([
                {"type": "reasoning", "text": "think"},
                {"type": "text", "text": "looking"},
                Log::call("read", "c1", r#"{"file_path":"a.txt"}"#),
                Log::call("bash", "c2", r#"{"command":"ls","timeoutMs":5000}"#),
            ]),
        );
        log.logged("read", "c1", r#"{"file_path":"a.txt"}"#);
        log.logged("bash", "c2", r#"{"command":"ls","timeoutMs":5000}"#);
        log.result("c1", "one");
        log.result("c2", "two");
        log
    };
    let session = session_of(&log, "step");
    // Parallel calls are answered together, and each call is logged once, in its step's raw.
    assert_eq!(kinds(&session), ["user", "step", "results"]);
    let step = &session.entries[1];
    let Kind::Step {
        model,
        reasoning,
        text,
        calls,
        end,
    } = &step.kind
    else {
        panic!("{step:?}")
    };
    assert_eq!(model.as_ref().unwrap().id, "m");
    assert_eq!(reasoning.len(), 1);
    assert_eq!(reasoning[0].text, "think");
    assert!(reasoning[0].opaque.is_none());
    assert_eq!(text, &["looking"]);
    assert_eq!(*end, StepEnd::ToolUse);
    assert_eq!(calls.len(), 2);
    assert_eq!(step.raw.len(), 3, "the message and its two logged calls");
    let Kind::Results { results } = &session.entries[2].kind else {
        panic!()
    };
    let ids: Vec<&str> = results.iter().map(|r| r.call_id.as_str()).collect();
    assert_eq!(ids, ["c1", "c2"]);
    assert_eq!(session.entries[2].raw.len(), 2);
    assert!(
        session.entries.iter().all(|e| !matches!(
            &e.kind,
            Kind::Meta { .. } if e.raw[0]["type"] == "tool/call"
        )),
        "a logged call is not also a marker"
    );
}

#[test]
fn dsh_tools_have_their_canonical_forms() {
    let mut log = Log::new("session-t");
    log.user("go");
    let calls: Vec<(&str, &str)> = vec![
        ("read", r#"{"file_path":"a"}"#),
        ("write", r#"{"file_path":"a","content":"x"}"#),
        (
            "edit",
            r#"{"file_path":"a","old_string":"x","new_string":"y"}"#,
        ),
        (
            "apply_patch",
            r#"{"input":"*** Begin Patch\n*** End Patch"}"#,
        ),
        ("bash", r#"{"command":"ls","timeoutMs":2500}"#),
        ("pwsh", r#"{"command":"dir","timeoutMs":1000}"#),
        ("grep", r#"{"pattern":"x"}"#),
        ("glob", r#"{"pattern":"*.rs"}"#),
        ("web_fetch", r#"{"url":"https://example.com"}"#),
        ("web_search", r#"{"queries":["x"]}"#),
        ("subagent", r#"{"description":"d","prompt":"p"}"#),
        ("skill", r#"{"name":"s"}"#),
        ("ask_user_question", r#"{"question":"?"}"#),
        ("send_message", r#"{"agent_id":"a","message":"m"}"#),
        ("bash", "not json"),
    ];
    let parts: Vec<Value> = calls
        .iter()
        .enumerate()
        .map(|(i, (name, args))| Log::call(name, &format!("c{i}"), args))
        .collect();
    log.assistant(1, Value::Array(parts));
    let session = session_of(&log, "tools");
    let Kind::Step { calls: read, .. } = &session.entries[1].kind else {
        panic!()
    };
    let canon: Vec<Option<CanonicalTool>> = read.iter().map(|c| c.canonical.clone()).collect();
    use CanonicalTool as C;
    assert_eq!(
        canon,
        [
            Some(C::Read),
            Some(C::Write),
            Some(C::Edit),
            Some(C::Edit),
            Some(C::Bash),
            Some(C::PowerShell),
            Some(C::Grep),
            Some(C::Glob),
            Some(C::WebFetch),
            Some(C::WebSearch),
            Some(C::Agent),
            Some(C::Skill),
            Some(C::AskUserQuestion),
            Some(C::Other("send_message".into())),
            Some(C::Bash),
        ]
    );
    // The shell timeout is milliseconds under the canonical key; the record keeps its own.
    assert_eq!(read[4].input, json!({"command": "ls", "timeout": 2500}));
    assert_eq!(read[5].input, json!({"command": "dir", "timeout": 1000}));
    assert_eq!(read[4].source_name, "bash");
    assert_eq!(
        read[14].input,
        json!("not json"),
        "arguments that are not JSON stay text"
    );
    assert!(session.entries[1].raw[0].to_string().contains("timeoutMs"));
}

#[test]
fn an_error_result_says_so() {
    let mut log = Log::new("session-e");
    log.user("go");
    log.assistant(1, json!([Log::call("bash", "c1", r#"{"command":"x"}"#)]));
    log.append(
        "tool/result",
        json!({"turn": 1, "step": 1, "message": {"role": "tool", "toolCallId": "c1",
            "source": {"kind": "tool", "callId": "c1"}, "content": [{"type": "text", "text": "boom"}], "isError": true}}),
    );
    log.assistant(1, json!([Log::call("bash", "c2", r#"{"command":"y"}"#)]));
    log.append(
        "tool/result",
        json!({"turn": 1, "step": 1, "error": {"name": "ToolError", "code": "timeout"},
            "message": {"role": "tool", "toolCallId": "c2", "source": {"kind": "tool", "callId": "c2"},
            "content": [{"type": "image", "attachment": "a1"}]}}),
    );
    let session = session_of(&log, "errors");
    let flags: Vec<(bool, bool)> = session
        .entries
        .iter()
        .filter_map(|e| match &e.kind {
            Kind::Results { results } => Some(results),
            _ => None,
        })
        .flatten()
        .map(|r| (r.is_error, matches!(r.content, ResultContent::Blocks(_))))
        .collect();
    assert_eq!(flags, [(true, false), (true, true)]);
}

// ---------------------------------------------------------------------------------------------
// prompts and markers

#[test]
fn the_users_words_and_other_agents_messages_are_turns_and_injected_context_is_not() {
    let mut log = Log::new("session-p");
    log.append(
        "system/message",
        json!({"turn": 1, "step": 1, "message": {"role": "system", "content": [{"type": "text", "text": "sys"}],
            "source": {"kind": "system-prompt"}}}),
    );
    log.user("typed");
    for kind in [
        "runtime-context",
        "skill-catalog",
        "time-context",
        "hooks-claude-code",
        "goal",
    ] {
        log.user_of(kind, kind);
    }
    log.user_of("agent-message", "from a child");
    log.user_of("subagent-settled", "settled");
    log.user_of("agent-message", "  ");
    log.append(
        "developer/message",
        json!({"turn": 1, "step": 1, "message": {"role": "developer", "content": [], "source": {"kind": "x"}}}),
    );
    log.assistant(1, json!([{"type": "text", "text": "ok"}]));
    let session = session_of(&log, "prompts");
    assert_eq!(kinds(&session), ["user", "user", "user", "step"]);
    let texts: Vec<&str> = session
        .entries
        .iter()
        .filter_map(|e| match &e.kind {
            Kind::User { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["typed", "from a child", "settled"]);
    let metas = meta_kinds(&session);
    assert_eq!(
        metas
            .iter()
            .filter(|k| **k == MetaKind::SystemPrompt)
            .count(),
        1
    );
    // The five injected kinds, the developer message and the agent message with no text.
    assert_eq!(
        metas.iter().filter(|k| **k == MetaKind::Internal).count(),
        7
    );
}

#[test]
fn a_user_turn_carries_its_attachments_and_every_text_part() {
    let mut log = Log::new("session-a");
    log.append(
        "user/message",
        json!({"role": "user", "source": {"kind": "user"}, "content": [
            {"type": "text", "text": "look at this"},
            {"type": "image", "attachment": "att-1", "offloaded": true},
            {"type": "text", "text": "and this"}]}),
    );
    let session = session_of(&log, "attachments");
    let Kind::User { text, attachments } = &session.entries[0].kind else {
        panic!()
    };
    assert_eq!(text, "look at this\nand this");
    assert_eq!(attachments.len(), 1);
    assert_eq!(attachments[0].kind, "image");
}

#[test]
fn a_turn_that_failed_or_was_stopped_says_so_on_its_last_step() {
    let mut log = Log::new("session-f");
    for (turn, reason) in [
        (1, json!({"kind": "error", "error": "boom"})),
        (2, json!({"kind": "aborted", "reason": "user"})),
        (3, json!({"kind": "completed"})),
    ] {
        log.user("go");
        log.assistant(turn, json!([{"type": "text", "text": "partial"}]));
        log.push(
            "turn/end",
            json!({"turn": turn, "reason": reason}),
            json!({}),
        );
    }
    // A step the harness marked interrupted is aborted whatever the turn says.
    log.append(
        "assistant/message",
        json!({"turn": 4, "step": 1, "interrupted": true, "message": {"role": "assistant",
            "content": [{"type": "text", "text": "cut"}], "source": {"kind": "model", "provider": "p", "model": "m"}}, "stream": []}),
    );
    let session = session_of(&log, "ends");
    let ends: Vec<StepEnd> = session
        .entries
        .iter()
        .filter_map(|e| match &e.kind {
            Kind::Step { end, .. } => Some(end.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        ends,
        [
            StepEnd::Error,
            StepEnd::Aborted,
            StepEnd::Completed,
            StepEnd::Aborted
        ]
    );
}

#[test]
fn an_unknown_event_is_refused_unless_it_says_it_is_ignorable() {
    let mut log = simple("session-u", "hello");
    log.push(
        "plugin:acme/ping",
        json!({"n": 1}),
        json!({"ignorable": true}),
    );
    let session = session_of(&log, "ignorable");
    assert!(meta_kinds(&session).contains(&MetaKind::Internal));
    assert!(
        session
            .entries
            .iter()
            .any(|e| e.raw[0]["type"] == "plugin:acme/ping"),
        "the event is kept"
    );

    log.push("future/thing", json!({}), json!({}));
    let error = format!("{:#}", read_log(&log, "unknown").unwrap_err());
    assert!(error.contains("future/thing"), "{error}");
    assert!(error.contains("ignorable"), "{error}");
}

#[test]
fn the_header_gates_the_format() {
    let mut log = simple("session-v", "hello");
    log.header["version"] = json!(3);
    let error = format!("{:#}", read_log(&log, "v3").unwrap_err());
    assert!(error.contains("format 3"), "{error}");
    assert!(error.contains("format 4"), "{error}");

    let scratch = support::scratch(HarnessId::DeepSeekHarness, "no-header");
    let path = write_to(
        &scratch,
        "session.v4.jsonl",
        b"{\"type\":\"turn/start\",\"seq\":0}\n",
    );
    let error = format!("{:#}", dsh::read_file(&path).unwrap_err());
    assert!(error.contains("session header"), "{error}");
}

// ---------------------------------------------------------------------------------------------
// surface rebuilding

#[test]
fn a_replace_puts_the_new_node_where_the_old_ones_were() {
    let mut log = Log::new("session-r");
    let first = log.user("first");
    log.assistant(1, json!([{"type": "text", "text": "a"}]));
    let last_old = log.assistant(1, json!([{"type": "text", "text": "b"}]));
    log.user("kept");
    log.push(
        "user/message",
        json!({"role": "user", "content": [{"type": "text", "text": "gist"}],
            "source": {"kind": "compact-checkpoint", "compactionId": "x"}}),
        json!({"surfaceOp": {"op": "replace", "startSeq": first, "endSeq": last_old}}),
    );
    let session = session_of(&log, "replace");
    // In the log the checkpoint is last; on the surface it leads, ahead of what it kept.
    assert_eq!(kinds(&session), ["compaction", "user"]);
    let Kind::Compaction {
        summary,
        kept_from,
        trigger,
        ..
    } = &session.entries[3].kind
    else {
        panic!("{:?}", kinds(&session))
    };
    assert_eq!(
        summary, "gist",
        "no summary event: the checkpoint's own text"
    );
    assert_eq!(kept_from.as_deref(), Some("3"));
    assert_eq!(*trigger, Trigger::Unknown);
    let off: Vec<&str> = session
        .entries
        .iter()
        .filter(|e| {
            e.kind
                == Kind::Meta {
                    kind: MetaKind::OffBranch,
                }
        })
        .map(|e| e.id.as_str())
        .collect();
    assert_eq!(off, ["0", "1", "2"]);
}

#[test]
fn a_replacement_that_is_itself_replaced_keeps_every_generation() {
    let mut log = Log::new("session-rr");
    let a = log.user("a");
    let b = log.user("b");
    let one = log.push(
        "user/message",
        json!({"role": "user", "content": [{"type": "text", "text": "one"}], "source": {"kind": "compact-checkpoint"}}),
        json!({"surfaceOp": {"op": "replace", "startSeq": a, "endSeq": b}}),
    );
    log.user("c");
    log.push(
        "user/message",
        json!({"role": "user", "content": [{"type": "text", "text": "two"}], "source": {"kind": "compact-checkpoint"}}),
        json!({"surfaceOp": {"op": "replace", "startSeq": one, "endSeq": one + 1}}),
    );
    let session = session_of(&log, "chain");
    assert_eq!(kinds(&session), ["compaction"]);
    let off = session
        .entries
        .iter()
        .filter(|e| {
            e.kind
                == Kind::Meta {
                    kind: MetaKind::OffBranch,
                }
        })
        .count();
    assert_eq!(off, 4, "a, b, the first checkpoint and c");
    let ids: Vec<&str> = session.entries.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(
        ids,
        ["0", "1", "2", "3", "4"],
        "oldest first, the live node last"
    );
}

#[test]
fn a_surface_operation_the_reader_does_not_know_is_refused() {
    let mut log = Log::new("session-o");
    log.push(
        "user/message",
        json!({"role": "user", "content": [], "source": {"kind": "user"}}),
        json!({"surfaceOp": {"op": "splice"}}),
    );
    let error = format!("{:#}", read_log(&log, "op").unwrap_err());
    assert!(error.contains("surfaceOp"), "{error}");
}

// ---------------------------------------------------------------------------------------------
// the file

#[test]
fn a_cut_off_zstd_tail_is_dropped_and_said() {
    let log = simple("session-z", "hello");
    let whole = log.zstd();
    let scratch = support::scratch(HarnessId::DeepSeekHarness, "torn");
    // The last frame loses its checksum and more.
    let path = write_to(&scratch, "session.v4.jsonl.zstd", &whole[..whole.len() - 9]);
    let session = dsh::read_file(&path).expect("a torn tail is not an error");
    assert_eq!(
        kinds(&session),
        ["user"],
        "the assistant frame was the cut one"
    );
    let last = session.entries.last().unwrap();
    assert_eq!(
        last.kind,
        Kind::Meta {
            kind: MetaKind::Unfinished
        }
    );
    assert!(
        last.raw[0].as_str().unwrap().contains("cut off"),
        "{:?}",
        last.raw
    );

    // Cut inside the header frame: nothing to read.
    let path = write_to(&scratch, "header.v4.jsonl.zstd", &whole[..10]);
    let error = format!("{:#}", dsh::read_file(&path).unwrap_err());
    assert!(error.contains("session header"), "{error}");
}

#[test]
fn a_cut_off_plain_line_is_dropped_and_kept_as_text() {
    let log = simple("session-p", "hello");
    let mut bytes = log.plain();
    bytes.extend(b"{\"type\":\"turn/end\",\"seq\":");
    let scratch = support::scratch(HarnessId::DeepSeekHarness, "torn-plain");
    let path = write_to(&scratch, "session.v4.jsonl", &bytes);
    let session = dsh::read_file(&path).unwrap();
    assert_eq!(kinds(&session), ["user", "step"]);
    assert_eq!(
        session.entries.last().unwrap().kind,
        Kind::Meta {
            kind: MetaKind::Unfinished
        }
    );
}

#[test]
fn a_frame_that_fails_its_checksum_is_an_error() {
    let log = simple("session-c", "hello");
    let mut bytes = log.zstd();
    // A byte of the second frame's content (the first event's text).
    let header_frame = stored_frame(log.lines()[0].as_bytes()).len();
    bytes[header_frame + 12 + 20] ^= 0x01;
    let scratch = support::scratch(HarnessId::DeepSeekHarness, "damaged");
    let path = write_to(&scratch, "session.v4.jsonl.zstd", &bytes);
    let error = format!("{:#}", dsh::read_file(&path).unwrap_err());
    assert!(error.contains("checksum"), "{error}");
}

#[test]
fn a_file_named_zstd_that_is_not_zstd_is_refused_and_a_plain_log_is_read_by_its_bytes() {
    let log = simple("session-n", "hello");
    let scratch = support::scratch(HarnessId::DeepSeekHarness, "sniff");
    let plain_as_zstd = write_to(&scratch, "a.v4.jsonl.zstd", &log.plain());
    assert_eq!(
        kinds(&dsh::read_file(&plain_as_zstd).unwrap()),
        ["user", "step"]
    );
    let zstd_as_plain = write_to(&scratch, "b.v4.jsonl", &log.zstd());
    assert_eq!(
        kinds(&dsh::read_file(&zstd_as_plain).unwrap()),
        ["user", "step"]
    );
    let junk = write_to(
        &scratch,
        "c.v4.jsonl.zstd",
        &[0xFD, 0x2F, 0xB5, 0x28, 0xFF, 0xFF, 0xFF],
    );
    assert!(dsh::read_file(&junk).is_err());
}

// ---------------------------------------------------------------------------------------------
// lineage

fn child(id: &str, parent: &str, extra: Value) -> Log {
    simple(id, "do the thing")
        .with_header(json!({"parentSession": parent, "delegationDepth": 1}))
        .with_header(extra)
}

#[test]
fn a_subagent_a_seeded_subagent_and_a_fork_are_told_apart() {
    let spawn = child(
        "kid-1",
        "session-p",
        json!({"origin": "subagent", "agentPreset": "explorer"}),
    );
    let seeded = child(
        "kid-2",
        "session-p",
        json!({"origin": "subagent", "isSeeded": true}),
    );
    let fork = child("kid-3", "session-p", json!({"isSeeded": true}));
    let roots = store(
        "lineage",
        &[
            ("--p--", "session-p", &simple("session-p", "root")),
            ("--p--", "kid-1", &spawn),
            ("--p--", "kid-2", &seeded),
            ("--p--", "kid-3", &fork),
        ],
    );
    let read = |id: &str| dsh::DeepSeekHarness.read(id, &roots).unwrap().meta;
    let one = read("kid-1");
    assert_eq!(one.parent.as_deref(), Some("session-p"));
    assert_eq!(
        one.parent_kind,
        Some(ParentKind::Spawn {
            call_id: None,
            agent: Some("explorer".into()),
            inherits_context: false
        })
    );
    assert!(matches!(
        read("kid-2").parent_kind,
        Some(ParentKind::Spawn {
            inherits_context: true,
            ..
        })
    ));
    assert_eq!(read("kid-3").parent_kind, Some(ParentKind::fork()));
    assert_eq!(read("session-p").parent_kind, None);
}

/// A seeded fork says where it left its parent: dsh copies the parent's events through the
/// boundary as they are and marks the end of the copy with `session/end-seed`.
#[test]
fn a_seeded_fork_names_the_parents_event_the_copy_ends_at() {
    let mut fork = child("kid-4", "session-p", json!({"isSeeded": true}));
    let boundary = fork.events.len() as u64 - 1;
    fork.push("session/end-seed", json!({"inherited": true}), json!({}));
    fork.user("and now this");
    let session = session_of(&fork, "seeded-fork");
    assert_eq!(session.meta.parent.as_deref(), Some("session-p"));
    assert_eq!(
        session.meta.parent_kind,
        Some(ParentKind::Fork {
            at: Some(boundary.to_string())
        })
    );
    // A marker dsh wrote on adopting a session (no inherited copy) says nothing about a fork.
    let mut adopted = child("kid-5", "session-p", json!({"isSeeded": true}));
    adopted.push("session/end-seed", json!({}), json!({}));
    assert_eq!(
        session_of(&adopted, "adopted").meta.parent_kind,
        Some(ParentKind::fork())
    );
}

#[test]
fn a_listing_shows_roots_and_forks_and_the_explorer_adds_subagents() {
    let roots = store(
        "listing",
        &[
            ("--p--", "session-p", &simple("session-p", "the root")),
            (
                "--p--",
                "kid-1",
                &child("kid-1", "session-p", json!({"origin": "subagent"})),
            ),
            (
                "--p--",
                "kid-3",
                &child("kid-3", "session-p", json!({"isSeeded": true})),
            ),
            ("_no-cwd", "session-n", &simple("session-n", "no cwd")),
        ],
    );
    let ids = |infos: Vec<mukade::reader::SessionInfo>| {
        let mut ids: Vec<String> = infos.into_iter().map(|i| i.id).collect();
        ids.sort();
        ids
    };
    assert_eq!(
        ids(dsh::DeepSeekHarness.list(&roots).unwrap()),
        ["kid-3", "session-n", "session-p"]
    );
    assert_eq!(
        ids(dsh::DeepSeekHarness.list_all(&roots).unwrap()),
        ["kid-1", "kid-3", "session-n", "session-p"]
    );
    let listed = dsh::DeepSeekHarness.list_all(&roots).unwrap();
    let root = listed.iter().find(|i| i.id == "session-p").unwrap();
    assert_eq!(
        root.title.as_deref(),
        Some("the root"),
        "the first prompt names an untitled session"
    );
    assert_eq!(root.cwd.as_deref(), Some("/work/demo"));
    assert!(root.updated.is_some() && root.path.ends_with("session.v4.jsonl.zstd"));
    let kid = listed.iter().find(|i| i.id == "kid-1").unwrap();
    assert_eq!(kid.parent.as_deref(), Some("session-p"));
    assert_eq!(
        kid.parent_kind.as_ref().map(ParentKind::name),
        Some("spawn")
    );
    let sessions = roots.dsh_sessions().unwrap();
    assert_eq!(dsh::store_counts(&sessions).unwrap(), (4, 2));
}

#[test]
fn a_title_is_the_last_one_the_log_wrote() {
    let mut log = simple("session-t", "the prompt");
    log.push(
        "session/title",
        json!({"title": "First", "messageSeqs": [0], "source": {"kind": "fallback"}}),
        json!({}),
    );
    log.push(
        "session/title",
        json!({"title": "Second", "messageSeqs": [0], "source": {"kind": "user"}}),
        json!({}),
    );
    let roots = store("title", &[("--p--", "session-t", &log)]);
    let listed = dsh::DeepSeekHarness.list(&roots).unwrap();
    assert_eq!(listed[0].title.as_deref(), Some("Second"));
    assert_eq!(
        dsh::DeepSeekHarness
            .read("session-t", &roots)
            .unwrap()
            .meta
            .title
            .as_deref(),
        Some("Second")
    );
}

// ---------------------------------------------------------------------------------------------
// the store

#[test]
fn an_id_is_found_by_its_uuid_or_a_unique_prefix() {
    let roots = store(
        "ids",
        &[
            (
                "--p--",
                "session-6de20419-aaaa",
                &simple("session-6de20419-aaaa", "one"),
            ),
            (
                "--p--",
                "session-6de2ffff-bbbb",
                &simple("session-6de2ffff-bbbb", "two"),
            ),
        ],
    );
    let found = |typed: &str| dsh::DeepSeekHarness.read(typed, &roots);
    assert_eq!(
        found("session-6de20419-aaaa").unwrap().meta.id,
        "session-6de20419-aaaa"
    );
    assert_eq!(found("6de20419").unwrap().meta.id, "session-6de20419-aaaa");
    assert_eq!(
        found("session-6de2f").unwrap().meta.id,
        "session-6de2ffff-bbbb"
    );
    let ambiguous = found("6de2").unwrap_err();
    assert!(
        matches!(
            ambiguous.downcast_ref::<Lookup>(),
            Some(Lookup::Ambiguous(_))
        ),
        "{ambiguous:#}"
    );
    let missing = found("zzz").unwrap_err();
    assert!(
        matches!(missing.downcast_ref::<Lookup>(), Some(Lookup::NotFound(_))),
        "{missing:#}"
    );
}

/// A read opens the log it names and no other: the id is matched against the folder names, so the
/// other logs of the store are neither decoded nor consulted (here one whose header id would make
/// the typed prefix ambiguous, and one that is no log at all).
#[test]
fn reading_one_session_does_not_consult_the_other_logs() {
    let roots = store(
        "read-one",
        &[
            (
                "--p--",
                "session-6de20419-aaaa",
                &simple("session-6de20419-aaaa", "one"),
            ),
            (
                "--p--",
                "session-9999-bbbb",
                &simple("session-6de20419-zzzz", "header says another id"),
            ),
        ],
    );
    let sessions = roots.dsh_sessions().unwrap();
    write_to(
        &sessions.join("--p--").join("session-7777-cccc"),
        "session.v4.jsonl.zstd",
        b"not a log",
    );
    assert_eq!(
        dsh::DeepSeekHarness
            .read("6de20419", &roots)
            .unwrap()
            .meta
            .id,
        "session-6de20419-aaaa"
    );
    // The corrupt one is reported when it is the one asked for, and only then.
    let error = format!(
        "{:#}",
        dsh::DeepSeekHarness
            .read("session-7777-cccc", &roots)
            .unwrap_err()
    );
    assert!(
        error.contains("not a DeepSeek Harness session log"),
        "{error}"
    );
}

/// A listing keeps what it read of each log under the log's size and modification time: a log that
/// is as it was is not decoded again.
#[test]
fn a_listing_remembers_a_log_that_has_not_changed() {
    let roots = store(
        "list-cache",
        &[(
            "--p--",
            "session-aaaa",
            &simple("session-aaaa", "the first prompt"),
        )],
    );
    let scratch = support::scratch(HarnessId::DeepSeekHarness, "list-cache-table");
    let log = roots
        .dsh_sessions()
        .unwrap()
        .join("--p--")
        .join("session-aaaa")
        .join("session.v4.jsonl.zstd");
    let settled = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
    let touch = |at: std::time::SystemTime| {
        std::fs::File::options()
            .write(true)
            .open(&log)
            .unwrap()
            .set_modified(at)
            .unwrap();
    };
    touch(settled);
    let first = dsh::list_all_cached(&roots, Some(&scratch)).unwrap();
    assert_eq!(first[0].title.as_deref(), Some("the first prompt"));

    // The same size and time, other bytes: only the cache can still say what it was.
    let size = std::fs::metadata(&log).unwrap().len() as usize;
    std::fs::write(&log, vec![b'x'; size]).unwrap();
    touch(settled);
    let again = dsh::list_all_cached(&roots, Some(&scratch)).unwrap();
    assert_eq!(
        again[0].title.as_deref(),
        Some("the first prompt"),
        "{again:#?}"
    );
    assert!(again[0].unreadable.is_none());

    // Without the cache the log is read, and is not one.
    let cold = dsh::list_all_cached(&roots, None).unwrap();
    assert!(cold[0].unreadable.is_some(), "{cold:#?}");
}

#[test]
fn without_a_home_there_is_no_store() {
    let roots = StoreRoots::default();
    for error in [
        dsh::DeepSeekHarness.list(&roots).unwrap_err(),
        dsh::DeepSeekHarness.read("x", &roots).unwrap_err(),
    ] {
        assert!(
            matches!(error.downcast_ref::<Lookup>(), Some(Lookup::NoStore(_))),
            "{error:#}"
        );
    }
    // A home with no sessions folder is an empty store.
    let scratch = support::scratch(HarnessId::DeepSeekHarness, "empty");
    let roots = support::roots(&scratch);
    assert!(dsh::DeepSeekHarness.list(&roots).unwrap().is_empty());
}

#[test]
fn a_plain_root_an_older_generation_and_a_broken_log_are_each_listed_as_they_are() {
    let scratch = support::scratch(HarnessId::DeepSeekHarness, "layouts");
    let roots = support::roots(&scratch);
    let sessions = roots.dsh_sessions().unwrap();
    // A root configured without compression.
    let plain = simple("session-plain", "plain");
    write_to(
        &sessions.join("--a--").join("session-plain"),
        "session.v4.jsonl",
        &plain.plain(),
    );
    // Only a format 3 log.
    write_to(
        &sessions.join("--a--").join("old"),
        "session.v3.jsonl",
        b"{}\n",
    );
    // Beside a format 4 log, an older generation is not read.
    let both = simple("session-both", "both");
    let dir = sessions.join("--a--").join("session-both");
    write_to(&dir, "session.v4.jsonl.zstd", &both.zstd());
    write_to(&dir, "session.v3.jsonl", b"{}\n");
    // A log that is not one, in a folder whose name is an encoded id.
    write_to(
        &sessions.join("--a--").join("odd~0020id"),
        "session.v4.jsonl.zstd",
        b"junk",
    );
    // Files directly in a project folder are the obsolete flat layout, not sessions.
    write_to(&sessions.join("--a--"), "session.v4.jsonl", &plain.plain());

    let listed = dsh::DeepSeekHarness.list(&roots).unwrap();
    let by_id = |id: &str| {
        listed
            .iter()
            .find(|i| i.id == id)
            .unwrap_or_else(|| panic!("{id} in {listed:#?}"))
    };
    assert_eq!(listed.len(), 4, "{listed:#?}");
    assert!(by_id("session-plain").unreadable.is_none());
    assert!(
        by_id("session-both")
            .path
            .ends_with("session.v4.jsonl.zstd")
    );
    assert!(
        by_id("old")
            .unreadable
            .as_deref()
            .unwrap()
            .contains("format 3")
    );
    assert!(
        by_id("odd id").unreadable.is_some(),
        "the folder name is decoded"
    );
    let error = format!(
        "{:#}",
        dsh::DeepSeekHarness.read("old", &roots).unwrap_err()
    );
    assert!(error.contains("format 3"), "{error}");
}

#[test]
fn reading_and_listing_leave_the_store_as_it_was() {
    let roots = store(
        "readonly",
        &[("--p--", "session-p", &simple("session-p", "x"))],
    );
    let sessions = roots.dsh_sessions().unwrap();
    let snapshot = |dir: &Path| {
        let mut files = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    files.push((path.clone(), std::fs::read(&path).unwrap()));
                }
            }
        }
        files.sort();
        files
    };
    let before = snapshot(&sessions);
    dsh::DeepSeekHarness.list_all(&roots).unwrap();
    dsh::DeepSeekHarness.read("session-p", &roots).unwrap();
    assert_eq!(snapshot(&sessions), before);
}

// ---------------------------------------------------------------------------------------------
// through the adapter, and out

#[test]
fn the_adapter_reads_the_file_and_the_store() {
    let adapter = harnesses::get(HarnessId::DeepSeekHarness);
    let from_file = adapter.read_file(&fixture()).expect("the fixture reads");
    assert_eq!(from_file.meta.harness, HarnessId::DeepSeekHarness);
    let scratch = support::scratch(HarnessId::DeepSeekHarness, "adapter");
    let roots = support::roots(&scratch);
    let sessions = roots.dsh_sessions().unwrap();
    let dir = sessions.join("--work-demo--").join(&from_file.meta.id);
    write_to(
        &dir,
        "session.v4.jsonl.zstd",
        &std::fs::read(fixture()).unwrap(),
    );
    let listed = adapter.reader().list(&roots).expect("the store lists");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, from_file.meta.id);
    assert_eq!(listed[0].title.as_deref(), Some("Layout and README typo"));
    assert_eq!(adapter.store_root(&roots), roots.dsh_home.as_deref());
    let stats = adapter.store_stats(&roots);
    assert_eq!(stats.sessions, Ok(1));
    assert_eq!(stats.forks, Some(Ok(0)));
    let read = adapter
        .reader()
        .read("0e5d1a2b", &roots)
        .expect("a UUID prefix reads");
    assert_eq!(read.entries, from_file.entries);
}

#[test]
fn a_deepseek_session_hops_into_every_file_destination() {
    let session = dsh::read_file(&fixture()).unwrap();
    support::assert_hops_keeping(&session, "Thanks. Run the tests.", "dsh-hop");
}
