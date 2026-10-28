//! Writers: [`Session`] → a new native session in a destination's store
//! (docs/session-interchange-spec.md §1.2, §8.2).

pub mod claude_code;
pub mod codex;
pub mod media;
pub mod opencode;
pub mod pi;
pub mod plan;

use crate::model::{HarnessId, Protocol, Session};
use crate::policy::ReasoningPolicy;
use crate::roots::StoreRoots;
use crate::tools::{Shell, ToolSet, offered};
use anyhow::Result;
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
    /// form (tables §4.1).
    pub dest_shell: Option<Shell>,
    /// The destination's offered tool set, when the caller knows it (tables §6). `None` uses the
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
    /// §3.3. The default drops reasoning the destination cannot replay.
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

/// One writer per harness.
pub trait Writer {
    fn harness(&self) -> HarnessId;

    /// The tools this destination offers for the pinned version (tables §2, L11).
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

/// The writer for a harness.
pub fn for_harness(harness: HarnessId) -> Box<dyn Writer> {
    match harness {
        HarnessId::ClaudeCode => Box::new(claude_code::ClaudeCode),
        HarnessId::Codex => Box::new(codex::Codex),
        HarnessId::OpenCode => Box::new(opencode::Opencode),
        HarnessId::Pi => Box::new(pi::Pi),
    }
}

/// Write a session's rows to a file, as JSON Lines with a trailing newline.
pub(crate) fn write_jsonl(path: &std::path::Path, rows: &[serde_json::Value]) -> Result<()> {
    use anyhow::Context;
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
        std::fs::rename(&temporary, path).with_context(|| format!("publish {}", path.display()))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}
