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
    // Every conversation row; the closing `last-prompt` row has no version, as a native one has
    // none.
    assert!(
        rows.iter()
            .filter(|r| r["type"] == "user" || r["type"] == "assistant")
            .all(|r| r["version"] == "2.1.311"),
        "the destination's own version"
    );
}

/// The folder name Claude Code 2.1.311 itself derives (`wL` in its binary, checked with that
/// JavaScript): every UTF-16 unit outside `[a-zA-Z0-9]` becomes `-`, and a name past 200 units is
/// cut there and suffixed with a hash of the path.
#[test]
fn the_project_folder_name_is_the_one_claude_code_derives() {
    use mukade::roots::claude_project_dir_name;
    use std::path::Path;
    assert_eq!(claude_project_dir_name(Path::new(r"C:\a\b")), "C--a-b");
    assert_eq!(
        claude_project_dir_name(Path::new("C:\\w\\\u{1F600}x")),
        "C--w---x"
    );
    let long = format!(r"C:\work\{}proj", r"deep\".repeat(45));
    let expected = format!("C--work-{}de-oxvvd", "deep-".repeat(38));
    assert_eq!(claude_project_dir_name(Path::new(&long)), expected);
}

/// Claude Code's /resume picker titles a session by the `lastPrompt` of its last `last-prompt`
/// row, else by its first user prompt. A native session ends with one, naming the last user
/// prompt and the last row; a written one does too, so it is listed under the prompt the
/// conversation ended on.
#[test]
fn the_session_ends_with_the_last_prompt_row_the_resume_picker_reads() {
    let (written, rows) = write_fixture("writer-claude-last-prompt");
    let last = rows.last().expect("rows");
    assert_eq!(last["type"], "last-prompt", "{last}");
    assert_eq!(last["sessionId"], written.id.as_str());
    let conversation: Vec<&Value> = rows
        .iter()
        .filter(|r| r["type"] == "user" || r["type"] == "assistant")
        .collect();
    assert_eq!(
        last["leafUuid"],
        conversation.last().unwrap()["uuid"],
        "the leaf is the last conversation row"
    );
    let prompt_of = |r: &Value| -> Option<String> {
        if r["type"] != "user" {
            return None;
        }
        match &r["message"]["content"] {
            Value::String(s) => Some(s.clone()),
            Value::Array(blocks) => blocks
                .iter()
                .find(|b| b["type"] == "text")
                .and_then(|b| b["text"].as_str().map(str::to_owned)),
            _ => None,
        }
    };
    let last_user_prompt = conversation
        .iter()
        .rev()
        .filter(|r| {
            !r["message"]["content"]
                .as_array()
                .is_some_and(|b| b.iter().any(|x| x["type"] == "tool_result"))
        })
        .find_map(|r| prompt_of(r))
        .expect("the fixture has a user prompt");
    let shown = last["lastPrompt"].as_str().expect("lastPrompt");
    assert!(
        last_user_prompt.starts_with(shown.trim_end_matches('…')),
        "{shown:?} is the start of {last_user_prompt:?}"
    );
    assert!(shown.chars().count() <= 201, "{shown}");
}

/// A session with a name is written with Claude Code's own `custom-title` row, so /resume lists
/// it under that name; a title the caller states wins over the source's; with neither there is
/// no title row.
#[test]
fn a_named_session_carries_its_name_as_a_custom_title() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "writer-claude-title");
    let mut opts = support::opts(HarnessId::ClaudeCode, &scratch);
    let write = |session: &mukade::model::Session, opts: &mukade::writer::WriteOpts| {
        let written = for_harness(HarnessId::ClaudeCode)
            .write(session, opts)
            .expect("write");
        let rows = support::read_jsonl(Path::new(&written.location));
        (written, rows)
    };
    let titles_of = |rows: &[Value]| -> Vec<Value> {
        rows.iter()
            .filter(|r| r["type"] == "custom-title")
            .cloned()
            .collect()
    };
    let mut session = support::read_source(HarnessId::ClaudeCode);
    session.meta.title = Some("Misura i task code access".into());

    opts.title = None;
    let (written, rows) = write(&session, &opts);
    let titles = titles_of(&rows);
    assert_eq!(titles.len(), 1, "{titles:?}");
    assert_eq!(titles[0]["customTitle"], "Misura i task code access");
    assert_eq!(titles[0]["sessionId"], written.id.as_str());
    assert_eq!(rows.last().unwrap()["type"], "last-prompt");

    opts.title = Some("Stated".into());
    let (_, rows) = write(&session, &opts);
    assert_eq!(titles_of(&rows)[0]["customTitle"], "Stated");

    opts.title = None;
    session.meta.title = None;
    let (_, rows) = write(&session, &opts);
    assert!(titles_of(&rows).is_empty(), "no name, no title row");
}
