//! The hop policy: what is carried, what is dropped, and what is written down as lost
//! (docs/session-interchange-spec.md §3.3, §4, §7).

use crate::model::{HarnessId, Protocol, Reasoning};
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

/// What becomes of one reasoning block.
#[derive(Debug, Clone, PartialEq)]
pub enum ReasoningPlan {
    /// Carried with its opaque payload, which must not be edited.
    Native(Reasoning),
    /// Carried as visible text, labelled.
    Text(String),
    /// Not carried at all.
    Drop,
}

/// Apply the policy to one reasoning block.
pub fn plan_reasoning(reasoning: &Reasoning, dest: Option<&Protocol>, policy: ReasoningPolicy) -> ReasoningPlan {
    match policy {
        ReasoningPolicy::Drop => ReasoningPlan::Drop,
        ReasoningPolicy::AsText => {
            if reasoning.text.is_empty() {
                ReasoningPlan::Drop
            } else {
                ReasoningPlan::Text(format!("{REASONING_AS_TEXT_LABEL}\n{}", reasoning.text))
            }
        }
        ReasoningPolicy::DropForeign => match (&reasoning.opaque, dest) {
            (Some(opaque), Some(dest))
                if opaque.protocol == *dest && (opaque.signature.is_some() || opaque.encrypted.is_some()) =>
            {
                ReasoningPlan::Native(reasoning.clone())
            }
            _ => ReasoningPlan::Drop,
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
        let keep = plan_reasoning(&reasoning(anthropic.clone(), Some("sig")), Some(&anthropic), ReasoningPolicy::DropForeign);
        assert!(matches!(keep, ReasoningPlan::Native(_)));
        let drop = plan_reasoning(&reasoning(responses, Some("sig")), Some(&anthropic), ReasoningPolicy::DropForeign);
        assert_eq!(drop, ReasoningPlan::Drop);
        let unsigned = plan_reasoning(&reasoning(anthropic.clone(), None), Some(&anthropic), ReasoningPolicy::DropForeign);
        assert_eq!(unsigned, ReasoningPlan::Drop);
        let unknown = plan_reasoning(&reasoning(anthropic, Some("sig")), None, ReasoningPolicy::DropForeign);
        assert_eq!(unknown, ReasoningPlan::Drop);
    }

    #[test]
    fn as_text_labels_and_drop_never_carries() {
        let r = reasoning(Protocol::AnthropicMessages, Some("sig"));
        let text = plan_reasoning(&r, None, ReasoningPolicy::AsText);
        assert_eq!(text, ReasoningPlan::Text(format!("{REASONING_AS_TEXT_LABEL}\nwhy")));
        assert_eq!(plan_reasoning(&r, None, ReasoningPolicy::Drop), ReasoningPlan::Drop);
    }
}
