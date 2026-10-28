//! The plan's own guarantees: what a destination must never receive (spec §7) and what the
//! destination's offered set changes (tables §6, L11).

mod support;

use serde_json::json;
use mukade::model::{
    CanonicalTool, Entry, HarnessId, Kind, Meta, ResultContent, Session, StepEnd, ToolCall, ToolResult,
};
use mukade::tools::{self, ToolSet};
use mukade::writer::plan::{self, PlannedBlock, PlannedResult};

fn session(entries: Vec<Entry>) -> Session {
    Session {
        meta: Meta {
            harness: HarnessId::ClaudeCode,
            harness_version: "2.1.311".into(),
            id: "src".into(),
            cwd: Some(support::cwd_for(HarnessId::ClaudeCode)),
            title: None,
            created: None,
            updated: None,
            models: vec![],
            git_branch: None,
            parent: None,
            source_path: None,
        },
        entries,
    }
}

fn step(id: &str, calls: Vec<ToolCall>) -> Entry {
    Entry { id: id.into(), kind: Kind::Step { model: None, reasoning: vec![], text: vec![], calls, end: StepEnd::ToolUse }, raw: vec![] }
}

fn results(id: &str, call_ids: &[&str]) -> Entry {
    Entry {
        id: id.into(),
        kind: Kind::Results {
            results: call_ids
                .iter()
                .map(|c| ToolResult {
                    call_id: (*c).to_string(),
                    content: ResultContent::Text("out".into()),
                    is_error: false,
                    truncated_by_source: false,
                })
                .collect(),
        },
        raw: vec![],
    }
}

fn read_call(id: &str) -> ToolCall {
    ToolCall {
        id: id.into(),
        source_name: "Read".into(),
        canonical: Some(CanonicalTool::Read),
        input: json!({"file_path": "a.txt"}),
    }
}

/// §7: a provider refuses a `tool_use` that the next message does not answer, so a call the source
/// never answered is written as text — with its id — and counted.
#[test]
fn an_unanswered_call_travels_as_text_instead_of_as_a_tool_call() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "plan-unanswered");
    let opts = support::opts(HarnessId::ClaudeCode, &scratch);
    let s = session(vec![step("e1", vec![read_call("c1"), read_call("c2")]), results("e2", &["c1"])]);
    let plan = plan::plan(&s, HarnessId::ClaudeCode, &opts);
    assert_eq!(plan.counts.unanswered_calls, 1);
    match &plan.blocks[0] {
        PlannedBlock::Step(step) => {
            assert_eq!(step.calls.len(), 1, "only the answered call stays a tool call");
            assert_eq!(step.calls[0].id, "c1");
            assert_eq!(step.text.len(), 1);
            assert!(step.text[0].contains("id c2"), "the text form names the call: {}", step.text[0]);
        }
        other => panic!("{other:?}"),
    }
    match &plan.blocks[1] {
        PlannedBlock::Results { results } => {
            assert_eq!(results, &vec![PlannedResult::Native {
                call_id: "c1".into(),
                content: ResultContent::Text("out".into()),
                is_error: false
            }]);
        }
        other => panic!("{other:?}"),
    }
}

/// §7: a result with no call anywhere is not written as a dangling tool result.
#[test]
fn an_orphan_result_is_dropped_and_counted() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "plan-orphan");
    let opts = support::opts(HarnessId::ClaudeCode, &scratch);
    let s = session(vec![step("e1", vec![read_call("c1")]), results("e2", &["c1", "ghost"])]);
    let plan = plan::plan(&s, HarnessId::ClaudeCode, &opts);
    assert_eq!(plan.counts.dropped_orphan_results, 1);
    match &plan.blocks[1] {
        PlannedBlock::Results { results } => assert_eq!(results.len(), 1),
        other => panic!("{other:?}"),
    }
}

/// A result that arrives after a user turn must not be matched against the calls of an earlier
/// step: the lists are cleared when a user turn or a compaction ends the pair.
#[test]
fn a_stray_result_after_a_user_turn_is_not_matched_to_an_earlier_step() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "plan-stray");
    let opts = support::opts(HarnessId::ClaudeCode, &scratch);
    let s = session(vec![
        step("e1", vec![read_call("c1")]),
        results("e2", &["c1"]),
        Entry { id: "e3".into(), kind: Kind::User { text: "hello".into(), attachments: vec![] }, raw: vec![] },
        results("e4", &["c1"]),
    ]);
    let plan = plan::plan(&s, HarnessId::ClaudeCode, &opts);
    assert_eq!(plan.counts.dropped_orphan_results, 1, "the stray result has no call left to answer");
    assert_eq!(plan.blocks.len(), 3, "step, results, user — the stray one is dropped");
}

/// Tables §6: the destination's offered set is per configuration, and a caller that knows it can
/// widen it — here Pi with `find` enabled keeps a Glob call instead of losing it to text.
#[test]
fn the_destination_offered_set_decides_translation() {
    let scratch = support::scratch(HarnessId::Pi, "plan-offered");
    let glob = ToolCall {
        id: "c1".into(),
        source_name: "Glob".into(),
        canonical: Some(CanonicalTool::Glob),
        input: json!({"pattern": "**/*.rs"}),
    };
    let s = session(vec![step("e1", vec![glob.clone()]), results("e2", &["c1"])]);

    let pinned = plan::plan(&s, HarnessId::Pi, &support::opts(HarnessId::Pi, &scratch));
    assert_eq!(pinned.counts.tools_text_form, 1, "Pi's pinned default set has no find");
    assert_eq!(pinned.counts.tools_translated, 0);

    let mut wide = support::opts(HarnessId::Pi, &scratch);
    wide.offered = Some(ToolSet { names: &["read", "bash", "edit", "write", "grep", "find", "ls"] });
    let widened = plan::plan(&s, HarnessId::Pi, &wide);
    assert_eq!(widened.counts.tools_translated, 1);
    assert_eq!(widened.counts.tools_text_form, 0);
    match &widened.blocks[0] {
        PlannedBlock::Step(step) => assert_eq!(step.calls[0].name, "find"),
        other => panic!("{other:?}"),
    }
    assert!(!tools::offered(HarnessId::Pi).contains("find"), "the pinned list is the bench's default set");
    assert!(tools::offered(HarnessId::Pi).contains("read"));
}
