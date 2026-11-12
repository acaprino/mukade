//! Provenance for reduced observation. Interchange entries and readers remain unchanged.
use crate::model::{Entry, HarnessId, Kind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    Typed,
    ShellCommand { command: String },
    Injected(&'static str),
    Unknown,
}

pub fn origin_of(harness: HarnessId, entry: &Entry) -> Origin {
    if !matches!(entry.kind, Kind::User { .. }) {
        return Origin::Unknown;
    }
    let Some(raw) = entry.raw.first() else {
        return if harness == HarnessId::ClaudeCode {
            Origin::Typed
        } else {
            Origin::Unknown
        };
    };
    match harness {
        HarnessId::Pi => match raw.get("type").and_then(|v| v.as_str()) {
            Some("custom_message") => Origin::Injected("custom_message"),
            Some("branch_summary") => Origin::Injected("branch_summary"),
            Some("message") => match raw.pointer("/message/role").and_then(|v| v.as_str()) {
                Some("bashExecution") => raw
                    .pointer("/message/command")
                    .and_then(|v| v.as_str())
                    .map(|command| Origin::ShellCommand {
                        command: command.into(),
                    })
                    .unwrap_or(Origin::Unknown),
                Some("user") => Origin::Typed,
                _ => Origin::Unknown,
            },
            _ => Origin::Unknown,
        },
        HarnessId::ClaudeCode => {
            if raw.get("isMeta").and_then(|v| v.as_bool()) == Some(true) {
                Origin::Injected("expanded_command")
            } else if raw.get("type").and_then(|v| v.as_str()) == Some("user") {
                Origin::Typed
            } else {
                Origin::Unknown
            }
        }
        HarnessId::Codex => {
            let Some(payload) = raw.get("payload") else {
                return Origin::Unknown;
            };
            if payload
                .pointer("/internal_chat_message_metadata_passthrough/content_item_metadata")
                .and_then(|v| v.as_array())
                .is_some_and(|items| {
                    items.iter().any(|item| {
                        item.pointer("/provenance/type").and_then(|v| v.as_str()) == Some("harness")
                    })
                })
            {
                return Origin::Injected("harness_context");
            }
            if payload.get("role").and_then(|v| v.as_str()) == Some("user")
                || payload.get("type").and_then(|v| v.as_str()) == Some("user_message")
            {
                Origin::Typed
            } else {
                Origin::Unknown
            }
        }
        HarnessId::OpenCode => {
            let data = raw.get("data").unwrap_or(raw);
            let decoded = data
                .as_str()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok());
            let data = decoded.as_ref().unwrap_or(data);
            if raw
                .get("type")
                .or_else(|| data.get("type"))
                .and_then(|v| v.as_str())
                == Some("user")
            {
                Origin::Typed
            } else {
                Origin::Unknown
            }
        }
    }
}
