//! A summary of a session written by the source harness's own model (design
//! `docs/superpowers/specs/2026-10-28-source-model-summary-design.md`).
//!
//! mukade holds no API key and calls no provider: the summary is asked of the source harness's own
//! binary, with its own credentials and configuration, in a run that saves nothing in its store.
//! What comes back travels like the source's own compaction does (§4.2): one handoff message, then
//! the turns the caller chose to keep.

use std::time::Duration;

use crate::model::HarnessId;

/// The line the destination's model reads before the summary, so it knows where the text came from.
/// `crate::hop::PORTABLE_HANDOFF` stays for the source's own compaction.
pub const MODEL_SUMMARY_HANDOFF: &str = "[mukade handoff v1] Summary of the earlier conversation, written by the source agent's model when the session was moved:";

/// The version of the prompt the summary is asked with, recorded in the manifest.
pub const SUMMARY_PROMPT_VERSION: &str = "v1";

/// How to ask the source harness for the summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummarizeOpts {
    /// The model to ask; `None` uses the session's own last model, else the harness's default.
    pub model: Option<String>,
    /// The deadline of the summarizing run.
    pub timeout: Duration,
    /// The source harness's executable override (`crate::exe` resolves the default).
    pub exe: Option<String>,
}

impl Default for SummarizeOpts {
    fn default() -> Self {
        SummarizeOpts {
            model: None,
            timeout: Duration::from_secs(300),
            exe: None,
        }
    }
}

/// What the source harness's model wrote, and what is known about the run that wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub text: String,
    pub harness: HarnessId,
    pub harness_version: Option<String>,
    pub model: Option<String>,
    /// The tokens or cost the harness reported, as compact JSON.
    pub usage: Option<String>,
    /// The command that removes what the run could not clean up (an OpenCode fork it failed to delete).
    pub leftover: Option<String>,
}

/// A summary as a hop used it: the text, and how many source entries it stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryFacts {
    pub summary: Summary,
    pub left_out: usize,
}
