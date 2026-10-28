//! Conformance tests for the OpenCode 2.1.4 reader (docs/session-interchange-spec.md Â§9).
//!
//! Requirements exercised: L4 (`compaction` + `metadata.keepID`), L5/L8 (one assistant row is one
//! step, holding reasoning, text and both parallel calls with their results), L6 (`native.signature`),
//! L9 (an assistant row with no `finish` is a killed step â€” and `session export` omits it).
//!
//! Two fixtures: the store dump (`rows.json`, what the database holds) and the interchange export
//! (`session-export.json`, what `opencode session export` writes).

use mukade::model::{
    CanonicalTool, HarnessId, Kind, MetaKind, Protocol, ResultContent, Session, StepEnd, Trigger,
};
use mukade::reader::opencode;
use std::path::PathBuf;

fn source(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/opencode/source")
        .join(name)
}

fn read_rows() -> Session {
    opencode::read_file(&source("rows.json")).expect("the store dump must read")
}

fn read_export() -> Session {
    opencode::read_file(&source("session-export.json")).expect("the export must read")
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
fn meta_comes_from_the_session_record() {
    let s = read_rows();
    assert_eq!(s.meta.harness, HarnessId::OpenCode);
    assert_eq!(s.meta.harness_version, "2.1.4");
    assert_eq!(s.meta.id, "ses_e82023514ffe02y4T8OoUu99VZ");
    assert_eq!(
        s.meta.cwd.as_deref(),
        Some("C:/Users/alfio/AppData/Local/shinatama-verify/H27/opencode/work")
    );
    assert_eq!(s.meta.title.as_deref(), Some("H27-opencode-source"));
    assert_eq!(s.meta.models.len(), 1);
    assert_eq!(s.meta.models[0].provider.as_deref(), Some("anthropic"));
    assert_eq!(s.meta.models[0].id, "h27-mock");
    assert_eq!(s.meta.created.as_deref(), Some("2026-10-27T15:32:27.395Z"));
    assert_eq!(s.meta.updated.as_deref(), Some("2026-10-27T15:32:37.925Z"));
}

#[test]
fn every_store_row_is_kept_exactly_once() {
    let doc: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(source("rows.json")).unwrap()).unwrap();
    let rows = doc["session_message"].as_array().unwrap();
    let s = read_rows();
    let mut source: Vec<String> = rows.iter().map(|r| r.to_string()).collect();
    let mut kept: Vec<String> = s.entries.iter().flat_map(|e| e.raw.iter()).map(|r| r.to_string()).collect();
    source.sort();
    kept.sort();
    assert_eq!(kept, source, "every row is kept verbatim, and only once");
}

#[test]
fn conversation_is_the_expected_entry_sequence() {
    let s = read_rows();
    let kinds: Vec<&str> = conversation(&s).iter().map(|e| kind_name(&e.kind)).collect();
    assert_eq!(
        kinds,
        vec![
            "user",       // T1
            "step",       // reasoning + text + two parallel read calls, results in the same row
            "results",    // derived from that row
            "step",       // H27-FINAL
            "user",       // T2
            "step",       // H27-REPLY
            "compaction",
            "user",       // T3
            "step",       // T3 reply
            "user",       // T4
            "step",       // killed: empty text, no finish
        ]
    );
    assert_eq!(s.entries.len(), 15, "14 rows become 15 entries (one row holds a step and its results)");
}

#[test]
fn one_assistant_row_is_one_step_with_both_calls() {
    let s = read_rows();
    let conv = conversation(&s);
    let Kind::Step { model, reasoning, text, calls, end } = &conv[1].kind else { panic!("expected a Step") };
    assert_eq!(model.as_ref().map(|m| m.id.as_str()), Some("h27-mock"));
    assert_eq!(text, &vec!["H27-PRE: reading the files.".to_string()]);
    assert_eq!(calls.len(), 2, "both parallel calls stay in one step");
    assert_eq!(calls[0].id, "toolu_h27call1x0");
    assert_eq!(calls[0].source_name, "read");
    assert_eq!(calls[0].canonical, Some(CanonicalTool::Read));
    assert!(calls[0].input["path"].as_str().unwrap_or_default().ends_with("alpha.txt"));
    assert_eq!(*end, StepEnd::ToolUse);

    let o = reasoning[0].opaque.as_ref().expect("OpenCode stores native.signature");
    assert_eq!(o.protocol, Protocol::AnthropicMessages);
    assert_eq!(o.signature.as_deref(), Some("h27-mock-signature"));
    assert_eq!(o.model.as_deref(), Some("h27-mock"));

    let Kind::Results { results } = &conv[2].kind else { panic!("expected Results") };
    assert_eq!(results.len(), 2, "the row's tool parts carry both results");
    assert_eq!(results[0].call_id, calls[0].id);
    assert_eq!(results[1].call_id, calls[1].id);
    assert_eq!(
        results[0].content,
        ResultContent::Text("Read file C:\\Users\\alfio\\AppData\\Local\\shinatama-verify\\H27\\opencode\\work\\alpha.txt, lines 1-1\n1: ALPHA-CONTENT-42".into()),
        "the result text travels verbatim"
    );
}

#[test]
fn compaction_keeps_the_summary_and_the_native_boundary() {
    let s = read_rows();
    let conv = conversation(&s);
    let Kind::Compaction { summary, kept_from, trigger, tokens_before } = &conv[6].kind else {
        panic!("expected the Compaction entry at index 6")
    };
    assert!(summary.starts_with("## Objective"), "{summary}");
    assert!(summary.contains("H27-SUMMARY: the user asked to read alpha.txt"));
    assert_eq!(*trigger, Trigger::Manual);
    assert_eq!(*tokens_before, Some(100));
    assert_eq!(
        kept_from.as_deref(),
        Some(conv[4].id.as_str()),
        "metadata.keepID names the T2 user row, the first message kept"
    );
}

/// L9: the row with no `finish` is a killed step. `session export` leaves it out, so the same
/// session read from the export ends on a user turn instead.
#[test]
fn a_row_without_finish_is_a_killed_step_and_the_export_omits_it() {
    let s = read_rows();
    let conv = conversation(&s);
    let last = conv.last().copied().unwrap();
    let Kind::Step { text, calls, end, .. } = &last.kind else { panic!("expected the killed assistant row last") };
    assert_eq!(*end, StepEnd::Killed);
    assert!(text.is_empty(), "the killed row holds an empty text block");
    assert!(calls.is_empty());
    assert!(!s.interrupted_last_turn(), "the killed row is the last entry, not a user turn");

    let exported = read_export();
    assert_eq!(exported.entries.len(), 14, "the export omits the killed row");
    let last = conversation(&exported).last().copied().unwrap();
    let Kind::User { text, .. } = &last.kind else { panic!("expected the T4 user turn last") };
    assert_eq!(text, "H27-HANG");
    assert!(exported.interrupted_last_turn());
}

#[test]
fn idle_markers_are_meta() {
    let s = read_rows();
    let idle = s
        .entries
        .iter()
        .filter(|e| matches!(e.kind, Kind::Meta { kind: MetaKind::Idle }))
        .count();
    assert_eq!(idle, 4, "an idle record is a turn marker, not conversation");
}

