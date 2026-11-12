//! Writers: [`Session`] â†’ a new native session in a destination's store
//! (docs/session-interchange-spec.md Â§1.2, Â§8.2).

pub mod claude_code;
pub mod codex;
pub mod media;
pub mod opencode;
pub mod pi;
pub mod plan;

use crate::model::{HarnessId, Protocol, Session};
use crate::native_compaction::NativeBudget;
use crate::policy::ReasoningPolicy;
use crate::roots::StoreRoots;
use crate::tools::{Shell, ToolSet, offered};
use anyhow::Result;
/// What a writer does with the work of a turn the source never finished: the steps and results
/// between a prompt and the interruption (L9).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Interrupted {
    /// Carry it: what the agent had done travels as it is.
    #[default]
    Keep,
    /// Leave it out, counted. The turn's prompt stays: it is the user's own, and complete.
    Drop,
}

impl Interrupted {
    /// The name the command line and the manifest use.
    pub fn as_str(self) -> &'static str {
        match self {
            Interrupted::Keep => "keep",
            Interrupted::Drop => "drop",
        }
    }
}

/// What a writer is told about the destination.
#[derive(Debug, Clone, Default)]
pub struct WriteOpts {
    /// Where the destination keeps its sessions. An empty root is an error for Claude Code, Codex
    /// and Pi, not a guess; OpenCode without a database uses the store its own configuration names.
    pub roots: StoreRoots,
    /// The project directory the destination files the session under. A hop keeps the project, so
    /// the caller sets it (the source's cwd is only the fallback).
    pub cwd: Option<String>,
    /// The destination's own version stamp, for the records that carry one.
    pub dest_version: Option<String>,
    /// The wire protocol the destination speaks, when its configuration states it. `None` means
    /// opaque reasoning cannot be proven replayable and is dropped.
    pub dest_protocol: Option<Protocol>,
    /// The shell the destination runs commands with. A shell call whose dialect differs is text
    /// form (tables Â§4.1).
    pub dest_shell: Option<Shell>,
    /// The destination's offered tool set, when the caller knows it (tables Â§6). `None` uses the
    /// list pinned for that harness's bench version.
    pub offered: Option<ToolSet>,
    /// The destination's model provider id (Codex `session_meta.model_provider`, OpenCode
    /// `providerID`). OpenCode's interchange format *requires* it on every model reference.
    pub model_provider: Option<String>,
    /// The model the destination will use. `None` keeps the source's model id.
    pub model: Option<String>,
    /// The destination's project: OpenCode's interchange format requires `info.projectID`, and the
    /// project is the destination's own (it derives one per directory).
    pub project_id: Option<String>,
    /// A previously reserved destination session ID, for retry-safe callers.
    pub dest_id: Option<String>,
    /// A previously reserved destination file, for retry-safe callers of file-based harnesses.
    pub dest_path: Option<std::path::PathBuf>,
    /// The destination's primary agent for an OpenCode session.
    pub dest_agent: Option<String>,
    /// Metadata supplied by the caller for the new OpenCode session.
    pub metadata: Option<serde_json::Value>,
    /// Explicit source-to-destination tool bindings supplied by the host's catalog.
    pub tool_map: Vec<(String, String, serde_json::Value, serde_json::Value)>,
    /// Omit recorded tool calls and their results, retaining only conversation text.
    pub without_tools: bool,
    /// Whether the work of interrupted turns travels. The default keeps it.
    pub interrupted: Interrupted,
    /// Â§3.3. The default drops reasoning the destination cannot replay.
    pub reasoning: ReasoningPolicy,
    /// A title, where the destination has one.
    pub title: Option<String>,
    /// The base instant for generated timestamps (RFC 3339). `None` means now; fixing it makes a
    /// written session reproducible.
    pub now: Option<String>,
    /// The destination harness's executable override, for spawned commands (OpenCode's
    /// `session import`, L1) and for the resume command the receipt states. `None` means the
    /// harness's own resolution: `MUKADE_*_EXE`, `PATH`, then the well-known install locations
    /// (`crate::exe`).
    pub exe: Option<String>,
    /// §4.4: destination-model facts for a deterministic native compaction boundary. The writer keeps
    /// the whole generated session and appends the destination's own record when needed.
    pub native_budget: Option<NativeBudget>,
}

/// A session that was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    pub id: String,
    /// The file (or store) the session was written to.
    pub location: String,
    /// The destination's own resume command, without the prompt.
    pub resume: Vec<String>,
}

/// A started external import may have written its session before its process failed.
#[derive(Debug)]
pub struct WriteOutcomeUnknown {
    pub id: String,
    pub location: Option<String>,
    pub reason: String,
}

impl std::fmt::Display for WriteOutcomeUnknown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "write outcome unknown for session {}: {}",
            self.id, self.reason
        )
    }
}

impl std::error::Error for WriteOutcomeUnknown {}

/// The CLI validates a reserved ID before it knows the source or starts a process: an id some
/// destination takes ([`Destination::validate_write`](crate::harnesses::Destination::validate_write)).
/// An id in a harness's own shape ([`Destination::claims_id`](crate::harnesses::Destination::claims_id))
/// gets that harness's verdict.
pub fn validate_id_syntax(id: &str) -> Result<()> {
    let opts = WriteOpts {
        dest_id: Some(id.to_string()),
        ..WriteOpts::default()
    };
    let destinations = crate::harnesses::all()
        .into_iter()
        .filter_map(|harness| harness.destination());
    if let Some(owner) = destinations
        .clone()
        .find(|destination| destination.claims_id(id))
    {
        return owner.validate_write(&opts);
    }
    let mut refusal = None;
    for destination in destinations {
        match destination.validate_write(&opts) {
            Ok(()) => return Ok(()),
            Err(error) => {
                refusal.get_or_insert(error);
            }
        }
    }
    Err(refusal.unwrap_or_else(|| anyhow::anyhow!("invalid destination session ID")))
}

/// One writer per harness.
pub trait Writer {
    fn harness(&self) -> HarnessId;

    /// The tools this destination offers for the pinned version (tables Â§2, L11).
    fn offered(&self) -> ToolSet {
        offered(self.harness())
    }

    /// Write the session as a new native session of this harness.
    fn write(&self, session: &Session, opts: &WriteOpts) -> Result<Written> {
        let plan = plan::plan(session, self.harness(), opts);
        self.write_planned(session, &plan, opts)
    }

    /// Write a session whose plan is already made: the hop plans once, checks that plan against
    /// the provider rules, and writes that same plan.
    fn write_planned(
        &self,
        session: &Session,
        plan: &plan::Plan,
        opts: &WriteOpts,
    ) -> Result<Written>;
}

/// The writer for a harness. A harness that is no destination gets one that refuses every write,
/// before anything is planned, with the reason [`crate::harnesses::destination`] gives (exit 2):
/// asking never panics.
pub fn for_harness(harness: HarnessId) -> Box<dyn Writer> {
    match crate::harnesses::destination(harness) {
        Ok(destination) => Box::new(destination.writer()),
        Err(refusal) => Box::new(Refused {
            harness,
            reason: match refusal {
                crate::hop::HopError::Unsupported(reason) => reason,
                other => other.to_string(),
            },
        }),
    }
}

/// The writer of a harness that is no destination: every write is refused.
struct Refused {
    harness: HarnessId,
    reason: String,
}

impl Refused {
    fn refuse(&self) -> Result<Written> {
        Err(crate::hop::HopError::Unsupported(self.reason.clone()).into())
    }
}

impl Writer for Refused {
    fn harness(&self) -> HarnessId {
        self.harness
    }

    fn write(&self, _session: &Session, _opts: &WriteOpts) -> Result<Written> {
        self.refuse()
    }

    fn write_planned(
        &self,
        _session: &Session,
        _plan: &plan::Plan,
        _opts: &WriteOpts,
    ) -> Result<Written> {
        self.refuse()
    }
}

/// Write a session's rows to a file, as JSON Lines with a trailing newline.
pub(crate) fn write_jsonl(path: &std::path::Path, rows: &[serde_json::Value]) -> Result<()> {
    use anyhow::Context;
    anyhow::ensure!(!path.exists(), "destination already exists: {}", path.display());
    let _span = crate::timing::span("serialize + write file");
    let mut text = String::new();
    for row in rows {
        text.push_str(&serde_json::to_string(row).context("serialize a record")?);
        text.push('\n');
    }
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("{} has no parent directory", path.display()))?;
    std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    // A write killed between creating the file and renaming it leaves `.mukade-*.tmp` here, a
    // whole copy of a session: the next write in this folder removes it once it is old enough
    // not to be another write in progress.
    crate::portable::sweep_stale(parent, ".mukade-", ".tmp", crate::portable::STALE_AFTER);
    let temporary = parent.join(format!(".mukade-{}.tmp", uuid::Uuid::new_v4().simple()));
    let result = (|| -> Result<()> {
        {
            use std::io::Write;
            let mut file = std::fs::File::create_new(&temporary)
                .with_context(|| format!("create {}", temporary.display()))?;
            file.write_all(text.as_bytes())
                .with_context(|| format!("write {}", temporary.display()))?;
            file.sync_all()
                .with_context(|| format!("flush {}", temporary.display()))?;
        }
        std::fs::hard_link(&temporary, path).with_context(|| format!("publish {} without overwriting", path.display()))?;
        let _ = std::fs::remove_file(&temporary);
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}
