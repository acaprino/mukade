//! The plan's own guarantees: what a destination must never receive (spec §7) and what the
//! destination's offered set changes (tables §6, L11).

mod support;

use mukade::model::{
    CanonicalTool, Entry, HarnessId, Kind, Meta, ResultContent, Session, StepEnd, ToolCall,
    ToolResult,
};
use mukade::tools::{self, ToolSet};
use mukade::writer::plan::{self, PlannedBlock, PlannedResult};
use serde_json::json;

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
            source_sha256: None,
        },
        entries,
    }
}

fn step(id: &str, calls: Vec<ToolCall>) -> Entry {
    Entry {
        id: id.into(),
        kind: Kind::Step {
            model: None,
            reasoning: vec![],
            text: vec![],
            calls,
            end: StepEnd::ToolUse,
        },
        raw: vec![],
    }
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
    let s = session(vec![
        step("e1", vec![read_call("c1"), read_call("c2")]),
        results("e2", &["c1"]),
    ]);
    let plan = plan::plan(&s, HarnessId::ClaudeCode, &opts);
    assert_eq!(plan.counts.unanswered_calls, 1);
    match &plan.blocks[0] {
        PlannedBlock::Step(step) => {
            assert_eq!(
                step.calls.len(),
                1,
                "only the answered call stays a tool call"
            );
            assert_eq!(step.calls[0].id, "c1");
            assert_eq!(step.text.len(), 1);
            assert!(
                step.text[0].contains("id c2"),
                "the text form names the call: {}",
                step.text[0]
            );
        }
        other => panic!("{other:?}"),
    }
    match &plan.blocks[1] {
        PlannedBlock::Results { results } => {
            assert_eq!(
                results,
                &vec![PlannedResult::Native {
                    call_id: "c1".into(),
                    content: ResultContent::Text("out".into()),
                    is_error: false
                }]
            );
        }
        other => panic!("{other:?}"),
    }
}

/// §7: a result with no call anywhere is not written as a dangling tool result.
#[test]
fn an_orphan_result_is_dropped_and_counted() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "plan-orphan");
    let opts = support::opts(HarnessId::ClaudeCode, &scratch);
    let s = session(vec![
        step("e1", vec![read_call("c1")]),
        results("e2", &["c1", "ghost"]),
    ]);
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
        Entry {
            id: "e3".into(),
            kind: Kind::User {
                text: "hello".into(),
                attachments: vec![],
            },
            raw: vec![],
        },
        results("e4", &["c1"]),
    ]);
    let plan = plan::plan(&s, HarnessId::ClaudeCode, &opts);
    assert_eq!(
        plan.counts.dropped_orphan_results, 1,
        "the stray result has no call left to answer"
    );
    assert_eq!(
        plan.blocks.len(),
        3,
        "step, results, user — the stray one is dropped"
    );
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
    assert_eq!(
        pinned.counts.tools_text_form, 1,
        "Pi's pinned default set has no find"
    );
    assert_eq!(pinned.counts.tools_translated, 0);

    let mut wide = support::opts(HarnessId::Pi, &scratch);
    wide.offered = Some(ToolSet {
        names: &["read", "bash", "edit", "write", "grep", "find", "ls"],
    });
    let widened = plan::plan(&s, HarnessId::Pi, &wide);
    assert_eq!(widened.counts.tools_translated, 1);
    assert_eq!(widened.counts.tools_text_form, 0);
    match &widened.blocks[0] {
        PlannedBlock::Step(step) => assert_eq!(step.calls[0].name, "find"),
        other => panic!("{other:?}"),
    }
    assert!(
        !tools::offered(HarnessId::Pi).contains("find"),
        "the pinned list is the bench's default set"
    );
    assert!(tools::offered(HarnessId::Pi).contains("read"));
}

/// §6.2 [V]: Pi reads a user message between tool results as the end of the tool flow, answers the
/// later calls with `No result provided` and then sends their real results too, which the provider
/// refuses. Native results come first, then the text that answers calls carried as text.
#[test]
fn native_results_come_before_the_text_that_answers_a_text_form_call() {
    let scratch = support::scratch(HarnessId::Pi, "plan-results-order");
    let opts = support::opts(HarnessId::Pi, &scratch);
    let unknown = ToolCall {
        id: "f1".into(),
        source_name: "Frobnicate".into(),
        canonical: None,
        input: json!({}),
    };
    let s = session(vec![
        step("e1", vec![read_call("r1"), unknown, read_call("r2")]),
        results("e2", &["r1", "f1", "r2"]),
    ]);
    let plan = plan::plan(&s, HarnessId::Pi, &opts);
    let Some(PlannedBlock::Results { results }) = plan.blocks.get(1) else {
        panic!("{:?}", plan.blocks)
    };
    let order: Vec<&str> = results
        .iter()
        .map(|result| match result {
            PlannedResult::Native { call_id, .. } => call_id.as_str(),
            PlannedResult::Text(_) => "text",
        })
        .collect();
    assert_eq!(order, ["r1", "r2", "text"]);
}

/// §7 A3: a call answered twice is refused by the provider. The first answer is kept; the rest are
/// dropped and counted.
#[test]
fn a_second_result_for_the_same_call_is_dropped_and_counted() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "plan-duplicate-result");
    let opts = support::opts(HarnessId::ClaudeCode, &scratch);
    let s = session(vec![
        step("e1", vec![read_call("c1")]),
        results("e2", &["c1", "c1"]),
    ]);
    let plan = plan::plan(&s, HarnessId::ClaudeCode, &opts);
    let Some(PlannedBlock::Results { results }) = plan.blocks.get(1) else {
        panic!("{:?}", plan.blocks)
    };
    assert_eq!(results.len(), 1, "{results:?}");
    assert_eq!(plan.counts.dropped_duplicate_results, 1);
    let losses = mukade::manifest::Manifest::losses_for(&plan.counts, false, false);
    assert!(
        losses.iter().any(|loss| loss.contains("duplicate")),
        "{losses:?}"
    );
}

fn reasoning_step(
    id: &str,
    protocol: mukade::model::Protocol,
    signature: Option<&str>,
    encrypted: Option<&str>,
) -> Entry {
    Entry {
        id: id.into(),
        kind: Kind::Step {
            model: None,
            reasoning: vec![mukade::model::Reasoning {
                text: String::new(),
                opaque: Some(mukade::model::Opaque {
                    protocol,
                    model: Some("m".into()),
                    signature: signature.map(str::to_owned),
                    encrypted: encrypted.map(str::to_owned),
                }),
            }],
            text: vec!["done".into()],
            calls: vec![],
            end: StepEnd::Completed,
        },
        raw: vec![],
    }
}

/// A4: Claude's `redacted_thinking` carries its payload in `data`, not in a signature. Written back
/// as `thinking` with a null signature it is refused and the payload is gone.
#[test]
fn redacted_thinking_is_written_back_as_redacted_thinking() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "plan-redacted-thinking");
    let opts = support::opts(HarnessId::ClaudeCode, &scratch);
    let s = session(vec![reasoning_step(
        "e1",
        mukade::model::Protocol::AnthropicMessages,
        None,
        Some("REDACTED-PAYLOAD"),
    )]);
    let written = mukade::writer::for_harness(HarnessId::ClaudeCode)
        .write(&s, &opts)
        .unwrap();
    let rows: Vec<serde_json::Value> = std::fs::read_to_string(&written.location)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let blocks: Vec<&serde_json::Value> = rows
        .iter()
        .filter_map(|row| {
            row.pointer("/message/content")
                .and_then(serde_json::Value::as_array)
        })
        .flatten()
        .collect();
    assert!(
        blocks
            .iter()
            .any(|b| b["type"] == "redacted_thinking" && b["data"] == "REDACTED-PAYLOAD"),
        "{blocks:?}"
    );
    assert!(
        !blocks.iter().any(|b| b["type"] == "thinking"),
        "{blocks:?}"
    );
}

/// A4: Pi replays thinking from its signature. Reasoning that has only an encrypted payload cannot
/// be written there, so it is dropped and counted instead of travelling as an unsigned block.
#[test]
fn reasoning_with_only_an_encrypted_payload_is_dropped_for_pi() {
    let scratch = support::scratch(HarnessId::Pi, "plan-encrypted-to-pi");
    let mut opts = support::opts(HarnessId::Pi, &scratch);
    opts.dest_protocol = Some(mukade::model::Protocol::OpenaiResponses);
    let s = session(vec![reasoning_step(
        "e1",
        mukade::model::Protocol::OpenaiResponses,
        None,
        Some("gAAAA-ENCRYPTED"),
    )]);
    let plan = plan::plan(&s, HarnessId::Pi, &opts);
    assert_eq!(plan.counts.reasoning_dropped, 1);
    let Some(PlannedBlock::Step(step)) = plan.blocks.first() else {
        panic!("{:?}", plan.blocks)
    };
    assert!(
        !step
            .reasoning
            .iter()
            .any(|r| matches!(r, mukade::policy::ReasoningPlan::Native(_))),
        "{:?}",
        step.reasoning
    );
}

/// Anthropic refuses a tool-call id outside `^[a-zA-Z0-9_-]+$`; Pi on `openai-responses` records
/// ids such as `call_x|fc_y`. The id is rewritten the same way for the call and its result, and
/// counted.
#[test]
fn a_call_id_anthropic_refuses_is_rewritten_for_call_and_result() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "plan-call-id");
    let opts = support::opts(HarnessId::ClaudeCode, &scratch);
    let s = session(vec![
        step("e1", vec![read_call("call_x|fc_y")]),
        results("e2", &["call_x|fc_y"]),
    ]);
    let plan = plan::plan(&s, HarnessId::ClaudeCode, &opts);
    let (Some(PlannedBlock::Step(step)), Some(PlannedBlock::Results { results })) =
        (plan.blocks.first(), plan.blocks.get(1))
    else {
        panic!("{:?}", plan.blocks)
    };
    let id = &step.calls[0].id;
    assert!(
        !id.is_empty()
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
        "{id}"
    );
    let PlannedResult::Native { call_id, .. } = &results[0] else {
        panic!("{results:?}")
    };
    assert_eq!(call_id, id);
    assert_eq!(plan.counts.rewritten_call_ids, 1);
    let losses = mukade::manifest::Manifest::losses_for(&plan.counts, false, false);
    assert!(
        losses.iter().any(|loss| loss.contains("ids rewritten")),
        "{losses:?}"
    );
}

/// No loss is silent: the records of an abandoned branch (a rewind, an edited prompt) stay in the
/// source and are not written, and the manifest says so.
#[test]
fn records_of_an_abandoned_branch_are_counted() {
    let scratch = support::scratch(HarnessId::Pi, "plan-off-branch");
    let opts = support::opts(HarnessId::Pi, &scratch);
    let off = Entry {
        id: "old".into(),
        kind: Kind::Meta {
            kind: mukade::model::MetaKind::OffBranch,
        },
        raw: vec![],
    };
    let s = session(vec![off.clone(), off, step("e1", vec![])]);
    let plan = plan::plan(&s, HarnessId::Pi, &opts);
    assert_eq!(plan.counts.dropped_off_branch, 2);
    let losses = mukade::manifest::Manifest::losses_for(&plan.counts, false, false);
    assert!(
        losses.iter().any(|loss| loss.contains("abandoned branch")),
        "{losses:?}"
    );
}

/// No loss is silent: a last line the source never finished writing is not read, and the manifest
/// says so.
#[test]
fn an_unfinished_last_line_is_counted() {
    let scratch = support::scratch(HarnessId::Pi, "plan-unfinished");
    let opts = support::opts(HarnessId::Pi, &scratch);
    let cut = Entry {
        id: "unfinished".into(),
        kind: Kind::Meta {
            kind: mukade::model::MetaKind::Unfinished,
        },
        raw: vec![],
    };
    let s = session(vec![step("e1", vec![]), cut]);
    let plan = plan::plan(&s, HarnessId::Pi, &opts);
    assert_eq!(plan.counts.dropped_unfinished_lines, 1);
    let losses = mukade::manifest::Manifest::losses_for(&plan.counts, false, false);
    assert!(
        losses.iter().any(|loss| loss.contains("unfinished")),
        "{losses:?}"
    );
}

/// A step's results may come in more than one group (a document with one result per message, an
/// internal row between them): they answer the same step and travel as one.
#[test]
fn results_split_over_two_groups_answer_the_same_step() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "plan-split-results");
    let opts = support::opts(HarnessId::ClaudeCode, &scratch);
    let s = session(vec![
        step("e1", vec![read_call("a"), read_call("b")]),
        results("e2", &["a"]),
        results("e3", &["b"]),
    ]);
    let plan = plan::plan(&s, HarnessId::ClaudeCode, &opts);
    assert_eq!(plan.blocks.len(), 2, "{:?}", plan.blocks);
    let Some(PlannedBlock::Results { results }) = plan.blocks.get(1) else {
        panic!("{:?}", plan.blocks)
    };
    assert_eq!(results.len(), 2);
    assert_eq!(plan.counts.dropped_orphan_results, 0);
    assert_eq!(plan.counts.unanswered_calls, 0);
}

/// The counts say what was written: a call moved to text because nothing answered it is not
/// counted as translated, and the source's own record of it is what the text shows.
#[test]
fn an_unanswered_call_is_counted_once_and_shown_as_the_source_made_it() {
    let scratch = support::scratch(HarnessId::Pi, "plan-unanswered-label");
    let opts = support::opts(HarnessId::Pi, &scratch);
    let s = session(vec![step("e1", vec![read_call("c1")])]);
    let plan = plan::plan(&s, HarnessId::Pi, &opts);
    assert_eq!(plan.counts.tools_translated, 0);
    assert_eq!(plan.counts.unanswered_calls, 1);
    let Some(PlannedBlock::Step(step)) = plan.blocks.first() else {
        panic!("{:?}", plan.blocks)
    };
    let text = step.text.join("\n");
    assert!(
        text.contains("Read") && text.contains("file_path"),
        "{text}"
    );
    assert!(!text.contains("not available in"), "{text}");
}

/// A source message stays a source message in the count, whatever the plan does with it.
#[test]
fn a_group_of_orphan_results_still_counts_as_a_source_message() {
    let scratch = support::scratch(HarnessId::Pi, "plan-orphan-count");
    let opts = support::opts(HarnessId::Pi, &scratch);
    let s = session(vec![step("e1", vec![]), results("e2", &["ghost"])]);
    let plan = plan::plan(&s, HarnessId::Pi, &opts);
    assert_eq!(plan.counts.messages_in, 2);
    assert_eq!(plan.counts.dropped_orphan_results, 1);
}

/// Reasoning kept as text comes before the answer it led to, as it did in the source.
#[test]
fn reasoning_as_text_comes_before_the_answer() {
    let scratch = support::scratch(HarnessId::Pi, "plan-reasoning-order");
    let mut opts = support::opts(HarnessId::Pi, &scratch);
    opts.reasoning = mukade::policy::ReasoningPolicy::AsText;
    let s = session(vec![Entry {
        id: "e1".into(),
        kind: Kind::Step {
            model: None,
            reasoning: vec![mukade::model::Reasoning {
                text: "THINKING".into(),
                opaque: None,
            }],
            text: vec!["ANSWER".into()],
            calls: vec![],
            end: StepEnd::Completed,
        },
        raw: vec![],
    }]);
    let plan = plan::plan(&s, HarnessId::Pi, &opts);
    let Some(PlannedBlock::Step(step)) = plan.blocks.first() else {
        panic!("{:?}", plan.blocks)
    };
    assert!(step.text[0].contains("THINKING"), "{:?}", step.text);
    assert_eq!(step.text[1], "ANSWER");
}

/// No loss is silent: outside Pi, which keeps a step's end, an aborted or failed step is written
/// as an ordinary reply, and the manifest says so.
#[test]
fn an_aborted_step_written_as_a_reply_is_counted() {
    let aborted = Entry {
        id: "e1".into(),
        kind: Kind::Step {
            model: None,
            reasoning: vec![],
            text: vec!["PARTIAL".into()],
            calls: vec![],
            end: StepEnd::Aborted,
        },
        raw: vec![],
    };
    let s = session(vec![aborted]);
    let scratch = support::scratch(HarnessId::ClaudeCode, "plan-aborted");
    let plan = plan::plan(
        &s,
        HarnessId::ClaudeCode,
        &support::opts(HarnessId::ClaudeCode, &scratch),
    );
    assert_eq!(plan.counts.aborted_steps_as_replies, 1);
    let losses = mukade::manifest::Manifest::losses_for(&plan.counts, false, false);
    assert!(
        losses.iter().any(|loss| loss.contains("aborted")),
        "{losses:?}"
    );
    let scratch = support::scratch(HarnessId::Pi, "plan-aborted");
    let plan = plan::plan(&s, HarnessId::Pi, &support::opts(HarnessId::Pi, &scratch));
    assert_eq!(
        plan.counts.aborted_steps_as_replies, 0,
        "Pi keeps the step's end"
    );
}
