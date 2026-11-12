//! DeepSeek Harness writer conformance: the files `dsh` itself must accept (session format 4).
//!
//! A hop writes into a scratch store; what lands is checked against what dsh's own persistence
//! enforces (`dsh-session-persistence-jsonl`, `dsh-session`): the folder names, the frames, the
//! header, contiguous `seq`, the `surfaceOp` markers, the `tool/call` events. The live proof
//! that dsh resumes such a file is `docs/verification/2026-11-12-dsh-writer-live.md`.

mod support;

use mukade::harnesses;
use mukade::model::{
    CanonicalTool, Entry, HarnessId, Kind, Meta, MetaKind, Session, StepEnd, ToolCall, ToolResult,
};
use mukade::reader::{Reader, deepseek_harness as dsh, zstd_frames};
use mukade::tools::{self, Shell, ToolSet, Translation};
use mukade::writer::{WriteOpts, for_harness};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const DSH: HarnessId = HarnessId::DeepSeekHarness;

/// The tools of the headless profile with the POSIX shell, so a test means the same everywhere.
const POSIX: ToolSet = ToolSet {
    names: &["read", "write", "edit", "glob", "grep", "web_fetch", "bash"],
};
const WINDOWS: ToolSet = ToolSet {
    names: &["read", "write", "edit", "glob", "grep", "web_fetch", "pwsh"],
};

fn opts(scratch: &Path) -> WriteOpts {
    WriteOpts {
        offered: Some(POSIX),
        dest_shell: Some(Shell::Posix),
        dest_protocol: None,
        ..support::opts(DSH, scratch)
    }
}

/// Write `session` toward dsh in a scratch store: the options, and the written file's location.
fn write(session: &Session, tag: &str) -> (WriteOpts, mukade::writer::Written) {
    let scratch = support::scratch(DSH, tag);
    let opts = opts(&scratch);
    let written = for_harness(DSH)
        .write(session, &opts)
        .unwrap_or_else(|error| panic!("{tag}: {error:#}"));
    (opts, written)
}

/// The header and events a written file holds, and the frames it is made of.
fn read_log(location: &str) -> (Value, Vec<Value>, usize) {
    let bytes = std::fs::read(location).unwrap();
    let (text, frames) = if zstd_frames::is_zstd(&bytes) {
        let decoded = zstd_frames::decode(&bytes).expect("every frame decodes with its checksum");
        assert_eq!(decoded.torn, None);
        assert_eq!(
            decoded.frames[0].iter().filter(|b| **b == b'\n').count(),
            1,
            "frame 1 is the header alone"
        );
        let frames = decoded.frames.len();
        (String::from_utf8(decoded.frames.concat()).unwrap(), frames)
    } else {
        (String::from_utf8(bytes).unwrap(), 1)
    };
    let mut rows: Vec<Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).expect("each line is JSON"))
        .collect();
    let header = rows.remove(0);
    (header, rows, frames)
}

fn kinds(events: &[Value]) -> Vec<&str> {
    events.iter().map(|e| e["type"].as_str().unwrap()).collect()
}

fn session(entries: Vec<Kind>) -> Session {
    Session {
        meta: Meta {
            harness: HarnessId::Pi,
            harness_version: "1".into(),
            id: "src".into(),
            cwd: Some(support::cwd_for(DSH)),
            title: Some("A title".into()),
            created: None,
            updated: None,
            models: Vec::new(),
            git_branch: None,
            parent: None,
            parent_kind: None,
            source_path: None,
            source_sha256: None,
        },
        entries: entries
            .into_iter()
            .enumerate()
            .map(|(i, kind)| Entry {
                id: format!("e{i}"),
                kind,
                raw: Vec::new(),
            })
            .collect(),
    }
}

fn user(text: &str) -> Kind {
    Kind::User {
        text: text.into(),
        attachments: Vec::new(),
    }
}

fn step(text: &[&str], calls: Vec<ToolCall>) -> Kind {
    let end = if calls.is_empty() {
        StepEnd::Completed
    } else {
        StepEnd::ToolUse
    };
    Kind::Step {
        model: None,
        reasoning: Vec::new(),
        text: text.iter().map(|t| t.to_string()).collect(),
        calls,
        end,
    }
}

fn call(id: &str, name: &str, canonical: CanonicalTool, input: Value) -> ToolCall {
    ToolCall {
        id: id.into(),
        source_name: name.into(),
        canonical: Some(canonical),
        input,
    }
}

fn results(items: &[(&str, &str)]) -> Kind {
    Kind::Results {
        results: items
            .iter()
            .map(|(id, text)| ToolResult {
                call_id: (*id).into(),
                content: mukade::model::ResultContent::Text((*text).into()),
                is_error: false,
                truncated_by_source: false,
            })
            .collect(),
    }
}

fn project_key_of(cwd: &str) -> String {
    mukade::writer::deepseek_harness::project_key(cwd)
}

// ---------------------------------------------------------------------------------------------
// the file

#[test]
fn the_session_lands_in_the_folder_dsh_names_for_its_id_and_cwd() {
    let source = support::read_source(HarnessId::ClaudeCode);
    let (opts, written) = write(&source, "dsh-path");
    assert!(written.id.starts_with("session-"), "{}", written.id);
    uuid_of(&written.id);
    let sessions = opts.roots.dsh_sessions().unwrap();
    let cwd = support::cwd_for(DSH);
    let expected = sessions
        .join(project_key_of(&cwd))
        .join(&written.id)
        .join("session.v4.jsonl.zstd");
    assert_eq!(PathBuf::from(&written.location), expected);
    assert!(expected.is_file());
    let (header, events, frames) = read_log(&written.location);
    assert!(frames >= 2, "the header, then the events");
    // The header holds exactly the keys dsh's `isHeaderLine` accepts, in dsh's order.
    let keys: Vec<&str> = header
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        [
            "type",
            "version",
            "id",
            "createdAt",
            "cwd",
            "isSeeded",
            "delegationDepth"
        ]
    );
    assert_eq!(header["version"], 4);
    assert_eq!(header["id"], written.id);
    assert_eq!(header["cwd"], cwd);
    assert_eq!(header["isSeeded"], false);
    assert_eq!(header["delegationDepth"], 0);
    assert!(header["createdAt"].as_i64().unwrap() > 0);
    assert!(!events.is_empty());
    assert_eq!(
        written.resume,
        [
            "dsh",
            "--profile",
            "headless",
            "--session-id",
            written.id.as_str()
        ]
    );
}

fn uuid_of(id: &str) {
    uuid::Uuid::parse_str(id.strip_prefix("session-").unwrap()).expect("session-<uuid>");
}

#[test]
fn the_events_obey_dshs_own_rules() {
    let source = support::read_source(HarnessId::ClaudeCode);
    let (_, written) = write(&source, "dsh-rules");
    let (_, events, _) = read_log(&written.location);
    // seq: 0-based, contiguous, time never going back.
    let mut previous = 0;
    for (i, event) in events.iter().enumerate() {
        assert_eq!(event["seq"], i, "{event}");
        let time = event["time"].as_i64().unwrap();
        assert!(time >= previous, "{event}");
        previous = time;
    }
    // Surface events say `append`; no other event has a surfaceOp.
    let surface = [
        "system/message",
        "developer/message",
        "user/message",
        "assistant/message",
        "tool/result",
    ];
    for event in &events {
        let is_surface = surface.contains(&event["type"].as_str().unwrap());
        assert_eq!(
            event.get("surfaceOp") == Some(&json!("append")),
            is_surface,
            "{event}"
        );
    }
    // Node 0 is an empty system prompt: dsh renders its own and replaces it.
    let system: Vec<&Value> = events
        .iter()
        .filter(|e| e["type"] == "system/message")
        .collect();
    assert_eq!(system.len(), 1);
    assert_eq!(system[0]["data"]["message"]["content"], json!([]));
    assert_eq!(
        system[0]["data"]["message"]["source"],
        json!({"kind": "system-prompt"})
    );
    // Turns and steps are balanced and numbered from 1.
    let mut open_turn = None;
    let mut open_step = None;
    for event in &events {
        let data = &event["data"];
        match event["type"].as_str().unwrap() {
            "turn/start" => {
                assert!(open_turn.is_none() && open_step.is_none(), "{event}");
                open_turn = Some(data["turn"].as_u64().unwrap());
            }
            "step/start" => {
                assert_eq!(data["turn"].as_u64(), open_turn, "{event}");
                assert!(open_step.is_none(), "{event}");
                open_step = Some(data["step"].as_u64().unwrap());
            }
            "step/end" => {
                assert_eq!(data["step"].as_u64(), open_step, "{event}");
                open_step = None;
            }
            "turn/end" => {
                assert!(open_step.is_none(), "{event}");
                assert_eq!(data["turn"].as_u64(), open_turn, "{event}");
                assert_eq!(data["reason"], json!({"kind": "completed"}));
                open_turn = None;
            }
            "assistant/message" | "tool/call" | "tool/result" => {
                assert_eq!(data["turn"].as_u64(), open_turn, "{event}");
                assert_eq!(data["step"].as_u64(), open_step, "{event}");
            }
            _ => {}
        }
    }
    assert!(
        open_turn.is_none() && open_step.is_none(),
        "a turn is left open"
    );
}

#[test]
fn every_call_is_logged_by_a_tool_call_event_and_answered_citing_it() {
    let source = support::read_source(HarnessId::ClaudeCode);
    let (_, written) = write(&source, "dsh-calls");
    let (_, events, _) = read_log(&written.location);
    let mut seen_calls = 0;
    for event in &events {
        if event["type"] != "assistant/message" {
            continue;
        }
        for part in event["data"]["message"]["content"].as_array().unwrap() {
            if part["type"] != "tool-call" {
                continue;
            }
            seen_calls += 1;
            let id = part["id"].as_str().unwrap();
            let logged: Vec<&Value> = events
                .iter()
                .filter(|e| e["type"] == "tool/call" && e["data"]["callId"] == id)
                .collect();
            assert_eq!(logged.len(), 1, "{id}");
            assert_eq!(logged[0]["data"]["name"], part["name"]);
            assert_eq!(logged[0]["data"]["arguments"], part["arguments"]);
            // The arguments are the JSON string of an object, as the model writes them.
            assert!(
                serde_json::from_str::<Value>(part["arguments"].as_str().unwrap())
                    .unwrap()
                    .is_object()
            );
            let result = events
                .iter()
                .find(|e| e["type"] == "tool/result" && e["data"]["message"]["toolCallId"] == id)
                .unwrap_or_else(|| panic!("{id} is answered"));
            assert_eq!(result["sourceEventSeqs"], json!([logged[0]["seq"]]));
            assert_eq!(
                result["data"]["message"]["source"],
                json!({"kind": "tool", "callId": id})
            );
            assert!(logged[0]["seq"].as_u64() < result["seq"].as_u64());
        }
    }
    assert!(seen_calls >= 2, "the fixture makes parallel calls");
}

#[test]
fn an_assistant_message_has_no_replay_state_and_names_its_model() {
    let source = support::read_source(HarnessId::ClaudeCode);
    let (_, written) = write(&source, "dsh-source");
    let (_, events, _) = read_log(&written.location);
    for event in events.iter().filter(|e| e["type"] == "assistant/message") {
        let message = &event["data"]["message"];
        assert_eq!(message["source"]["kind"], "model");
        assert!(message["source"].get("replayState").is_none());
        assert_eq!(
            message["source"]["provider"], "h27",
            "the options' provider"
        );
        assert!(
            message["source"]["model"]
                .as_str()
                .is_some_and(|m| !m.is_empty())
        );
        assert_eq!(event["data"]["stream"], json!([]));
        assert!(message["id"].as_str().is_some());
    }
}

// ---------------------------------------------------------------------------------------------
// the round trip

#[test]
fn a_claude_session_reads_back_from_dsh_as_the_same_conversation() {
    let source = support::read_source(HarnessId::ClaudeCode);
    let (_, written) = write(&source, "dsh-roundtrip-claude");
    let back = dsh::read_file(Path::new(&written.location)).expect("dsh reads what it was given");
    assert_eq!(back.meta.id, written.id);
    assert_eq!(
        back.meta.cwd.as_deref(),
        Some(support::cwd_for(DSH).as_str())
    );
    assert_eq!(back.meta.title.as_deref(), Some("interchange test"));
    // Every prompt comes back, in order (a call that travelled as text answers with an extra
    // user message), and every step of the source is a step of dsh's.
    let prompts = support::user_texts(&source);
    let mut back_prompts = support::user_texts(&back).into_iter();
    for prompt in &prompts {
        assert!(
            back_prompts.any(|text| text == *prompt),
            "{prompt:?} is back, in order"
        );
    }
    let steps = |session: &Session| {
        session
            .conversation()
            .filter(|e| matches!(&e.kind, Kind::Step { text, calls, .. } if !text.is_empty() || !calls.is_empty()))
            .count()
    };
    assert_eq!(steps(&back), steps(&source));
    // The calls dsh offers keep their ids.
    let ids = |session: &Session| -> Vec<String> {
        session
            .conversation()
            .filter_map(|e| match &e.kind {
                Kind::Step { calls, .. } => Some(calls),
                _ => None,
            })
            .flatten()
            .map(|c| c.id.clone())
            .collect()
    };
    for id in ids(&back) {
        assert!(ids(&source).contains(&id), "{id}");
    }
}

#[test]
fn a_dsh_session_hops_into_dsh_with_its_calls_and_results() {
    let source = dsh::read_file(&support::source_path(DSH)).unwrap();
    let scratch = support::scratch(DSH, "dsh-roundtrip-dsh");
    let mut opts = opts(&scratch);
    opts.cwd = source.meta.cwd.clone();
    // The fixture's cwd is `/work/demo`: absolute on every platform dsh runs on.
    let written = for_harness(DSH).write(&source, &opts).expect("write");
    let back = dsh::read_file(Path::new(&written.location)).unwrap();
    let carried: Vec<String> = support::roundtrip_script(&back);
    // The source's conversation after its checkpoint, with the user's second prompt.
    let calls: Vec<String> = back
        .entries
        .iter()
        .filter_map(|e| match &e.kind {
            Kind::Step { calls, .. } => Some(calls),
            _ => None,
        })
        .flatten()
        .map(|c| format!("{}:{}", c.source_name, c.id))
        .collect();
    // `edit` (once, pruned result) is dsh's own tool; `pwsh` is text form toward the POSIX shell
    // this test states, so it is not a call.
    assert_eq!(calls, ["edit:call-edit"]);
    let texts: Vec<&String> = carried.iter().filter(|s| s.starts_with("user|")).collect();
    assert!(texts[0].contains("README typo fix"), "{texts:?}");
    assert!(texts.iter().any(|t| t.contains("Run the tests")));
    let results: Vec<String> = back
        .entries
        .iter()
        .filter_map(|e| match &e.kind {
            Kind::Results { results } => Some(results),
            _ => None,
        })
        .flatten()
        .map(|r| format!("{}={}", r.call_id, r.content.text()))
        .collect();
    assert_eq!(results[0], "call-edit=[pruned]");
}

#[test]
fn a_written_session_is_listed_and_read_by_its_id_in_the_store() {
    let source = support::read_source(HarnessId::ClaudeCode);
    let (opts, written) = write(&source, "dsh-store");
    let found = dsh::DeepSeekHarness.list(&opts.roots).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, written.id);
    assert_eq!(
        found[0].cwd.as_deref(),
        Some(support::cwd_for(DSH).as_str())
    );
    assert_eq!(found[0].title.as_deref(), Some("interchange test"));
    let back = dsh::DeepSeekHarness.read(&written.id, &opts.roots).unwrap();
    assert_eq!(back.meta.id, written.id);
}

// ---------------------------------------------------------------------------------------------
// shapes of the conversation

#[test]
fn turns_steps_and_text_form_calls_are_laid_out_as_dsh_lays_them() {
    let source = session(vec![
        user("first"),
        step(
            &["looking"],
            vec![call(
                "c1",
                "Bash",
                CanonicalTool::Bash,
                json!({"command": "ls", "description": "list"}),
            )],
        ),
        results(&[("c1", "a\nb")]),
        step(
            &["a subagent"],
            vec![call(
                "c2",
                "Agent",
                CanonicalTool::Agent,
                json!({"prompt": "go"}),
            )],
        ),
        results(&[("c2", "done")]),
        step(&["finished"], Vec::new()),
        user("second"),
        step(&["ok"], Vec::new()),
    ]);
    let (_, written) = write(&source, "dsh-shapes");
    let (_, events, _) = read_log(&written.location);
    assert_eq!(
        kinds(&events),
        [
            "turn/start",
            "step/start",
            "system/message",
            "user/message",
            "session/title",
            // step 1: the shell call and its result
            "assistant/message",
            "tool/call",
            "tool/result",
            "step/end",
            // step 2: the subagent call is text form, so its result is a user message
            "step/start",
            "assistant/message",
            "user/message",
            "step/end",
            // step 3
            "step/start",
            "assistant/message",
            "step/end",
            "turn/end",
            // turn 2
            "turn/start",
            "step/start",
            "user/message",
            "assistant/message",
            "step/end",
            "turn/end",
        ]
    );
    let bash = &events[6]["data"];
    assert_eq!(bash["name"], "bash");
    assert_eq!(
        serde_json::from_str::<Value>(bash["arguments"].as_str().unwrap()).unwrap(),
        json!({"description": "list", "command": "ls"})
    );
    let title = &events[4]["data"];
    assert_eq!(title["title"], "interchange test");
    assert_eq!(title["messageSeqs"], json!([3]));
    // The text-form call sits in the assistant text; its result is the user's message after it.
    let text = events[10]["data"]["message"]["content"][1]["text"]
        .as_str()
        .unwrap();
    assert!(text.contains("Agent") && text.contains("c2"), "{text}");
    let answer = events[11]["data"]["content"][0]["text"].as_str().unwrap();
    assert!(answer.contains("done"), "{answer}");
}

#[test]
fn a_compaction_is_carried_as_a_user_message_that_opens_a_turn() {
    let source = session(vec![
        user("old"),
        step(&["old answer"], Vec::new()),
        Kind::Compaction {
            summary: "the gist".into(),
            kept_from: None,
            trigger: mukade::model::Trigger::Unknown,
            tokens_before: None,
        },
        user("new"),
        step(&["new answer"], Vec::new()),
    ]);
    let (_, written) = write(&source, "dsh-compaction");
    let back = dsh::read_file(Path::new(&written.location)).unwrap();
    let users: Vec<String> = back
        .entries
        .iter()
        .filter_map(|e| match &e.kind {
            Kind::User { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(users.len(), 3);
    assert!(users[1].contains("the gist"), "{users:?}");
}

#[test]
fn a_session_with_no_conversation_is_still_a_session_dsh_opens() {
    let source = session(Vec::new());
    let (_, written) = write(&source, "dsh-empty");
    let (_, events, _) = read_log(&written.location);
    assert!(events.is_empty());
    assert!(
        dsh::read_file(Path::new(&written.location))
            .unwrap()
            .entries
            .is_empty()
    );
}

#[test]
fn reasoning_travels_only_as_text_when_asked_and_never_as_a_signed_block() {
    let mut with_reasoning = session(vec![
        user("go"),
        Kind::Step {
            model: None,
            reasoning: vec![mukade::model::Reasoning {
                text: "I think".into(),
                opaque: None,
            }],
            text: vec!["answer".into()],
            calls: Vec::new(),
            end: StepEnd::Completed,
        },
    ]);
    with_reasoning.meta.title = None;
    let scratch = support::scratch(DSH, "dsh-reasoning");
    let mut opts = opts(&scratch);
    opts.reasoning = mukade::policy::ReasoningPolicy::AsText;
    opts.title = None;
    let written = for_harness(DSH).write(&with_reasoning, &opts).unwrap();
    let (_, events, _) = read_log(&written.location);
    let assistant = events
        .iter()
        .find(|e| e["type"] == "assistant/message")
        .unwrap();
    let parts = assistant["data"]["message"]["content"].as_array().unwrap();
    assert!(parts.iter().all(|p| p["type"] == "text"), "{parts:?}");
    assert!(parts[0]["text"].as_str().unwrap().contains("I think"));
    assert!(
        events.iter().all(|e| e["type"] != "session/title"),
        "no title, no title event"
    );
}

// ---------------------------------------------------------------------------------------------
// tools toward dsh

fn translated(
    canonical: CanonicalTool,
    name: &str,
    input: Value,
    shell: Shell,
    offered: ToolSet,
) -> Translation {
    let call = call("c", name, canonical, input);
    tools::translate_with(&call, DSH, Some(shell), offered)
}

fn as_call(translation: Translation) -> (String, Value) {
    match translation {
        Translation::Call { name, input } => (name.to_string(), input),
        Translation::Text { why } => panic!("text form: {why}"),
    }
}

#[test]
fn files_search_and_fetch_translate_into_dshs_own_tools() {
    let posix = |c, n: &str, i| translated(c, n, i, Shell::Posix, POSIX);
    assert_eq!(
        as_call(posix(
            CanonicalTool::Read,
            "Read",
            json!({"file_path": "a", "offset": 3, "limit": 9})
        )),
        (
            "read".into(),
            json!({"file_path": "a", "offset": 3, "limit": 9})
        )
    );
    assert_eq!(
        as_call(posix(CanonicalTool::Read, "read", json!({"path": "a"}))),
        ("read".into(), json!({"file_path": "a"}))
    );
    assert_eq!(
        as_call(posix(
            CanonicalTool::Write,
            "Write",
            json!({"file_path": "a", "content": "x"})
        )),
        ("write".into(), json!({"file_path": "a", "content": "x"}))
    );
    assert_eq!(
        as_call(posix(
            CanonicalTool::Edit,
            "Edit",
            json!({"file_path": "a", "old_string": "x", "new_string": "y", "replace_all": true})
        )),
        (
            "edit".into(),
            json!({"file_path": "a", "old_string": "x", "new_string": "y", "replace_all": true})
        )
    );
    assert_eq!(
        as_call(posix(
            CanonicalTool::Grep,
            "Grep",
            json!({"pattern": "p", "path": "src", "glob": "*.rs"})
        )),
        (
            "grep".into(),
            json!({"pattern": "p", "path": "src", "include": "*.rs"})
        )
    );
    assert_eq!(
        as_call(posix(
            CanonicalTool::Glob,
            "Glob",
            json!({"pattern": "*.rs", "path": "src"})
        )),
        ("glob".into(), json!({"pattern": "*.rs", "path": "src"}))
    );
    assert_eq!(
        as_call(posix(
            CanonicalTool::WebFetch,
            "web_fetch",
            json!({"url": "https://example.com"})
        )),
        ("web_fetch".into(), json!({"url": "https://example.com"}))
    );
}

#[test]
fn a_shell_call_is_the_tool_of_dshs_own_shell_with_its_timeout_in_milliseconds() {
    assert_eq!(
        as_call(translated(
            CanonicalTool::Bash,
            "Bash",
            json!({"command": "ls", "description": "list", "timeout": 2500}),
            Shell::Posix,
            POSIX
        )),
        (
            "bash".into(),
            json!({"description": "list", "command": "ls", "timeoutMs": 2500})
        )
    );
    assert_eq!(
        as_call(translated(
            CanonicalTool::PowerShell,
            "PowerShell",
            json!({"command": "dir", "description": "list", "workdir": "C:/w"}),
            Shell::PowerShell,
            WINDOWS
        )),
        (
            "pwsh".into(),
            json!({"description": "list", "command": "dir", "workdir": "C:/w"})
        )
    );
}

#[test]
fn what_dshs_tool_would_not_accept_travels_as_text() {
    let text = |canonical, name: &str, input, shell, offered| {
        matches!(
            translated(canonical, name, input, shell, offered),
            Translation::Text { .. }
        )
    };
    // dsh's shell tools require a description; a command in the other dialect is never rewritten.
    assert!(text(
        CanonicalTool::Bash,
        "bash",
        json!({"command": "ls"}),
        Shell::Posix,
        POSIX
    ));
    assert!(text(
        CanonicalTool::PowerShell,
        "pwsh",
        json!({"command": "dir", "description": "d"}),
        Shell::Posix,
        POSIX
    ));
    assert!(text(
        CanonicalTool::Bash,
        "Bash",
        json!({"command": "ls", "description": "d", "run_in_background": true}),
        Shell::Posix,
        POSIX
    ));
    // A bash call toward Windows's pwsh: the dialects differ.
    assert!(text(
        CanonicalTool::Bash,
        "Bash",
        json!({"command": "ls", "description": "d"}),
        Shell::PowerShell,
        WINDOWS
    ));
    // No counterpart for these arguments.
    assert!(text(
        CanonicalTool::Grep,
        "Grep",
        json!({"pattern": "p", "head_limit": 5}),
        Shell::Posix,
        POSIX
    ));
    assert!(text(
        CanonicalTool::Grep,
        "Grep",
        json!({"pattern": "p", "-i": true}),
        Shell::Posix,
        POSIX
    ));
    assert!(text(
        CanonicalTool::Glob,
        "glob",
        json!({"pattern": "p", "limit": 3}),
        Shell::Posix,
        POSIX
    ));
    assert!(text(
        CanonicalTool::Read,
        "Read",
        json!({"file_path": "a", "pages": "1-3"}),
        Shell::Posix,
        POSIX
    ));
    assert!(text(
        CanonicalTool::WebFetch,
        "WebFetch",
        json!({"url": "https://example.com", "prompt": "summarize"}),
        Shell::Posix,
        POSIX
    ));
    assert!(text(
        CanonicalTool::MultiEdit,
        "MultiEdit",
        json!({"file_path": "a", "edits": [{"old_string": "a", "new_string": "b"}, {"old_string": "c", "new_string": "d"}]}),
        Shell::Posix,
        POSIX
    ));
    // Capabilities of another harness.
    for (canonical, name) in [
        (CanonicalTool::WebSearch, "WebSearch"),
        (CanonicalTool::Agent, "Task"),
        (CanonicalTool::Skill, "Skill"),
        (CanonicalTool::Ls, "ls"),
        (CanonicalTool::Other("x".into()), "x"),
    ] {
        assert!(
            text(
                canonical,
                name,
                json!({"query": "q", "prompt": "p"}),
                Shell::Posix,
                POSIX
            ),
            "{name}"
        );
    }
}

#[test]
fn a_tool_the_set_does_not_offer_is_never_named() {
    // The Windows set has no `bash`: even a POSIX call toward a POSIX shell stays text.
    assert!(matches!(
        translated(
            CanonicalTool::Bash,
            "Bash",
            json!({"command": "ls", "description": "d"}),
            Shell::Posix,
            WINDOWS
        ),
        Translation::Text { .. }
    ));
    let own = harnesses::get(DSH).destination().unwrap().offered();
    assert!(own.contains("read") && own.contains(if cfg!(windows) { "pwsh" } else { "bash" }));
    assert!(!own.contains(if cfg!(windows) { "bash" } else { "pwsh" }));
    let shell = harnesses::get(DSH)
        .destination()
        .unwrap()
        .default_shell(&Default::default(), None);
    assert_eq!(
        shell,
        Some(if cfg!(windows) {
            Shell::PowerShell
        } else {
            Shell::Posix
        })
    );
}

// ---------------------------------------------------------------------------------------------
// the destination's rules

#[test]
fn a_reserved_id_is_used_and_a_malformed_one_refused() {
    let source = session(vec![user("hi"), step(&["ok"], Vec::new())]);
    let scratch = support::scratch(DSH, "dsh-dest-id");
    let mut opts = opts(&scratch);
    let id = format!("session-{}", uuid::Uuid::new_v4());
    opts.dest_id = Some(id.clone());
    let written = for_harness(DSH).write(&source, &opts).unwrap();
    assert_eq!(written.id, id);
    assert!(written.location.contains(&id));
    // The same id is never written twice.
    let error = format!("{:#}", for_harness(DSH).write(&source, &opts).unwrap_err());
    assert!(error.contains("already exists"), "{error}");
    // A plain UUID is an id too.
    opts.dest_id = Some(uuid::Uuid::new_v4().to_string());
    assert!(for_harness(DSH).write(&source, &opts).is_ok());
    for bad in ["../x", "session-", "session-not-a-uuid", "x", ""] {
        opts.dest_id = Some(bad.into());
        assert!(for_harness(DSH).write(&source, &opts).is_err(), "{bad:?}");
    }
    let claims = harnesses::get(DSH).destination().unwrap().claims_id(&id);
    assert!(claims);
    assert!(mukade::writer::validate_id_syntax(&id).is_ok());
}

#[test]
fn what_dsh_cannot_take_is_refused_before_anything_is_written() {
    let source = session(vec![user("hi")]);
    let scratch = support::scratch(DSH, "dsh-refusals");
    let base = opts(&scratch);
    let refused = |change: &dyn Fn(&mut WriteOpts), expect: &str| {
        let mut opts = base.clone();
        change(&mut opts);
        let error = format!("{:#}", for_harness(DSH).write(&source, &opts).unwrap_err());
        assert!(error.contains(expect), "{error}");
    };
    refused(
        &|o| o.dest_path = Some(base.roots.dsh_home.clone().unwrap().join("x")),
        "--dest-path",
    );
    refused(&|o| o.dest_agent = Some("a".into()), "--dest-agent");
    refused(&|o| o.metadata = Some(json!({})), "--metadata-file");
    refused(
        &|o| {
            o.native_budget = Some(mukade::native_compaction::NativeBudget {
                context: 100_000,
                max_output_tokens: None,
                text_token_factor: None,
            })
        },
        "--compaction native",
    );
    refused(&|o| o.cwd = Some("relative/dir".into()), "absolute");
    refused(&|o| o.roots.dsh_home = None, "DSH_HOME");
    assert!(
        !base.roots.dsh_sessions().unwrap().exists(),
        "nothing was written"
    );
}

#[test]
fn without_a_cwd_the_session_is_filed_under_no_cwd() {
    let mut source = session(vec![user("hi"), step(&["ok"], Vec::new())]);
    source.meta.cwd = None;
    let scratch = support::scratch(DSH, "dsh-no-cwd");
    let mut opts = opts(&scratch);
    opts.cwd = None;
    let written = for_harness(DSH).write(&source, &opts).unwrap();
    let sessions = opts.roots.dsh_sessions().unwrap();
    assert_eq!(
        PathBuf::from(&written.location),
        sessions
            .join("_no-cwd")
            .join(&written.id)
            .join("session.v4.jsonl.zstd")
    );
    let (header, _, _) = read_log(&written.location);
    assert!(header.get("cwd").is_none());
    assert!(
        dsh::read_file(Path::new(&written.location))
            .unwrap()
            .meta
            .cwd
            .is_none()
    );
}

#[test]
fn an_id_and_a_cwd_with_unsafe_characters_are_escaped_as_dsh_escapes_them() {
    let mut source = session(vec![user("hi")]);
    let cwd = if cfg!(windows) {
        r"D:\Work Dir\é"
    } else {
        "/work dir/é"
    };
    source.meta.cwd = Some(cwd.into());
    let scratch = support::scratch(DSH, "dsh-escapes");
    let mut opts = opts(&scratch);
    opts.cwd = None;
    let written = for_harness(DSH).write(&source, &opts).unwrap();
    let key = if cfg!(windows) {
        "--D-Work~0020Dir-~00E9--"
    } else {
        "--work~0020dir-~00E9--"
    };
    assert!(
        written
            .location
            .replace('\\', "/")
            .contains(&format!("/sessions/{key}/session-")),
        "{}",
        written.location
    );
    let (header, _, _) = read_log(&written.location);
    assert_eq!(
        header["cwd"], cwd,
        "the header keeps the path whole: the folder is lossy"
    );
}

#[test]
fn a_root_of_plain_logs_is_written_plain_and_any_other_root_is_zstd() {
    let source = session(vec![user("hi"), step(&["ok"], Vec::new())]);
    let scratch = support::scratch(DSH, "dsh-plain-root");
    let opts = opts(&scratch);
    let sessions = opts.roots.dsh_sessions().unwrap();
    // An existing plain log: dsh opens a root in one encoding only.
    let existing = sessions.join("--p--").join("session-existing");
    std::fs::create_dir_all(&existing).unwrap();
    std::fs::write(existing.join("session.v4.jsonl"), "{}\n").unwrap();
    let written = for_harness(DSH).write(&source, &opts).unwrap();
    assert!(
        written.location.ends_with("session.v4.jsonl"),
        "{}",
        written.location
    );
    let bytes = std::fs::read(&written.location).unwrap();
    assert!(!zstd_frames::is_zstd(&bytes));
    let (header, events, _) = read_log(&written.location);
    assert_eq!(header["type"], "session");
    assert!(!events.is_empty());
    assert!(dsh::read_file(Path::new(&written.location)).is_ok());
    // A root with both (or none) takes dsh's default, zstd.
    std::fs::create_dir_all(sessions.join("--p--").join("session-z")).unwrap();
    std::fs::write(
        sessions
            .join("--p--")
            .join("session-z")
            .join("session.v4.jsonl.zstd"),
        b"",
    )
    .unwrap();
    let again = for_harness(DSH).write(&source, &opts).unwrap();
    assert!(
        again.location.ends_with("session.v4.jsonl.zstd"),
        "{}",
        again.location
    );
}

// ---------------------------------------------------------------------------------------------
// images, through a hop

#[test]
fn an_image_of_another_harness_is_not_carried_and_is_counted() {
    let claude_image = json!({"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "AAAA"}});
    let source = session(vec![
        Kind::User {
            text: "look".into(),
            attachments: vec![mukade::model::Attachment {
                kind: "image".into(),
                path: None,
                raw: claude_image,
            }],
        },
        step(&["seen"], Vec::new()),
    ]);
    let scratch = support::scratch(DSH, "dsh-image");
    let opts = opts(&scratch);
    let receipt = mukade::hop::import(DSH, &source, &support::import_opts(&opts, &source, None))
        .expect("the hop");
    assert_eq!(receipt.manifest["counts"]["dropped_attachments"], 1);
    assert_eq!(receipt.manifest["strict"]["checked"], true);
    let (_, events, _) = read_log(&receipt.location);
    let message = events.iter().find(|e| e["type"] == "user/message").unwrap();
    assert_eq!(
        message["data"]["content"],
        json!([{"type": "text", "text": "look"}])
    );
}

#[test]
fn a_dsh_attachment_part_stays_in_a_dsh_session() {
    let part = json!({"type": "image", "attachment": "att-1", "offloaded": true});
    let mut source = session(vec![
        Kind::User {
            text: "look".into(),
            attachments: vec![mukade::model::Attachment {
                kind: "image".into(),
                path: None,
                raw: part.clone(),
            }],
        },
        step(&["seen"], Vec::new()),
    ]);
    source.meta.harness = DSH;
    let (_, written) = write(&source, "dsh-own-image");
    let (_, events, _) = read_log(&written.location);
    let message = events.iter().find(|e| e["type"] == "user/message").unwrap();
    assert_eq!(message["data"]["content"][1], part);
}

// ---------------------------------------------------------------------------------------------
// the whole hop

#[test]
fn the_fixtures_of_every_file_harness_hop_into_dsh_passing_the_strict_gate() {
    for source_id in [HarnessId::ClaudeCode, HarnessId::Codex, HarnessId::Pi, DSH] {
        let source = support::read_source(source_id);
        let scratch = support::scratch(DSH, &format!("dsh-hop-{}", source_id.as_str()));
        let mut write_opts = opts(&scratch);
        if source_id == DSH {
            write_opts.cwd = source.meta.cwd.clone();
        }
        let receipt = mukade::hop::import(
            DSH,
            &source,
            &support::import_opts(&write_opts, &source, None),
        )
        .unwrap_or_else(|error| panic!("{source_id}: {error:?}"));
        assert_eq!(receipt.manifest["strict"]["checked"], true, "{source_id}");
        for repair in [
            "dropped_orphan_results",
            "dropped_duplicate_results",
            "unanswered_calls",
        ] {
            assert_eq!(
                receipt.manifest["counts"][repair], 0,
                "{source_id}: {repair}"
            );
        }
        assert_eq!(receipt.manifest["dest"]["harness"], "deepseek_harness");
        let back = dsh::read_file(Path::new(&receipt.location)).unwrap();
        let first_prompt = support::user_texts(&source)
            .into_iter()
            .next()
            .unwrap_or_default();
        assert!(
            support::user_texts(&back)
                .iter()
                .any(|t| t.contains(first_prompt.trim())),
            "{source_id}: {:#?}",
            support::user_texts(&back)
        );
        assert_eq!(
            receipt.resume_command[..2],
            ["dsh".to_string(), "--profile".to_string()]
        );
    }
}

#[test]
fn meta_entries_are_never_written_as_conversation() {
    let mut source = session(vec![user("hi"), step(&["ok"], Vec::new())]);
    source.entries.push(Entry {
        id: "m".into(),
        kind: Kind::Meta {
            kind: MetaKind::Internal,
        },
        raw: vec![json!({"secret": "bookkeeping"})],
    });
    let (_, written) = write(&source, "dsh-meta");
    let text = String::from_utf8(
        zstd_frames::decode(&std::fs::read(&written.location).unwrap())
            .unwrap()
            .frames
            .concat(),
    )
    .unwrap();
    assert!(!text.contains("bookkeeping"));
}
