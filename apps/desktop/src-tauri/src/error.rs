//! The one error every command answers with (spec §6): `kind` tells the window how to show it.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CmdError {
    /// `refused` (the request cannot be done as asked), `not_found`, `failed` (it was tried and
    /// broke), `unreachable` (the chat service did not answer).
    pub kind: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}

impl CmdError {
    pub fn refused(message: impl Into<String>) -> Self {
        Self::new("refused", message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new("not_found", message)
    }

    pub fn failed(message: impl Into<String>) -> Self {
        Self::new("failed", message)
    }

    pub fn unreachable(message: impl Into<String>) -> Self {
        Self::new("unreachable", message)
    }

    fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            exit_code: None,
        }
    }
}

/// A hop or import failure, with the command line's exit code for it.
impl From<mukade::HopError> for CmdError {
    fn from(error: mukade::HopError) -> Self {
        let exit_code = error.exit_code();
        let kind = match &error {
            mukade::HopError::NotFound(_) => "not_found",
            mukade::HopError::Unsupported(_)
            | mukade::HopError::SourceChanged { .. }
            | mukade::HopError::StrictCheckFailed(_) => "refused",
            _ => "failed",
        };
        Self {
            kind,
            message: error.to_string(),
            exit_code: Some(exit_code),
        }
    }
}

pub type CmdResult<T> = Result<T, CmdError>;
