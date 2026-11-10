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
pub use reasoning::{ChatReasoning, Opaque, Reasoning};
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
    /// The SHA-256 of the source's own bytes (the file, or the text `session export` printed), as
    /// `sha256sum` would print it: what the manifest states as the source's fingerprint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_sha256: Option<String>,
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

/// An entry that says its turn was interrupted: a step the source killed or aborted, or the abort
/// marker the source wrote (Codex `<turn_aborted>`, Claude Code `[Request interrupted by user]`).
fn interrupts(entry: &Entry) -> bool {
    matches!(
        entry.kind,
        Kind::Step {
            end: StepEnd::Killed | StepEnd::Aborted,
            ..
        } | Kind::Meta {
            kind: MetaKind::TurnAborted
        }
    )
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

    /// The last turn asked for and never finished (L9): a prompt with no step after it, or whose
    /// work was interrupted — a step the source killed or aborted, or its abort marker after the
    /// work. The manifest records it as `interrupted_last_turn`.
    pub fn interrupted_last_turn(&self) -> bool {
        let Some(start) = self
            .entries
            .iter()
            .rposition(|e| matches!(e.kind, Kind::User { .. } | Kind::Compaction { .. }))
        else {
            return false;
        };
        if !matches!(self.entries[start].kind, Kind::User { .. }) {
            return false;
        }
        let turn = &self.entries[start + 1..];
        turn.iter().any(interrupts)
            || !turn
                .iter()
                .any(|e| matches!(e.kind, Kind::Step { .. } | Kind::Results { .. }))
    }

    /// One flag per entry: whether it is work (a step, or its results) of a turn the source never
    /// finished. A turn runs from a prompt (or a compaction) to the next one; it is interrupted
    /// when it holds a step that was killed or aborted, or the source's own abort marker. The
    /// prompt itself is never work: it is the user's, and complete.
    pub fn interrupted_work(&self) -> Vec<bool> {
        let mut work = vec![false; self.entries.len()];
        let mut start = 0;
        while start < self.entries.len() {
            let end = self.entries[start + 1..]
                .iter()
                .position(|e| matches!(e.kind, Kind::User { .. } | Kind::Compaction { .. }))
                .map_or(self.entries.len(), |next| start + 1 + next);
            if self.entries[start..end].iter().any(interrupts) {
                for (flag, entry) in work[start..end].iter_mut().zip(&self.entries[start..end]) {
                    *flag = matches!(entry.kind, Kind::Step { .. } | Kind::Results { .. });
                }
            }
            start = end;
        }
        work
    }

    /// The last compaction entry, if the source compacted the conversation at all.
    pub fn last_compaction(&self) -> Option<&Entry> {
        self.entries
            .iter()
            .rev()
            .find(|e| matches!(e.kind, Kind::Compaction { .. }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, kind: Kind) -> Entry {
        Entry {
            id: id.into(),
            kind,
            raw: vec![],
        }
    }

    fn prompt(id: &str) -> Entry {
        entry(
            id,
            Kind::User {
                text: id.into(),
                attachments: Vec::new(),
            },
        )
    }

    fn step(id: &str, end: StepEnd) -> Entry {
        entry(
            id,
            Kind::Step {
                model: None,
                reasoning: Vec::new(),
                text: vec![id.into()],
                calls: Vec::new(),
                end,
            },
        )
    }

    fn results(id: &str) -> Entry {
        entry(
            id,
            Kind::Results {
                results: Vec::new(),
            },
        )
    }

    fn marker(id: &str) -> Entry {
        entry(
            id,
            Kind::Meta {
                kind: MetaKind::TurnAborted,
            },
        )
    }

    fn session(entries: Vec<Entry>) -> Session {
        Session {
            meta: Meta {
                harness: HarnessId::Pi,
                harness_version: String::new(),
                id: "s".into(),
                cwd: None,
                title: None,
                created: None,
                updated: None,
                models: Vec::new(),
                git_branch: None,
                parent: None,
                source_path: None,
                source_sha256: None,
            },
            entries,
        }
    }

    /// The work of an interrupted turn is every step and result between its prompt and the next
    /// one, whether the sign is an aborted step or the source's marker. Prompts are never work.
    #[test]
    fn the_work_of_an_interrupted_turn_is_all_its_steps_and_results() {
        let s = session(vec![
            prompt("u1"),
            step("a1", StepEnd::Completed),
            prompt("u2"),
            step("a2", StepEnd::ToolUse),
            results("r2"),
            step("a3", StepEnd::Aborted),
            prompt("u3"),
            step("a4", StepEnd::ToolUse),
            results("r4"),
            marker("x4"),
            prompt("u4"),
            step("a5", StepEnd::Killed),
        ]);
        let work: Vec<&str> = s
            .entries
            .iter()
            .zip(s.interrupted_work())
            .filter(|(_, interrupted)| *interrupted)
            .map(|(entry, _)| entry.id.as_str())
            .collect();
        assert_eq!(work, ["a2", "r2", "a3", "a4", "r4", "a5"]);
    }

    /// A turn the agent worked on and then left unfinished is an interrupted last turn too, not
    /// only one with no reply at all.
    #[test]
    fn a_last_turn_cut_after_some_work_is_interrupted() {
        let cut_after_a_tool = session(vec![
            prompt("u1"),
            step("a1", StepEnd::ToolUse),
            results("r1"),
            step("a2", StepEnd::Aborted),
        ]);
        assert!(cut_after_a_tool.interrupted_last_turn());
        let marked = session(vec![
            prompt("u1"),
            step("a1", StepEnd::ToolUse),
            results("r1"),
            marker("x1"),
        ]);
        assert!(marked.interrupted_last_turn());
        let finished = session(vec![
            prompt("u1"),
            step("a1", StepEnd::Aborted),
            prompt("u2"),
            step("a2", StepEnd::Completed),
        ]);
        assert!(!finished.interrupted_last_turn());
    }
}
