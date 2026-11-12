//! Codex CLI writer conformance (spec L7, §2.2).

mod support;

use mukade::model::{HarnessId, Kind};
use mukade::native_compaction::NativeBudget;
use mukade::writer::for_harness;
use serde_json::Value;
use std::path::Path;

/// The first payload of a given `type`.
fn payload(rows: &[Value], kind: &str) -> Value {
    rows.iter()
        .map(|r| r.get("payload").cloned().unwrap_or(Value::Null))
        .find(|p| p.get("type").and_then(Value::as_str) == Some(kind))
        .unwrap_or(Value::Null)
}

/// Every payload of a given `type`.
fn payloads(rows: &[Value], kind: &str) -> Vec<Value> {
    rows.iter()
        .map(|r| r.get("payload").cloned().unwrap_or(Value::Null))
        .filter(|p| p.get("type").and_then(Value::as_str) == Some(kind))
        .collect()
}

/// L7: `session_meta` carries Codex's own fields, with the **destination's** version and provider.
#[test]
fn session_meta_states_the_destination_and_never_the_source() {
    let (written, rows) = support::write_fixture(HarnessId::Codex, "writer-codex-meta");
    let meta = &rows[0];
    assert_eq!(meta["type"], "session_meta", "the rollout starts with it");
    let payload = &meta["payload"];
    assert_eq!(
        payload["id"], written.id,
        "the thread id is the new session id"
    );
    assert!(
        payload.get("session_id").is_none(),
        "a written rollout carries no session_id: Codex refuses the file when it does"
    );
    assert!(
        payload.get("base_instructions").is_some(),
        "the parser wants the key present"
    );
    assert_eq!(payload["originator"], "codex_cli_rs");
    assert_eq!(
        payload["source"], "cli",
        "a `exec` source would be hidden from `thread/list`"
    );
    assert_eq!(
        payload["cli_version"], "0.164.0",
        "the destination's version, not the source's 2.1.311"
    );
    assert_eq!(
        payload["model_provider"], "h27",
        "the destination config's provider, not `openai`"
    );
    assert_eq!(payload["cwd"], support::cwd_for(HarnessId::Codex));
    assert!(
        payload.get("git").is_none(),
        "this source records no branch, so the writer invents none"
    );
}

/// With no provider stated and none configured, the rollout names Codex's own default: Codex
/// 0.167.2 indexes a rollout without `model_provider` under the empty provider and refuses to
/// resume it (``Model provider `` not found``, live on 2026-11-12).
#[test]
fn session_meta_names_codex_default_provider_when_none_is_known() {
    let scratch = support::scratch(HarnessId::Codex, "writer-codex-default-provider");
    let session = support::read_source(HarnessId::ClaudeCode);
    let mut opts = support::opts(HarnessId::Codex, &scratch);
    opts.model_provider = None;
    let written = for_harness(HarnessId::Codex)
        .write(&session, &opts)
        .expect("write");
    let rows = support::read_jsonl(Path::new(&written.location));
    assert_eq!(rows[0]["payload"]["model_provider"], "openai");
}

/// A branch, when the source session has one, travels into `session_meta` — the shape Codex writes.
#[test]
fn a_git_branch_is_carried_when_the_source_has_one() {
    let scratch = support::scratch(HarnessId::Codex, "writer-codex-git");
    let session = support::read_source(HarnessId::ClaudeCode);
    assert_eq!(session.meta.git_branch.as_deref(), Some("HEAD"));
    let opts = support::opts(HarnessId::Codex, &scratch);
    let written = for_harness(HarnessId::Codex)
        .write(&session, &opts)
        .expect("write");
    let rows = support::read_jsonl(Path::new(&written.location));
    assert_eq!(rows[0]["payload"]["git"]["branch"], "HEAD");
    assert_eq!(
        rows[0]["payload"]["cli_version"], "0.164.0",
        "still the destination's version"
    );
}

#[test]
fn the_rollout_lands_in_the_date_folder_codex_scans() {
    let (written, _) = support::write_fixture(HarnessId::Codex, "writer-codex-path");
    let path = written.location.replace('\\', "/");
    assert!(path.contains("/sessions/2026/10/27/"), "{path}");
    assert!(
        Path::new(&written.location)
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("rollout-2026-10-27T16-00-00-")
                && n.ends_with(&format!("{}.jsonl", written.id))),
        "{path}"
    );
    assert_eq!(
        written.resume,
        vec!["codex", "exec", "resume", written.id.as_str()]
    );
}

/// L7: a writer must not leave a stale `threads` row behind. It writes no SQLite at all.
#[test]
fn no_database_is_touched() {
    let (written, _) = support::write_fixture(HarnessId::Codex, "writer-codex-nodb");
    let home = Path::new(&written.location)
        .ancestors()
        .nth(5)
        .expect("the sessions root")
        .to_path_buf();
    // The Codex home must exist and hold the sessions folder, or the check below says nothing.
    assert!(
        home.join("sessions").is_dir(),
        "{} is not the Codex home",
        home.display()
    );
    let leftovers: Vec<String> = std::fs::read_dir(&home)
        .expect("the Codex home is readable")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains("sqlite") || n.contains("state_"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "no SQLite state was written: {leftovers:?}"
    );
}

#[test]
fn the_conversation_is_response_items_with_a_turn_context_per_step() {
    let (_, rows) = support::write_fixture(HarnessId::Codex, "writer-codex-items");
    let contexts = rows.iter().filter(|r| r["type"] == "turn_context").count();
    assert_eq!(contexts, 4, "one turn per model step");
    assert_eq!(payload(&rows, "message")["role"], "user");
    assert_eq!(
        payload(&rows, "reasoning")["summary"][0]["type"],
        "summary_text"
    );
    assert_eq!(
        payload(&rows, "reasoning")["encrypted_content"],
        "h27-mock-encrypted-reasoning",
        "the opaque payload is carried unmodified"
    );

    // Every ordinal is present and contiguous, like a native rollout.
    let ordinals: Vec<u64> = rows
        .iter()
        .map(|r| r["ordinal"].as_u64().unwrap())
        .collect();
    assert_eq!(ordinals, (0..rows.len() as u64).collect::<Vec<_>>());
    assert!(rows.iter().all(|r| r["timestamp"].is_string()));
}

#[test]
fn message_ids_are_in_codex_own_local_shape() {
    // Codex 0.166 forwards a rollout id it does not recognise to the Responses API, which refuses
    // it: `This request does not support user-supplied input item IDs.` (live, 2026-11-04). Its own
    // local items carry `msg_<uuid v7, dashed>`; a written rollout must use the same shape or the
    // thread cannot be resumed.
    let (_, rows) = support::write_fixture(HarnessId::Codex, "writer-codex-items");
    let messages = payloads(&rows, "message");
    let ids: Vec<&str> = messages.iter().filter_map(|p| p["id"].as_str()).collect();
    assert!(!ids.is_empty(), "the fixture writes message ids");
    for id in ids {
        let Some(rest) = id.strip_prefix("msg_") else {
            panic!("{id}: not a message id");
        };
        let bytes: Vec<char> = rest.chars().collect();
        assert_eq!(bytes.len(), 36, "{id}: not a dashed uuid");
        for (i, c) in bytes.iter().enumerate() {
            let dash = i == 8 || i == 13 || i == 18 || i == 23;
            assert_eq!(
                *c == '-',
                dash,
                "{id}: position {i} is a dash only in the uuid shape"
            );
            if !dash {
                assert!(c.is_ascii_hexdigit(), "{id}: {c} is not hexadecimal");
            }
        }
        assert_eq!(bytes[14], '7', "{id}: not a version 7 uuid");
    }
}

#[test]
fn calls_and_results_are_paired_by_call_id() {
    let (_, rows) = support::write_fixture(HarnessId::Codex, "writer-codex-tools");
    let calls = payloads(&rows, "function_call");
    assert_eq!(calls.len(), 2, "both parallel calls");
    // The source's commands ran in the shell its environment block names (powershell), so the
    // canonical form is a PowerShell call and the destination — also powershell — keeps it.
    assert!(calls.iter().all(|c| c["name"] == "exec_command"));
    let arguments: Value = serde_json::from_str(
        calls[0]["arguments"]
            .as_str()
            .expect("arguments is a JSON string"),
    )
    .expect("it parses");
    assert!(
        arguments["cmd"].as_str().unwrap().contains("alpha.txt"),
        "{arguments}"
    );

    let outputs: Vec<String> = payloads(&rows, "function_call_output")
        .iter()
        .filter_map(|p| p["call_id"].as_str().map(str::to_owned))
        .collect();
    assert_eq!(outputs, vec!["call_h27call1x0", "call_h27call1x1"]);
    for call in &calls {
        assert!(outputs.contains(&call["call_id"].as_str().unwrap().to_string()));
    }
}

/// §4.4 for Codex: the rollout keeps every generated row, and one `compacted` record is appended.
/// Its `replacement_history` is the retained tail Codex will send after the summary.
#[test]
fn a_native_budget_appends_a_codex_compacted_record_without_dropping_history() {
    let source = support::read_source(HarnessId::Codex);
    let first_user = support::user_texts(&source)
        .into_iter()
        .next()
        .expect("a user turn");
    let (_, baseline) = support::write_fixture(HarnessId::Codex, "writer-codex-native-baseline");

    let scratch = support::scratch(HarnessId::Codex, "writer-codex-native-boundary");
    let mut opts = support::opts(HarnessId::Codex, &scratch);
    opts.native_budget = Some(NativeBudget {
        context: 40_000,
        max_output_tokens: Some(32_000),
        text_token_factor: None,
    });
    let written = for_harness(HarnessId::Codex)
        .write(&source, &opts)
        .expect("write");
    let rows = support::read_jsonl(Path::new(&written.location));

    let boundary = rows.last().expect("a compacted record");
    assert_eq!(boundary["type"], "compacted");
    let message = boundary["payload"]["message"].as_str().unwrap();
    assert!(message.contains("mukade native compaction"));
    assert!(message.contains(&first_user.chars().take(40).collect::<String>()));

    let replacement = boundary["payload"]["replacement_history"]
        .as_array()
        .expect("the retained tail");
    let response_items: Vec<&Value> = rows[..rows.len() - 1]
        .iter()
        .filter(|row| row["type"] == "response_item")
        .map(|row| &row["payload"])
        .collect();
    assert_eq!(
        replacement.len(),
        2,
        "the summary travels in replacement_history, then the retained item"
    );
    let summary_item = &replacement[0];
    assert_eq!(summary_item["type"], "message");
    assert_eq!(summary_item["role"], "user");
    let summary_text = summary_item["content"][0]["text"].as_str().unwrap();
    assert!(summary_text.contains("mukade native compaction"));
    assert!(summary_text.contains(&first_user.chars().take(40).collect::<String>()));
    let mut expected = (*response_items.last().unwrap()).clone();
    expected.as_object_mut().unwrap().remove("id");
    assert_eq!(replacement[1], expected);
    assert!(
        replacement.iter().all(|item| item.get("id").is_none()),
        "Codex's Responses API refuses user-supplied input item IDs in replacement_history"
    );

    assert_eq!(rows.len(), baseline.len() + 1, "only the boundary is added");
    assert_eq!(
        rows[..baseline.len()]
            .iter()
            .map(|row| row["type"].as_str())
            .collect::<Vec<_>>(),
        baseline
            .iter()
            .map(|row| row["type"].as_str())
            .collect::<Vec<_>>()
    );

    let read_back = mukade::reader::codex::read_file(Path::new(&written.location))
        .expect("Codex reads its own compacted record");
    assert!(read_back.entries.iter().any(|entry| matches!(&entry.kind, Kind::Compaction { summary, .. } if summary.contains("mukade native compaction"))));
}

/// The `item_completed` events of a rollout, with the turn each names.
fn items(rows: &[Value]) -> Vec<Value> {
    payloads(rows, "item_completed")
}

/// Codex 0.167 draws a resumed thread from `item_completed` events of a `paginated` thread, never
/// from the response items the model reads (live, 2026-11-12: a written thread showed an empty
/// screen; the same thread with these records shows its prompts and answers).
#[test]
fn the_history_codex_draws_travels_as_item_completed_events() {
    let (written, rows) = support::write_fixture(HarnessId::Codex, "writer-codex-screen");
    assert_eq!(rows[0]["payload"]["history_mode"], "paginated");

    let items = items(&rows);
    assert!(
        items
            .iter()
            .all(|e| e["thread_id"] == written.id && e["turn_id"].is_string()),
        "each event names the thread and its turn"
    );

    // One UserMessage per prompt, with the prompt's text.
    let prompts: Vec<String> = payloads(&rows, "message")
        .iter()
        .filter(|m| m["role"] == "user")
        .filter_map(|m| m["content"][0]["text"].as_str().map(str::to_owned))
        .filter(|t| !t.starts_with("<environment_context>"))
        .collect();
    let users: Vec<String> = items
        .iter()
        .filter(|e| e["item"]["type"] == "UserMessage")
        .map(|e| e["item"]["content"][0]["text"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(users, prompts);

    // One AgentMessage per assistant message, with its text and phase.
    let answers: Vec<(String, Value)> = payloads(&rows, "message")
        .iter()
        .filter(|m| m["role"] == "assistant")
        .map(|m| (m["content"][0]["text"].as_str().unwrap().to_owned(), m["phase"].clone()))
        .collect();
    let agents: Vec<(String, Value)> = items
        .iter()
        .filter(|e| e["item"]["type"] == "AgentMessage")
        .map(|e| {
            let item = &e["item"];
            (item["content"][0]["text"].as_str().unwrap().to_owned(), item["phase"].clone())
        })
        .collect();
    assert!(!answers.is_empty());
    assert_eq!(agents, answers);
}

/// A shell call Codex can run shows as `Ran <command>` with its output: a `CommandExecution`.
#[test]
fn a_shell_call_is_drawn_as_a_command_execution_with_its_output() {
    let (_, rows) = support::write_fixture(HarnessId::Codex, "writer-codex-screen-calls");
    let calls = payloads(&rows, "function_call");
    let outputs = payloads(&rows, "function_call_output");
    let executions: Vec<Value> = items(&rows)
        .into_iter()
        .filter(|e| e["item"]["type"] == "CommandExecution")
        .map(|e| e["item"].clone())
        .collect();
    assert_eq!(executions.len(), calls.len(), "one per exec_command call");
    for (call, execution) in calls.iter().zip(&executions) {
        let arguments: Value = serde_json::from_str(call["arguments"].as_str().unwrap()).unwrap();
        assert_eq!(execution["id"], call["call_id"]);
        assert_eq!(execution["parsed_cmd"][0]["cmd"], arguments["cmd"]);
        assert_eq!(execution["command"].as_array().unwrap().last().unwrap(), &arguments["cmd"]);
        let output = outputs
            .iter()
            .find(|o| o["call_id"] == call["call_id"])
            .expect("the call's result");
        assert_eq!(execution["aggregated_output"], output["output"]);
        assert_eq!(execution["status"], "completed");
    }
}

/// A turn another prompt follows ended: without its `task_complete`, Codex draws `Conversation
/// interrupted` under it (live, 2026-11-12, three times in one written thread).
#[test]
fn every_turn_another_prompt_follows_is_completed() {
    let mut session = support::read_source(HarnessId::ClaudeCode);
    // Every turn of the fixture ends on a final answer; make the first one end on its calls, as a
    // turn does when the user writes again before the model answers.
    let second_prompt = session
        .entries
        .iter()
        .enumerate()
        .filter(|(_, e)| matches!(e.kind, Kind::User { .. }))
        .nth(1)
        .map(|(i, _)| i)
        .expect("a second prompt");
    for entry in session.entries[..second_prompt].iter_mut() {
        if let Kind::Step { end, .. } = &mut entry.kind {
            *end = mukade::model::StepEnd::ToolUse;
        }
    }
    let scratch = support::scratch(HarnessId::Codex, "writer-codex-turns");
    let opts = support::opts(HarnessId::Codex, &scratch);
    let written = for_harness(HarnessId::Codex)
        .write(&session, &opts)
        .expect("write");
    let rows = support::read_jsonl(Path::new(&written.location));
    let events: Vec<Value> = rows
        .iter()
        .filter(|r| r["type"] == "event_msg")
        .map(|r| r["payload"].clone())
        .filter(|p| p["type"] == "task_started" || p["type"] == "task_complete")
        .collect();
    let started: Vec<&Value> = events.iter().filter(|p| p["type"] == "task_started").collect();
    assert!(started.len() > 1, "the fixture has several prompts");
    for (n, turn) in started.iter().enumerate().take(started.len() - 1) {
        let id = &turn["turn_id"];
        let at = events.iter().position(|p| p == *turn).unwrap();
        let next = events.iter().position(|p| p == started[n + 1]).unwrap();
        assert!(
            events[at + 1..next]
                .iter()
                .any(|p| p["type"] == "task_complete" && &p["turn_id"] == id),
            "turn {id} ends before the next prompt"
        );
    }
}
