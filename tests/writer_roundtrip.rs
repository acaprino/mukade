//! Round trips: a source session written by our writer must read back as the same conversation
//! (docs/session-interchange-spec.md §9: a conformance bench is the test).
//!
//! Each harness's own fixture is written into a scratch directory under the system temporary folder
//! and read again with that harness's reader. Nothing outside the scratch area is touched, and
//! OpenCode's document is exercised through the same `{info, messages[]}` file its `session import`
//! consumes; spawning a real harness is the caller's business, not a test's.

mod support;

use mukade::model::HarnessId;
use mukade::policy::Clock;
use mukade::reader::opencode;
use mukade::writer::opencode::document_for;
use mukade::writer::{WriteOpts, Written, for_harness};
use std::path::Path;

fn write(harness: HarnessId, tag: &str) -> (WriteOpts, Written) {
    let scratch = support::scratch(harness, tag);
    let session = support::read_source(harness);
    let opts = support::opts(harness, &scratch);
    let written = for_harness(harness)
        .write(&session, &opts)
        .expect("write the session");
    (opts, written)
}

/// The document OpenCode's `session import` would be handed, written to the scratch area so it can
/// be read back like any other file.
fn write_opencode_document(tag: &str) -> std::path::PathBuf {
    let scratch = support::scratch(HarnessId::OpenCode, tag);
    let session = support::read_source(HarnessId::OpenCode);
    let mut opts = support::opts(HarnessId::OpenCode, &scratch);
    opts.project_id = Some("project-under-test".into());
    let mut clock = Clock::new(opts.now.as_deref());
    let doc = document_for(
        &session,
        "ses_roundtrip",
        support::cwd_for(HarnessId::OpenCode).as_str(),
        &opts,
        &mut clock,
    )
    .expect("document");
    let file = scratch.join("document.json");
    std::fs::write(&file, serde_json::to_string(&doc).unwrap()).unwrap();
    file
}

#[test]
fn claude_code_source_round_trips_through_our_claude_code_writer() {
    let (_, written) = write(HarnessId::ClaudeCode, "roundtrip-claude");
    let back =
        mukade::reader::claude_code::read_file(Path::new(&written.location)).expect("read back");
    let source = support::read_source(HarnessId::ClaudeCode);
    assert_eq!(
        support::roundtrip_script(&back),
        support::roundtrip_script(&source)
    );
    assert_eq!(
        back.meta.id, written.id,
        "the written session has the destination's own id"
    );
    assert!(!back.interrupted_last_turn() == !source.interrupted_last_turn());
}

#[test]
fn codex_source_round_trips_through_our_codex_writer() {
    let (_, written) = write(HarnessId::Codex, "roundtrip-codex");
    let back = mukade::reader::codex::read_file(Path::new(&written.location)).expect("read back");
    let source = support::read_source(HarnessId::Codex);
    assert_eq!(
        support::roundtrip_script(&back),
        support::roundtrip_script(&source)
    );
    assert_eq!(
        back.meta.harness_version, "0.164.0",
        "the written rollout states the destination's version"
    );
    assert!(back.interrupted_last_turn());
    // The model Codex will use is stated in each turn's context, which the rollout keeps.
    let rows = support::read_jsonl(Path::new(&written.location));
    let model = rows
        .iter()
        .find(|r| r["type"] == "turn_context")
        .map(|r| r["payload"]["model"].clone())
        .unwrap_or(serde_json::Value::Null);
    assert_eq!(model, serde_json::json!("h27-mock"));
}

#[test]
fn opencode_source_round_trips_through_our_opencode_document() {
    let file = write_opencode_document("roundtrip-opencode");
    let back = opencode::read_file(&file).expect("read back");
    let source = support::read_source(HarnessId::OpenCode);
    assert_eq!(
        support::roundtrip_script(&back),
        support::roundtrip_script(&source)
    );
    assert_eq!(back.meta.id, "ses_roundtrip");
}

#[test]
fn pi_source_round_trips_through_our_pi_writer() {
    let (_, written) = write(HarnessId::Pi, "roundtrip-pi");
    let back = mukade::reader::pi::read_file(Path::new(&written.location)).expect("read back");
    let source = support::read_source(HarnessId::Pi);
    assert_eq!(
        support::roundtrip_script(&back),
        support::roundtrip_script(&source)
    );
    assert!(back.interrupted_last_turn());
}

/// A hop that changes harness keeps every user turn, every assistant text and every tool result,
/// and states what it dropped in the plan's counts.
#[test]
fn a_cross_harness_hop_keeps_the_conversation_and_counts_its_losses() {
    use mukade::writer::plan;
    let scratch = support::scratch(HarnessId::Pi, "roundtrip-cross");
    let session = support::read_source(HarnessId::ClaudeCode);
    let opts = support::opts(HarnessId::Pi, &scratch);
    let plan = plan::plan(&session, HarnessId::Pi, &opts);
    assert_eq!(
        plan.counts.tools_translated, 2,
        "Claude's Read reaches Pi's read"
    );
    assert_eq!(plan.counts.tools_text_form, 0);
    assert_eq!(
        plan.counts.reasoning_dropped, 0,
        "both speak Anthropic messages here"
    );

    let written = for_harness(HarnessId::Pi)
        .write(&session, &opts)
        .expect("write");
    let back = mukade::reader::pi::read_file(Path::new(&written.location)).expect("read back");
    let texts_back: Vec<String> = support::roundtrip_script(&back);
    for text in [
        "H27-TOOLS",
        "H27-PRE",
        "H27-FINAL",
        "H27-T2",
        "H27-REPLY",
        "H27-T3",
        "ALPHA-CONTENT-42",
    ] {
        assert!(
            texts_back.iter().any(|s| s.contains(text)),
            "{text} did not survive the hop: {texts_back:?}"
        );
    }

    // With the destination on another protocol the reasoning cannot be replayed, and the plan says
    // so instead of dropping it silently.
    let mut other = support::opts(
        HarnessId::Pi,
        &support::scratch(HarnessId::Pi, "roundtrip-cross-2"),
    );
    other.dest_protocol = Some(mukade::model::Protocol::OpenaiResponses);
    let plan = plan::plan(&session, HarnessId::Pi, &other);
    assert_eq!(plan.counts.reasoning_dropped, 1);
}

fn step_models(session: &mukade::model::Session) -> Vec<Option<mukade::model::ModelRef>> {
    session
        .conversation()
        .filter_map(|entry| match &entry.kind {
            mukade::model::Kind::Step { model, .. } => Some(model.clone()),
            _ => None,
        })
        .collect()
}

/// A document keeps what a writer needs: each step's provider as well as its model id, and the
/// entry ids a compaction boundary points at.
#[test]
fn a_document_keeps_step_providers_and_entry_ids() {
    use mukade::wire::{from_document, to_document};
    let source = support::read_source(HarnessId::Pi);
    let back = from_document(&to_document(&source)).unwrap();
    assert_eq!(step_models(&back), step_models(&source));

    let source = support::read_source(HarnessId::ClaudeCode);
    let back = from_document(&to_document(&source)).unwrap();
    let Some(mukade::model::Kind::Compaction {
        kept_from: Some(kept_from),
        ..
    }) = back.last_compaction().map(|entry| &entry.kind)
    else {
        panic!("the compaction and its boundary travel")
    };
    assert!(back.entry(kept_from).is_some(), "{kept_from} resolves");
}

/// A document is read whole or refused: an unknown block is an error, not a silent drop, and a
/// tool result's content may be a plain string, as Claude writes it.
#[test]
fn a_document_block_is_read_or_refused() {
    use mukade::wire::{from_document, to_document};
    let source = support::read_source(HarnessId::ClaudeCode);
    let mut doc = to_document(&source);
    doc["messages"][0]["content"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"type": "hologram", "data": 1}));
    let error = from_document(&doc).unwrap_err();
    assert!(format!("{error:#}").contains("hologram"), "{error:#}");

    let mut doc = to_document(&source);
    let message = doc["messages"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|m| m["content"][0]["type"] == "tool_result")
        .unwrap();
    message["content"][0]["content"] = "PLAIN-RESULT".into();
    let back = from_document(&doc).unwrap();
    let found = back.conversation().any(|entry| match &entry.kind {
        mukade::model::Kind::Results { results } => results
            .iter()
            .any(|r| r.content == mukade::model::ResultContent::Text("PLAIN-RESULT".into())),
        _ => false,
    });
    assert!(found);
}
