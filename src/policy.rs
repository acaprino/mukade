//! The hop policy: what is carried, what is dropped, and what is written down as lost
//! (docs/session-interchange-spec.md §3.3, §4, §7).

use crate::model::{HarnessId, Opaque, Protocol, Reasoning};
use chrono::{DateTime, Duration, SecondsFormat, Utc};

/// The label `AsText` puts in front of reasoning the destination cannot replay.
pub const REASONING_AS_TEXT_LABEL: &str = "[earlier reasoning, not replayable]";

/// What to do with reasoning blocks (`HopRequest.reasoning`, §3.3). The default is `DropForeign`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReasoningPolicy {
    /// Keep a block only when the destination speaks the same provider protocol **and** the opaque
    /// payload is present unmodified; otherwise drop it and count it.
    #[default]
    DropForeign,
    /// Keep it as visible, labelled text.
    AsText,
    /// Never carry reasoning.
    Drop,
}

/// The wire protocol a destination speaks by default. `None` means the destination's provider is
/// not known, and a policy that needs to compare protocols must then drop the opaque payload.
pub fn destination_protocol(harness: HarnessId) -> Option<Protocol> {
    match harness {
        HarnessId::ClaudeCode => Some(Protocol::AnthropicMessages),
        HarnessId::Codex => Some(Protocol::OpenaiResponses),
        // OpenCode and Pi speak whatever their configured provider speaks.
        HarnessId::OpenCode | HarnessId::Pi => None,
    }
}

/// Whether a destination's own format has a place for this opaque payload. Claude writes a
/// signature as `thinking` and an encrypted payload as `redacted_thinking`; Pi replays a
/// `thinkingSignature`, and an Anthropic encrypted payload as its own `redacted: true` block;
/// Codex only `encrypted_content`; OpenCode keeps both.
pub fn payload_fits(opaque: &Opaque, dest: HarnessId) -> bool {
    match dest {
        HarnessId::ClaudeCode | HarnessId::OpenCode => {
            opaque.signature.is_some() || opaque.encrypted.is_some()
        }
        HarnessId::Pi => {
            opaque.signature.is_some()
                || (opaque.encrypted.is_some() && opaque.protocol == Protocol::AnthropicMessages)
        }
        HarnessId::Codex => opaque.encrypted.is_some(),
    }
}

/// What becomes of one reasoning block.
#[derive(Debug, Clone, PartialEq)]
pub enum ReasoningPlan {
    /// Carried with its opaque payload, which must not be edited.
    Native(Reasoning),
    /// Carried as visible text, labelled.
    Text(String),
    /// Not carried at all, for this reason.
    Drop(ReasoningDrop),
}

/// Why a reasoning block was not carried: the manifest says it, so the reader knows what would
/// have kept it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReasoningDrop {
    /// `--reasoning drop`.
    Requested,
    /// The block has neither a signature nor an encrypted payload to be replayed with.
    NoPayload,
    /// The destination's provider protocol is not known (OpenCode, Pi), so no protocol matches.
    UnknownProtocol,
    /// The block was made by another provider protocol than the destination's.
    ForeignProtocol,
    /// Same protocol, but the destination's format has no place for this payload (§3.3, A4).
    NoPlaceInFormat,
    /// `--reasoning as_text` on a block that has no visible text (an encrypted payload only).
    NoVisibleText,
}

impl ReasoningDrop {
    /// The reason as a loss line says it.
    pub fn describe(self) -> &'static str {
        match self {
            ReasoningDrop::Requested => "not carried, as --reasoning drop asks",
            ReasoningDrop::NoPayload => "no signature or encrypted payload to replay it with",
            ReasoningDrop::UnknownProtocol => {
                "the destination's provider protocol is not known; name it with --dest-protocol"
            }
            ReasoningDrop::ForeignProtocol => "made by another provider protocol",
            ReasoningDrop::NoPlaceInFormat => {
                "the destination's format has no place for its payload"
            }
            ReasoningDrop::NoVisibleText => "encrypted only, no visible text to write as text",
        }
    }
}

/// Apply the policy to one reasoning block.
pub fn plan_reasoning(
    reasoning: &Reasoning,
    dest: Option<&Protocol>,
    policy: ReasoningPolicy,
) -> ReasoningPlan {
    match policy {
        ReasoningPolicy::Drop => ReasoningPlan::Drop(ReasoningDrop::Requested),
        ReasoningPolicy::AsText => {
            if reasoning.text.is_empty() {
                ReasoningPlan::Drop(ReasoningDrop::NoVisibleText)
            } else {
                ReasoningPlan::Text(format!("{REASONING_AS_TEXT_LABEL}\n{}", reasoning.text))
            }
        }
        ReasoningPolicy::DropForeign => match (&reasoning.opaque, dest) {
            (Some(opaque), _) if opaque.signature.is_none() && opaque.encrypted.is_none() => {
                ReasoningPlan::Drop(ReasoningDrop::NoPayload)
            }
            (None, _) => ReasoningPlan::Drop(ReasoningDrop::NoPayload),
            (Some(_), None) => ReasoningPlan::Drop(ReasoningDrop::UnknownProtocol),
            (Some(opaque), Some(dest)) if opaque.protocol == *dest => {
                ReasoningPlan::Native(reasoning.clone())
            }
            (Some(_), Some(_)) => ReasoningPlan::Drop(ReasoningDrop::ForeignProtocol),
        },
    }
}

/// Timestamps for a written session: one source of truth, so the rows a writer emits are ordered
/// and reproducible when the caller fixes the base.
#[derive(Debug, Clone)]
pub struct Clock {
    base: DateTime<Utc>,
    step: i64,
}

impl Clock {
    /// A clock that starts at `base` (an RFC 3339 string the caller chose) and advances 1 ms per
    /// call. An unparsable base falls back to now.
    pub fn new(base: Option<&str>) -> Clock {
        let base = base
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);
        Clock { base, step: 0 }
    }

    /// The next timestamp, ISO 8601 with milliseconds and `Z`, the shape all four harnesses use.
    pub fn next_iso(&mut self) -> String {
        self.step += 1;
        (self.base + Duration::milliseconds(self.step)).to_rfc3339_opts(SecondsFormat::Millis, true)
    }

    /// The same instant as epoch milliseconds (OpenCode's own unit).
    pub fn next_millis(&mut self) -> i64 {
        self.step += 1;
        (self.base + Duration::milliseconds(self.step)).timestamp_millis()
    }

    pub fn base_millis(&self) -> i64 {
        self.base.timestamp_millis()
    }
}

/// A fresh session id: a hop always writes a new session (§1.3 rule 1).
pub fn new_session_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Opaque;

    fn reasoning(protocol: Protocol, signature: Option<&str>) -> Reasoning {
        Reasoning {
            text: "why".into(),
            opaque: Some(Opaque {
                protocol,
                model: Some("m".into()),
                signature: signature.map(str::to_owned),
                encrypted: None,
            }),
        }
    }

    #[test]
    fn drop_foreign_keeps_only_the_same_protocol_with_its_payload() {
        let anthropic = Protocol::AnthropicMessages;
        let responses = Protocol::OpenaiResponses;
        let keep = plan_reasoning(
            &reasoning(anthropic.clone(), Some("sig")),
            Some(&anthropic),
            ReasoningPolicy::DropForeign,
        );
        assert!(matches!(keep, ReasoningPlan::Native(_)));
        let drop = plan_reasoning(
            &reasoning(responses, Some("sig")),
            Some(&anthropic),
            ReasoningPolicy::DropForeign,
        );
        assert_eq!(drop, ReasoningPlan::Drop(ReasoningDrop::ForeignProtocol));
        let unsigned = plan_reasoning(
            &reasoning(anthropic.clone(), None),
            Some(&anthropic),
            ReasoningPolicy::DropForeign,
        );
        assert_eq!(unsigned, ReasoningPlan::Drop(ReasoningDrop::NoPayload));
        let unknown = plan_reasoning(
            &reasoning(anthropic, Some("sig")),
            None,
            ReasoningPolicy::DropForeign,
        );
        assert_eq!(unknown, ReasoningPlan::Drop(ReasoningDrop::UnknownProtocol));
    }

    /// The reason a block is dropped tells the user what to do: the live run of 2026-10-28 into Pi
    /// dropped a block that had its signature, because Pi's protocol was not named.
    #[test]
    fn each_reason_says_what_happened() {
        assert!(
            ReasoningDrop::UnknownProtocol
                .describe()
                .contains("--dest-protocol")
        );
        assert!(
            ReasoningDrop::Requested
                .describe()
                .contains("--reasoning drop")
        );
        let all = [
            ReasoningDrop::Requested,
            ReasoningDrop::NoPayload,
            ReasoningDrop::UnknownProtocol,
            ReasoningDrop::ForeignProtocol,
            ReasoningDrop::NoPlaceInFormat,
            ReasoningDrop::NoVisibleText,
        ];
        let mut texts: Vec<&str> = all.iter().map(|why| why.describe()).collect();
        texts.sort_unstable();
        texts.dedup();
        assert_eq!(texts.len(), all.len(), "each reason reads differently");
    }

    #[test]
    fn as_text_labels_and_drop_never_carries() {
        let r = reasoning(Protocol::AnthropicMessages, Some("sig"));
        let text = plan_reasoning(&r, None, ReasoningPolicy::AsText);
        assert_eq!(
            text,
            ReasoningPlan::Text(format!("{REASONING_AS_TEXT_LABEL}\nwhy"))
        );
        assert_eq!(
            plan_reasoning(&r, None, ReasoningPolicy::Drop),
            ReasoningPlan::Drop(ReasoningDrop::Requested)
        );
        let encrypted_only = Reasoning {
            text: String::new(),
            ..r
        };
        assert_eq!(
            plan_reasoning(&encrypted_only, None, ReasoningPolicy::AsText),
            ReasoningPlan::Drop(ReasoningDrop::NoVisibleText)
        );
    }
}
