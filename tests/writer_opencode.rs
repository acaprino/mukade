//! OpenCode writer conformance (spec L1, L8, §2.3).

mod support;

use serde_json::Value;
use mukade::model::HarnessId;
use mukade::policy::Clock;
use mukade::writer::opencode::{document_for, import_command};
use std::path::Path;

fn document(tag: &str) -> (Value, mukade::writer::WriteOpts, std::path::PathBuf) {
    let scratch = support::scratch(HarnessId::OpenCode, tag);
    let session = support::read_source(HarnessId::OpenCode);
    let mut opts = support::opts(HarnessId::OpenCode, &scratch);
    opts.project_id = Some("project-under-test".into());
    let mut clock = Clock::new(opts.now.as_deref());
    let cwd = support::cwd_for(HarnessId::OpenCode);
    (document_for(&session, "ses_test", &cwd, &opts, &mut clock).expect("document"), opts, scratch)
}

fn messages(document: &Value) -> Vec<&Value> {
    document["messages"].as_array().map(|m| m.iter().collect()).unwrap_or_default()
}

/// L1: without `--standalone`, OpenCode 2.1.4 hands the work to the background service of the
/// user's real home.
#[test]
fn the_import_command_always_carries_standalone() {
    let argv = import_command("opencode", Path::new(r"C:\tmp\doc.json"), r"C:\work");
    assert_eq!(argv[0], "opencode");
    assert_eq!(&argv[1..4], &["session", "import", r"C:\tmp\doc.json"]);
    assert!(argv.contains(&"--standalone".to_string()), "{argv:?}");
    assert_eq!(argv[argv.len() - 3], "--directory");
    assert_eq!(argv[argv.len() - 2], r"C:\work");
}

/// L8: one assistant message per step, holding reasoning, text and **both** tool parts, with each
/// call's result inside its own part.
#[test]
fn one_assistant_message_per_step_holds_both_calls_and_their_results() {
    let (doc, _, _) = document("writer-opencode-grouping");
    let assistant = messages(&doc)
        .into_iter()
        .find(|m| m["type"] == "assistant")
        .expect("an assistant message")
        .clone();
    let content = assistant["content"].as_array().unwrap();
    let kinds: Vec<&str> = content.iter().map(|b| b["type"].as_str().unwrap_or("")).collect();
    assert_eq!(kinds, vec!["reasoning", "text", "tool", "tool"], "one row per step, all its blocks");

    let tools: Vec<&Value> = content.iter().filter(|b| b["type"] == "tool").collect();
    assert_eq!(tools[0]["id"], "toolu_h27call1x0");
    assert_eq!(tools[0]["name"], "read");
    assert_eq!(tools[0]["state"]["status"], "completed");
    assert!(tools[0]["state"]["input"]["path"].as_str().unwrap().ends_with("alpha.txt"));
    assert_eq!(
        tools[0]["state"]["content"][0]["text"],
        "Read file C:\\Users\\alfio\\AppData\\Local\\shinatama-verify\\H27\\opencode\\work\\alpha.txt, lines 1-1\n1: ALPHA-CONTENT-42",
        "the result stays in the part that made the call"
    );
    assert_eq!(tools[1]["id"], "toolu_h27call1x1");
    assert!(assistant["content"].as_array().unwrap()[0]["native"]["signature"].as_str() == Some("h27-mock-signature"));
    assert_eq!(assistant["finish"], "tool-calls");

    // One assistant message per step: the source has five steps, but the fifth is the row the
    // killed process left behind — empty text, no call — and an empty assistant message is invalid
    // in every wire dialect, so the plan records it as dropped instead of writing it.
    assert_eq!(messages(&doc).iter().filter(|m| m["type"] == "assistant").count(), 4);
}

#[test]
fn the_document_states_where_the_session_belongs() {
    let (doc, opts, _) = document("writer-opencode-info");
    assert_eq!(doc["info"]["id"], "ses_test");
    assert_eq!(doc["info"]["location"]["directory"], support::cwd_for(HarnessId::OpenCode));
    assert_eq!(doc["info"]["title"], opts.title.clone().unwrap());
    assert_eq!(doc["info"]["model"]["providerID"], "h27");
    assert_eq!(doc["info"]["projectID"], "project-under-test", "the destination's own project id");
    assert_eq!(doc["info"]["cost"], 0);
    assert_eq!(doc["info"]["tokens"]["input"], 0, "OpenCode refuses an info without tokens");
    let users: Vec<&str> = messages(&doc)
        .iter()
        .filter(|m| m["type"] == "user")
        .filter_map(|m| m["text"].as_str())
        .collect();
    assert!(users.first().is_some_and(|u| u.starts_with("H27-TOOLS read")), "{users:?}");
    assert!(users.iter().any(|u| u.contains("H27-SUMMARY")), "the source's summary travelled");
}

/// OpenCode 2.1.4 refuses to import a tool part in state `error` without its `error`, a
/// `Session.StructuredError` ([V] 2026-10-28: `session import` answers `Bad Request`). OpenCode
/// itself records a failed call as `tool.execution` with the result's text as the message.
#[test]
fn a_failed_call_carries_its_error_text() {
    let scratch = support::scratch(HarnessId::OpenCode, "writer-opencode-error");
    let mut session = support::read_source(HarnessId::OpenCode);
    let failed = session
        .entries
        .iter_mut()
        .find_map(|entry| match &mut entry.kind {
            mukade::model::Kind::Results { results } => results.first_mut(),
            _ => None,
        })
        .expect("the fixture has a tool result");
    failed.is_error = true;
    failed.content = mukade::model::ResultContent::Text("ENOENT: no such file".into());
    let mut opts = support::opts(HarnessId::OpenCode, &scratch);
    opts.project_id = Some("project-under-test".into());
    let mut clock = Clock::new(opts.now.as_deref());
    let cwd = support::cwd_for(HarnessId::OpenCode);
    let document = document_for(&session, "ses_test", &cwd, &opts, &mut clock).expect("document");
    let state = messages(&document)
        .iter()
        .filter_map(|message| message["content"].as_array())
        .flatten()
        .map(|part| &part["state"])
        .find(|state| state["status"] == "error")
        .expect("the failed call is a tool part in state error");
    assert_eq!(state["error"]["type"], "tool.execution", "{state}");
    assert_eq!(state["error"]["message"], "ENOENT: no such file", "{state}");
}
