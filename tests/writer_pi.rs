//! Pi writer conformance (spec L3, L12, §2.4).

mod support;

use mukade::model::{
    CanonicalTool, Entry, HarnessId, Kind, Meta, Protocol, ResultContent, Session, StepEnd,
    ToolCall, ToolResult,
};
use mukade::writer::for_harness;
use serde_json::{Value, json};
use std::path::Path;

fn write_fixture(tag: &str) -> (mukade::writer::Written, Vec<Value>) {
    let scratch = support::scratch(HarnessId::Pi, tag);
    let session = support::read_source(HarnessId::Pi);
    let opts = support::opts(HarnessId::Pi, &scratch);
    let written = for_harness(HarnessId::Pi)
        .write(&session, &opts)
        .expect("write");
    let rows = support::read_jsonl(Path::new(&written.location));
    (written, rows)
}

fn messages(rows: &[Value]) -> Vec<&Value> {
    rows.iter().filter(|r| r["type"] == "message").collect()
}

/// L3: all the calls of one step live in **one** assistant message, and each is answered by its
/// own `toolResult` record — Pi otherwise inserts `No result provided` and sends the result twice.
#[test]
fn both_parallel_calls_are_in_one_assistant_message_with_one_result_each() {
    let (_, rows) = write_fixture("writer-pi-grouping");
    let assistant = messages(&rows)
        .into_iter()
        .find(|r| r["message"]["role"] == "assistant")
        .expect("an assistant message")
        .clone();
    let content = assistant["message"]["content"].as_array().unwrap();
    let calls: Vec<&Value> = content.iter().filter(|b| b["type"] == "toolCall").collect();
    assert_eq!(
        calls.len(),
        2,
        "both parallel calls in one message, not one message each: {content:?}"
    );
    assert_eq!(calls[0]["id"], "toolu_h27call1x0");
    assert_eq!(calls[1]["id"], "toolu_h27call1x1");
    assert_eq!(assistant["message"]["stopReason"], "toolUse");

    let results: Vec<&Value> = messages(&rows)
        .into_iter()
        .filter(|r| r["message"]["role"] == "toolResult")
        .collect();
    assert_eq!(results.len(), 2, "one record per call");
    assert_eq!(results[0]["message"]["toolCallId"], "toolu_h27call1x0");
    assert_eq!(results[1]["message"]["toolCallId"], "toolu_h27call1x1");
    assert!(results.iter().all(|r| r["message"]["isError"] == false));
    // The results follow the message that called, and the chain is unbroken.
    let ids: Vec<String> = messages(&rows)
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect();
    let parents: Vec<Value> = messages(&rows)
        .iter()
        .map(|r| r["parentId"].clone())
        .collect();
    assert_eq!(parents[0], Value::Null, "the first message has no parent");
    for (i, parent) in parents.iter().enumerate().skip(1) {
        assert_eq!(
            parent.as_str(),
            Some(ids[i - 1].as_str()),
            "each message hangs on the one before it"
        );
    }
}

#[test]
fn the_session_file_lands_where_pi_looks_for_it() {
    let (written, rows) = write_fixture("writer-pi-path");
    let path = written.location.replace('\\', "/");
    let project = pi_project_dir_name(std::path::Path::new(&support::cwd_for(HarnessId::Pi)));
    assert!(path.contains(&format!("/sessions/{project}/")), "{path}");
    assert!(
        Path::new(&written.location)
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("2026-10-27T16-00-00-000Z_")
                && n.ends_with(&format!("{}.jsonl", written.id))),
        "{path}"
    );
    assert_eq!(rows[0]["type"], "session");
    assert_eq!(rows[0]["version"], 3);
    assert_eq!(rows[0]["id"], written.id);
    assert_eq!(
        written.resume.last().map(String::as_str),
        Some(written.location.as_str()),
        "Pi is given the file path, not an id it would search for"
    );
}

/// L12: Pi counts the timeout in seconds, so a canonical millisecond value is converted.
#[test]
fn a_shell_call_carries_its_timeout_in_seconds() {
    let scratch = support::scratch(HarnessId::Pi, "writer-pi-timeout");
    let session = Session {
        meta: Meta {
            harness: HarnessId::ClaudeCode,
            harness_version: "2.1.311".into(),
            id: "src".into(),
            cwd: Some(support::cwd_for(HarnessId::ClaudeCode)),
            title: None,
            created: None,
            updated: None,
            models: vec![],
            git_branch: None,
            parent: None,
            source_path: None,
        },
        entries: vec![
            Entry {
                id: "e1".into(),
                kind: Kind::Step {
                    model: None,
                    reasoning: vec![],
                    text: vec![],
                    calls: vec![ToolCall {
                        id: "c1".into(),
                        source_name: "Bash".into(),
                        canonical: Some(CanonicalTool::Bash),
                        input: json!({"command": "make -j4", "timeout": 5000, "description": "build"}),
                    }],
                    end: StepEnd::ToolUse,
                },
                raw: vec![],
            },
            // §7: a call the source never answered is not written as a tool call, so the answer
            // has to be here for this test to be about the timeout at all.
            Entry {
                id: "e2".into(),
                kind: Kind::Results {
                    results: vec![ToolResult {
                        call_id: "c1".into(),
                        content: ResultContent::Text("done".into()),
                        is_error: false,
                        truncated_by_source: false,
                    }],
                },
                raw: vec![],
            },
        ],
    };
    let opts = support::opts(HarnessId::Pi, &scratch);
    let written = for_harness(HarnessId::Pi)
        .write(&session, &opts)
        .expect("write");
    let rows = support::read_jsonl(Path::new(&written.location));
    let call = messages(&rows)
        .into_iter()
        .flat_map(|r| {
            r["message"]["content"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
        .find(|b| b["type"] == "toolCall")
        .expect("the shell call");
    assert_eq!(call["name"], "bash");
    assert_eq!(call["arguments"]["command"], "make -j4");
    assert_eq!(
        call["arguments"]["timeout"], 5,
        "5000 ms is 5 seconds for Pi"
    );
    assert_eq!(
        call["arguments"]["description"],
        Value::Null,
        "description is dropped, not invented"
    );
}

/// A call the destination cannot express travels as text in the assistant message, so the request
/// stays valid for the provider.
#[test]
fn an_untranslatable_call_becomes_text_in_the_message() {
    let scratch = support::scratch(HarnessId::Pi, "writer-pi-textform");
    let mut session = support::read_source(HarnessId::ClaudeCode);
    session.entries.push(Entry {
        id: "extra".into(),
        kind: Kind::Step {
            model: None,
            reasoning: vec![],
            text: vec![],
            calls: vec![ToolCall {
                id: "sub-1".into(),
                source_name: "Agent".into(),
                canonical: Some(CanonicalTool::Agent),
                input: json!({"prompt": "look around"}),
            }],
            end: StepEnd::ToolUse,
        },
        raw: vec![],
    });
    let opts = support::opts(HarnessId::Pi, &scratch);
    let written = for_harness(HarnessId::Pi)
        .write(&session, &opts)
        .expect("write");
    let rows = support::read_jsonl(Path::new(&written.location));
    let texts: Vec<String> = messages(&rows)
        .into_iter()
        .flat_map(|r| {
            r["message"]["content"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
        .filter(|b| b["type"] == "text")
        .filter_map(|b| b["text"].as_str().map(str::to_owned))
        .collect();
    assert!(
        texts
            .iter()
            .any(|t| t.starts_with("[tool call · Agent · id sub-1 · not available in pi]")),
        "{texts:?}"
    );
    assert!(
        !messages(&rows)
            .into_iter()
            .flat_map(|r| r["message"]["content"]
                .as_array()
                .cloned()
                .unwrap_or_default())
            .any(|b| b["type"] == "toolCall" && b["id"] == "sub-1"),
        "no invented tool call"
    );
}

#[test]
fn the_recorded_protocol_is_written_only_when_the_caller_states_it() {
    let scratch = support::scratch(HarnessId::Pi, "writer-pi-api");
    let session = support::read_source(HarnessId::Pi);
    let mut opts = support::opts(HarnessId::Pi, &scratch);
    let written = for_harness(HarnessId::Pi)
        .write(&session, &opts)
        .expect("write");
    let rows = support::read_jsonl(Path::new(&written.location));
    assert_eq!(
        messages(&rows)
            .into_iter()
            .find(|r| r["message"]["role"] == "assistant")
            .unwrap()["message"]["api"],
        "anthropic-messages"
    );

    opts.dest_protocol = Some(Protocol::OpenaiResponses);
    let written = for_harness(HarnessId::Pi)
        .write(&session, &opts)
        .expect("write");
    let rows = support::read_jsonl(Path::new(&written.location));
    assert_eq!(
        messages(&rows)
            .into_iter()
            .find(|r| r["message"]["role"] == "assistant")
            .unwrap()["message"]["api"],
        "openai-responses"
    );
}

/// Pi 1.2.0's own rule (`getDefaultSessionDirPath`): the resolved path, one leading separator
/// dropped, and only `/`, `\` and `:` replaced.
fn pi_project_dir_name(cwd: &std::path::Path) -> String {
    let resolved = std::path::absolute(cwd)
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let trimmed = resolved
        .strip_prefix(['/', '\\'])
        .unwrap_or(&resolved)
        .replace(['/', '\\', ':'], "-");
    format!("--{trimmed}--")
}

/// Pi replays an aborted or failed turn differently from a finished one: the step keeps its end.
#[test]
fn an_aborted_step_stays_aborted_in_pi() {
    use mukade::model::{Entry, Kind, StepEnd};
    let scratch = support::scratch(HarnessId::Pi, "writer-pi-aborted");
    let opts = support::opts(HarnessId::Pi, &scratch);
    let mut session = support::read_source(HarnessId::ClaudeCode);
    session.entries.push(Entry {
        id: "aborted".into(),
        kind: Kind::Step {
            model: None,
            reasoning: vec![],
            text: vec!["PARTIAL".into()],
            calls: vec![],
            end: StepEnd::Aborted,
        },
        raw: vec![],
    });
    let written = mukade::writer::for_harness(HarnessId::Pi)
        .write(&session, &opts)
        .unwrap();
    let last = std::fs::read_to_string(&written.location)
        .unwrap()
        .lines()
        .last()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .unwrap();
    assert_eq!(last["message"]["stopReason"], "aborted", "{last}");
}
