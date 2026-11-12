//! What a command answers when it cannot do what it was asked: a kind the interface can act on, the
//! library's own words, and the exit code the command line would have used.

use mukade::HopError;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CmdError {
    /// `refused` (the library declined, nothing changed), `not_found`, `failed`, `unreachable` (the
    /// chat service).
    pub kind: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}

impl CmdError {
    pub fn refused(message: impl Into<String>) -> Self {
        Self::new("refused", message, Some(2))
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new("not_found", message, Some(2))
    }

    pub fn failed(message: impl Into<String>) -> Self {
        Self::new("failed", message, None)
    }

    pub fn unreachable(message: impl Into<String>) -> Self {
        Self::new("unreachable", message, None)
    }

    fn new(kind: &'static str, message: impl Into<String>, exit_code: Option<i32>) -> Self {
        Self {
            kind,
            message: message.into(),
            exit_code,
        }
    }
}

impl std::fmt::Display for CmdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl From<HopError> for CmdError {
    fn from(error: HopError) -> Self {
        let kind = match &error {
            HopError::NotFound(_) => "not_found",
            HopError::Unsupported(_) | HopError::SourceChanged { .. } => "refused",
            _ => "failed",
        };
        Self::new(kind, error.to_string(), Some(error.exit_code()))
    }
}

impl From<anyhow::Error> for CmdError {
    fn from(error: anyhow::Error) -> Self {
        Self::failed(format!("{error:#}"))
    }
}

pub type CmdResult<T> = Result<T, CmdError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hop_error_keeps_its_exit_code_and_picks_a_kind() {
        let refused = CmdError::from(HopError::Unsupported("a pair it does not do".into()));
        assert_eq!((refused.kind, refused.exit_code), ("refused", Some(2)));
        let missing = CmdError::from(HopError::NotFound("no such session".into()));
        assert_eq!((missing.kind, missing.exit_code), ("not_found", Some(2)));
        let failed = CmdError::from(HopError::WriteFailed("disk full".into()));
        assert_eq!((failed.kind, failed.exit_code), ("failed", Some(4)));
        assert!(failed.message.contains("disk full"));
    }

    #[test]
    fn the_wire_form_names_kind_and_message_and_omits_a_missing_code() {
        let json = serde_json::to_value(CmdError::unreachable("service stopped")).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"kind": "unreachable", "message": "service stopped"})
        );
    }
}
