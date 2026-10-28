//! Tool calls, their results, and the canonical (Claude-named) form a reader maps them to.
//!
//! Translation policy: [docs/tool-conversion-tables.md]. Canonical first: the reader maps every
//! call to its canonical form and keeps the source name and the source input; the writer maps
//! canonical → destination.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The canonical tool vocabulary, named after Claude Code's tools. A call that has no equivalent
/// in the canonical vocabulary keeps its own name in [`CanonicalTool::Other`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CanonicalTool {
    Bash,
    PowerShell,
    Read,
    Write,
    Edit,
    MultiEdit,
    Grep,
    Glob,
    Ls,
    WebFetch,
    WebSearch,
    Agent,
    Skill,
    AskUserQuestion,
    Mcp { server: String, tool: String },
    Other(String),
}

impl CanonicalTool {
    /// The canonical form of a Claude Code tool name.
    pub fn from_claude(name: &str) -> CanonicalTool {
        if let Some(rest) = name.strip_prefix("mcp__")
            && let Some((server, tool)) = rest.split_once("__")
        {
            return CanonicalTool::Mcp {
                server: server.to_string(),
                tool: tool.to_string(),
            };
        }
        match name {
            "Bash" => CanonicalTool::Bash,
            "PowerShell" => CanonicalTool::PowerShell,
            "Read" => CanonicalTool::Read,
            "Write" => CanonicalTool::Write,
            "Edit" => CanonicalTool::Edit,
            "MultiEdit" => CanonicalTool::MultiEdit,
            "Grep" => CanonicalTool::Grep,
            "Glob" => CanonicalTool::Glob,
            "LS" | "List" => CanonicalTool::Ls,
            "WebFetch" => CanonicalTool::WebFetch,
            "WebSearch" => CanonicalTool::WebSearch,
            "Agent" | "Task" => CanonicalTool::Agent,
            "Skill" => CanonicalTool::Skill,
            "AskUserQuestion" => CanonicalTool::AskUserQuestion,
            other => CanonicalTool::Other(other.to_string()),
        }
    }

    /// The canonical form of an OpenCode tool name (§2 of docs/tool-conversion-tables.md).
    pub fn from_opencode(name: &str) -> CanonicalTool {
        match name {
            "read" => CanonicalTool::Read,
            "write" => CanonicalTool::Write,
            "edit" => CanonicalTool::Edit,
            "shell" => CanonicalTool::Bash,
            "grep" => CanonicalTool::Grep,
            "glob" => CanonicalTool::Glob,
            "webfetch" => CanonicalTool::WebFetch,
            "websearch" => CanonicalTool::WebSearch,
            "question" => CanonicalTool::AskUserQuestion,
            "skill" => CanonicalTool::Skill,
            "subagent" => CanonicalTool::Agent,
            other => CanonicalTool::Other(other.to_string()),
        }
    }

    /// The canonical form of a Pi tool name. Pi offers `grep`, `find` and `ls` only when the
    /// destination enables them, which is the writer's problem, not the reader's.
    pub fn from_pi(name: &str) -> CanonicalTool {
        match name {
            "read" => CanonicalTool::Read,
            "write" => CanonicalTool::Write,
            "edit" => CanonicalTool::Edit,
            "bash" => CanonicalTool::Bash,
            "grep" => CanonicalTool::Grep,
            "find" => CanonicalTool::Glob,
            "ls" => CanonicalTool::Ls,
            "subagent" => CanonicalTool::Agent,
            other => CanonicalTool::Other(other.to_string()),
        }
    }

    /// The canonical form of a Codex tool name. Codex Desktop's `exec` code-mode script is not
    /// read yet (spec L10 is open, §10): it stays one opaque call, and travels in text form.
    pub fn from_codex(name: &str) -> CanonicalTool {
        match name {
            "exec_command" | "local_shell" => CanonicalTool::Bash,
            "apply_patch" => CanonicalTool::Edit,
            "web_search" => CanonicalTool::WebSearch,
            "request_user_input" => CanonicalTool::AskUserQuestion,
            other => CanonicalTool::Other(other.to_string()),
        }
    }
}

/// One tool call the model made.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    /// The source's tool call id; it is kept across the hop so results stay paired.
    pub id: String,
    /// The name the source harness used.
    pub source_name: String,
    /// The canonical form, when the reader could establish one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical: Option<CanonicalTool>,
    /// The call's arguments, exactly as the source recorded them.
    pub input: Value,
}

/// What a tool call returned.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultContent {
    /// The result text the model saw, verbatim.
    Text(String),
    /// A structured result (image blocks and the like), kept as recorded.
    Blocks(Vec<Value>),
}

impl ResultContent {
    /// The result as plain text, for the text form of a non-translatable call.
    pub fn text(&self) -> String {
        match self {
            ResultContent::Text(t) => t.clone(),
            ResultContent::Blocks(blocks) => blocks
                .iter()
                .filter_map(|b| b.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }
}

/// One tool result. Results are never rewritten: the text the model saw travels verbatim.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolResult {
    /// The [`ToolCall::id`] this answers.
    pub call_id: String,
    pub content: ResultContent,
    pub is_error: bool,
    /// Whether the source itself truncated the result before storing it.
    #[serde(default)]
    pub truncated_by_source: bool,
}

/// A file or image a user turn carried. The readers keep what the source recorded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attachment {
    /// The source's own kind for it (Claude `file`, Pi image blocks, …).
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// The record verbatim, for a writer that understands this kind.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub raw: Value,
}
