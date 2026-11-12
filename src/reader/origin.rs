//! Provenance for reduced observation. Interchange entries and readers remain unchanged.
use crate::model::{Entry, HarnessId, Kind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    Typed,
    ShellCommand { command: String },
    Injected(&'static str),
    Unknown,
}

/// Who produced a user entry; the harness's adapter answers from the entry's first record.
pub fn origin_of(harness: HarnessId, entry: &Entry) -> Origin {
    if !matches!(entry.kind, Kind::User { .. }) {
        return Origin::Unknown;
    }
    crate::harnesses::get(harness).user_origin(entry.raw.first())
}
