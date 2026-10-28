//! Reasoning: the visible text, and the opaque part that only its own provider can re-read.

use super::harness::Protocol;
use serde::{Deserialize, Serialize};

/// The part of a reasoning block that is signed or encrypted by the provider that produced it.
///
/// All four native stores keep it (Claude `signature`, Codex `encrypted_content`, OpenCode
/// `native.signature`, Pi `thinkingSignature`) and all four send it back on their own resume.
/// It must never be synthesized, edited or concatenated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Opaque {
    /// The protocol that produced the payload, which decides who may receive it unmodified.
    pub protocol: Protocol,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// A signature over the reasoning text (Anthropic thinking, OpenCode, Pi).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// A provider-encrypted payload carried instead of the plain text (Codex, Anthropic
    /// `redacted_thinking`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encrypted: Option<String>,
}

/// One reasoning block of a model response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reasoning {
    /// The reasoning text the model produced (may be empty when the provider only sent an
    /// encrypted payload).
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opaque: Option<Opaque>,
}
