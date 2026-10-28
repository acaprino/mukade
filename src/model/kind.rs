//! What an entry is: conversation the writers must carry, or a marker they must not.

use super::harness::ModelRef;
use super::reasoning::Reasoning;
use super::tool::{Attachment, ToolCall, ToolResult};
use serde::{Deserialize, Serialize};

/// How a model step ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepEnd {
    /// The model finished its answer.
    Completed,
    /// The model stopped to call tools; a [`Kind::Results`] entry should follow.
    ToolUse,
    /// The turn was aborted (Codex `<turn_aborted>`, Pi `stopReason:"aborted"`).
    Aborted,
    /// The turn failed.
    Error,
    /// The harness was killed mid-turn; only a partial step (or nothing) was written.
    Killed,
    /// The source recorded an end this version does not classify.
    Unknown,
}

/// What triggered a native compaction.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    Manual,
    Auto,
    #[default]
    Unknown,
}

/// Classes of non-conversation records. They are kept in `raw`, never written as conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetaKind {
    /// Slash-command rows and their output (`<command-name>`, `<local-command-*>`).
    Command,
    /// A reply the harness synthesized rather than received (Claude "No response requested.").
    Synthetic,
    /// An abort marker the source wrote into the conversation (`<turn_aborted>`).
    TurnAborted,
    /// A turn-end marker (`idle`, `task_complete`, …).
    Idle,
    /// A model or thinking-level change row.
    ModelChange,
    /// A stored system prompt or environment block.
    SystemPrompt,
    /// A file re-injected into the transcript.
    Attachment,
    /// Any other harness-internal record (requests, cost, telemetry, queues).
    Internal,
    /// A record of a branch the conversation left (a rewind, an edited prompt): kept, but the
    /// harness does not resume it.
    OffBranch,
}

/// One unit of the interchange model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// A user turn: the text the user typed, plus anything attached to it.
    User {
        text: String,
        attachments: Vec<Attachment>,
    },
    /// One model response. Parallel tool calls of one response stay together in `calls`.
    Step {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<ModelRef>,
        reasoning: Vec<Reasoning>,
        text: Vec<String>,
        calls: Vec<ToolCall>,
        end: StepEnd,
    },
    /// Answers to the previous step's calls, by id.
    Results { results: Vec<ToolResult> },
    /// A native compaction record: the source's own summary and the boundary it kept.
    Compaction {
        summary: String,
        /// The entry id the retained tail starts from, when the source states it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kept_from: Option<String>,
        trigger: Trigger,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tokens_before: Option<u64>,
    },
    /// Not conversation. Never written into a destination as conversation.
    Meta { kind: MetaKind },
}
