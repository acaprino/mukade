//! Codex CLI writer conformance (spec L7, §2.2).

mod support;

use serde_json::Value;
use mukade::model::HarnessId;
use mukade::writer::for_harness;
use std::path::Path;

fn write_fixture(tag: &str) -> (mukade::writer::Written, Vec<Value>) {
    let scratch = support::scratch(HarnessId::Codex, tag);
    let session = support::read_source(HarnessId::Codex);
    let opts = support::opts(HarnessId::Codex, &scratch);
    let written = for_harness(HarnessId::Codex).write(&session, &opts).expect("write");
    let rows = support::read_jsonl(Path::new(&written.location));
    (written, rows)
}

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
    let (written, rows) = write_fixture("writer-codex-meta");
    let meta = &rows[0];
    assert_eq!(meta["type"], "session_meta", "the rollout starts with it");
    let payload = &meta["payload"];
    assert_eq!(payload["id"], written.id, "the thread id is the new session id");
    assert!(
        payload.get("session_id").is_none(),
        "a written rollout carries no session_id: Codex refuses the file when it does"
    );
    assert!(payload.get("base_instructions").is_some(), "the parser wants the key present");
    assert_eq!(payload["originator"], "codex_cli_rs");
    assert_eq!(payload["source"], "cli", "a `exec` source would be hidden from `thread/list`");
    assert_eq!(payload["cli_version"], "0.164.0", "the destination's version, not the source's 2.1.311");
    assert_eq!(payload["model_provider"], "h27", "the destination config's provider, not `openai`");
    assert_eq!(payload["cwd"], support::cwd_for(HarnessId::Codex));
    assert!(
        payload.get("git").is_none(),
        "this source records no branch, so the writer invents none"
    );
}

/// A branch, when the source session has one, travels into `session_meta` — the shape Codex writes.
#[test]
fn a_git_branch_is_carried_when_the_source_has_one() {
    let scratch = support::scratch(HarnessId::Codex, "writer-codex-git");
    let session = support::read_source(HarnessId::ClaudeCode);
    assert_eq!(session.meta.git_branch.as_deref(), Some("HEAD"));
    let opts = support::opts(HarnessId::Codex, &scratch);
    let written = for_harness(HarnessId::Codex).write(&session, &opts).expect("write");
    let rows = support::read_jsonl(Path::new(&written.location));
    assert_eq!(rows[0]["payload"]["git"]["branch"], "HEAD");
    assert_eq!(rows[0]["payload"]["cli_version"], "0.164.0", "still the destination's version");
}

#[test]
fn the_rollout_lands_in_the_date_folder_codex_scans() {
    let (written, _) = write_fixture("writer-codex-path");
    let path = written.location.replace('\\', "/");
    assert!(path.contains("/sessions/2026/10/27/"), "{path}");
    assert!(
        Path::new(&written.location)
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("rollout-2026-10-27T16-00-00-") && n.ends_with(&format!("{}.jsonl", written.id))),
        "{path}"
    );
    assert_eq!(written.resume, vec!["codex", "exec", "resume", written.id.as_str()]);
}

/// L7: a writer must not leave a stale `threads` row behind. It writes no SQLite at all.
#[test]
fn no_database_is_touched() {
    let (written, _) = write_fixture("writer-codex-nodb");
    let home = Path::new(&written.location).ancestors().nth(5).expect("the sessions root").to_path_buf();
    let leftovers: Vec<String> = std::fs::read_dir(&home)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.contains("sqlite") || n.contains("state_"))
                .collect()
        })
        .unwrap_or_default();
    assert!(leftovers.is_empty(), "no SQLite state was written: {leftovers:?}");
}

#[test]
fn the_conversation_is_response_items_with_a_turn_context_per_step() {
    let (_, rows) = write_fixture("writer-codex-items");
    let contexts = rows.iter().filter(|r| r["type"] == "turn_context").count();
    assert_eq!(contexts, 4, "one turn per model step");
    assert_eq!(payload(&rows, "message")["role"], "user");
    assert_eq!(payload(&rows, "reasoning")["summary"][0]["type"], "summary_text");
    assert_eq!(
        payload(&rows, "reasoning")["encrypted_content"], "h27-mock-encrypted-reasoning",
        "the opaque payload is carried unmodified"
    );

    // Every ordinal is present and contiguous, like a native rollout.
    let ordinals: Vec<u64> = rows.iter().map(|r| r["ordinal"].as_u64().unwrap()).collect();
    assert_eq!(ordinals, (0..rows.len() as u64).collect::<Vec<_>>());
    assert!(rows.iter().all(|r| r["timestamp"].is_string()));
}

#[test]
fn calls_and_results_are_paired_by_call_id() {
    let (_, rows) = write_fixture("writer-codex-tools");
    let calls = payloads(&rows, "function_call");
    assert_eq!(calls.len(), 2, "both parallel calls");
    // The source's commands ran in the shell its environment block names (powershell), so the
    // canonical form is a PowerShell call and the destination — also powershell — keeps it.
    assert!(calls.iter().all(|c| c["name"] == "exec_command"));
    let arguments: Value =
        serde_json::from_str(calls[0]["arguments"].as_str().expect("arguments is a JSON string")).expect("it parses");
    assert!(arguments["cmd"].as_str().unwrap().contains("alpha.txt"), "{arguments}");

    let outputs: Vec<String> = payloads(&rows, "function_call_output")
        .iter()
        .filter_map(|p| p["call_id"].as_str().map(str::to_owned))
        .collect();
    assert_eq!(outputs, vec!["call_h27call1x0", "call_h27call1x1"]);
    for call in &calls {
        assert!(outputs.contains(&call["call_id"].as_str().unwrap().to_string()));
    }
}
