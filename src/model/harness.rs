//! Which harness a session belongs to, and which wire protocol a model call used.

use serde::{Deserialize, Serialize};

/// The four harnesses the library reads and writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessId {
    ClaudeCode,
    Codex,
    #[serde(rename = "opencode")]
    OpenCode,
    Pi,
}

impl HarnessId {
    pub const ALL: [HarnessId; 4] = [Self::ClaudeCode, Self::Codex, Self::OpenCode, Self::Pi];

    /// The name used on the command line, in the manifest and in paths.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude_code",
            Self::Codex => "codex",
            Self::OpenCode => "opencode",
            Self::Pi => "pi",
        }
    }
}

impl std::fmt::Display for HarnessId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for HarnessId {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> anyhow::Result<Self> {
        Self::ALL
            .into_iter()
            .find(|h| h.as_str() == s)
            .ok_or_else(|| {
                anyhow::anyhow!("unknown harness {s:?} (claude_code, codex, opencode, pi)")
            })
    }
}

/// The wire protocol a model call used. It decides whether an opaque reasoning payload can be
/// replayed: only a destination speaking the same protocol may receive it unmodified (§3.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Protocol {
    AnthropicMessages,
    OpenaiResponses,
    OpenaiChat,
    Other(String),
}

impl Protocol {
    /// The protocol a harness names through its provider id or its `api` field. Unknown providers
    /// stay unknown rather than being guessed at: an unknown protocol drops opaque reasoning.
    pub fn from_provider(provider: &str) -> Protocol {
        match provider.to_ascii_lowercase().as_str() {
            "anthropic" | "anthropic-messages" | "claude" => Protocol::AnthropicMessages,
            "openai" | "openai-responses" | "responses" | "codex" => Protocol::OpenaiResponses,
            "openai-chat" | "chat" | "chat-completions" | "openai-completions" => {
                Protocol::OpenaiChat
            }
            other => Protocol::Other(other.to_string()),
        }
    }
}

/// A model, as a harness names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelRef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// The model id as the harness recorded it.
    pub id: String,
}
