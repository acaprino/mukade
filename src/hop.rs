//! One hop, end to end (§1.2): read a source session, apply the compaction policy, plan, check the
//! strict gate, write, and report what happened in a [`Manifest`].
//!
//! `hop` is the whole pipeline; [`import`] is the second half alone, for a caller that already holds
//! a [`Session`]; [`export`] is the first half alone.

use crate::manifest::Manifest;
use crate::model::{Entry, HarnessId, Kind, Protocol, Session};
use crate::policy::ReasoningPolicy;
use crate::roots::StoreRoots;
use crate::sha256::records_sha256;
use crate::strict::check_plan;
use crate::tools::{Shell, ToolSet};
use crate::writer::{self, WriteOpts, Written, plan};
use serde_json::Value;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The text every portable hop writes before the source's own summary (§4.2, verbatim).
pub const PORTABLE_HANDOFF: &str = "[mukade handoff v1] Summary of the earlier conversation, written by the source agent's own compaction:";

/// Which session to read: its native id, or a file that holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRef {
    pub harness: HarnessId,
    pub id: String,
}

impl SessionRef {
    pub fn id(harness: HarnessId, id: impl Into<String>) -> Self {
        SessionRef {
            harness,
            id: id.into(),
        }
    }
}

/// Where the summary of a portable hop comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SummarySource {
    /// The source's own native compaction record (L4).
    SourceRecord,
    /// Text the caller supplies, e.g. written by a cheaper model.
    Supplied(String),
}

/// Where the tail of a portable hop starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeepFrom {
    /// The boundary the source's own compaction recorded.
    SourceBoundary,
    /// The last `n` user turns of the selection.
    LastTurns(usize),
    /// A named entry; an unambiguous prefix is accepted.
    Message(String),
}

/// What to do with the source's compaction (§4).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Compaction {
    /// The whole selected history (§4.1).
    #[default]
    None,
    /// The summary as one user message, then the tail from the boundary (§4.2).
    Portable {
        summary: SummarySource,
        keep_from: KeepFrom,
    },
    /// The whole history, then the destination's own compaction (§4.3). The compaction itself is
    /// run by the caller, as the bench does: it needs the destination's environment.
    DestinationNative,
}

impl Compaction {
    pub fn name(&self) -> &'static str {
        match self {
            Compaction::None => "none",
            Compaction::Portable { .. } => "portable",
            Compaction::DestinationNative => "destination_native",
        }
    }
}

/// Ask the destination to resume, and keep its reply for the receipt (§5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumePrompt {
    pub text: String,
    pub timeout: Duration,
}

impl ResumePrompt {
    pub fn new(text: impl Into<String>) -> Self {
        ResumePrompt {
            text: text.into(),
            timeout: Duration::from_secs(300),
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

/// The destination-side facts a writer needs that the spec's §1.2 leaves to [`WriteOpts`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WriteDetails {
    pub dest_version: Option<String>,
    pub dest_protocol: Option<Protocol>,
    pub dest_shell: Option<Shell>,
    pub offered: Option<ToolSet>,
    pub model_provider: Option<String>,
    pub model: Option<String>,
    pub project_id: Option<String>,
    pub now: Option<String>,
    pub opencode_exe: Option<String>,
}

/// What the manifest says the source was.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceFacts {
    pub harness: String,
    pub version: Option<String>,
    pub session_id: String,
    pub content_sha256: String,
}

impl SourceFacts {
    /// The facts of a session we hold: its own metadata, and the fingerprint of its records.
    pub fn of(session: &Session) -> Self {
        let records: Vec<Value> = session
            .entries
            .iter()
            .flat_map(|entry| entry.raw.clone())
            .collect();
        SourceFacts {
            harness: session.meta.harness.as_str().to_string(),
            version: Some(session.meta.harness_version.clone()).filter(|v| !v.is_empty()),
            session_id: session.meta.id.clone(),
            content_sha256: records_sha256(&records),
        }
    }
}

/// A whole hop request (§1.2).
#[derive(Debug, Clone)]
pub struct HopRequest {
    pub source: SessionRef,
    pub dest: HarnessId,
    pub cwd: Option<String>,
    pub compaction: Compaction,
    pub resume: Option<ResumePrompt>,
    pub reasoning: ReasoningPolicy,
    pub roots: StoreRoots,
    pub write: WriteDetails,
}

/// The second half of a hop: write a session we already hold into a destination.
#[derive(Debug, Clone)]
pub struct ImportOpts {
    pub cwd: Option<String>,
    pub roots: StoreRoots,
    pub reasoning: ReasoningPolicy,
    pub write: WriteDetails,
    pub resume: Option<ResumePrompt>,
    /// What the manifest states about the source.
    pub source_facts: SourceFacts,
    /// `none` | `portable` | `destination_native`, as requested.
    pub compaction: String,
}

/// What a hop produced (§1.2).
#[derive(Debug, Clone, PartialEq)]
pub struct Receipt {
    pub dest_id: String,
    pub location: String,
    pub resume_command: Vec<String>,
    pub manifest: Value,
    /// The destination's reply, when a resume prompt was given.
    pub resume_output: Option<String>,
}

/// The failures a hop can report, mapped to the command line's exit codes (technical design 5.5.2:
/// 0 done, 2 refused with a reason, 3 needs the user, 4 failed and nothing changed, 5 failed and
/// recovery is needed).
#[derive(Debug, Clone, PartialEq)]
pub enum HopError {
    /// The source is not there, or its store is not configured. Nothing was written.
    NotFound(String),
    /// A pair or an option this library does not do. Nothing was written.
    Unsupported(String),
    /// The destination refused the write, or we could not write it. Nothing changed.
    WriteFailed(String),
    /// The source could not be read as the harness it claims to be.
    VerifyFailed(String),
    /// The write happened but the caller cannot use it (a resume that failed, a timed out prompt).
    ResumeFailed {
        reason: String,
        receipt: Box<Receipt>,
    },
    /// The plan would produce a request a real provider refuses (§7). Nothing was written.
    StrictCheckFailed(Vec<String>),
}

impl HopError {
    pub fn exit_code(&self) -> i32 {
        match self {
            HopError::NotFound(_) | HopError::Unsupported(_) => 2,
            HopError::WriteFailed(_) | HopError::StrictCheckFailed(_) => 4,
            HopError::VerifyFailed(_) | HopError::ResumeFailed { .. } => 5,
        }
    }
}

impl std::fmt::Display for HopError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HopError::NotFound(what) => write!(f, "not found: {what}"),
            HopError::Unsupported(what) => write!(f, "unsupported: {what}"),
            HopError::WriteFailed(what) => write!(f, "write failed: {what}"),
            HopError::VerifyFailed(what) => write!(f, "could not read the source: {what}"),
            HopError::ResumeFailed { reason, receipt } => write!(
                f,
                "resume failed: {reason} (the session was written: {}; the receipt and manifest are included in this error)",
                receipt.location
            ),
            HopError::StrictCheckFailed(violations) => {
                write!(
                    f,
                    "the plan would be refused by a provider: {}",
                    violations.join("; ")
                )
            }
        }
    }
}

impl std::error::Error for HopError {}

/// Read a session with its harness's reader and rules (§8.1).
pub fn export(source: &SessionRef, roots: &StoreRoots) -> Result<Session, HopError> {
    let reader = crate::reader::for_harness(source.harness)
        .map_err(|error| HopError::Unsupported(format!("{error:#}")))?;
    reader.read(&source.id, roots).map_err(classify_read)
}

/// Write a session we hold into a destination.
pub fn import(dest: HarnessId, session: &Session, opts: &ImportOpts) -> Result<Receipt, HopError> {
    let write_opts = WriteOpts {
        roots: opts.roots.clone(),
        cwd: opts.cwd.clone().or_else(|| session.meta.cwd.clone()),
        dest_version: opts.write.dest_version.clone(),
        dest_protocol: opts.write.dest_protocol.clone(),
        dest_shell: opts.write.dest_shell,
        offered: opts.write.offered,
        model_provider: opts.write.model_provider.clone(),
        model: opts.write.model.clone(),
        project_id: opts.write.project_id.clone(),
        reasoning: opts.reasoning,
        title: session.meta.title.clone(),
        now: opts.write.now.clone(),
        opencode_exe: opts.write.opencode_exe.clone(),
    };

    // §7: the gate runs on the plan the writer will use, before anything reaches the store.
    let plan = plan::plan(session, dest, &write_opts);
    let dest_protocol = write_opts
        .dest_protocol
        .clone()
        .or_else(|| crate::policy::destination_protocol(dest));
    let violations = check_plan(&plan, dest_protocol.as_ref());
    if !violations.is_empty() {
        return Err(HopError::StrictCheckFailed(violations));
    }

    let written: Written = writer::for_harness(dest)
        .write(session, &write_opts)
        .map_err(|error| HopError::WriteFailed(format!("{error:#}")))?;

    let mut receipt = Receipt {
        dest_id: written.id.clone(),
        location: written.location.clone(),
        resume_command: written.resume.clone(),
        manifest: Value::Null,
        resume_output: None,
    };

    let mut losses = Manifest::losses_for(
        &plan.counts,
        session.interrupted_last_turn(),
        session.last_compaction().is_some(),
    );
    if let Some(prompt) = opts.resume.as_ref() {
        match run_resume(&written.resume, &prompt.text, prompt.timeout) {
            Ok(output) => receipt.resume_output = Some(output),
            Err(HopError::ResumeFailed { reason, .. }) => {
                losses.push(format!("the resume prompt did not complete: {reason}"));
                receipt.manifest =
                    manifest_of(dest, session, opts, &written, &plan, losses).to_json();
                return Err(HopError::ResumeFailed {
                    reason,
                    receipt: Box::new(receipt),
                });
            }
            Err(other) => return Err(other),
        }
    }

    receipt.manifest = manifest_of(dest, session, opts, &written, &plan, losses).to_json();
    Ok(receipt)
}

/// Read, plan, check, write: the whole pipeline of §1.
pub fn hop(req: &HopRequest) -> Result<Receipt, HopError> {
    let source = export(&req.source, &req.roots)?;
    let facts = SourceFacts::of(&source);
    let selection = select(&source, &req.compaction)?;
    let opts = ImportOpts {
        cwd: req.cwd.clone(),
        roots: req.roots.clone(),
        reasoning: req.reasoning,
        write: req.write.clone(),
        resume: req.resume.clone(),
        source_facts: facts,
        compaction: req.compaction.name().to_string(),
    };
    import(req.dest, &selection, &opts)
}

/// Apply compaction to a session already read from a file, then finish the normal import path.
pub fn hop_session(
    dest: HarnessId,
    source: &Session,
    compaction: &Compaction,
    opts: &ImportOpts,
) -> Result<Receipt, HopError> {
    let selection = select(source, compaction)?;
    import(dest, &selection, opts)
}

/// The session a compaction policy hands to the writer.
fn select(source: &Session, compaction: &Compaction) -> Result<Session, HopError> {
    match compaction {
        Compaction::None => Ok(source.clone()),
        Compaction::DestinationNative => Err(HopError::Unsupported(
            "Compaction::DestinationNative: the destination's own compaction has to be triggered by the \
             caller, inside the destination's own environment (a resume command, an HTTP endpoint or an \
             RPC, one per harness); this library writes the whole history, which is what \
             Compaction::None does"
                .into(),
        )),
        Compaction::Portable { summary, keep_from } => {
            let (text, trigger, tokens_before, boundary) = match summary {
                SummarySource::Supplied(text) => (text.clone(), crate::model::Trigger::Unknown, None, None),
                SummarySource::SourceRecord => match source.last_compaction().map(|entry| &entry.kind) {
                    Some(Kind::Compaction { summary, trigger, tokens_before, kept_from }) => (
                        summary.clone(),
                        trigger.clone(),
                        *tokens_before,
                        kept_from.clone(),
                    ),
                    _ => {
                        return Err(HopError::Unsupported(
                            "Compaction::Portable with SummarySource::SourceRecord, but the source has no native \
                             compaction record to carry"
                                .into(),
                        ));
                    }
                },
            };
            // §4.2: one user message, the handoff line first, then the tail from the boundary.
            let cut = cut_index(source, keep_from, boundary.as_deref())?;
            let mut entries = vec![Entry {
                id: "portable-handoff".into(),
                kind: Kind::Compaction {
                    summary: format!("{PORTABLE_HANDOFF}\n{text}"),
                    kept_from: source.entries.get(cut).map(|entry| entry.id.clone()),
                    trigger,
                    tokens_before,
                },
                raw: Vec::new(),
            }];
            entries.extend(source.entries.get(cut..).unwrap_or_default().iter().cloned());
            let mut selected = source.clone();
            selected.entries = entries;
            Ok(selected)
        }
    }
}

/// Where the tail starts, as an index into the source's entries.
fn cut_index(
    source: &Session,
    keep_from: &KeepFrom,
    boundary: Option<&str>,
) -> Result<usize, HopError> {
    match keep_from {
        KeepFrom::LastTurns(turns) => {
            let users: Vec<usize> = source
                .entries
                .iter()
                .enumerate()
                .filter(|(_, entry)| matches!(entry.kind, Kind::User { .. }))
                .map(|(i, _)| i)
                .collect();
            Ok(users
                .iter()
                .rev()
                .take(*turns)
                .next_back()
                .copied()
                .unwrap_or(0))
        }
        KeepFrom::Message(id) => find_entry(source, id),
        KeepFrom::SourceBoundary => {
            if let Some(id) = boundary {
                return find_entry(source, id).map_err(|_| {
                    HopError::Unsupported(format!(
                        "the source's compaction boundary {id:?} is missing or ambiguous; no tail was guessed"
                    ))
                });
            }
            // Codex keeps the boundary as "the first conversation entry after the compaction", and a
            // boundary that names a row we folded into another entry lands here too.
            source
                .entries
                .iter()
                .rposition(|entry| matches!(entry.kind, Kind::Compaction { .. }))
                .map(|index| index + 1)
                .ok_or_else(|| {
                    HopError::Unsupported(
                        "Compaction::Portable with KeepFrom::SourceBoundary, but the source has no compaction \
                         boundary to keep from"
                            .into(),
                    )
                })
        }
    }
}

fn find_entry(source: &Session, id: &str) -> Result<usize, HopError> {
    if let Some(index) = source.entries.iter().position(|entry| entry.id == id) {
        return Ok(index);
    }
    let mut matches = source
        .entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.id.starts_with(id));
    let (index, _) = matches
        .next()
        .ok_or_else(|| HopError::Unsupported(format!("no entry {id:?} in the source")))?;
    if let Some((_, entry)) = matches.next() {
        return Err(HopError::Unsupported(format!(
            "entry prefix {id:?} is ambiguous (also matches {:?})",
            entry.id
        )));
    }
    Ok(index)
}

fn manifest_of(
    dest: HarnessId,
    session: &Session,
    opts: &ImportOpts,
    written: &Written,
    plan: &plan::Plan,
    losses: Vec<String>,
) -> Manifest {
    let dest_version = opts.write.dest_version.clone();
    let source_version = opts.source_facts.version.clone();
    let at = |harness: &str, version: Option<&String>| match version {
        Some(version) => format!("{harness}@{version}"),
        None => harness.to_string(),
    };
    Manifest {
        source_harness: opts.source_facts.harness.clone(),
        source_version: opts.source_facts.version.clone(),
        source_session_id: opts.source_facts.session_id.clone(),
        source_content_sha256: opts.source_facts.content_sha256.clone(),
        dest_harness: dest.as_str().to_string(),
        dest_version: dest_version.clone(),
        dest_session_id: written.id.clone(),
        dest_location: written.location.clone(),
        reader: at(&opts.source_facts.harness, source_version.as_ref()),
        writer: at(dest.as_str(), dest_version.as_ref()),
        compaction: opts.compaction.clone(),
        reasoning: match opts.reasoning {
            ReasoningPolicy::DropForeign => "drop_foreign",
            ReasoningPolicy::AsText => "as_text",
            ReasoningPolicy::Drop => "drop",
        }
        .to_string(),
        resume_prompt: opts.resume.is_some(),
        counts: plan.counts.clone(),
        losses: {
            let mut losses = losses;
            losses.extend(extra_losses(session));
            losses
        },
        strict_checked: true,
        strict_violations: Vec::new(),
    }
}

/// Losses that belong to the selection rather than to the plan.
fn extra_losses(session: &Session) -> Vec<String> {
    let mut losses = Vec::new();
    let kept_from = match session.last_compaction().map(|entry| &entry.kind) {
        Some(Kind::Compaction { kept_from, .. }) => Some(kept_from.clone()),
        _ => None,
    };
    if let Some(kept_from) = kept_from {
        let kept = session
            .entries
            .iter()
            .rposition(|entry| matches!(entry.kind, Kind::Compaction { .. }))
            .map(|index| session.entries[index + 1..].len())
            .unwrap_or(0);
        losses.push(format!(
            "the source's own compaction is in the session (kept {kept} entries after the boundary{})",
            kept_from.map(|id| format!(", from {id}")).unwrap_or_default()
        ));
    }
    losses
}

/// Run the destination's own resume command with the prompt as the last argument (§5).
///
/// The command is the one the writer stated (`Written::resume`), so the caller decides the
/// environment: pointing it at a scratch store and a mock is what keeps this free. A hop that asks
/// for a resume without setting that environment resumes against the user's real configuration.
fn resume_error(reason: String) -> HopError {
    HopError::ResumeFailed {
        reason,
        receipt: Box::new(Receipt {
            dest_id: String::new(),
            location: String::new(),
            resume_command: Vec::new(),
            manifest: Value::Null,
            resume_output: None,
        }),
    }
}

pub fn run_resume(argv: &[String], prompt: &str, timeout: Duration) -> Result<String, HopError> {
    let (program, args) = argv
        .split_first()
        .ok_or_else(|| resume_error("the destination stated no resume command".into()))?;
    let mut child = Command::new(program)
        .args(args)
        .arg(prompt)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| resume_error(format!("{program}: {error}")))?;

    let stdout = child
        .stdout
        .take()
        .map(|out| std::thread::spawn(move || read_to_string(out)));
    let stderr = child
        .stderr
        .take()
        .map(|err| std::thread::spawn(move || read_to_string(err)));
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let out = stdout
                    .and_then(|handle| handle.join().ok())
                    .unwrap_or_default();
                let err = stderr
                    .and_then(|handle| handle.join().ok())
                    .unwrap_or_default();
                if status.success() {
                    return Ok(out.trim_end().to_string());
                }
                return Err(resume_error(format!(
                    "{} exited with {status}: {}",
                    argv.join(" "),
                    err.trim()
                )));
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(resume_error(format!(
                        "{} did not finish within {timeout:?}",
                        argv.join(" ")
                    )));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(error) => return Err(resume_error(format!("{program}: {error}"))),
        }
    }
}

fn read_to_string(mut reader: impl std::io::Read) -> String {
    let mut text = String::new();
    let _ = reader.read_to_string(&mut text);
    text
}

/// A read failure is a refusal when the session (or the store) is not there, and a verification
/// failure when the harness is configured but what we found is not a session we understand.
fn classify_read(error: anyhow::Error) -> HopError {
    let message = format!("{error:#}");
    let missing = message.contains("is not set")
        || (message.contains("no ")
            && (message.contains("session") || message.contains("rollout")));
    if missing {
        HopError::NotFound(message)
    } else {
        HopError::VerifyFailed(message)
    }
}

/// Read a session straight from a file, for a source that is not in a store (the spec's
/// `SessionRef` may be "harness + native id (or file path)").
pub fn export_path(harness: HarnessId, path: &std::path::Path) -> Result<Session, HopError> {
    let session = match harness {
        HarnessId::ClaudeCode => crate::reader::claude_code::read_file(path),
        HarnessId::Codex => crate::reader::codex::read_file(path),
        HarnessId::OpenCode => crate::reader::opencode::read_file(path),
        HarnessId::Pi => crate::reader::pi::read_file(path),
    };
    session.map_err(classify_read)
}
