//! The rendering plan: what a [`Session`] becomes for one destination.
//!
//! This is the only place that applies the tool tables (L11–L14) and the reasoning policy, so all
//! four writers and the CLI's document projection agree on what is carried, what becomes text and
//! what is dropped. A writer's job is then only to lay the plan out as native records.

use crate::model::{
    Attachment, HarnessId, Kind, ModelRef, Protocol, ResultContent, Session, StepEnd,
};
use crate::policy::{ReasoningPlan, plan_reasoning};
use crate::tools::{self, Shell, Translation};
use crate::writer::WriteOpts;
use serde_json::Value;

/// One call that will be written as the destination's own tool.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedCall {
    pub id: String,
    /// The destination's tool name.
    pub name: String,
    /// The name the source harness used, kept so a call that has to fall back to text form can
    /// still say where it came from.
    pub source_name: String,
    pub input: Value,
}

/// One result, either native (its call was translated) or text (its call travelled as text).
#[derive(Debug, Clone, PartialEq)]
pub enum PlannedResult {
    Native {
        call_id: String,
        content: ResultContent,
        is_error: bool,
    },
    Text(String),
}

/// One model step, ready to be laid out.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedStep {
    pub model: Option<ModelRef>,
    pub reasoning: Vec<ReasoningPlan>,
    /// The step's own text plus the text form of the calls it could not translate.
    pub text: Vec<String>,
    pub calls: Vec<PlannedCall>,
    pub end: StepEnd,
}

impl PlannedStep {
    pub fn is_empty(&self) -> bool {
        self.text.is_empty() && self.calls.is_empty()
    }

    /// The reasoning blocks to write, with their opaque payloads.
    pub fn native_reasoning(&self) -> impl Iterator<Item = &crate::model::Reasoning> {
        self.reasoning.iter().filter_map(|r| match r {
            ReasoningPlan::Native(native) => Some(native),
            _ => None,
        })
    }
}

/// One entry of the plan.
#[derive(Debug, Clone, PartialEq)]
pub enum PlannedBlock {
    User {
        text: String,
        attachments: Vec<Attachment>,
    },
    Step(PlannedStep),
    Results {
        results: Vec<PlannedResult>,
    },
    /// A compaction: the source's own summary, carried as one user message (L4).
    Compaction {
        summary: String,
    },
}

/// What the plan did, for the manifest (§6.3).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Counts {
    /// Conversation entries in the source (not `Meta`: those are not conversation).
    pub messages_in: usize,
    /// Blocks the destination will receive.
    pub messages_out: usize,
    pub tools_translated: usize,
    pub tools_text_form: usize,
    pub text_form_tools: Vec<String>,
    pub reasoning_dropped: usize,
    /// A result whose call is nowhere in the source.
    pub dropped_orphan_results: usize,
    /// A call the source never answered: the provider would refuse it as a tool call (L7).
    pub unanswered_calls: usize,
    /// A step that lost everything it had and cannot be written as an empty message.
    pub dropped_empty_steps: usize,
    /// User attachments that could not be carried by a destination.
    pub dropped_attachments: usize,
    /// Structured blocks flattened by a destination that only accepts text results.
    pub dropped_result_blocks: usize,
}

/// A whole session, planned.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub blocks: Vec<PlannedBlock>,
    pub counts: Counts,
}

/// Plan the conversation of a session for a destination.
pub fn plan(session: &Session, dest: HarnessId, opts: &WriteOpts) -> Plan {
    let dest_protocol = opts
        .dest_protocol
        .clone()
        .or_else(|| crate::policy::destination_protocol(dest));
    // The destination's offered set is per version and configuration (tables §6); a caller that
    // knows it passes it in, otherwise the pinned list is used.
    let offered = opts.offered.unwrap_or_else(|| tools::offered(dest));
    let mut blocks: Vec<PlannedBlock> = Vec::new();
    let mut counts = Counts::default();
    // The calls of the step just planned: which of them the destination took natively, and which
    // of them exist at all (a result whose call is nowhere is an orphan and is dropped).
    let mut last_step_native: Vec<String> = Vec::new();
    let mut last_step_calls: Vec<String> = Vec::new();

    for entry in session.conversation() {
        counts.messages_in += 1;
        match &entry.kind {
            Kind::User { text, attachments } => {
                // A user turn ends the previous step's pair of call lists: a stray result after it
                // must not be matched against calls that belong to an earlier step.
                last_step_native.clear();
                last_step_calls.clear();
                blocks.push(PlannedBlock::User {
                    text: text.clone(),
                    attachments: attachments.clone(),
                });
            }
            Kind::Step {
                model,
                reasoning,
                text,
                calls,
                end,
            } => {
                let mut planned = PlannedStep {
                    model: model.clone(),
                    reasoning: Vec::new(),
                    text: text.clone(),
                    calls: Vec::new(),
                    end: end.clone(),
                };
                for r in reasoning {
                    match plan_reasoning(r, dest_protocol.as_ref(), opts.reasoning) {
                        ReasoningPlan::Native(native) => {
                            planned.reasoning.push(ReasoningPlan::Native(native))
                        }
                        ReasoningPlan::Text(t) => {
                            planned.reasoning.push(ReasoningPlan::Text(t.clone()));
                            planned.text.push(t);
                        }
                        ReasoningPlan::Drop => {
                            counts.reasoning_dropped += 1;
                            planned.reasoning.push(ReasoningPlan::Drop);
                        }
                    }
                }
                last_step_calls.clear();
                last_step_native.clear();
                for call in calls {
                    last_step_calls.push(call.id.clone());
                    match tools::translate_with(call, dest, opts.dest_shell, offered) {
                        Translation::Call { name, input } => {
                            counts.tools_translated += 1;
                            last_step_native.push(call.id.clone());
                            planned.calls.push(PlannedCall {
                                id: call.id.clone(),
                                name: name.to_string(),
                                source_name: call.source_name.clone(),
                                input,
                            });
                        }
                        Translation::Text { .. } => {
                            for result in session
                                .conversation()
                                .filter_map(|entry| match &entry.kind {
                                    Kind::Results { results } => Some(results),
                                    _ => None,
                                })
                                .flatten()
                                .filter(|result| result.call_id == call.id)
                            {
                                if let ResultContent::Blocks(blocks) = &result.content {
                                    counts.dropped_result_blocks += blocks
                                        .iter()
                                        .filter(|block| {
                                            block.get("type").and_then(Value::as_str)
                                                != Some("text")
                                        })
                                        .count();
                                }
                            }
                            counts.tools_text_form += 1;
                            if !counts.text_form_tools.contains(&call.source_name) {
                                counts.text_form_tools.push(call.source_name.clone());
                            }
                            planned.text.push(tools::text_form_call(call, dest));
                        }
                    }
                }
                blocks.push(PlannedBlock::Step(planned));
            }
            Kind::Results { results } => {
                let mut planned = Vec::new();
                for result in results {
                    if last_step_native.contains(&result.call_id) {
                        if matches!(dest, HarnessId::ClaudeCode | HarnessId::Codex)
                            && let ResultContent::Blocks(blocks) = &result.content
                        {
                            counts.dropped_result_blocks += blocks
                                .iter()
                                .filter(|block| {
                                    block.get("type").and_then(Value::as_str) != Some("text")
                                })
                                .count();
                        }
                        planned.push(PlannedResult::Native {
                            call_id: result.call_id.clone(),
                            content: result.content.clone(),
                            is_error: result.is_error,
                        });
                    } else if last_step_calls.contains(&result.call_id) {
                        planned.push(PlannedResult::Text(tools::text_form_result(result)));
                    } else {
                        // No call of the session answers to this id: an orphan result is dropped by
                        // policy and counted, never written as a dangling tool result.
                        counts.dropped_orphan_results += 1;
                    }
                }
                if planned.is_empty() {
                    counts.messages_in -= 1;
                    continue;
                }
                last_step_native.clear();
                last_step_calls.clear();
                blocks.push(PlannedBlock::Results { results: planned });
            }
            Kind::Compaction { summary, .. } => {
                last_step_native.clear();
                last_step_calls.clear();
                blocks.push(PlannedBlock::Compaction {
                    summary: summary.clone(),
                });
            }
            Kind::Meta { .. } => {
                counts.messages_in -= 1;
            }
        }
    }

    move_unanswered_calls_to_text(&mut blocks, &mut counts, dest);
    drop_empty_steps(&mut blocks, &mut counts);
    if matches!(dest, HarnessId::Codex | HarnessId::OpenCode) {
        for block in &blocks {
            if let PlannedBlock::User { attachments, .. } = block {
                counts.dropped_attachments +=
                    attachments.iter().filter(|a| a.raw.is_null()).count();
            }
        }
    }
    counts.messages_out = blocks.len();
    Plan { blocks, counts }
}

/// §7: every `tool_use` must be answered in the next message. A call the source never answered is
/// not written as a tool call — the provider would refuse the whole request — but it is not lost
/// either: it travels in text form, with its call id, and the plan counts it.
fn move_unanswered_calls_to_text(
    blocks: &mut [PlannedBlock],
    counts: &mut Counts,
    dest: HarnessId,
) {
    for i in 0..blocks.len() {
        let unanswered: Vec<usize> = match &blocks[i] {
            PlannedBlock::Step(step) => {
                let answered: Vec<&str> = match blocks.get(i + 1) {
                    Some(PlannedBlock::Results { results }) => results
                        .iter()
                        .filter_map(|r| match r {
                            PlannedResult::Native { call_id, .. } => Some(call_id.as_str()),
                            PlannedResult::Text(_) => None,
                        })
                        .collect(),
                    _ => Vec::new(),
                };
                step.calls
                    .iter()
                    .enumerate()
                    .filter(|(_, call)| !answered.contains(&call.id.as_str()))
                    .map(|(n, _)| n)
                    .collect()
            }
            _ => Vec::new(),
        };
        if unanswered.is_empty() {
            continue;
        }
        let mut texts = Vec::new();
        if let PlannedBlock::Step(step) = &mut blocks[i] {
            for n in unanswered.iter().rev() {
                let call = step.calls.remove(*n);
                let as_text = crate::model::ToolCall {
                    id: call.id,
                    source_name: call.source_name,
                    canonical: None,
                    input: call.input,
                };
                texts.push(tools::text_form_call(&as_text, dest));
            }
            texts.reverse();
            counts.unanswered_calls += texts.len();
            step.text.extend(texts);
        }
    }
}

/// A step that lost everything it had (reasoning-only under `Drop`, for example) is not written:
/// an assistant message with no content is invalid in every wire dialect. Its results go too.
fn drop_empty_steps(blocks: &mut Vec<PlannedBlock>, counts: &mut Counts) {
    let mut i = 0;
    while i < blocks.len() {
        let empty_calls: Option<Vec<String>> = match &blocks[i] {
            PlannedBlock::Step(step) if step.is_empty() => {
                Some(step.calls.iter().map(|c| c.id.clone()).collect())
            }
            _ => None,
        };
        let Some(call_ids) = empty_calls else {
            i += 1;
            continue;
        };
        blocks.remove(i);
        counts.dropped_empty_steps += 1;
        if let Some(PlannedBlock::Results { results }) = blocks.get_mut(i) {
            let before = results.len();
            results.retain(|r| !matches!(r, PlannedResult::Native { call_id, .. } if call_ids.contains(call_id)));
            counts.dropped_orphan_results += before - results.len();
            if results.is_empty() {
                blocks.remove(i);
            }
        }
    }
}

/// The wire protocol name a destination's records use, for reasoning provenance and for Pi's
/// `api` field.
pub fn protocol_name(protocol: &Protocol) -> &str {
    match protocol {
        Protocol::AnthropicMessages => "anthropic-messages",
        Protocol::OpenaiResponses => "openai-responses",
        Protocol::OpenaiChat => "openai-chat",
        Protocol::Other(name) => name,
    }
}

/// The shell dialect a destination runs commands with, when its configuration states it.
pub fn shell_of(name: &str) -> Option<Shell> {
    match name.to_ascii_lowercase().as_str() {
        "bash" | "sh" | "zsh" | "posix" => Some(Shell::Posix),
        "powershell" | "pwsh" => Some(Shell::PowerShell),
        _ => None,
    }
}

/// The model id a plan writes into the destination's records: the destination's own model when the
/// caller states it, otherwise the source's.
pub fn model_id(step: &PlannedStep, session: &Session, opts: &WriteOpts) -> Option<String> {
    opts.model.clone().or_else(|| {
        step.model
            .clone()
            .or_else(|| session.meta.models.first().cloned())
            .map(|m| m.id)
    })
}

/// The provider id a plan writes into the destination's records.
pub fn provider_id(step: &PlannedStep, session: &Session, opts: &WriteOpts) -> Option<String> {
    opts.model_provider.clone().or_else(|| {
        step.model
            .clone()
            .or_else(|| session.meta.models.first().cloned())
            .and_then(|m| m.provider)
    })
}
