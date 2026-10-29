//! Claude Code histories whose physical write order is not their conversation order.

use mukade::hop::{
    self, Compaction, ImportOpts, KeepFrom, SourceFacts, SummarySource, WriteDetails,
};
use mukade::model::{Entry, HarnessId, Kind, MetaKind, Protocol, Session, StepEnd};
use mukade::policy::ReasoningPolicy;
use mukade::reader::{claude_code, pi};
use mukade::roots::StoreRoots;
use mukade::strict::check_plan;
use mukade::writer::{WriteOpts, plan};
use serde_json::{Value, json};
use std::path::PathBuf;

fn user(uuid: Option<&str>, parent: Option<&str>, content: Value) -> Value {
    json!({
        "type": "user", "uuid": uuid, "parentUuid": parent, "isSidechain": false,
        "sessionId": "synthetic", "timestamp": "2026-10-01T09:00:00Z",
        "message": {"role": "user", "content": content}
    })
}

fn assistant(
    uuid: Option<&str>,
    parent: Option<&str>,
    message_id: &str,
    blocks: Value,
    stop: Option<&str>,
) -> Value {
    json!({
        "type": "assistant", "uuid": uuid, "parentUuid": parent, "isSidechain": false,
        "sessionId": "synthetic", "timestamp": "2026-10-01T09:00:00Z",
        "message": {"id": message_id, "role": "assistant", "model": "synthetic",
                    "content": blocks, "stop_reason": stop}
    })
}

fn reply(uuid: &str, parent: &str, text: &str) -> Value {
    assistant(
        Some(uuid),
        Some(parent),
        &format!("msg-{uuid}"),
        json!([{"type": "text", "text": text}]),
        Some("end_turn"),
    )
}

fn call(uuid: Option<&str>, parent: &str, message_id: &str, id: &str) -> Value {
    assistant(
        uuid,
        Some(parent),
        message_id,
        json!([{"type": "tool_use", "id": id, "name": "Read", "input": {"file_path": format!("{id}.txt")}}]),
        Some("tool_use"),
    )
}

fn result(uuid: Option<&str>, parent: Option<&str>, id: &str) -> Value {
    user(
        uuid,
        parent,
        json!([{"type": "tool_result", "tool_use_id": id, "content": format!("result {id}")}]),
    )
}

fn boundary(uuid: &str, logical_parent: &str, metadata: Value) -> Value {
    json!({
        "type": "system", "subtype": "compact_boundary", "uuid": uuid, "parentUuid": null,
        "logicalParentUuid": logical_parent, "isSidechain": false, "sessionId": "synthetic",
        "timestamp": "2026-10-01T09:00:00Z", "compactMetadata": metadata
    })
}

fn summary(uuid: &str, parent: &str, text: &str) -> Value {
    let mut row = user(Some(uuid), Some(parent), json!(text));
    row["isCompactSummary"] = json!(true);
    row["isVisibleInTranscriptOnly"] = json!(true);
    row
}

fn entries(session: &Session) -> Vec<&Entry> {
    session.conversation().collect()
}

fn assert_raw_partition(session: &Session, rows: &[Value]) {
    let mut expected: Vec<String> = rows.iter().map(Value::to_string).collect();
    let mut actual: Vec<String> = session
        .entries
        .iter()
        .flat_map(|entry| entry.raw.iter())
        .map(Value::to_string)
        .collect();
    expected.sort();
    actual.sort();
    assert_eq!(
        actual, expected,
        "every source row is retained exactly once"
    );
}

fn assert_pairable(session: &Session) {
    for dest in [
        HarnessId::ClaudeCode,
        HarnessId::Codex,
        HarnessId::OpenCode,
        HarnessId::Pi,
    ] {
        let opts = WriteOpts::default();
        let planned = plan::plan(session, dest, &opts);
        assert_eq!(planned.counts.dropped_orphan_results, 0, "{dest:?}");
        assert_eq!(planned.counts.unanswered_calls, 0, "{dest:?}");
        let protocol = mukade::policy::destination_protocol(dest);
        assert_eq!(
            check_plan(&planned, protocol.as_ref()),
            Vec::<String>::new()
        );
    }
}

#[test]
fn compaction_out_of_file_order_preserves_the_summary_and_both_sides() {
    let rows = vec![
        boundary(
            "boundary",
            "a1",
            json!({"trigger": "manual", "preTokens": 42}),
        ),
        user(Some("u1"), None, json!("before")),
        reply("a1", "u1", "early"),
        summary("summary", "boundary", "the source summary"),
        user(Some("u2"), Some("summary"), json!("after")),
        reply("a2", "u2", "late"),
    ];
    let session = claude_code::parse(&rows, None).unwrap();
    let conversation = entries(&session);
    assert_eq!(conversation.len(), 5);
    assert!(matches!(&conversation[0].kind, Kind::User { text, .. } if text == "before"));
    assert!(matches!(&conversation[1].kind, Kind::Step { text, .. } if text == &["early"]));
    assert!(
        matches!(&conversation[2].kind, Kind::Compaction { summary, tokens_before, .. }
        if summary == "the source summary" && *tokens_before == Some(42))
    );
    assert!(matches!(&conversation[3].kind, Kind::User { text, .. } if text == "after"));
    assert!(matches!(&conversation[4].kind, Kind::Step { text, .. } if text == &["late"]));
    assert_raw_partition(&session, &rows);
}

fn out_of_order_parallel() -> Vec<Value> {
    vec![
        result(Some("rb"), Some("cb"), "b"),
        call(Some("cb"), "ca", "parallel", "b"),
        result(Some("ra"), Some("ca"), "a"),
        user(Some("u1"), None, json!("read both")),
        call(Some("ca"), "u1", "parallel", "a"),
        reply("done", "ra", "finished"),
    ]
}

#[test]
fn recovered_parallel_blocks_and_results_remain_one_paired_response() {
    let rows = out_of_order_parallel();
    let session = claude_code::parse(&rows, None).unwrap();
    let conversation = entries(&session);
    assert_eq!(conversation.len(), 4);
    let Kind::Step { calls, .. } = &conversation[1].kind else {
        panic!("missing step")
    };
    assert_eq!(
        calls
            .iter()
            .map(|call| call.id.as_str())
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
    let Kind::Results { results } = &conversation[2].kind else {
        panic!("missing results")
    };
    assert_eq!(
        results
            .iter()
            .map(|result| result.call_id.as_str())
            .collect::<Vec<_>>(),
        ["b", "a"]
    );
    assert!(matches!(&conversation[3].kind, Kind::Step { text, .. } if text == &["finished"]));
    assert_raw_partition(&session, &rows);
    assert_pairable(&session);
}

#[test]
fn recovered_blocks_keep_their_intermediate_bookkeeping_ancestors() {
    let rows = vec![
        json!({"type": "attachment", "uuid": "environment", "parentUuid": "ca"}),
        result(Some("rb"), Some("cb"), "b"),
        call(Some("cb"), "environment", "parallel", "b"),
        result(Some("ra"), Some("ca"), "a"),
        user(Some("u1"), None, json!("read both")),
        call(Some("ca"), "u1", "parallel", "a"),
        reply("done", "ra", "finished"),
    ];
    let session = claude_code::parse(&rows, None).unwrap();
    let conversation = entries(&session);
    assert!(matches!(&conversation[1].kind, Kind::Step { calls, .. } if calls.len() == 2));
    assert!(matches!(&conversation[2].kind, Kind::Results { results } if results.len() == 2));
    let at = |id: &str| {
        session
            .entries
            .iter()
            .position(|entry| entry.id == id)
            .unwrap()
    };
    assert!(at("environment") > at("ca"));
    assert_raw_partition(&session, &rows);
    assert_pairable(&session);
}

#[test]
fn a_recovered_block_with_a_missing_parent_is_not_guessed() {
    let rows = vec![
        result(Some("rb"), Some("cb"), "b"),
        call(Some("cb"), "missing", "parallel", "b"),
        result(Some("ra"), Some("ca"), "a"),
        user(Some("u1"), None, json!("read both")),
        call(Some("ca"), "u1", "parallel", "a"),
        reply("done", "ra", "finished"),
    ];
    assert!(claude_code::parse(&rows, None).is_err());
}

#[test]
fn a_result_during_streaming_does_not_split_its_assistant_response() {
    let rows = vec![
        user(Some("u1"), None, json!("read both")),
        call(Some("ca"), "u1", "parallel", "a"),
        result(Some("ra"), Some("ca"), "a"),
        call(Some("cb"), "ra", "parallel", "b"),
        result(Some("rb"), Some("cb"), "b"),
        reply("done", "rb", "finished"),
    ];
    let session = claude_code::parse(&rows, None).unwrap();
    let conversation = entries(&session);
    assert_eq!(conversation.len(), 4, "{conversation:?}");
    assert!(matches!(&conversation[1].kind, Kind::Step { calls, .. } if calls.len() == 2));
    assert!(matches!(&conversation[2].kind, Kind::Results { results } if results.len() == 2));
    assert_raw_partition(&session, &rows);
    assert_pairable(&session);
}

#[test]
fn an_assistant_response_cannot_cross_an_expanded_slash_command() {
    let mut expanded = user(Some("expanded"), Some("command"), json!("expanded prompt"));
    expanded["isMeta"] = json!(true);
    let rows = vec![
        user(Some("u1"), None, json!("first")),
        assistant(
            Some("a1"),
            Some("u1"),
            "same-response",
            json!([{"type":"text", "text":"first answer"}]),
            Some("end_turn"),
        ),
        user(
            Some("command"),
            Some("a1"),
            json!("<command-name>/review</command-name>"),
        ),
        expanded,
        assistant(
            Some("a2"),
            Some("expanded"),
            "same-response",
            json!([{"type":"text", "text":"another answer"}]),
            Some("end_turn"),
        ),
    ];
    assert!(claude_code::parse(&rows, None).is_err());
}

#[test]
fn an_unchained_expanded_prompt_is_placed_after_its_slash_command() {
    let mut expanded = user(None, Some("command"), json!("expanded prompt"));
    expanded["isMeta"] = json!(true);
    let rows = vec![
        expanded,
        user(Some("u1"), None, json!("first")),
        reply("a1", "u1", "first answer"),
        user(
            Some("command"),
            Some("a1"),
            json!("<command-name>/review</command-name>"),
        ),
        reply("done", "command", "review complete"),
    ];
    let session = claude_code::parse(&rows, None).unwrap();
    let texts: Vec<&str> = session
        .conversation()
        .filter_map(|entry| match &entry.kind {
            Kind::User { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["first", "expanded prompt"]);
    assert_raw_partition(&session, &rows);
}

#[test]
fn unrelated_meta_cannot_steal_a_recovered_slash_command_prompt() {
    let mut unrelated = user(None, None, json!("unrelated bookkeeping"));
    unrelated["isMeta"] = json!(true);
    let mut expanded = user(None, Some("command"), json!("actual expanded prompt"));
    expanded["isMeta"] = json!(true);
    let rows = vec![
        user(Some("u1"), None, json!("first")),
        reply("a1", "u1", "first answer"),
        user(
            Some("command"),
            Some("a1"),
            json!("<command-name>/review</command-name>"),
        ),
        unrelated,
        expanded,
        reply("done", "command", "review complete"),
    ];
    let session = claude_code::parse(&rows, None).unwrap();
    let texts: Vec<&str> = session
        .conversation()
        .filter_map(|entry| match &entry.kind {
            Kind::User { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["first", "actual expanded prompt"]);
    assert_raw_partition(&session, &rows);
}

#[test]
fn bookkeeping_with_empty_uuids_gets_distinct_entry_ids() {
    let rows = vec![
        user(Some("u1"), None, json!("first")),
        reply("a1", "u1", "answer"),
        json!({"type": "attachment", "uuid": ""}),
        json!({"type": "attachment", "uuid": ""}),
    ];
    let session = claude_code::parse(&rows, None).unwrap();
    let ids: std::collections::HashSet<&str> = session
        .entries
        .iter()
        .map(|entry| entry.id.as_str())
        .collect();
    assert_eq!(ids.len(), session.entries.len());
    assert_raw_partition(&session, &rows);
}

#[test]
fn checkpoint_selects_the_branch_even_when_another_branch_was_written_later() {
    let rows = vec![
        user(Some("u1"), None, json!("first")),
        reply("a1", "u1", "first answer"),
        user(Some("old"), Some("a1"), json!("selected")),
        reply("old-answer", "old", "selected answer"),
        user(Some("new"), Some("a1"), json!("abandoned")),
        reply("new-answer", "new", "abandoned answer"),
        json!({"type": "last-prompt", "leafUuid": "old-answer"}),
    ];
    let session = claude_code::parse(&rows, None).unwrap();
    let texts: Vec<&str> = session
        .conversation()
        .filter_map(|entry| match &entry.kind {
            Kind::User { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["first", "selected"]);
    let off_branch = session
        .entries
        .iter()
        .filter(|entry| {
            matches!(
                entry.kind,
                Kind::Meta {
                    kind: MetaKind::OffBranch
                }
            )
        })
        .count();
    assert_eq!(off_branch, 2);
    assert_raw_partition(&session, &rows);
}

#[test]
fn a_rewind_after_the_checkpoint_selects_the_new_user_branch() {
    let rows = vec![
        user(Some("u1"), None, json!("first")),
        reply("a1", "u1", "first answer"),
        user(Some("old"), Some("a1"), json!("old branch")),
        reply("old-answer", "old", "old answer"),
        json!({"type": "last-prompt", "leafUuid": "old-answer"}),
        user(Some("edited"), Some("a1"), json!("edited branch")),
        reply("edited-answer", "edited", "edited answer"),
    ];
    let session = claude_code::parse(&rows, None).unwrap();
    let texts: Vec<&str> = session
        .conversation()
        .filter_map(|entry| match &entry.kind {
            Kind::User { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["first", "edited branch"]);
    assert_raw_partition(&session, &rows);
}

#[test]
fn a_checkpoint_flushed_before_its_leaf_keeps_the_selected_branch() {
    let rows = vec![
        json!({"type": "last-prompt", "leafUuid": "old-answer"}),
        user(Some("u1"), None, json!("first")),
        reply("a1", "u1", "first answer"),
        user(Some("old"), Some("a1"), json!("selected")),
        reply("old-answer", "old", "selected answer"),
        user(Some("new"), Some("a1"), json!("abandoned")),
        reply("new-answer", "new", "abandoned answer"),
    ];
    let session = claude_code::parse(&rows, None).unwrap();
    let texts: Vec<_> = session
        .conversation()
        .filter_map(|entry| match &entry.kind {
            Kind::User { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["first", "selected"]);
    assert_raw_partition(&session, &rows);
}

#[test]
fn a_mixed_result_and_prompt_can_start_a_rewound_branch() {
    let rows = vec![
        user(Some("u1"), None, json!("first")),
        reply("a1", "u1", "first answer"),
        user(Some("old"), Some("a1"), json!("old branch")),
        reply("old-answer", "old", "old answer"),
        json!({"type": "last-prompt", "leafUuid": "old-answer"}),
        user(
            Some("edited"),
            Some("a1"),
            json!([
                {"type": "tool_result", "tool_use_id": "call", "content": "result"},
                {"type": "text", "text": "edited branch"}
            ]),
        ),
        reply("edited-answer", "edited", "edited answer"),
    ];
    let session = claude_code::parse(&rows, None).unwrap();
    let texts: Vec<_> = session
        .conversation()
        .filter_map(|entry| match &entry.kind {
            Kind::User { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["first", "edited branch"]);
    assert_raw_partition(&session, &rows);
}

#[test]
fn a_tool_source_hint_does_not_recover_an_explicitly_abandoned_result() {
    let mut abandoned = result(Some("other-result"), Some("other"), "a");
    abandoned["sourceToolAssistantUUID"] = json!("ca");
    let rows = vec![
        user(Some("u1"), None, json!("read")),
        call(Some("ca"), "u1", "response", "a"),
        user(Some("other"), Some("u1"), json!("abandoned branch")),
        abandoned,
        reply("done", "ca", "finished"),
    ];
    let session = claude_code::parse(&rows, None).unwrap();
    assert!(
        !session
            .conversation()
            .any(|entry| matches!(entry.kind, Kind::Results { .. }))
    );
    assert!(
        session
            .entries
            .iter()
            .any(|entry| entry.id == "other-result"
                && matches!(
                    entry.kind,
                    Kind::Meta {
                        kind: MetaKind::OffBranch
                    }
                ))
    );
    assert_raw_partition(&session, &rows);
}

#[test]
fn a_late_interruption_marker_does_not_start_a_new_branch() {
    let rows = vec![
        user(Some("u1"), None, json!("first")),
        reply("a1", "u1", "first answer"),
        user(Some("selected"), Some("a1"), json!("selected branch")),
        reply("selected-answer", "selected", "selected answer"),
        json!({"type": "last-prompt", "leafUuid": "selected-answer"}),
        user(
            Some("interrupt"),
            Some("a1"),
            json!("[Request interrupted by user]"),
        ),
    ];
    let session = claude_code::parse(&rows, None).unwrap();
    let texts: Vec<&str> = session
        .conversation()
        .filter_map(|entry| match &entry.kind {
            Kind::User { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["first", "selected branch"]);
    assert_raw_partition(&session, &rows);
}

#[test]
fn a_continuation_after_the_checkpoint_crosses_a_new_compaction() {
    let rows = vec![
        user(Some("u1"), None, json!("before")),
        reply("a1", "u1", "early"),
        json!({"type": "last-prompt", "leafUuid": "a1"}),
        boundary("boundary", "a1", json!({"trigger": "auto"})),
        summary("summary", "boundary", "summary"),
        user(Some("u2"), Some("summary"), json!("after")),
        reply("a2", "u2", "late"),
    ];
    let session = claude_code::parse(&rows, None).unwrap();
    assert!(
        session
            .conversation()
            .any(|entry| matches!(&entry.kind, Kind::User { text, .. } if text == "after"))
    );
    assert!(session.last_compaction().is_some());
    assert_raw_partition(&session, &rows);
}

#[test]
fn an_unchained_result_is_placed_by_its_unique_tool_call_id() {
    let rows = vec![
        result(None, None, "a"),
        user(Some("u1"), None, json!("read")),
        call(Some("ca"), "u1", "response", "a"),
        reply("done", "ca", "finished"),
    ];
    let session = claude_code::parse(&rows, None).unwrap();
    let conversation = entries(&session);
    assert_eq!(conversation.len(), 4);
    assert!(
        matches!(&conversation[1].kind, Kind::Step { calls, .. } if calls.len() == 1 && calls[0].id == "a")
    );
    assert!(
        matches!(&conversation[2].kind, Kind::Results { results } if results.len() == 1 && results[0].call_id == "a")
    );
    assert_raw_partition(&session, &rows);
    assert_pairable(&session);
}

#[test]
fn an_unchained_response_block_is_recovered_by_message_id() {
    let rows = vec![
        call(None, "ca", "response", "b"),
        result(None, None, "b"),
        user(Some("u1"), None, json!("read")),
        call(Some("ca"), "u1", "response", "a"),
        result(Some("ra"), Some("ca"), "a"),
        reply("done", "ra", "finished"),
    ];
    let session = claude_code::parse(&rows, None).unwrap();
    let conversation = entries(&session);
    assert!(matches!(&conversation[1].kind, Kind::Step { calls, .. } if calls.len() == 2));
    assert!(matches!(&conversation[2].kind, Kind::Results { results } if results.len() == 2));
    assert_raw_partition(&session, &rows);
    assert_pairable(&session);
}

#[test]
fn an_unchained_compaction_summary_is_placed_by_its_boundary() {
    let mut unchained_summary = user(None, Some("boundary"), json!("retained summary"));
    unchained_summary["isCompactSummary"] = json!(true);
    let rows = vec![
        user(Some("u1"), None, json!("before")),
        reply("a1", "u1", "early"),
        boundary("boundary", "a1", json!({"trigger": "manual"})),
        user(Some("u2"), Some("boundary"), json!("after")),
        reply("done", "u2", "late"),
        unchained_summary,
        json!({"type": "last-prompt", "leafUuid": "done"}),
    ];
    let session = claude_code::parse(&rows, None).unwrap();
    assert!(
        matches!(&session.last_compaction().unwrap().kind, Kind::Compaction { summary, .. } if summary == "retained summary")
    );
    assert_raw_partition(&session, &rows);
}

#[test]
fn two_possible_results_for_one_unchained_call_are_refused() {
    let rows = vec![
        result(None, None, "a"),
        result(None, None, "a"),
        user(Some("u1"), None, json!("read")),
        call(Some("ca"), "u1", "response", "a"),
        reply("done", "ca", "finished"),
    ];
    assert!(claude_code::parse(&rows, None).is_err());
}

#[test]
fn an_explicitly_abandoned_result_is_not_taken_just_because_its_id_matches() {
    let rows = vec![
        user(Some("u1"), None, json!("read")),
        call(Some("ca"), "u1", "response", "a"),
        user(Some("other"), Some("u1"), json!("abandoned branch")),
        result(Some("other-result"), Some("other"), "a"),
        reply("done", "ca", "finished"),
    ];
    let session = claude_code::parse(&rows, None).unwrap();
    assert!(
        !session
            .conversation()
            .any(|entry| matches!(entry.kind, Kind::Results { .. }))
    );
    assert!(
        session
            .entries
            .iter()
            .any(|entry| entry.id == "other-result"
                && matches!(
                    entry.kind,
                    Kind::Meta {
                        kind: MetaKind::OffBranch
                    }
                ))
    );
    assert_raw_partition(&session, &rows);
}

#[test]
fn an_unchained_result_on_an_abandoned_branch_is_kept_as_metadata() {
    let rows = vec![
        user(Some("u1"), None, json!("read")),
        call(Some("ca"), "u1", "response", "a"),
        user(Some("other"), Some("u1"), json!("abandoned branch")),
        result(None, Some("other"), "a"),
        reply("done", "ca", "finished"),
    ];
    let session = claude_code::parse(&rows, None).unwrap();
    assert!(
        !session
            .conversation()
            .any(|entry| matches!(entry.kind, Kind::Results { .. }))
    );
    assert_eq!(
        session
            .entries
            .iter()
            .filter(|entry| matches!(
                entry.kind,
                Kind::Meta {
                    kind: MetaKind::OffBranch
                }
            ))
            .count(),
        2
    );
    assert_raw_partition(&session, &rows);
}

#[test]
fn a_recovered_result_cannot_cross_a_new_user_prompt() {
    let mut late_result = result(Some("late-result"), Some("u2"), "a");
    late_result["sourceToolAssistantUUID"] = json!("ca");
    let rows = vec![
        user(Some("u1"), None, json!("read")),
        call(Some("ca"), "u1", "response", "a"),
        user(Some("u2"), Some("ca"), json!("new request")),
        reply("done", "u2", "finished"),
        late_result,
        json!({"type": "last-prompt", "leafUuid": "done"}),
    ];
    assert!(claude_code::parse(&rows, None).is_err());
}

#[test]
fn a_reconstructed_chain_cannot_start_with_an_unprompted_assistant() {
    let rows = vec![
        user(Some("u1"), Some("a0"), json!("later prompt")),
        assistant(
            Some("a0"),
            None,
            "first-response",
            json!([{"type": "text", "text": "unprompted"}]),
            Some("end_turn"),
        ),
        reply("a1", "u1", "later answer"),
    ];
    assert!(claude_code::parse(&rows, None).is_err());
}

#[test]
fn an_unanchored_conversation_row_is_refused_instead_of_guessed_or_dropped() {
    let rows = vec![
        user(Some("u2"), Some("a1"), json!("second")),
        user(Some("u1"), None, json!("first")),
        reply("a1", "u1", "answer"),
        user(None, None, json!("cannot be placed")),
        reply("a2", "u2", "answer two"),
    ];
    assert!(claude_code::parse(&rows, None).is_err());
    let without_any_chain = vec![user(None, None, json!("cannot be placed"))];
    assert!(claude_code::parse(&without_any_chain, None).is_err());
}

#[test]
fn multiple_compactions_keep_both_summaries_and_portable_uses_the_latest() {
    let rows = vec![
        boundary("b2", "a2", json!({"trigger": "auto"})),
        user(Some("u1"), None, json!("one")),
        boundary("b1", "a1", json!({"trigger": "manual"})),
        user(Some("u2"), Some("s1"), json!("two")),
        reply("a1", "u1", "answer one"),
        summary("s1", "b1", "first summary"),
        reply("a2", "u2", "answer two"),
        summary("s2", "b2", "latest summary"),
        user(Some("u3"), Some("s2"), json!("three")),
        reply("a3", "u3", "answer three"),
    ];
    let session = claude_code::parse(&rows, None).unwrap();
    let summaries: Vec<&str> = session
        .conversation()
        .filter_map(|entry| match &entry.kind {
            Kind::Compaction { summary, .. } => Some(summary.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(summaries, ["first summary", "latest summary"]);
    let (portable, _) = hop::compact(
        &session,
        &Compaction::Portable {
            summary: SummarySource::SourceRecord,
            keep_from: KeepFrom::SourceBoundary,
        },
        &StoreRoots::default(),
    )
    .unwrap();
    let portable_text = serde_json::to_string(&portable).unwrap();
    assert!(portable_text.contains("latest summary"));
    assert!(!portable_text.contains("first summary"));
    assert!(portable_text.contains("three"));
    assert_raw_partition(&session, &rows);
}

#[test]
fn a_late_preserved_compaction_row_does_not_create_a_false_cycle() {
    let rows = vec![
        json!({"type": "attachment", "uuid": "reminder", "parentUuid": "summary", "isSidechain": false}),
        reply("a1", "u1", "early"),
        user(Some("u1"), None, json!("before")),
        boundary(
            "boundary",
            "reminder",
            json!({"trigger": "auto", "preservedMessages": {"uuids": ["u1", "a1", "reminder"]}}),
        ),
        summary("summary", "boundary", "summary"),
        user(Some("u2"), Some("reminder"), json!("after")),
        reply("a2", "u2", "late"),
    ];
    let session = claude_code::parse(&rows, None).unwrap();
    let conversation = entries(&session);
    assert_eq!(conversation.len(), 5);
    assert!(matches!(&conversation[0].kind, Kind::User { text, .. } if text == "before"));
    assert!(
        matches!(&conversation[2].kind, Kind::Compaction { summary, .. } if summary == "summary")
    );
    assert!(matches!(&conversation[3].kind, Kind::User { text, .. } if text == "after"));
    assert_raw_partition(&session, &rows);
}

#[test]
fn missing_preserved_conversation_and_unknown_compaction_predecessors_are_refused() {
    let missing = vec![
        user(Some("u1"), None, json!("before")),
        reply("a1", "u1", "early"),
        boundary(
            "boundary",
            "a1",
            json!({"preservedMessages": {"uuids": ["missing"]}}),
        ),
        summary("summary", "boundary", "summary"),
        user(Some("u2"), Some("summary"), json!("after")),
        reply("a2", "u2", "late"),
    ];
    assert!(claude_code::parse(&missing, None).is_err());
    let cycle = vec![
        boundary("boundary", "reminder", json!({})),
        summary("summary", "boundary", "summary"),
        json!({"type": "attachment", "uuid": "reminder", "parentUuid": "summary"}),
        user(Some("u2"), Some("reminder"), json!("after")),
        reply("a2", "u2", "late"),
    ];
    assert!(claude_code::parse(&cycle, None).is_err());
}

#[test]
fn a_preserved_uuid_inside_a_merged_response_resolves_to_its_entry() {
    let rows = vec![
        user(Some("u1"), None, json!("before")),
        assistant(
            Some("a1"),
            Some("u1"),
            "response",
            json!([{"type": "text", "text": "one"}]),
            None,
        ),
        assistant(
            Some("a2"),
            Some("a1"),
            "response",
            json!([{"type": "text", "text": "two"}]),
            Some("end_turn"),
        ),
        boundary(
            "boundary",
            "a2",
            json!({"preservedSegment": {"headUuid": "a2", "tailUuid": "a2"}}),
        ),
        summary("summary", "boundary", "summary"),
        user(Some("u2"), Some("summary"), json!("after")),
        reply("done", "u2", "late"),
    ];
    let session = claude_code::parse(&rows, None).unwrap();
    let Kind::Compaction { kept_from, .. } = &session.last_compaction().unwrap().kind else {
        panic!()
    };
    assert_eq!(kept_from.as_deref(), Some("a1"));
    let (portable, _) = hop::compact(
        &session,
        &Compaction::Portable {
            summary: SummarySource::SourceRecord,
            keep_from: KeepFrom::SourceBoundary,
        },
        &StoreRoots::default(),
    )
    .unwrap();
    assert!(
        portable
            .conversation()
            .any(|entry| matches!(&entry.kind, Kind::Step { text, .. } if text == &["one", "two"]))
    );
}

#[test]
fn a_preserved_message_list_alone_sets_the_portable_tail() {
    let rows = vec![
        user(Some("u1"), None, json!("old turn")),
        reply("a1", "u1", "old answer"),
        user(Some("u2"), Some("a1"), json!("retained turn")),
        reply("a2", "u2", "retained answer"),
        boundary(
            "boundary",
            "a2",
            json!({"preservedMessages": {"uuids": ["u2", "a2"]}}),
        ),
        summary("summary", "boundary", "summary"),
        user(Some("u3"), Some("summary"), json!("later turn")),
        reply("a3", "u3", "later answer"),
    ];
    let session = claude_code::parse(&rows, None).unwrap();
    let (portable, _) = hop::compact(
        &session,
        &Compaction::Portable {
            summary: SummarySource::SourceRecord,
            keep_from: KeepFrom::SourceBoundary,
        },
        &StoreRoots::default(),
    )
    .unwrap();
    let texts: Vec<&str> = portable
        .conversation()
        .filter_map(|entry| match &entry.kind {
            Kind::User { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["retained turn", "later turn"]);
}

#[test]
fn a_preserved_list_starting_with_off_chain_bookkeeping_keeps_the_retained_turn() {
    let rows = vec![
        user(Some("u1"), None, json!("old turn")),
        reply("a1", "u1", "old answer"),
        user(Some("u2"), Some("a1"), json!("retained turn")),
        reply("a2", "u2", "retained answer"),
        boundary(
            "boundary",
            "a2",
            json!({"preservedMessages": {"uuids": ["attachment", "u2", "a2"]}}),
        ),
        summary("summary", "boundary", "summary"),
        user(Some("u3"), Some("summary"), json!("later turn")),
        reply("a3", "u3", "later answer"),
        json!({"type": "attachment", "uuid": "attachment", "parentUuid": "u2"}),
        json!({"type": "last-prompt", "leafUuid": "a3"}),
    ];
    let session = claude_code::parse(&rows, None).unwrap();
    let (portable, _) = hop::compact(
        &session,
        &Compaction::Portable {
            summary: SummarySource::SourceRecord,
            keep_from: KeepFrom::SourceBoundary,
        },
        &StoreRoots::default(),
    )
    .unwrap();
    let texts: Vec<&str> = portable
        .conversation()
        .filter_map(|entry| match &entry.kind {
            Kind::User { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["retained turn", "later turn"]);
}

#[test]
fn an_inconsistent_preserved_segment_is_not_used_as_a_portable_boundary() {
    let rows = vec![
        user(Some("u1"), None, json!("before")),
        reply("a1", "u1", "early"),
        boundary(
            "boundary",
            "a1",
            json!({
                "preservedMessages": {"uuids": ["a1"]},
                "preservedSegment": {"headUuid": "missing", "tailUuid": "a1"}
            }),
        ),
        summary("summary", "boundary", "summary"),
        user(Some("u2"), Some("summary"), json!("after")),
        reply("done", "u2", "late"),
    ];
    assert!(claude_code::parse(&rows, None).is_err());
}

fn semantic_kinds(session: &Session) -> Vec<Kind> {
    session
        .conversation()
        .map(|entry| {
            let mut kind = entry.kind.clone();
            if let Kind::Results { results } = &mut kind {
                results.sort_by(|a, b| a.call_id.cmp(&b.call_id));
            }
            kind
        })
        .collect()
}

#[test]
fn shuffled_recorded_fixture_reconstructs_the_same_conversation() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/claude_code/source/c45db7bc-542c-4201-89de-b590c6fdb797.jsonl");
    let text = std::fs::read_to_string(path).unwrap();
    let source: Vec<Value> = text
        .trim_start_matches('\u{feff}')
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let leaf = source
        .iter()
        .rev()
        .find(|row| {
            matches!(row["type"].as_str(), Some("user" | "assistant"))
                && row["isSidechain"] != true
                && row["uuid"].as_str().is_some()
        })
        .unwrap()["uuid"]
        .as_str()
        .unwrap()
        .to_owned();
    let checkpoint = json!({"type": "last-prompt", "leafUuid": leaf});
    let mut reference = source.clone();
    reference.push(checkpoint.clone());
    let expected = semantic_kinds(&claude_code::parse(&reference, None).unwrap());
    for seed in 1..=24u64 {
        let mut shuffled = source.clone();
        let mut state = seed;
        for i in (1..shuffled.len()).rev() {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            shuffled.swap(i, (state as usize) % (i + 1));
        }
        shuffled.push(checkpoint.clone());
        let session = claude_code::parse(&shuffled, None)
            .unwrap_or_else(|error| panic!("seed {seed}: {error:#}"));
        assert_eq!(semantic_kinds(&session), expected, "seed {seed}");
        assert_raw_partition(&session, &shuffled);
    }
}

#[test]
fn a_long_reverse_written_chain_is_processed_without_recursive_traversal() {
    const ROWS: usize = 20_000;
    let mut source = Vec::with_capacity(ROWS + 1);
    for index in 0..ROWS {
        let id = format!("id-{index}");
        let parent = index.checked_sub(1).map(|value| format!("id-{value}"));
        let row = if index % 2 == 0 {
            user(
                Some(&id),
                parent.as_deref(),
                json!(format!("prompt-{index}")),
            )
        } else {
            reply(&id, parent.as_deref().unwrap(), &format!("answer-{index}"))
        };
        source.push(row);
    }
    source.reverse();
    source.push(json!({"type": "last-prompt", "leafUuid": format!("id-{}", ROWS - 1)}));
    let session = claude_code::parse(&source, None).unwrap();
    let conversation = entries(&session);
    assert_eq!(conversation.len(), ROWS);
    assert!(matches!(&conversation[0].kind, Kind::User { text, .. } if text == "prompt-0"));
    assert!(
        matches!(&conversation[ROWS - 1].kind, Kind::Step { text, .. } if text == &["answer-19999"])
    );
    assert_eq!(
        session
            .entries
            .iter()
            .map(|entry| entry.raw.len())
            .sum::<usize>(),
        ROWS + 1
    );
}

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("mukade-claude-order-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn reconstructed_history_hops_to_pi_without_losing_pairs_or_source_fingerprint() {
    let scratch = Scratch::new();
    let source_path = scratch.0.join("source.jsonl");
    let rows = out_of_order_parallel();
    let bytes = rows
        .iter()
        .map(|row| format!("{row}\n"))
        .collect::<String>();
    std::fs::write(&source_path, &bytes).unwrap();
    let source = claude_code::read_file(&source_path).unwrap();
    assert_eq!(
        source.meta.source_sha256.as_deref(),
        Some(mukade::sha256::sha256_hex(bytes.as_bytes()).as_str())
    );
    let opts = ImportOpts {
        cwd: Some(scratch.0.to_string_lossy().into_owned()),
        roots: StoreRoots {
            pi_agent_dir: Some(scratch.0.clone()),
            ..StoreRoots::default()
        },
        reasoning: ReasoningPolicy::DropForeign,
        write: WriteDetails {
            dest_version: Some("1.4.0".into()),
            dest_protocol: Some(Protocol::AnthropicMessages),
            ..WriteDetails::default()
        },
        resume: None,
        source_facts: SourceFacts::of(&source),
        compaction: "none".into(),
        summary: None,
    };
    let receipt = hop::hop_session(HarnessId::Pi, &source, &Compaction::None, &opts).unwrap();
    let written = pi::read_file(std::path::Path::new(&receipt.location)).unwrap();
    let conversation = entries(&written);
    assert!(
        matches!(&conversation[1].kind, Kind::Step { calls, end: StepEnd::ToolUse, .. } if calls.len() == 2)
    );
    assert!(matches!(&conversation[2].kind, Kind::Results { results } if results.len() == 2));
    assert_eq!(receipt.manifest["counts"]["dropped_orphan_results"], 0);
    assert_eq!(std::fs::read_to_string(&source_path).unwrap(), bytes);
}

#[test]
fn reconstructed_parallel_calls_round_trip_through_each_destination_format() {
    let scratch = Scratch::new();
    let source = claude_code::parse(&out_of_order_parallel(), None).unwrap();
    let cwd = scratch.0.to_string_lossy().into_owned();
    let roots = StoreRoots {
        claude_config_dir: Some(scratch.0.join("claude")),
        codex_home: Some(scratch.0.join("codex")),
        pi_agent_dir: Some(scratch.0.join("pi")),
        opencode_db: None,
    };
    for dest in [
        HarnessId::ClaudeCode,
        HarnessId::Codex,
        HarnessId::OpenCode,
        HarnessId::Pi,
    ] {
        let opts = WriteOpts {
            roots: roots.clone(),
            cwd: Some(cwd.clone()),
            dest_version: Some("1.0.0".into()),
            model_provider: Some("tri-llm-gateway".into()),
            model: Some("synthetic".into()),
            project_id: Some("synthetic-project".into()),
            now: Some("2026-10-01T09:00:00Z".into()),
            ..WriteOpts::default()
        };
        let written = if dest == HarnessId::OpenCode {
            let mut clock = mukade::policy::Clock::new(opts.now.as_deref());
            let document = mukade::writer::opencode::document_for(
                &source,
                "ses_synthetic",
                &cwd,
                &opts,
                &mut clock,
            )
            .unwrap();
            mukade::reader::opencode::parse(&document, None).unwrap()
        } else {
            let result = mukade::writer::for_harness(dest)
                .write(&source, &opts)
                .unwrap();
            let path = std::path::Path::new(&result.location);
            match dest {
                HarnessId::ClaudeCode => claude_code::read_file(path).unwrap(),
                HarnessId::Codex => mukade::reader::codex::read_file(path).unwrap(),
                HarnessId::Pi => pi::read_file(path).unwrap(),
                HarnessId::OpenCode => unreachable!(),
            }
        };
        let calls: usize = written
            .conversation()
            .map(|entry| match &entry.kind {
                Kind::Step { calls, .. } => calls.len(),
                _ => 0,
            })
            .sum();
        let results: usize = written
            .conversation()
            .map(|entry| match &entry.kind {
                Kind::Results { results } => results.len(),
                _ => 0,
            })
            .sum();
        if dest == HarnessId::Codex {
            // Codex has no native Read tool; both calls and their results travel as text.
            assert_eq!((calls, results), (0, 0));
            let document = mukade::wire::to_document(&written).to_string();
            for part in ["id a", "id b", "result a", "result b"] {
                assert!(document.contains(part), "{part}: {document}");
            }
            assert_eq!(plan::plan(&source, dest, &opts).counts.tools_text_form, 2);
        } else {
            assert_eq!((calls, results), (2, 2), "{dest:?}");
        }
        assert!(written.conversation().any(|entry| matches!(&entry.kind, Kind::Step { text, .. } if text.iter().any(|part| part.contains("finished")))), "{dest:?}");
    }
}
