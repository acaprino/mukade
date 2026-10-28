//! The library's own session model (docs/session-interchange-spec.md §3.1).
//!
//! Three layers, as the conformance bench confirms:
//!
//! 1. the **semantic core** in [`Kind`], which every writer understands;
//! 2. the **source record kept verbatim** in [`Entry::raw`], so reading loses nothing, not even
//!    the records this version does not understand;
//! 3. the **destination's choices**, made by the writer, never stored here.
//!
//! Entries are in **native order** — OpenCode `seq`, Codex `ordinal`, Claude Code and Pi parent
//! links — never sorted by time.

pub mod harness;
pub mod kind;
pub mod reasoning;
pub mod tool;

pub use harness::{HarnessId, ModelRef, Protocol};
pub use kind::{Kind, MetaKind, StepEnd, Trigger};
pub use reasoning::{Opaque, Reasoning};
pub use tool::{Attachment, CanonicalTool, ResultContent, ToolCall, ToolResult};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Session-level facts that are not conversation: where it came from, what it is called.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Meta {
    /// The harness the session was read from (or is being written to, in a receipt).
    pub harness: HarnessId,
    /// That harness's own version string, as it stamps its records.
    pub harness_version: String,
    /// The harness's native session id. A hop always writes a *new* one.
    pub id: String,
    /// The project directory the session is filed under.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated: Option<String>,
    /// Every model the source used, in first-seen order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<ModelRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_branch: Option<String>,
    /// The session this one was forked from; deliberately *not* carried into a hop.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Where the session was read from, for the receipt and the manifest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
}

/// One conversation, ready to be written somewhere else.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub meta: Meta,
    pub entries: Vec<Entry>,
}

/// One unit of the conversation, with the source record(s) it was read from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// The native identity of the entry (the source record's own id where it has one). Unique
    /// within the session, so a [`Kind::Compaction`] can point at the entry it kept.
    pub id: String,
    pub kind: Kind,
    /// The source record(s) this entry was read from, verbatim, in native order.
    pub raw: Vec<Value>,
}

impl Session {
    /// The entries that are conversation, in order: everything the destination should receive.
    pub fn conversation(&self) -> impl Iterator<Item = &Entry> {
        self.entries
            .iter()
            .filter(|e| !matches!(e.kind, Kind::Meta { .. }))
    }

    pub fn entry(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// The last turn asked for but never answered: a user entry with no step after it, or only
    /// steps that were killed or aborted before they answered. The manifest records it as
    /// `interrupted_last_turn` (L9).
    pub fn interrupted_last_turn(&self) -> bool {
        let unanswered = |e: &&Entry| {
            matches!(
                e.kind,
                Kind::Step {
                    end: StepEnd::Killed | StepEnd::Aborted,
                    ..
                }
            )
        };
        matches!(
            self.entries
                .iter()
                .rev()
                .filter(|e| !matches!(e.kind, Kind::Meta { .. }))
                .find(|e| !unanswered(e)),
            Some(Entry {
                kind: Kind::User { .. },
                ..
            })
        )
    }

    /// The last compaction entry, if the source compacted the conversation at all.
    pub fn last_compaction(&self) -> Option<&Entry> {
        self.entries
            .iter()
            .rev()
            .find(|e| matches!(e.kind, Kind::Compaction { .. }))
    }
}
