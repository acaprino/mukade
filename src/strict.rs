//! The strict gate of §7: what a real provider would refuse, checked on the plan **before** anything
//! is written. The plan already repairs what it can by policy (an orphan result is dropped, an
//! unanswered call travels as text, foreign reasoning is not carried); this gate is what makes that
//! promise checkable — a plan that still violates a provider rule fails the hop instead of writing a
//! file the destination will refuse.

use crate::model::Protocol;
use crate::policy::ReasoningPlan;
use crate::writer::plan::{Plan, PlannedBlock, PlannedResult, anthropic_id_char};

/// The provider rules of §7, as far as the written request can be judged from the plan.
pub fn check_plan(plan: &Plan, dest_protocol: Option<&Protocol>) -> Vec<String> {
    let mut violations = Vec::new();
    for (i, block) in plan.blocks.iter().enumerate() {
        match block {
            PlannedBlock::Step(step) => {
                if step.is_empty() {
                    violations.push(format!("block {i}: an assistant step with no content"));
                }
                let answers: &[PlannedResult] = match plan.blocks.get(i + 1) {
                    Some(PlannedBlock::Results { results }) => results,
                    _ => &[],
                };
                for call in &step.calls {
                    if dest_protocol == Some(&Protocol::AnthropicMessages)
                        && (call.id.is_empty() || !call.id.chars().all(anthropic_id_char))
                    {
                        violations.push(format!(
                            "block {i}: tool call id {:?} is outside Anthropic's id pattern",
                            call.id
                        ));
                    }
                    let answered = answers.iter().any(|r| match r {
                        PlannedResult::Native { call_id, .. } => *call_id == call.id,
                        PlannedResult::Text(_) => false,
                    });
                    if !answered {
                        violations.push(format!(
                            "block {i}: tool call {} ({}) has no answer in the next message",
                            call.id, call.name
                        ));
                    }
                }
                // A4 / R3: thinking is only carried natively when it can be replayed.
                for reasoning in &step.reasoning {
                    match reasoning {
                        ReasoningPlan::Native(r) => {
                            if r.opaque.is_none() {
                                violations.push(format!(
                                    "block {i}: reasoning carried natively without its opaque payload"
                                ));
                            }
                            let protocol = r.opaque.as_ref().map(|o| o.protocol.clone());
                            if let (Some(carried), Some(dest)) = (protocol, dest_protocol)
                                && carried != *dest
                            {
                                violations.push(format!(
                                    "block {i}: reasoning of protocol {carried:?} carried into {dest:?}"
                                ));
                            }
                        }
                        ReasoningPlan::Text(t) if t.is_empty() => {
                            violations.push(format!("block {i}: an empty reasoning text block"));
                        }
                        _ => {}
                    }
                }
            }
            PlannedBlock::Results { results } => {
                let calls = match i.checked_sub(1).and_then(|before| plan.blocks.get(before)) {
                    Some(PlannedBlock::Step(step)) => &step.calls,
                    _ => {
                        violations.push(format!("block {i}: results without a preceding step"));
                        continue;
                    }
                };
                let mut after_text = false;
                let mut answered: Vec<&str> = Vec::new();
                for result in results {
                    match result {
                        PlannedResult::Native { call_id, .. } => {
                            // A3: one call, one answer.
                            if answered.contains(&call_id.as_str()) {
                                violations
                                    .push(format!("block {i}: call {call_id} is answered twice"));
                            }
                            answered.push(call_id);
                            if !calls.iter().any(|call| &call.id == call_id) {
                                violations.push(format!("block {i}: result {call_id} answers no call of the step before it"));
                            }
                            // §6.2 [V]: Pi reads text between tool results as the end of the tool
                            // flow and answers the later calls itself.
                            if after_text {
                                violations
                                    .push(format!("block {i}: result {call_id} comes after text"));
                            }
                        }
                        PlannedResult::Text(_) => after_text = true,
                    }
                }
            }
            PlannedBlock::User { .. } | PlannedBlock::Compaction { .. } => {}
        }
    }
    violations
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Reasoning, ResultContent};
    use crate::writer::plan::{Counts, PlannedCall, PlannedStep};
    use serde_json::json;

    fn call(id: &str) -> PlannedCall {
        PlannedCall {
            id: id.into(),
            name: "read".into(),
            source_name: "Read".into(),
            input: json!({}),
            source_input: json!({}),
        }
    }

    fn step(calls: Vec<PlannedCall>) -> PlannedStep {
        PlannedStep {
            model: None,
            reasoning: Vec::new(),
            text: vec!["ok".into()],
            calls,
            end: crate::model::StepEnd::Completed,
        }
    }

    fn plan_of(blocks: Vec<PlannedBlock>) -> Plan {
        Plan {
            blocks,
            counts: Counts::default(),
        }
    }

    /// The gate must catch what the provider would refuse, not just agree with the plan.
    #[test]
    fn a_call_without_its_answer_is_a_violation() {
        let plan = plan_of(vec![PlannedBlock::Step(step(vec![call("c1")]))]);
        let found = check_plan(&plan, None);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("no answer"), "{found:?}");
    }

    #[test]
    fn a_result_without_its_call_is_a_violation() {
        let plan = plan_of(vec![
            PlannedBlock::Step(step(vec![call("c1")])),
            PlannedBlock::Results {
                results: vec![PlannedResult::Native {
                    call_id: "ghost".into(),
                    content: ResultContent::Text("x".into()),
                    is_error: false,
                }],
            },
        ]);
        let found = check_plan(&plan, None);
        assert!(
            found.iter().any(|v| v.contains("answers no call")),
            "{found:?}"
        );
        assert!(
            found.iter().any(|v| v.contains("no answer")),
            "both directions are reported: {found:?}"
        );
    }

    #[test]
    fn an_answered_pair_is_clean() {
        let plan = plan_of(vec![
            PlannedBlock::Step(step(vec![call("c1")])),
            PlannedBlock::Results {
                results: vec![PlannedResult::Native {
                    call_id: "c1".into(),
                    content: ResultContent::Text("x".into()),
                    is_error: false,
                }],
            },
        ]);
        assert!(check_plan(&plan, None).is_empty());
    }

    /// §6.2 [V]: text between tool results ends the tool flow for Pi, which then refuses the rest.
    #[test]
    fn a_native_result_after_text_is_a_violation() {
        let native = |id: &str| PlannedResult::Native {
            call_id: id.into(),
            content: ResultContent::Text("x".into()),
            is_error: false,
        };
        let plan = plan_of(vec![
            PlannedBlock::Step(step(vec![call("c1"), call("c2")])),
            PlannedBlock::Results {
                results: vec![native("c1"), PlannedResult::Text("t".into()), native("c2")],
            },
        ]);
        let found = check_plan(&plan, None);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("after text"), "{found:?}");
    }

    /// §7 A3: one call, one answer.
    #[test]
    fn a_call_answered_twice_is_a_violation() {
        let native = PlannedResult::Native {
            call_id: "c1".into(),
            content: ResultContent::Text("x".into()),
            is_error: false,
        };
        let plan = plan_of(vec![
            PlannedBlock::Step(step(vec![call("c1")])),
            PlannedBlock::Results {
                results: vec![native.clone(), native],
            },
        ]);
        let found = check_plan(&plan, None);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("answered twice"), "{found:?}");
    }

    /// Anthropic accepts a tool-call id only in `^[a-zA-Z0-9_-]+$`.
    #[test]
    fn a_call_id_outside_the_anthropic_pattern_is_a_violation() {
        let plan = plan_of(vec![
            PlannedBlock::Step(step(vec![call("a|b")])),
            PlannedBlock::Results {
                results: vec![PlannedResult::Native {
                    call_id: "a|b".into(),
                    content: ResultContent::Text("x".into()),
                    is_error: false,
                }],
            },
        ]);
        assert!(check_plan(&plan, None).is_empty());
        let found = check_plan(&plan, Some(&Protocol::AnthropicMessages));
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("id pattern"), "{found:?}");
    }

    #[test]
    fn reasoning_of_another_protocol_is_a_violation() {
        let mut s = step(Vec::new());
        s.reasoning.push(ReasoningPlan::Native(Reasoning {
            text: "hmm".into(),
            opaque: Some(crate::model::Opaque {
                protocol: Protocol::AnthropicMessages,
                model: Some("m".into()),
                signature: Some("sig".into()),
                encrypted: None,
            }),
        }));
        let plan = plan_of(vec![PlannedBlock::Step(s)]);
        assert!(check_plan(&plan, Some(&Protocol::AnthropicMessages)).is_empty());
        let found = check_plan(&plan, Some(&Protocol::OpenaiResponses));
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("protocol"), "{found:?}");
    }
}
