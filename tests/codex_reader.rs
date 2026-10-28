//! Conformance tests for the Codex CLI 0.164.0 reader (docs/session-interchange-spec.md Â§9).
//!
//! Requirements exercised: L4 (the `compacted` record becomes one `Compaction` entry, and the
//! assistant record that wrote the summary is not conversation), L5 (the conversation lives in
//! `response_item` records only), L6 (reasoning keeps `encrypted_content`), L9 (a graceful
//! `<turn_aborted>` marker and a hard-killed last turn).

use mukade::model::{
    CanonicalTool, HarnessId, Kind, MetaKind, Protocol, ResultContent, Session, StepEnd, Trigger,
};
use mukade::reader::codex;
use serde_json::Value;
use std::path::PathBuf;

const ROLLOUT: &str = "rollout-2026-10-27T16-43-08-01a17e07-93f7-7a73-85c7-9f0205836600.jsonl";

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/codex/source")
        .join(ROLLOUT)
}

fn read() -> Session {
    codex::read_file(&fixture()).expect("the fixture must read")
}

fn conversation(s: &Session) -> Vec<&mukade::model::Entry> {
    s.conversation().collect()
}

fn kind_name(k: &Kind) -> &'static str {
    match k {
        Kind::User { .. } => "user",
        Kind::Step { .. } => "step",
        Kind::Results { .. } => "results",
        Kind::Compaction { .. } => "compaction",
        Kind::Meta { .. } => "meta",
    }
}

#[test]
fn meta_comes_from_the_session_header() {
    let s = read();
    assert_eq!(s.meta.harness, HarnessId::Codex);
    assert_eq!(s.meta.harness_version, "0.164.0");
    assert_eq!(s.meta.id, "01a17e07-93f7-7a73-85c7-9f0205836600");
    assert_eq!(
        s.meta.cwd.as_deref(),
        Some(r"C:\Users\alfio\AppData\Local\shinatama-verify\H27\codex\work")
    );
    assert_eq!(s.meta.models.len(), 1);
    assert_eq!(s.meta.models[0].provider.as_deref(), Some("h27"));
    assert_eq!(s.meta.models[0].id, "h27-mock");
    assert_eq!(s.meta.created.as_deref(), Some("2026-10-27T15:43:08.286Z"));
    assert_eq!(s.meta.updated.as_deref(), Some("2026-10-27T15:43:27.631Z"));
}

/// Every source record is kept verbatim, exactly once. Codex interleaves events with the model's
/// records, so a step's rows are not adjacent in the file: the order checked is that of each
/// entry's *first* row, and the multiset of rows is compared whole.
#[test]
fn every_source_record_is_kept_exactly_once() {
    let text = std::fs::read_to_string(fixture()).unwrap();
    let rows: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let s = read();

    let mut source_rows: Vec<String> = rows.iter().map(|r| r.to_string()).collect();
    let mut kept: Vec<String> = s
        .entries
        .iter()
        .flat_map(|e| e.raw.iter())
        .map(|r| r.to_string())
        .collect();
    source_rows.sort();
    kept.sort();
    assert_eq!(
        kept, source_rows,
        "rows are kept verbatim, none lost and none duplicated"
    );

    let position = |r: &Value| rows.iter().position(|x| x == r);
    let firsts: Vec<usize> = s
        .entries
        .iter()
        .filter_map(|e| e.raw.first().and_then(position))
        .collect();
    assert!(
        firsts.windows(2).all(|w| w[0] <= w[1]),
        "entries follow the native order: {firsts:?}"
    );
}

#[test]
fn conversation_is_the_expected_entry_sequence() {
    let s = read();
    let kinds: Vec<&str> = conversation(&s)
        .iter()
        .map(|e| kind_name(&e.kind))
        .collect();
    assert_eq!(
        kinds,
        vec![
            "user",       // T1
            "step",       // reasoning + text + two parallel exec_command calls
            "results",    // both function_call_outputs
            "step",       // H27-FINAL
            "user",       // T2
            "step",       // H27-REPLY
            "compaction", // compacted
            "user",       // T3
            "step",       // T3 reply
            "user",       // T4a, later aborted
            "user",       // T4b, killed while hanging
        ]
    );
    assert_eq!(s.entries.len(), 70, "75 rows become 70 entries");
}

#[test]
fn parallel_calls_and_reasoning_stay_in_one_step() {
    let s = read();
    let conv = conversation(&s);
    let Kind::Step {
        model,
        reasoning,
        text,
        calls,
        end,
    } = &conv[1].kind
    else {
        panic!("expected a Step")
    };
    assert_eq!(model.as_ref().map(|m| m.id.as_str()), Some("h27-mock"));
    assert_eq!(text, &vec!["H27-PRE: reading the files.".to_string()]);
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].id, "call_h27call1x0");
    assert_eq!(calls[0].source_name, "exec_command");
    assert_eq!(
        calls[0].canonical,
        Some(CanonicalTool::PowerShell),
        "the session's <environment_context> says <shell>powershell</shell>, and the call names none"
    );
    assert!(
        calls[0].input["cmd"]
            .as_str()
            .unwrap_or_default()
            .contains("alpha.txt"),
        "the arguments string is decoded: {:?}",
        calls[0].input
    );
    assert_eq!(*end, StepEnd::ToolUse);

    let r = &reasoning[0];
    assert_eq!(
        r.text,
        "H27-REASONING: the user wants two files; read them in parallel."
    );
    let o = r.opaque.as_ref().expect("Codex stores encrypted_content");
    assert_eq!(o.protocol, Protocol::OpenaiResponses);
    assert_eq!(o.encrypted.as_deref(), Some("h27-mock-encrypted-reasoning"));
    assert_eq!(o.signature, None);
    assert_eq!(o.model.as_deref(), Some("h27-mock"));
}

#[test]
fn results_are_paired_by_call_id() {
    let s = read();
    let conv = conversation(&s);
    let Kind::Results { results } = &conv[2].kind else {
        panic!("expected Results")
    };
    let ids: Vec<&str> = results.iter().map(|r| r.call_id.as_str()).collect();
    assert_eq!(ids, vec!["call_h27call1x0", "call_h27call1x1"]);
    let Kind::Step { calls, .. } = &conv[1].kind else {
        panic!()
    };
    assert_eq!(calls.len(), results.len());
    for r in results {
        assert!(calls.iter().any(|c| c.id == r.call_id));
        assert!(
            r.content.text().starts_with("Chunk ID:"),
            "the output travels verbatim"
        );
        let ResultContent::Text(_) = r.content else {
            panic!("expected text")
        };
    }
}

/// L4: `compacted` becomes one entry holding the source summary; the assistant record that wrote
/// that summary is the compaction's own turn, so it is folded in rather than carried.
#[test]
fn compaction_keeps_the_summary_and_folds_the_summary_turn() {
    let s = read();
    let conv = conversation(&s);
    let Kind::Compaction {
        summary,
        kept_from,
        trigger,
        tokens_before,
    } = &conv[6].kind
    else {
        panic!("expected the Compaction entry at index 6")
    };
    assert!(
        summary.contains("H27-SUMMARY: the user asked to read alpha.txt"),
        "{summary}"
    );
    assert!(summary.starts_with("Another language model started to solve this problem"));
    assert_eq!(*trigger, Trigger::Unknown, "Codex records no trigger field");
    assert_eq!(*tokens_before, Some(100));
    assert_eq!(
        kept_from.as_deref(),
        Some(conv[7].id.as_str()),
        "the tail starts at T3"
    );

    // The summary the model wrote is not conversation: no step carries that turn's text.
    assert!(
        !s.entries.iter().any(|e| matches!(&e.kind, Kind::Step { text, .. }
            if text.iter().any(|t| t.contains("Another language model started to solve this problem")))),
        "the compaction turn is not a step"
    );
    // ...but it is still in the record, inside the compaction entry.
    let compaction = s
        .entries
        .iter()
        .find(|e| matches!(e.kind, Kind::Compaction { .. }))
        .unwrap();
    assert_eq!(
        compaction.raw.len(),
        2,
        "the summary record and the compacted record"
    );
    assert!(compaction.raw[0].to_string().contains("H27-SUMMARY"));
}

/// L9: the graceful `<turn_aborted>` travels as a marker, never as user text; the hard kill
/// leaves a user turn with no reply.
#[test]
fn aborts_are_markers_and_the_killed_turn_has_no_reply() {
    let s = read();
    let aborted: Vec<&MetaKind> = s
        .entries
        .iter()
        .filter_map(|e| match &e.kind {
            Kind::Meta { kind } if *kind == MetaKind::TurnAborted => Some(kind),
            _ => None,
        })
        .collect();
    assert_eq!(
        aborted.len(),
        2,
        "the <turn_aborted> record and the turn_aborted event"
    );
    assert!(
        !s.entries
            .iter()
            .any(|e| matches!(&e.kind, Kind::User { text, .. } if text.contains("<turn_aborted>"))),
        "the marker must not become user text"
    );

    assert!(s.interrupted_last_turn());
    let conv = conversation(&s);
    let Kind::User { text, .. } = &conv[10].kind else {
        panic!("expected the killed turn last")
    };
    assert_eq!(text, "H27-HANG T4b");
    let Kind::User {
        text: aborted_text, ..
    } = &conv[9].kind
    else {
        panic!()
    };
    assert_eq!(aborted_text, "H27-HANG T4a");
}

#[test]
fn internal_records_are_meta() {
    let s = read();
    let metas: Vec<MetaKind> = s
        .entries
        .iter()
        .filter_map(|e| match &e.kind {
            Kind::Meta { kind } => Some(*kind),
            _ => None,
        })
        .collect();
    let count = |k: MetaKind| metas.iter().filter(|m| **m == k).count();
    assert_eq!(count(MetaKind::Idle), 10, "task_started and task_complete");
    assert_eq!(count(MetaKind::ModelChange), 10, "thread_settings_applied");
    assert_eq!(
        count(MetaKind::SystemPrompt),
        2,
        "developer instructions and <environment_context>"
    );
    assert_eq!(
        count(MetaKind::Internal),
        35,
        "session_meta, token_count, item_completed, turn_context, token_usage_record, world_state"
    );
}

#[test]
fn incomplete_last_turn_still_works_with_no_conversation_at_all() {
    // A rollout with no conversation (only the header) must read, not fail.
    let rows = vec![serde_json::json!({
        "timestamp": "2026-10-27T15:43:08.360Z",
        "ordinal": 0,
        "type": "session_meta",
        "payload": {"session_id": "abc", "id": "abc", "cwd": r"C:\x", "cli_version": "0.164.0"}
    })];
    let s = codex::parse(&rows, None).unwrap();
    assert_eq!(s.meta.id, "abc");
    assert_eq!(s.entries.len(), 1);
    assert!(conversation(&s).is_empty());
    assert!(!s.interrupted_last_turn());
}

/// A prompt that only mentions a marker tag is the user's own words.
#[test]
fn a_prompt_that_mentions_a_marker_tag_is_a_user_turn() {
    let mut rows: Vec<serde_json::Value> = std::fs::read_to_string(fixture())
        .unwrap()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    rows.push(serde_json::json!({
        "timestamp": "2026-10-27T16:50:00.000Z", "type": "response_item",
        "payload": {"type": "message", "role": "user", "content": [
            {"type": "input_text", "text": "What is the <environment_context> block in my rollout? LITERAL-USER"}]},
    }));
    let s = codex::parse(&rows, None).unwrap();
    let last = s.conversation().last().unwrap();
    let Kind::User { text, .. } = &last.kind else {
        panic!("{:?}", last.kind)
    };
    assert!(text.contains("LITERAL-USER"), "{text}");
}

fn rows_with(extra: Vec<serde_json::Value>) -> Session {
    let mut rows: Vec<serde_json::Value> = std::fs::read_to_string(fixture())
        .unwrap()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    for payload in extra {
        rows.push(serde_json::json!({
            "timestamp": "2026-10-27T16:50:00.000Z", "type": "response_item", "payload": payload,
        }));
    }
    codex::parse(&rows, None).unwrap()
}

fn calls_of(s: &Session) -> Vec<mukade::model::ToolCall> {
    s.conversation()
        .filter_map(|e| match &e.kind {
            Kind::Step { calls, .. } => Some(calls.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

/// A `local_shell_call` has no `name` or `arguments`: its command is in `action`.
#[test]
fn a_local_shell_call_keeps_its_command() {
    let s = rows_with(vec![serde_json::json!({
        "type": "local_shell_call", "call_id": "call_ls1", "status": "completed",
        "action": {"type": "exec", "command": ["ls", "-la"]},
    })]);
    let call = calls_of(&s)
        .into_iter()
        .find(|c| c.id == "call_ls1")
        .unwrap();
    assert_eq!(call.source_name, "local_shell");
    assert_eq!(call.input["command"], serde_json::json!(["ls", "-la"]));
}

/// A web search the model ran is part of its step, not an internal record.
#[test]
fn a_web_search_call_is_part_of_the_step() {
    let s = rows_with(vec![serde_json::json!({
        "type": "web_search_call", "id": "ws_1", "status": "completed",
        "action": {"type": "search", "query": "WEBSEARCH-QUERY"},
    })]);
    let call = calls_of(&s).into_iter().find(|c| c.id == "ws_1").unwrap();
    assert_eq!(call.source_name, "web_search");
    assert_eq!(call.input["query"], "WEBSEARCH-QUERY");
}

/// A tool output given as content items stays structured: text as text, an image as an image.
#[test]
fn a_structured_tool_output_stays_structured() {
    let s = rows_with(vec![
        serde_json::json!({"type": "function_call", "call_id": "call_img", "name": "view_image", "arguments": "{}"}),
        serde_json::json!({"type": "function_call_output", "call_id": "call_img", "output": [
            {"type": "input_text", "text": "IMG-CAPTION"},
            {"type": "input_image", "image_url": "data:image/png;base64,iVBORw0KGgo="},
        ]}),
    ]);
    let result = s
        .conversation()
        .filter_map(|e| match &e.kind {
            Kind::Results { results } => Some(results.clone()),
            _ => None,
        })
        .flatten()
        .find(|r| r.call_id == "call_img")
        .unwrap();
    assert_eq!(
        result.content,
        ResultContent::Blocks(vec![
            serde_json::json!({"type": "text", "text": "IMG-CAPTION"}),
            serde_json::json!({"type": "input_image", "image_url": "data:image/png;base64,iVBORw0KGgo="}),
        ])
    );
}
