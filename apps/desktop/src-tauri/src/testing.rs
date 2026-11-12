//! Fixtures shared by the unit tests: scratch stores holding the repository's fixture sessions.

use crate::locations::{Location, standard_locations};
use crate::scratch::Scratch;
use mukade::model::HarnessId;
use mukade::roots::StoreRoots;
use std::path::Path;

pub(crate) const CLAUDE_FIXTURE: &str = "c45db7bc-542c-4201-89de-b590c6fdb797";

pub(crate) fn fixtures() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../fixtures"))
}

/// A scratch Claude Code store holding the fixture session, and an empty Pi store beside it.
pub(crate) fn claude_store(scratch: &Scratch) -> StoreRoots {
    let claude = scratch.join("claude");
    let project = claude.join("projects").join("p");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::copy(
        fixtures().join(format!("claude_code/source/{CLAUDE_FIXTURE}.jsonl")),
        project.join(format!("{CLAUDE_FIXTURE}.jsonl")),
    )
    .unwrap();
    StoreRoots {
        claude_config_dir: Some(claude),
        pi_agent_dir: Some(scratch.join("pi")),
        ..Default::default()
    }
}

pub(crate) fn location(roots: &StoreRoots, harness: HarnessId) -> Location {
    standard_locations(roots)
        .into_iter()
        .find(|location| location.harness == harness)
        .unwrap()
}

pub(crate) fn claude(roots: &StoreRoots) -> Location {
    location(roots, HarnessId::ClaudeCode)
}
