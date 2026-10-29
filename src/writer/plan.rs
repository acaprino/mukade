//! The rendering plan: what a [`Session`] becomes for one destination.
//!
//! This is the only place that applies the tool tables (L11–L14) and the reasoning policy, so all
//! four writers and the CLI's document projection agree on what is carried, what becomes text and
//! what is dropped. A writer's job is then only to lay the plan out as native records.

use crate::model::{
    Attachment, HarnessId, Kind, MetaKind, ModelRef, Protocol, ResultContent, Session, StepEnd,
};
use crate::policy::{ReasoningDrop, ReasoningPlan, payload_fits, plan_reasoning};
use crate::tools::{self, Shell, Translation};
use crate::writer::{WriteOpts, media};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

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
    /// The input as the source recorded it, for a call that ends up in text form after all.
    pub source_input: Value,
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
    /// Tool calls deliberately omitted with their results.
    pub tools_omitted: usize,
    pub tools_text_form: usize,
    pub text_form_tools: Vec<String>,
    /// Reasoning blocks not carried, in all.
    pub reasoning_dropped: usize,
    /// The same blocks by reason, in the order the reasons first came up.
    pub reasoning_dropped_why: Vec<(ReasoningDrop, usize)>,
    /// Reasoning blocks written as labelled text (`--reasoning as_text`): readable by the
    /// destination's model, but no longer reasoning its provider can replay.
    pub reasoning_as_text: usize,
    /// A result whose call is nowhere in the source.
    pub dropped_orphan_results: usize,
    /// A second answer to a call already answered: the provider refuses a call answered twice (A3).
    pub dropped_duplicate_results: usize,
    /// Tool-call ids the destination's protocol refuses, rewritten for the call and its result.
    pub rewritten_call_ids: usize,
    /// Source records of a branch the conversation left (a rewind, an edited prompt), not written.
    pub dropped_off_branch: usize,
    /// A last source line cut off mid-write, kept as text and not read.
    pub dropped_unfinished_lines: usize,
    /// Steps the source aborted or that failed, written as ordinary replies by a destination with
    /// no way to mark them (Pi keeps the step's end).
    pub aborted_steps_as_replies: usize,
    /// A call the source never answered: the provider would refuse it as a tool call (L7).
    pub unanswered_calls: usize,
    /// A step that lost everything it had and cannot be written as an empty message.
    pub dropped_empty_steps: usize,
    /// User attachments that could not be carried by a destination.
    pub dropped_attachments: usize,
    /// Structured blocks flattened by a destination that only accepts text results.
    pub dropped_result_blocks: usize,
    /// Compaction summaries with no readable text (encrypted by the source's provider).
    pub dropped_unreadable_summaries: usize,
}

impl Counts {
    /// Count one reasoning block not carried, under its reason.
    fn drop_reasoning(&mut self, why: ReasoningDrop) {
        self.reasoning_dropped += 1;
        match self
            .reasoning_dropped_why
            .iter_mut()
            .find(|(seen, _)| *seen == why)
        {
            Some((_, n)) => *n += 1,
            None => self.reasoning_dropped_why.push((why, 1)),
        }
    }
}

/// A whole session, planned.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub blocks: Vec<PlannedBlock>,
    pub counts: Counts,
}

/// Tool-call ids as the destination's protocol accepts them. Anthropic refuses an id outside
/// `^[a-zA-Z0-9_-]+$`, and Pi on `openai-responses` records ids such as `call_x|fc_y`: such an id
/// is rewritten once, the same way for the call and for its result, and counted.
struct CallIds {
    rewrite: bool,
    given: HashMap<String, String>,
    used: HashSet<String>,
}

impl CallIds {
    fn new(rewrite: bool) -> Self {
        CallIds {
            rewrite,
            given: HashMap::new(),
            used: HashSet::new(),
        }
    }

    fn of(&mut self, id: &str, counts: &mut Counts) -> String {
        if !self.rewrite {
            return id.to_string();
        }
        if let Some(given) = self.given.get(id) {
            return given.clone();
        }
        let base: String = id
            .chars()
            .map(|c| if anthropic_id_char(c) { c } else { '_' })
            .collect();
        let base = if base.is_empty() {
            "toolu_mukade".to_string()
        } else {
            base
        };
        let mut candidate = base.clone();
        let mut n = 1;
        while self.used.contains(&candidate) {
            n += 1;
            candidate = format!("{base}_{n}");
        }
        if candidate != id {
            counts.rewritten_call_ids += 1;
        }
        self.used.insert(candidate.clone());
        self.given.insert(id.to_string(), candidate.clone());
        candidate
    }
}

/// A character Anthropic accepts in a tool-call id.
pub fn anthropic_id_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

/// A user turn's attachments as the destination holds them: the source's own records when the
/// destination is the source's harness, an image in the destination's own shape otherwise. What no
/// shape fits (a PDF, a record a document carried without its block) is dropped and counted.
fn carry_attachments(
    attachments: &[Attachment],
    source: HarnessId,
    dest: HarnessId,
    counts: &mut Counts,
) -> Vec<Attachment> {
    let mut carried = Vec::new();
    for attachment in attachments {
        let raw = if attachment.raw.is_null() {
            None
        } else if source == dest {
            Some(attachment.raw.clone())
        } else {
            media::image_of(&attachment.raw).map(|image| media::user_block(&image, dest))
        };
        match raw {
            Some(raw) => carried.push(Attachment {
                raw,
                ..attachment.clone()
            }),
            None => counts.dropped_attachments += 1,
        }
    }
    carried
}

/// A result's structured blocks as a destination that keeps them (Pi, OpenCode) holds them: an
/// image in its own shape, text as text, and anything else dropped and counted. Claude Code and
/// Codex results are flattened to text by their writers, counted where the plan takes them.
fn carry_result_content(
    content: &ResultContent,
    source: HarnessId,
    dest: HarnessId,
    counts: &mut Counts,
) -> ResultContent {
    let ResultContent::Blocks(blocks) = content else {
        return content.clone();
    };
    if source == dest || matches!(dest, HarnessId::ClaudeCode | HarnessId::Codex) {
        return content.clone();
    }
    let mut carried = Vec::new();
    for block in blocks {
        if block.get("type").and_then(Value::as_str) == Some("text") {
            carried.push(block.clone());
        } else if let Some(image) = media::image_of(block) {
            carried.push(media::result_block(&image, dest));
        } else {
            counts.dropped_result_blocks += 1;
        }
    }
    ResultContent::Blocks(carried)
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
    let source_records = |of: MetaKind| {
        session
            .entries
            .iter()
            .filter(|entry| matches!(&entry.kind, Kind::Meta { kind } if *kind == of))
            .count()
    };
    let mut counts = Counts {
        dropped_off_branch: source_records(MetaKind::OffBranch),
        dropped_unfinished_lines: source_records(MetaKind::Unfinished),
        ..Counts::default()
    };
    // The calls of the step just planned: which of them the destination took natively, and which
    // of them exist at all (a result whose call is nowhere is an orphan and is dropped).
    let mut last_step_native: Vec<String> = Vec::new();
    let mut last_step_calls: Vec<String> = Vec::new();
    // The calls of that step already answered, across its result groups.
    let mut step_answered: Vec<String> = Vec::new();
    let source = session.meta.harness;
    let mut ids = CallIds::new(dest_protocol.as_ref() == Some(&Protocol::AnthropicMessages));

    // The structured (non-text) result blocks of each call, counted once: a call that travels as
    // text loses them, and looking them up per call would rescan every result.
    let mut structured: HashMap<&str, usize> = HashMap::new();
    for entry in session.conversation() {
        if let Kind::Results { results } = &entry.kind {
            for result in results {
                if let ResultContent::Blocks(blocks) = &result.content {
                    let n = blocks
                        .iter()
                        .filter(|block| block.get("type").and_then(Value::as_str) != Some("text"))
                        .count();
                    if n > 0 {
                        *structured.entry(result.call_id.as_str()).or_default() += n;
                    }
                }
            }
        }
    }

    for entry in session.conversation() {
        counts.messages_in += 1;
        match &entry.kind {
            Kind::User { text, attachments } => {
                // A user turn ends the previous step's pair of call lists: a stray result after it
                // must not be matched against calls that belong to an earlier step.
                last_step_native.clear();
                last_step_calls.clear();
                step_answered.clear();
                blocks.push(PlannedBlock::User {
                    text: text.clone(),
                    attachments: carry_attachments(attachments, source, dest, &mut counts),
                });
            }
            Kind::Step {
                model,
                reasoning,
                text,
                calls,
                end,
            } => {
                if matches!(end, StepEnd::Aborted | StepEnd::Error) && dest != HarnessId::Pi {
                    counts.aborted_steps_as_replies += 1;
                }
                let mut planned = PlannedStep {
                    model: model.clone(),
                    reasoning: Vec::new(),
                    text: text.clone(),
                    calls: Vec::new(),
                    end: end.clone(),
                };
                // Reasoning kept as text goes before the answer it led to.
                let mut lead = 0;
                for r in reasoning {
                    match plan_reasoning(r, dest_protocol.as_ref(), opts.reasoning) {
                        // A4: the payload must have a place in the destination's own format, or
                        // the block would be written without it and refused.
                        ReasoningPlan::Native(native)
                            if native
                                .opaque
                                .as_ref()
                                .is_some_and(|opaque| payload_fits(opaque, dest)) =>
                        {
                            planned.reasoning.push(ReasoningPlan::Native(native))
                        }
                        ReasoningPlan::Text(t) => {
                            counts.reasoning_as_text += 1;
                            planned.reasoning.push(ReasoningPlan::Text(t.clone()));
                            planned.text.insert(lead, t);
                            lead += 1;
                        }
                        dropped => {
                            let why = match dropped {
                                ReasoningPlan::Drop(why) => why,
                                _ => ReasoningDrop::NoPlaceInFormat,
                            };
                            counts.drop_reasoning(why);
                            planned.reasoning.push(ReasoningPlan::Drop(why));
                        }
                    }
                }
                last_step_calls.clear();
                last_step_native.clear();
                step_answered.clear();
                for call in calls {
                    if opts.without_tools {
                        counts.tools_omitted += 1;
                        continue;
                    }
                    last_step_calls.push(call.id.clone());
                    match tools::translate_with(call, dest, opts.dest_shell, offered) {
                        Translation::Call { name, input } => {
                            counts.tools_translated += 1;
                            last_step_native.push(call.id.clone());
                            planned.calls.push(PlannedCall {
                                id: ids.of(&call.id, &mut counts),
                                name: name.to_string(),
                                source_name: call.source_name.clone(),
                                input,
                                source_input: call.input.clone(),
                            });
                        }
                        Translation::Text { .. } => {
                            counts.dropped_result_blocks +=
                                structured.get(call.id.as_str()).copied().unwrap_or(0);
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
                if opts.without_tools {
                    continue;
                }
                let mut planned = Vec::new();
                for result in results {
                    // A3: the first answer to a call is the one that travels.
                    if step_answered.contains(&result.call_id) {
                        counts.dropped_duplicate_results += 1;
                        continue;
                    }
                    step_answered.push(result.call_id.clone());
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
                            call_id: ids.of(&result.call_id, &mut counts),
                            content: carry_result_content(
                                &result.content,
                                source,
                                dest,
                                &mut counts,
                            ),
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
                    continue;
                }
                // One step's results may come in several groups (a document with one result per
                // message, an internal row between them): they answer the same step, as one.
                let results = match blocks.last_mut() {
                    Some(PlannedBlock::Results { results }) => results,
                    _ => {
                        blocks.push(PlannedBlock::Results {
                            results: Vec::new(),
                        });
                        let Some(PlannedBlock::Results { results }) = blocks.last_mut() else {
                            unreachable!("just pushed")
                        };
                        results
                    }
                };
                results.extend(planned);
                // §6.2 [V]: text between tool results ends the tool flow for Pi, which then answers
                // the later calls itself and is refused; the native results go first.
                results.sort_by_key(|result| matches!(result, PlannedResult::Text(_)));
            }
            Kind::Compaction { summary, .. } => {
                last_step_native.clear();
                last_step_calls.clear();
                step_answered.clear();
                // A summary with no readable text (Codex 0.165 keeps it encrypted) would be an
                // empty user message, which a provider may refuse: it is left out and counted.
                if summary.trim().is_empty() {
                    counts.dropped_unreadable_summaries += 1;
                } else {
                    blocks.push(PlannedBlock::Compaction {
                        summary: summary.clone(),
                    });
                }
            }
            Kind::Meta { .. } => {
                counts.messages_in -= 1;
            }
        }
    }

    move_unanswered_calls_to_text(&mut blocks, &mut counts);
    drop_empty_steps(&mut blocks, &mut counts);
    counts.messages_out = blocks.len();
    Plan { blocks, counts }
}

/// §7: every `tool_use` must be answered in the next message. A call the source never answered is
/// not written as a tool call — the provider would refuse the whole request — but it is not lost
/// either: it travels in text form, with its call id, and the plan counts it.
fn move_unanswered_calls_to_text(blocks: &mut [PlannedBlock], counts: &mut Counts) {
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
                    input: call.source_input,
                };
                texts.push(tools::text_form_unanswered(&as_text));
                counts.tools_translated -= 1;
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
