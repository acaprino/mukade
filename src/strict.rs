//! The strict gate of §7: what a real provider would refuse, checked on the plan **before** anything
//! is written. The plan already repairs what it can by policy (an orphan result is dropped, an
//! unanswered call travels as text, foreign reasoning is not carried); this gate is what makes that
//! promise checkable — a plan that still violates a provider rule fails the hop instead of writing a
//! file the destination will refuse.

use crate::model::Protocol;
use crate::policy::ReasoningPlan;
use crate::writer::plan::{Plan, PlannedBlock, PlannedResult};

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
                for result in results {
                    if let PlannedResult::Native { call_id, .. } = result
                        && !calls.iter().any(|call| &call.id == call_id)
                    {
                        violations.push(format!("block {i}: result {call_id} answers no call of the step before it"));
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
        PlannedCall { id: id.into(), name: "read".into(), source_name: "Read".into(), input: json!({}) }
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
        Plan { blocks, counts: Counts::default() }
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
        assert!(found.iter().any(|v| v.contains("answers no call")), "{found:?}");
        assert!(found.iter().any(|v| v.contains("no answer")), "both directions are reported: {found:?}");
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
