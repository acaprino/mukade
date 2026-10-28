//! Claude Code writer conformance (spec L2, §2.1).

mod support;

use mukade::model::HarnessId;
use mukade::writer::for_harness;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

fn write_fixture(tag: &str) -> (mukade::writer::Written, Vec<Value>) {
    let scratch = support::scratch(HarnessId::ClaudeCode, tag);
    let session = support::read_source(HarnessId::ClaudeCode);
    let opts = support::opts(HarnessId::ClaudeCode, &scratch);
    let written = for_harness(HarnessId::ClaudeCode)
        .write(&session, &opts)
        .expect("write");
    let rows = support::read_jsonl(Path::new(&written.location));
    (written, rows)
}

#[test]
fn the_session_lands_where_claude_code_looks_for_it() {
    let (written, _) = write_fixture("writer-claude-path");
    let path = written.location.replace('\\', "/");
    assert!(path.contains("/projects/"), "{path}");
    assert!(path.ends_with(&format!("{}.jsonl", written.id)), "{path}");
    let cwd = support::cwd_for(HarnessId::ClaudeCode);
    let project = mukade::roots::claude_project_dir_name(std::path::Path::new(&cwd));
    assert!(
        path.contains(&project),
        "the project folder is the sanitized working directory {project}: {path}"
    );
    assert_eq!(
        written.resume,
        vec!["claude", "-p", "--resume", written.id.as_str()]
    );
}

/// L2: one API response is one row per content block, all sharing `message.id`.
#[test]
fn each_step_is_one_row_per_block_sharing_a_message_id() {
    let (_, rows) = write_fixture("writer-claude-rows");
    let mut by_message: Vec<(String, Vec<Value>)> = Vec::new();
    for row in rows.iter().filter(|r| r["type"] == "assistant") {
        let id = row["message"]["id"]
            .as_str()
            .expect("every assistant row carries message.id")
            .to_string();
        match by_message.last_mut() {
            Some((last, blocks)) if *last == id => blocks.push(row.clone()),
            _ => by_message.push((id, vec![row.clone()])),
        }
    }
    let first = &by_message[0].1;
    let kinds: Vec<&str> = first
        .iter()
        .map(|r| r["message"]["content"][0]["type"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(
        kinds,
        vec!["thinking", "text", "tool_use", "tool_use"],
        "the two parallel calls stay in the response they came from"
    );
    for row in &rows {
        if row["type"] == "assistant" {
            assert_eq!(
                row["message"]["content"].as_array().map(Vec::len),
                Some(1),
                "one block per row"
            );
        }
    }
    // Every block row of one response shares the whole message envelope, not just its id.
    for (_, blocks) in &by_message {
        let stop: Vec<&Value> = blocks
            .iter()
            .map(|r| &r["message"]["stop_reason"])
            .collect();
        assert!(
            stop.windows(2).all(|w| w[0] == w[1]),
            "one stop_reason per response"
        );
    }
}

/// L2: each tool_result row's parent is its **own** tool_use row. Chaining every row on the
/// previous one is what made Claude Code report `[Tool result missing due to internal error]`.
#[test]
fn every_result_row_hangs_on_its_own_tool_use_row() {
    let (_, rows) = write_fixture("writer-claude-results");
    let mut owner: HashMap<String, String> = HashMap::new();
    for row in &rows {
        if row["type"] != "assistant" {
            continue;
        }
        let block = &row["message"]["content"][0];
        if block["type"] == "tool_use" {
            owner.insert(
                block["id"].as_str().unwrap().to_string(),
                row["uuid"].as_str().unwrap().to_string(),
            );
        }
    }
    assert_eq!(owner.len(), 2, "both parallel calls were written");

    let mut results = Vec::new();
    for row in &rows {
        if row["type"] != "user" || row["message"]["content"][0]["type"] != "tool_result" {
            continue;
        }
        let block = &row["message"]["content"][0];
        let call_id = block["tool_use_id"].as_str().unwrap();
        assert_eq!(
            row["parentUuid"].as_str(),
            owner.get(call_id).map(String::as_str),
            "result {call_id} must hang on its own tool_use row"
        );
        results.push(call_id.to_string());
    }
    assert_eq!(
        results,
        vec!["toolu_h27call2x1", "toolu_h27call2x0"],
        "both results, in completion order"
    );
}

#[test]
fn user_turns_compaction_and_the_killed_turn_survive() {
    let (_, rows) = write_fixture("writer-claude-turns");
    let users: Vec<String> = rows
        .iter()
        .filter(|r| r["type"] == "user" && r["message"]["content"].is_string())
        .map(|r| r["message"]["content"].as_str().unwrap().to_string())
        .collect();
    assert!(users[0].starts_with("H27-TOOLS read"), "{}", users[0]);
    assert!(
        users.iter().any(|u| u.contains("H27-SUMMARY")),
        "the source's summary travelled"
    );
    assert_eq!(
        users.last().map(String::as_str),
        Some("H27-HANG"),
        "the interrupted turn is the last row"
    );
    assert!(
        rows.iter().all(|r| r["version"] == "2.1.311"),
        "the destination's own version"
    );
}
