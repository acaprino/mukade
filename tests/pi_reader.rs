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
    let rows: Vec<Value> = text.lines().filter(|l| !l.trim().is_empty()).map(|l| serde_json::from_str(l).unwrap()).collect();
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
    let kinds: Vec<&str> = conversation(&s).iter().map(|e| kind_name(&e.kind)).collect();
    assert_eq!(
        kinds,
        vec![
            "user",       // T1
            "step",       // thinking + text + two parallel toolCalls in one message
            "results",    // one toolResult record per call
            "step",       // H27-FINAL
            "user",       // T2
            "step",       // H27-REPLY
            "compaction",
            "user",       // T3
            "step",       // T3 reply
            "user",       // T4, killed: no reply was written
        ]
    );
    assert_eq!(s.entries.len(), 14, "15 rows become 14 entries");
}

#[test]
fn both_parallel_calls_are_in_one_assistant_message() {
    let s = read();
    let conv = conversation(&s);
    let Kind::Step { model, reasoning, text, calls, end } = &conv[1].kind else { panic!("expected a Step") };
    assert_eq!(model.as_ref().map(|m| m.id.as_str()), Some("h27-mock"));
    assert_eq!(text, &vec!["H27-PRE: reading the files.".to_string()]);
    assert_eq!(calls.len(), 2, "Pi writes both parallel calls in one message");
    assert_eq!(calls[0].id, "toolu_h27call1x0");
    assert_eq!(calls[0].source_name, "read");
    assert_eq!(calls[0].canonical, Some(CanonicalTool::Read));
    assert!(calls[0].input["path"].as_str().unwrap_or_default().ends_with("alpha.txt"));
    assert_eq!(calls[1].id, "toolu_h27call1x1");
    assert_eq!(*end, StepEnd::ToolUse);

    let o = reasoning[0].opaque.as_ref().expect("Pi stores thinkingSignature");
    assert_eq!(o.protocol, Protocol::AnthropicMessages);
    assert_eq!(o.signature.as_deref(), Some("h27-mock-signature"));
    assert_eq!(o.encrypted, None);
}

#[test]
fn results_are_one_record_each_pairing_with_the_calls() {
    let s = read();
    let conv = conversation(&s);
    let Kind::Results { results } = &conv[2].kind else { panic!("expected Results") };
    let ids: Vec<&str> = results.iter().map(|r| r.call_id.as_str()).collect();
    assert_eq!(ids, vec!["toolu_h27call1x0", "toolu_h27call1x1"]);
    assert_eq!(results[0].content, ResultContent::Text("ALPHA-CONTENT-42".into()));
    assert_eq!(results[1].content, ResultContent::Text("BETA-CONTENT-17".into()));
    assert!(results.iter().all(|r| !r.is_error));
    let Kind::Step { calls, .. } = &conv[1].kind else { panic!() };
    assert_eq!(calls.len(), results.len());
}

#[test]
fn compaction_keeps_the_summary_and_first_kept_entry() {
    let s = read();
    let conv = conversation(&s);
    let Kind::Compaction { summary, kept_from, trigger, tokens_before } = &conv[6].kind else {
        panic!("expected the Compaction entry at index 6")
    };
    assert!(summary.contains("H27-SUMMARY: the user asked to read alpha.txt"), "{summary}");
    assert!(summary.contains("<read-files>"), "Pi appends its own read-files block: {summary}");
    assert_eq!(*tokens_before, Some(120));
    assert_eq!(*trigger, Trigger::Unknown, "Pi records no reason for a compaction");
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
    let Kind::User { text, .. } = &conv[9].kind else { panic!("expected the killed user turn last") };
    assert_eq!(text, "H27-HANG interrupted turn");
    assert!(
        !s.entries.iter().any(|e| matches!(&e.kind, Kind::Step { text, .. }
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
    assert_eq!(count(MetaKind::SystemPrompt), 1, "the stored system message is replayed, not carried as conversation");
    assert_eq!(count(MetaKind::ModelChange), 2, "model_change and thinking_level_change");
    assert_eq!(count(MetaKind::Internal), 1, "the session header");
    assert_eq!(s.entries.len(), 14);
}

