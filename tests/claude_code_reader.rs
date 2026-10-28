//! Conformance tests for the Claude Code 2.1.311 reader.
//!
//! The bench is the test (docs/session-interchange-spec.md Â§9): the fixture is the native session
//! A real Claude Code session, kept verbatim as a fixture.
//! Requirements exercised here: L4 (native compaction becomes a `Compaction` entry),
//! L5 (transcript-only rows become `Meta`, rows merged by `message.id`, results paired by
//! `tool_use_id`), L6 (opaque reasoning kept with protocol + model), L9 (a hard-killed last turn
//! is a user entry with no reply).
//!
//! Scenario in the fixture: T1 two parallel `Read` calls with thinking, T2, a native `/compact`,
//! T3 after it, then T4 killed while hanging.

use mukade::model::{
    CanonicalTool, Entry, HarnessId, Kind, MetaKind, Protocol, ResultContent, Session, StepEnd,
    Trigger,
};
use mukade::reader::claude_code;
use std::path::PathBuf;

const SESSION_ID: &str = "c45db7bc-542c-4201-89de-b590c6fdb797";

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/claude_code/source")
        .join(format!("{SESSION_ID}.jsonl"))
}

fn read() -> Session {
    claude_code::read_file(&fixture()).expect("the fixture must read")
}

fn conversation(s: &Session) -> Vec<&Entry> {
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
fn meta_comes_from_the_session_records() {
    let s = read();
    assert_eq!(s.meta.harness, HarnessId::ClaudeCode);
    assert_eq!(s.meta.harness_version, "2.1.311");
    assert_eq!(s.meta.id, SESSION_ID);
    assert_eq!(
        s.meta.cwd.as_deref(),
        Some(r"C:\Users\alfio\AppData\Local\shinatama-verify\H27\claude_code\work")
    );
    assert_eq!(s.meta.git_branch.as_deref(), Some("HEAD"));
    assert_eq!(s.meta.models.len(), 1);
    assert_eq!(s.meta.models[0].id, "claude-opus-5-5");
    assert_eq!(s.meta.created.as_deref(), Some("2026-10-27T15:29:05.240Z"));
    assert_eq!(s.meta.updated.as_deref(), Some("2026-10-27T15:29:09.868Z"));
    assert!(
        s.meta.source_path.is_some(),
        "the reader records where it read from"
    );
}

/// Nothing is lost by reading: every source record stays verbatim in exactly one entry, in file
/// (native) order, with no duplication.
#[test]
fn raw_rows_partition_the_source_file_in_order() {
    use serde_json::Value;
    let text = std::fs::read_to_string(fixture()).unwrap();
    let rows: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let s = read();
    let kept: Vec<&Value> = s.entries.iter().flat_map(|e| e.raw.iter()).collect();
    assert_eq!(kept.len(), rows.len(), "every source row is kept");
    for (a, b) in kept.iter().zip(rows.iter()) {
        assert_eq!(*a, b, "rows are kept verbatim and in native order");
    }
    let mut ids: Vec<&str> = s.entries.iter().map(|e| e.id.as_str()).collect();
    ids.sort_unstable();
    let n = ids.len();
    ids.dedup();
    assert_eq!(
        ids.len(),
        n,
        "entry ids are unique, so the compaction can point at one"
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
            "user",       // T1 prompt
            "step",       // msg_h27_2: thinking + text + two parallel Read calls
            "results",    // both tool results
            "step",       // msg_h27_3: H27-FINAL
            "user",       // T2
            "step",       // msg_h27_5: H27-REPLY
            "compaction", // /compact
            "user",       // T3
            "step",       // msg_h27_9
            "user",       // T4 killed while hanging
        ]
    );
}

/// L5: one API response = one row per block sharing `message.id`; the reader merges them into one
/// `Step`, so the two parallel calls stay together.
#[test]
fn parallel_calls_stay_in_one_step() {
    let s = read();
    let conv = conversation(&s);
    let Kind::Step {
        reasoning,
        text,
        calls,
        end,
        model,
    } = &conv[1].kind
    else {
        panic!("expected a Step, got {}", kind_name(&conv[1].kind))
    };
    assert_eq!(reasoning.len(), 1);
    assert_eq!(text, &vec!["H27-PRE: reading the files.".to_string()]);
    assert_eq!(calls.len(), 2, "both parallel calls are in one Step");
    assert_eq!(calls[0].id, "toolu_h27call2x0");
    assert_eq!(calls[0].source_name, "Read");
    assert_eq!(calls[0].canonical, Some(CanonicalTool::Read));
    assert_eq!(
        calls[0].input["file_path"],
        serde_json::json!(
            r"C:\Users\alfio\AppData\Local\shinatama-verify\H27\claude_code\work\alpha.txt"
        )
    );
    assert_eq!(calls[1].id, "toolu_h27call2x1");
    assert_eq!(calls[1].source_name, "Read");
    assert_eq!(*end, StepEnd::ToolUse);
    assert_eq!(
        model.as_ref().map(|m| m.id.as_str()),
        Some("claude-opus-5-5")
    );
}

/// L6: the signed reasoning part is kept, with the protocol it came from and its model.
#[test]
fn reasoning_keeps_its_opaque_payload() {
    let s = read();
    let conv = conversation(&s);
    let Kind::Step { reasoning, .. } = &conv[1].kind else {
        panic!()
    };
    let r = &reasoning[0];
    assert_eq!(
        r.text,
        "H27-REASONING: the user wants two files; read them in parallel."
    );
    let o = r
        .opaque
        .as_ref()
        .expect("Claude stores the thinking signature");
    assert_eq!(o.protocol, Protocol::AnthropicMessages);
    assert_eq!(o.model.as_deref(), Some("claude-opus-5-5"));
    assert_eq!(o.signature.as_deref(), Some("h27-mock-signature"));
    assert_eq!(o.encrypted, None);
}

/// L5: results are paired by `tool_use_id`, not by the parent chain (the file chains each result
/// under its own `tool_use` row and writes them in completion order: beta first, alpha second).
#[test]
fn results_are_paired_by_tool_use_id_not_by_parent_chain() {
    let s = read();
    let conv = conversation(&s);
    let Kind::Results { results } = &conv[2].kind else {
        panic!("expected Results")
    };
    let ids: Vec<&str> = results.iter().map(|r| r.call_id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["toolu_h27call2x1", "toolu_h27call2x0"],
        "completion order is kept"
    );
    assert!(results.iter().all(|r| !r.is_error));
    assert_eq!(
        results[0].content,
        ResultContent::Text("1\tBETA-CONTENT-17".into())
    );
    assert_eq!(
        results[1].content,
        ResultContent::Text("1\tALPHA-CONTENT-42".into())
    );

    // ...and every result answers a call of the step before it, with none left over.
    let Kind::Step { calls, .. } = &conv[1].kind else {
        panic!()
    };
    for r in results {
        assert!(
            calls.iter().any(|c| c.id == r.call_id),
            "{} has its call",
            r.call_id
        );
    }
    assert_eq!(calls.len(), results.len(), "no call is left unanswered");
}

/// L4: the native compaction becomes one `Compaction` entry carrying the source's own summary.
#[test]
fn native_compaction_becomes_one_compaction_entry() {
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
    assert!(summary.contains("This session is being continued from a previous conversation"));
    assert_eq!(*trigger, Trigger::Manual);
    assert_eq!(*tokens_before, Some(120));
    assert_eq!(
        kept_from.as_deref(),
        Some(conv[5].id.as_str()),
        "Claude's preservedSegment keeps the last reply written before the boundary"
    );
}

/// L5: transcript-only rows are never conversation â€” the `/compact` command block, the
/// re-injected attachments and the 2.1.311 internal records all become `Meta` entries.
#[test]
fn transcript_only_and_internal_rows_are_meta() {
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
    assert_eq!(
        count(MetaKind::Command),
        3,
        "caveat, <command-name> and <local-command-stdout>"
    );
    assert_eq!(count(MetaKind::Attachment), 27);
    assert_eq!(
        count(MetaKind::Internal),
        47,
        "queue-operation 10, api-request* 11, atis-latch 5, last-prompt 5, tools-used 4, cost-state 4, mode 3, api-request-shape 2, api-request 5"
    );
    // No `/compact` row or command output leaked into the conversation.
    for e in conversation(&s) {
        if let Kind::User { text, .. } = &e.kind {
            assert!(
                !text.contains("<command-name>"),
                "command rows are not conversation"
            );
            assert!(
                !text.contains("<local-command-"),
                "command rows are not conversation"
            );
        }
    }
    // 92 source rows become 87 entries: 4 rows of one step, 2 result rows and the
    // boundary+summary pair are each merged.
    assert_eq!(s.entries.len(), 87);
    assert_eq!(conversation(&s).len(), 10);
}

/// L9: T4 was killed while the model was hanging, so the last entry is a user turn with no reply.
#[test]
fn killed_last_turn_is_a_user_entry_with_no_reply() {
    let s = read();
    assert!(s.interrupted_last_turn());
    let last = conversation(&s).last().copied().expect("a last entry");
    let Kind::User { text, .. } = &last.kind else {
        panic!("expected the killed user turn last")
    };
    assert_eq!(text, "H27-HANG");
    assert!(
        !s.entries
            .iter()
            .any(|e| matches!(&e.kind, Kind::Step { text, .. }
            if text.iter().any(|t| t.as_str() == "No response requested."))),
        "the killed process wrote no reply"
    );
}

#[test]
fn completed_steps_report_their_end() {
    let s = read();
    let ends: Vec<StepEnd> = conversation(&s)
        .iter()
        .filter_map(|e| match &e.kind {
            Kind::Step { end, .. } => Some(end.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        ends,
        vec![
            StepEnd::ToolUse,
            StepEnd::Completed,
            StepEnd::Completed,
            StepEnd::Completed
        ]
    );
}

/// L9: on the next load of a killed session Claude Code writes a `<synthetic>` assistant row
/// ("No response requested.", or an API error). It is not the model's reply: it is `Meta`, and the
/// turn before it stays unanswered.
#[test]
fn a_synthetic_reply_is_meta_and_the_turn_stays_unanswered() {
    let text = std::fs::read_to_string(fixture()).unwrap();
    let mut rows: Vec<serde_json::Value> = text
        .trim_start_matches('\u{feff}')
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    rows.push(serde_json::json!({
        "type": "assistant",
        "uuid": "synthetic-1",
        "parentUuid": rows.iter().rev().find_map(|row| row.get("uuid")).unwrap(),
        "isSidechain": false,
        "sessionId": SESSION_ID,
        "timestamp": "2026-10-27T15:40:00.000Z",
        "message": {
            "id": "msg_synthetic",
            "role": "assistant",
            "model": "<synthetic>",
            "content": [{"type": "text", "text": "No response requested."}],
            "stop_reason": "stop_sequence",
        },
    }));
    let s = claude_code::parse(&rows, None).unwrap();
    assert!(
        s.interrupted_last_turn(),
        "the synthetic row is not an answer"
    );
    let last = s.entries.last().unwrap();
    assert!(
        matches!(
            last.kind,
            Kind::Meta {
                kind: MetaKind::Synthetic
            }
        ),
        "{:?}",
        last.kind
    );
}

/// The fixture's rows, decoded, for tests that extend the recorded session.
fn fixture_rows() -> Vec<serde_json::Value> {
    std::fs::read_to_string(fixture())
        .unwrap()
        .trim_start_matches('\u{feff}')
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// A rewind or an edited prompt starts a new branch from an earlier row; the abandoned rows stay in
/// the file. Claude Code resumes the branch of the last row, walking `parentUuid` back from it, so
/// the abandoned rows are not conversation: they are kept as `Meta(OffBranch)`. The parallel call
/// whose `tool_use` and `tool_result` rows hang beside the chain still belongs to it.
#[test]
fn an_abandoned_branch_is_not_conversation() {
    let mut rows = fixture_rows();
    let t1_reply = rows
        .iter()
        .find(|row| {
            row.pointer("/message/content/0/text")
                .and_then(serde_json::Value::as_str)
                == Some("H27-FINAL: both files were read.")
        })
        .and_then(|row| row["uuid"].as_str())
        .expect("the T1 reply")
        .to_string();
    rows.push(serde_json::json!({
        "type": "user", "uuid": "edited-1", "parentUuid": t1_reply, "isSidechain": false,
        "sessionId": SESSION_ID, "timestamp": "2026-10-27T15:40:00.000Z",
        "message": {"role": "user", "content": "H27-EDITED"},
    }));
    rows.push(serde_json::json!({
        "type": "assistant", "uuid": "edited-2", "parentUuid": "edited-1", "isSidechain": false,
        "sessionId": SESSION_ID, "timestamp": "2026-10-27T15:40:01.000Z",
        "message": {"id": "msg_edited", "role": "assistant", "model": "claude-opus-5-5",
            "content": [{"type": "text", "text": "EDITED-REPLY"}], "stop_reason": "end_turn"},
    }));
    let s = claude_code::parse(&rows, None).unwrap();

    let users: Vec<&str> = s
        .conversation()
        .filter_map(|e| match &e.kind {
            Kind::User { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(users.len(), 2, "{users:?}");
    assert!(users[0].starts_with("H27-TOOLS"), "{users:?}");
    assert_eq!(users[1], "H27-EDITED");
    assert!(
        !s.conversation()
            .any(|e| matches!(e.kind, Kind::Compaction { .. }))
    );
    assert!(!s.interrupted_last_turn());

    let calls: usize = s
        .conversation()
        .map(|e| match &e.kind {
            Kind::Step { calls, .. } => calls.len(),
            _ => 0,
        })
        .sum();
    let results: usize = s
        .conversation()
        .map(|e| match &e.kind {
            Kind::Results { results } => results.len(),
            _ => 0,
        })
        .sum();
    assert_eq!(
        (calls, results),
        (2, 2),
        "both parallel calls and both results stay"
    );

    let off = s
        .entries
        .iter()
        .filter(|e| {
            matches!(
                e.kind,
                Kind::Meta {
                    kind: MetaKind::OffBranch
                }
            )
        })
        .count();
    assert!(
        off >= 4,
        "T2, the compaction, T3 and T4 are off the branch: {off}"
    );
}
