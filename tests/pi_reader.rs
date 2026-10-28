//! Conformance tests for the Pi 1.2.0 reader (docs/session-interchange-spec.md Â§9).
//!
//! Requirements exercised: L4 (`compaction` + `firstKeptEntryId`), L5 (records are chained by
//! `parentId`; the stored system message is not conversation), L6 (`thinkingSignature`), L9 (a
//! killed turn leaves only the user entry).

use mukade::model::{
    CanonicalTool, HarnessId, Kind, MetaKind, Protocol, ResultContent, Session, StepEnd, Trigger,
};
use mukade::reader::pi;
use std::path::PathBuf;

const SESSION: &str = "2026-10-27T16-00-39-689Z_01a17e17-9f07-743d-8072-8b3b13d65ab5.jsonl";

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/pi/source")
        .join(SESSION)
}

fn read() -> Session {
    pi::read_file(&fixture()).expect("the fixture must read")
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
    assert_eq!(s.meta.harness, HarnessId::Pi);
    assert_eq!(s.meta.id, "01a17e17-9f07-743d-8072-8b3b13d65ab5");
    assert_eq!(
        s.meta.cwd.as_deref(),
        Some(r"C:\Users\alfio\AppData\Local\shinatama-verify\H27\pi\work")
    );
    assert_eq!(s.meta.created.as_deref(), Some("2026-10-27T16:00:39.689Z"));
    assert_eq!(s.meta.updated.as_deref(), Some("2026-10-27T16:00:43.040Z"));
    assert_eq!(s.meta.models.len(), 1);
    assert_eq!(s.meta.models[0].provider.as_deref(), Some("h27"));
    assert_eq!(s.meta.models[0].id, "h27-mock");
    assert_eq!(s.meta.parent, None, "the source session is not a fork");
}

#[test]
fn every_source_record_is_kept_exactly_once_in_native_order() {
    use serde_json::Value;
    let text = std::fs::read_to_string(fixture()).unwrap();
    let rows: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let s = read();
    let kept: Vec<&Value> = s.entries.iter().flat_map(|e| e.raw.iter()).collect();
    assert_eq!(kept.len(), rows.len());
    for (a, b) in kept.iter().zip(rows.iter()) {
        assert_eq!(*a, b, "rows are kept verbatim and in native order");
    }
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
            "user",    // T1
            "step",    // thinking + text + two parallel toolCalls in one message
            "results", // one toolResult record per call
            "step",    // H27-FINAL
            "user",    // T2
            "step",    // H27-REPLY
            "compaction",
            "user", // T3
            "step", // T3 reply
            "user", // T4, killed: no reply was written
        ]
    );
    assert_eq!(s.entries.len(), 14, "15 rows become 14 entries");
}

#[test]
fn both_parallel_calls_are_in_one_assistant_message() {
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
    assert_eq!(
        calls.len(),
        2,
        "Pi writes both parallel calls in one message"
    );
    assert_eq!(calls[0].id, "toolu_h27call1x0");
    assert_eq!(calls[0].source_name, "read");
    assert_eq!(calls[0].canonical, Some(CanonicalTool::Read));
    assert!(
        calls[0].input["path"]
            .as_str()
            .unwrap_or_default()
            .ends_with("alpha.txt")
    );
    assert_eq!(calls[1].id, "toolu_h27call1x1");
    assert_eq!(*end, StepEnd::ToolUse);

    let o = reasoning[0]
        .opaque
        .as_ref()
        .expect("Pi stores thinkingSignature");
    assert_eq!(o.protocol, Protocol::AnthropicMessages);
    assert_eq!(o.signature.as_deref(), Some("h27-mock-signature"));
    assert_eq!(o.encrypted, None);
}

#[test]
fn results_are_one_record_each_pairing_with_the_calls() {
    let s = read();
    let conv = conversation(&s);
    let Kind::Results { results } = &conv[2].kind else {
        panic!("expected Results")
    };
    let ids: Vec<&str> = results.iter().map(|r| r.call_id.as_str()).collect();
    assert_eq!(ids, vec!["toolu_h27call1x0", "toolu_h27call1x1"]);
    assert_eq!(
        results[0].content,
        ResultContent::Text("ALPHA-CONTENT-42".into())
    );
    assert_eq!(
        results[1].content,
        ResultContent::Text("BETA-CONTENT-17".into())
    );
    assert!(results.iter().all(|r| !r.is_error));
    let Kind::Step { calls, .. } = &conv[1].kind else {
        panic!()
    };
    assert_eq!(calls.len(), results.len());
}

#[test]
fn compaction_keeps_the_summary_and_first_kept_entry() {
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
    assert!(
        summary.contains("<read-files>"),
        "Pi appends its own read-files block: {summary}"
    );
    assert_eq!(*tokens_before, Some(120));
    assert_eq!(
        *trigger,
        Trigger::Unknown,
        "Pi records no reason for a compaction"
    );
    assert_eq!(
        kept_from.as_deref(),
        Some(conv[4].id.as_str()),
        "firstKeptEntryId names the T2 user entry"
    );
}

/// L9: the killed T4 turn wrote only the user entry; nothing synthetic was added on disk.
#[test]
fn killed_last_turn_is_a_user_entry_with_no_reply() {
    let s = read();
    assert!(s.interrupted_last_turn());
    let conv = conversation(&s);
    let Kind::User { text, .. } = &conv[9].kind else {
        panic!("expected the killed user turn last")
    };
    assert_eq!(text, "H27-HANG interrupted turn");
    assert!(
        !s.entries
            .iter()
            .any(|e| matches!(&e.kind, Kind::Step { text, .. }
            if text.iter().any(|t| t.contains("No response requested")))),
        "nothing synthesized a reply"
    );
}

#[test]
fn the_stored_system_message_and_session_records_are_meta() {
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
        count(MetaKind::SystemPrompt),
        1,
        "the stored system message is replayed, not carried as conversation"
    );
    assert_eq!(
        count(MetaKind::ModelChange),
        2,
        "model_change and thinking_level_change"
    );
    assert_eq!(count(MetaKind::Internal), 1, "the session header");
    assert_eq!(s.entries.len(), 14);
}

/// Navigating back in Pi's tree starts a new branch from an earlier entry and may leave a
/// `branch_summary` there; the abandoned entries stay in the file. Pi resumes the path from the
/// last entry back through `parentId`, and shows the branch summary to the model as a user
/// message. The abandoned entries are kept as `Meta(OffBranch)`.
#[test]
fn an_abandoned_branch_is_not_conversation_and_its_summary_is() {
    let mut rows: Vec<serde_json::Value> = std::fs::read_to_string(fixture())
        .unwrap()
        .trim_start_matches('\u{feff}')
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let by_role = |role: &str| {
        rows.iter()
            .rev()
            .find(|row| {
                row.pointer("/message/role")
                    .and_then(serde_json::Value::as_str)
                    == Some(role)
            })
            .cloned()
            .unwrap()
    };
    let (mut user, mut reply) = (by_role("user"), by_role("assistant"));
    // The end of T1: the assistant entry the tool results led to.
    let t1_end = rows[8]["id"].as_str().unwrap().to_string();
    rows.push(serde_json::json!({
        "type": "branch_summary", "id": "bs000001", "parentId": t1_end,
        "timestamp": "2026-10-27T16:30:00.000Z", "fromId": "0069c063",
        "summary": "ABANDONED-BRANCH-SUMMARY",
    }));
    user["id"] = "ed000001".into();
    user["parentId"] = "bs000001".into();
    user["message"]["content"] = serde_json::json!([{"type": "text", "text": "PI-EDITED"}]);
    reply["id"] = "ed000002".into();
    reply["parentId"] = "ed000001".into();
    reply["message"]["content"] = serde_json::json!([{"type": "text", "text": "PI-EDITED-REPLY"}]);
    rows.push(user);
    rows.push(reply);
    let s = pi::parse(&rows, None).unwrap();

    let users: Vec<&str> = s
        .conversation()
        .filter_map(|e| match &e.kind {
            Kind::User { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(users.len(), 3, "{users:?}");
    assert!(users[1].contains("ABANDONED-BRANCH-SUMMARY"), "{users:?}");
    assert!(
        users[1].starts_with(
            "The following is a summary of a branch that this conversation came back from:"
        ),
        "{users:?}"
    );
    assert_eq!(users[2], "PI-EDITED");
    assert!(
        !s.conversation()
            .any(|e| matches!(e.kind, Kind::Compaction { .. }))
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
    assert_eq!(
        off, 6,
        "T2, the compaction, T3 and the killed T4 are off the branch"
    );
}

fn rows_with(extra: Vec<serde_json::Value>) -> Session {
    let mut rows: Vec<serde_json::Value> = std::fs::read_to_string(fixture())
        .unwrap()
        .trim_start_matches('\u{feff}')
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let mut parent = rows.last().unwrap()["id"].clone();
    for (n, mut entry) in extra.into_iter().enumerate() {
        let id = format!("ex{n:06}");
        entry["id"] = id.clone().into();
        entry["parentId"] = parent;
        entry["timestamp"] = "2026-10-27T16:40:00.000Z".into();
        parent = id.into();
        rows.push(entry);
    }
    pi::parse(&rows, None).unwrap()
}

/// `!cat file` in Pi stores a `bashExecution` message that Pi gives the model as a user message
/// (`bashExecutionToText`); `!!` marks one Pi keeps out of the context.
#[test]
fn a_bash_execution_reaches_the_conversation_as_pi_shows_it() {
    let s = rows_with(vec![
        serde_json::json!({"type": "message", "message": {"role": "bashExecution", "command": "cat a.txt",
            "output": "BASHEXEC-OUTPUT", "exitCode": 2, "cancelled": false, "truncated": false, "timestamp": 1}}),
        serde_json::json!({"type": "message", "message": {"role": "bashExecution", "command": "secret",
            "output": "HIDDEN", "exitCode": 0, "excludeFromContext": true, "timestamp": 2}}),
    ]);
    let texts: Vec<&str> = s
        .conversation()
        .filter_map(|e| match &e.kind {
            Kind::User { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        texts.contains(&"Ran `cat a.txt`\n```\nBASHEXEC-OUTPUT\n```\n\nCommand exited with code 2"),
        "{texts:?}"
    );
    assert!(!texts.iter().any(|t| t.contains("HIDDEN")), "{texts:?}");
}

/// An extension's `custom_message` is in the model's context as a user message.
#[test]
fn a_custom_message_reaches_the_conversation() {
    let s = rows_with(vec![serde_json::json!({
        "type": "custom_message", "customType": "reminder", "content": "CUSTOM-CONTENT", "display": true,
    })]);
    let Kind::User { text, .. } = &s.conversation().last().unwrap().kind else {
        panic!("{:?}", s.conversation().last().unwrap().kind)
    };
    assert_eq!(text, "CUSTOM-CONTENT");
}

/// Pi's bash timeout is a number of seconds, fractions included; the canonical unit is ms.
#[test]
fn a_fractional_bash_timeout_becomes_milliseconds() {
    let s = rows_with(vec![serde_json::json!({"type": "message", "message": {
        "role": "assistant", "content": [{"type": "toolCall", "id": "call_t", "name": "bash",
            "arguments": {"command": "sleep 1", "timeout": 1.5}}],
        "api": "anthropic-messages", "provider": "h27", "model": "m", "stopReason": "toolUse", "timestamp": 3}})]);
    let call = s
        .conversation()
        .filter_map(|e| match &e.kind {
            Kind::Step { calls, .. } => Some(calls.clone()),
            _ => None,
        })
        .flatten()
        .find(|c| c.id == "call_t")
        .unwrap();
    assert_eq!(call.input["timeout"], 1500);
}

/// Native order is the file's own order (and its tree), never the timestamps.
#[test]
fn the_file_order_wins_over_timestamps() {
    let mut rows: Vec<serde_json::Value> = std::fs::read_to_string(fixture())
        .unwrap()
        .trim_start_matches('\u{feff}')
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let texts = |s: &Session| -> Vec<String> {
        s.conversation()
            .filter_map(|e| match &e.kind {
                Kind::User { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    };
    let before = texts(&pi::parse(&rows, None).unwrap());
    let n = rows.len();
    for (i, row) in rows.iter_mut().enumerate() {
        row["timestamp"] = format!("2026-10-27T15:{:02}:00.000Z", 59 - (i * 59 / n)).into();
        if let Some(message) = row
            .get_mut("message")
            .and_then(serde_json::Value::as_object_mut)
        {
            message.insert("timestamp".into(), serde_json::Value::from(n - i));
        }
    }
    assert_eq!(texts(&pi::parse(&rows, None).unwrap()), before);
}

fn pi_assistant(text: &str) -> serde_json::Value {
    serde_json::json!({"type": "message", "message": {
        "role": "assistant", "content": [{"type": "text", "text": text}],
        "api": "anthropic-messages", "provider": "h27", "model": "m", "stopReason": "stop", "timestamp": 3}})
}

fn pi_user(text: &str) -> serde_json::Value {
    serde_json::json!({"type": "message", "message": {"role": "user", "content": text, "timestamp": 2}})
}

fn context_edit(target: &str, replacement: serde_json::Value) -> serde_json::Value {
    serde_json::json!({"type": "context_edit", "targetId": target, "replacement": replacement})
}

fn step_texts(s: &Session) -> Vec<String> {
    s.conversation()
        .filter_map(|e| match &e.kind {
            Kind::Step { text, .. } => Some(text.join("\n")),
            _ => None,
        })
        .collect()
}

/// A `context_edit` changes what the model is shown, as Pi's own context builder does: the target
/// keeps its role and takes the replacement's content, or is left out when the replacement is
/// null. The source record stays verbatim in `raw`.
#[test]
fn a_context_edit_changes_what_the_model_sees_and_the_record_stays() {
    // ex000000 user, ex000001 assistant, ex000002 the edit, ex000003 user
    let s = rows_with(vec![
        pi_user("EDIT-Q"),
        pi_assistant("ORIGINAL-ANSWER"),
        context_edit("ex000001", serde_json::json!({"content": "EDITED-ANSWER"})),
        pi_user("AFTER-Q"),
    ]);
    let texts = step_texts(&s);
    assert!(texts.iter().any(|t| t == "EDITED-ANSWER"), "{texts:?}");
    assert!(
        !texts.iter().any(|t| t.contains("ORIGINAL-ANSWER")),
        "{texts:?}"
    );
    let kept = s
        .entries
        .iter()
        .flat_map(|e| e.raw.iter())
        .any(|row| row.to_string().contains("ORIGINAL-ANSWER"));
    assert!(kept, "the source record is kept verbatim");
}

#[test]
fn a_context_edit_with_a_null_replacement_leaves_the_message_out() {
    let s = rows_with(vec![
        pi_user("EDIT-Q"),
        pi_assistant("REMOVED-ANSWER"),
        context_edit("ex000001", serde_json::Value::Null),
        pi_user("AFTER-Q"),
    ]);
    assert!(
        !step_texts(&s).iter().any(|t| t.contains("REMOVED-ANSWER")),
        "{:?}",
        step_texts(&s)
    );
    assert!(
        s.entries
            .iter()
            .flat_map(|e| e.raw.iter())
            .any(|row| row.to_string().contains("REMOVED-ANSWER"))
    );
}

/// The latest edit of a target wins, and a user message takes its replacement's content too.
#[test]
fn the_latest_context_edit_wins_and_user_messages_can_be_edited() {
    let s = rows_with(vec![
        pi_user("ORIGINAL-QUESTION"),
        context_edit("ex000000", serde_json::json!({"content": "FIRST-EDIT"})),
        context_edit("ex000000", serde_json::json!({"content": "SECOND-EDIT"})),
    ]);
    let users: Vec<&str> = s
        .conversation()
        .filter_map(|e| match &e.kind {
            Kind::User { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(users.contains(&"SECOND-EDIT"), "{users:?}");
    assert!(
        !users
            .iter()
            .any(|t| t.contains("FIRST-EDIT") || t.contains("ORIGINAL-QUESTION")),
        "{users:?}"
    );
}

/// Pi keeps Anthropic's `redacted_thinking` as a thinking block with a placeholder text, the
/// encrypted data in `thinkingSignature` and `redacted: true` (seen live through OpenRouter on
/// 2026-10-28; Pi's own anthropic-messages code sends it back as `redacted_thinking {data}`). It is
/// an encrypted payload with no visible text, not a signature over "[Reasoning redacted]".
#[test]
fn a_redacted_thinking_block_is_an_encrypted_payload_without_text() {
    let s = rows_with(vec![serde_json::json!({
        "type": "message",
        "message": {"role": "assistant", "api": "anthropic-messages", "provider": "openrouter",
                    "model": "anthropic/claude-haiku-5.5", "stopReason": "stop",
                    "content": [
                        {"type": "thinking", "thinking": "[Reasoning redacted]",
                         "thinkingSignature": "CAQSxAUKEAgTGAI4-REDACTED-DATA", "redacted": true},
                        {"type": "thinking", "thinking": "visible", "thinkingSignature": "SIG"},
                        {"type": "text", "text": "391"}]}
    })]);
    let reasoning = s
        .conversation()
        .filter_map(|e| match &e.kind {
            Kind::Step { reasoning, .. } => Some(reasoning.clone()),
            _ => None,
        })
        .last()
        .unwrap();
    assert_eq!(reasoning.len(), 2);
    let redacted = &reasoning[0];
    assert_eq!(redacted.text, "");
    let opaque = redacted.opaque.as_ref().unwrap();
    assert_eq!(
        opaque.encrypted.as_deref(),
        Some("CAQSxAUKEAgTGAI4-REDACTED-DATA")
    );
    assert_eq!(opaque.signature, None);
    assert_eq!(opaque.protocol, mukade::model::Protocol::AnthropicMessages);
    let signed = reasoning[1].opaque.as_ref().unwrap();
    assert_eq!(signed.signature.as_deref(), Some("SIG"));
    assert_eq!(signed.encrypted, None);
}
