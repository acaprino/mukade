//! What each command does, as plain functions over [`Shared`] so the tests call them without a
//! window. The `#[tauri::command]` wrappers in `main.rs` only move them off the window's thread.
//!
//! Every per-harness answer comes from the library (`mukade::harnesses::get(..)`, the readers,
//! `mukade::catalog`); nothing here branches on a harness (`tests/architecture.rs`).

use crate::error::{CmdError, CmdResult};
use crate::location::{self, Location};
use crate::view::{self, Installation, ReceiptView, SessionRow, Transcript};
use mukade::dest;
use mukade::exe;
use mukade::hop::{
    Compaction, HopError, ImportOpts, KeepFrom, ResumePrompt, SessionRef, SourceFacts,
    SummarySource, WriteDetails,
};
use mukade::model::{ParentKind, Session};
use mukade::policy::ReasoningPolicy;
use mukade::reader::{self, SessionInfo};
use mukade::roots::StoreRoots;
use mukade::summary::SummarizeOpts;
use mukade::tools::Shell;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

/// One location and what its reader listed.
#[derive(Clone)]
pub struct Listing {
    pub location: Location,
    pub sessions: Vec<SessionInfo>,
    pub error: Option<String>,
}

/// The session the reader has open, kept for its Raw tab.
struct Opened {
    key: usize,
    id: String,
    session: Session,
}

/// What the commands share. Behind an `Arc` so a blocking job can hold it.
pub struct Shared {
    standard: StoreRoots,
    listings: Mutex<Vec<Listing>>,
    opened: Mutex<Option<Opened>>,
    /// Hops and imports running now: the window refuses to close while one does (spec §4.2).
    pub busy: AtomicUsize,
    pub args: Args,
}

/// The command line (spec F14): `--turns N`, `--scan <dir>` any number of times.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Args {
    pub turns: usize,
    pub scan: Vec<String>,
}

impl Args {
    pub fn parse(args: &[String]) -> Self {
        let mut parsed = Args {
            turns: 50,
            scan: Vec::new(),
        };
        let mut index = 0;
        while index < args.len() {
            let arg = &args[index];
            let value = |inline: Option<&str>| {
                inline
                    .map(str::to_string)
                    .or_else(|| args.get(index + 1).cloned())
            };
            match arg.split_once('=') {
                Some(("--turns", inline)) => {
                    parsed.turns = inline.parse().unwrap_or(parsed.turns);
                }
                Some(("--scan", inline)) => parsed.scan.push(inline.to_string()),
                _ if arg == "--turns" => {
                    parsed.turns = value(None)
                        .and_then(|value| value.parse().ok())
                        .unwrap_or(parsed.turns);
                    index += 1;
                }
                _ if arg == "--scan" => {
                    if let Some(folder) = value(None) {
                        parsed.scan.push(folder);
                    }
                    index += 1;
                }
                _ => {}
            }
            index += 1;
        }
        parsed
    }
}

/// Counts a hop or import as running while it lives.
pub struct Busy(Arc<Shared>);

impl Busy {
    pub fn new(shared: &Arc<Shared>) -> Self {
        shared.busy.fetch_add(1, Ordering::SeqCst);
        Busy(shared.clone())
    }
}

impl Drop for Busy {
    fn drop(&mut self) {
        self.0.busy.fetch_sub(1, Ordering::SeqCst);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The stores this process's variables name, each missing one at the catalog's default location.
pub fn resolve_roots() -> StoreRoots {
    let mut roots = StoreRoots::from_env();
    for harness in mukade::harnesses::all() {
        let root = harness.store_root_mut(&mut roots);
        if root.is_none() {
            *root = mukade::catalog::store_default(harness.id());
        }
    }
    roots
}

impl Shared {
    pub fn new(standard: StoreRoots, args: Args) -> Self {
        let listings = location::standard_locations(&standard)
            .into_iter()
            .map(|location| Listing {
                location,
                sessions: Vec::new(),
                error: None,
            })
            .collect();
        Self {
            standard,
            listings: Mutex::new(listings),
            opened: Mutex::new(None),
            busy: AtomicUsize::new(0),
            args,
        }
    }

    /// The location of `key` and its listing, cloned so no lock is held while a store is read.
    fn listing(&self, key: usize) -> CmdResult<Listing> {
        lock(&self.listings)
            .get(key)
            .cloned()
            .ok_or_else(|| CmdError::not_found(format!("no installation {key}")))
    }

    #[cfg(test)]
    pub fn location(&self, key: usize) -> CmdResult<Location> {
        self.listing(key).map(|listing| listing.location)
    }

    fn info(&self, key: usize, id: &str) -> CmdResult<(Location, SessionInfo)> {
        let listing = self.listing(key)?;
        let info = listing
            .sessions
            .iter()
            .find(|info| info.id == id)
            .cloned()
            .ok_or_else(|| {
                CmdError::not_found(format!("{id} is not listed in {}", listing.location.label))
            })?;
        Ok((listing.location, info))
    }

    fn installations(&self) -> Vec<Installation> {
        lock(&self.listings)
            .iter()
            .enumerate()
            .map(|(key, listing)| {
                Installation::of(
                    key,
                    &listing.location,
                    listing.sessions.len(),
                    listing.error.clone(),
                )
            })
            .collect()
    }
}

/// The answer of `installations` and `scan`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Inventory {
    pub installations: Vec<Installation>,
    /// The installations the scan added, by key.
    pub added: Vec<usize>,
    /// A scan stopped at its depth or folder budget before the whole tree was read.
    pub stopped_early: bool,
    pub status: String,
}

/// Scan `folders` for installations (spec F10), then list every store again (F11).
pub fn inventory(shared: &Shared, folders: &[String]) -> Inventory {
    let mut locations: Vec<Location> = lock(&shared.listings)
        .iter()
        .map(|listing| listing.location.clone())
        .collect();
    let before = locations.len();
    let mut stopped_early = false;
    for folder in folders {
        let scan = mukade::detect::scan_installations(
            Path::new(folder),
            mukade::detect::DEFAULT_SCAN_DEPTH,
        );
        stopped_early |= scan.truncated;
        location::add_installations(&mut locations, &scan.installations, &shared.standard);
    }
    let listings: Vec<Listing> = locations
        .into_iter()
        .map(|location| {
            let listed = reader::for_harness(location.harness)
                .and_then(|reader| reader.list_all(&location.roots));
            match listed {
                Ok(sessions) => Listing {
                    location,
                    sessions,
                    error: None,
                },
                Err(error) => Listing {
                    location,
                    sessions: Vec::new(),
                    error: Some(format!("{error:#}")),
                },
            }
        })
        .collect();
    let added: Vec<usize> = (before..listings.len()).collect();
    let errors = listings
        .iter()
        .filter(|listing| listing.error.is_some())
        .count();
    *lock(&shared.listings) = listings;
    let mut parts = Vec::new();
    if !folders.is_empty() {
        parts.push(format!(
            "{} installation(s) added from {}",
            added.len(),
            folders.join(", ")
        ));
    }
    if errors > 0 {
        parts.push(format!("{errors} store(s) could not be read"));
    }
    if stopped_early {
        parts.push("the scan stopped early (depth or folder budget)".to_string());
    }
    let status = if parts.is_empty() {
        "ready".to_string()
    } else {
        parts.join(" · ")
    };
    Inventory {
        installations: shared.installations(),
        added,
        stopped_early,
        status,
    }
}

pub fn sessions(shared: &Shared, key: usize) -> CmdResult<Vec<SessionRow>> {
    Ok(shared
        .listing(key)?
        .sessions
        .iter()
        .map(SessionRow::from)
        .collect())
}

fn read(location: &Location, info: &SessionInfo) -> CmdResult<Session> {
    if let Some(why) = &info.unreadable {
        return Err(CmdError::refused(format!(
            "{} cannot be read: {why}",
            info.id
        )));
    }
    Ok(mukade::export(
        &SessionRef::id(location.harness, info.id.clone()),
        &location.roots,
    )?)
}

/// The listing's title, else the id.
fn label(info: &SessionInfo) -> String {
    info.title
        .clone()
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| info.id.clone())
}

pub fn transcript(shared: &Shared, key: usize, id: &str, turns: usize) -> CmdResult<Transcript> {
    let (location, info) = shared.info(key, id)?;
    let session = {
        let opened = lock(&shared.opened);
        opened
            .as_ref()
            .filter(|opened| opened.key == key && opened.id == id)
            .map(|opened| opened.session.clone())
    };
    let session = match session {
        Some(session) => session,
        None => read(&location, &info)?,
    };
    let view = view::transcript(&session, &label(&info), turns);
    *lock(&shared.opened) = Some(Opened {
        key,
        id: id.to_string(),
        session,
    });
    Ok(view)
}

/// The native records one entry was read from (the reader's Raw tab).
pub fn raw(shared: &Shared, key: usize, id: &str, entry_id: &str) -> CmdResult<Vec<Value>> {
    let cached = lock(&shared.opened)
        .as_ref()
        .filter(|opened| opened.key == key && opened.id == id)
        .and_then(|opened| {
            opened
                .session
                .entry(entry_id)
                .map(|entry| entry.raw.clone())
        });
    if let Some(raw) = cached {
        return Ok(raw);
    }
    let (location, info) = shared.info(key, id)?;
    read(&location, &info)?
        .entry(entry_id)
        .map(|entry| entry.raw.clone())
        .ok_or_else(|| CmdError::not_found(format!("no entry {entry_id} in {id}")))
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Destination {
    pub key: usize,
    pub label: String,
    /// The shell the destination runs for this session's folder: `posix`, `powershell`, or none
    /// known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_shell: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct HopOptions {
    pub source_label: String,
    pub title: String,
    pub destinations: Vec<Destination>,
    pub compactions: Vec<&'static str>,
    pub subagents: Vec<String>,
    pub forks: Vec<String>,
    pub all_branches_supported: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub live: Option<bool>,
}

fn shell_name(shell: Shell) -> &'static str {
    match shell {
        Shell::Posix => "posix",
        Shell::PowerShell => "powershell",
    }
}

/// The ids of the listed sessions that came from `id`, subagents and forks apart.
fn children_of(sessions: &[SessionInfo], id: &str) -> (Vec<String>, Vec<String>) {
    let mut subagents = Vec::new();
    let mut forks = Vec::new();
    for info in sessions
        .iter()
        .filter(|info| info.parent.as_deref() == Some(id) && info.id != id)
    {
        match info.parent_kind {
            Some(ParentKind::Spawn { .. }) => subagents.push(label(info)),
            Some(ParentKind::Fork { .. }) => forks.push(label(info)),
            _ => {}
        }
    }
    (subagents, forks)
}

pub fn hop_options(shared: &Shared, key: usize, id: &str) -> CmdResult<HopOptions> {
    let listing = shared.listing(key)?;
    let (source, info) = shared.info(key, id)?;
    if let Some(why) = &info.unreadable {
        return Err(CmdError::refused(format!("{id} cannot be read: {why}")));
    }
    let cwd = info.cwd.clone();
    let locations: Vec<Location> = lock(&shared.listings)
        .iter()
        .map(|listing| listing.location.clone())
        .collect();
    let destinations = location::destinations(&locations, key)
        .into_iter()
        .map(|index| (index, &locations[index]))
        .map(|(index, location)| Destination {
            key: index,
            label: location.label.clone(),
            default_shell: dest::shell_in(
                location.harness,
                &location.roots,
                cwd.as_deref().map(Path::new),
            )
            .map(shell_name),
        })
        .collect();
    let (subagents, forks) = children_of(&listing.sessions, id);
    let all_branches_supported = mukade::harnesses::get(source.harness)
        .read_all_branches_by_id(id, &source.roots)
        .is_some();
    Ok(HopOptions {
        source_label: source.label,
        title: label(&info),
        destinations,
        compactions: vec!["none", "portable", "summary"],
        subagents,
        forks,
        all_branches_supported,
        live: info.live,
    })
}

#[derive(Debug, Clone, Deserialize)]
pub struct HopArgs {
    pub source: usize,
    pub id: String,
    pub dest: usize,
    pub compaction: String,
    #[serde(default)]
    pub children: bool,
    #[serde(default)]
    pub all_branches: bool,
    #[serde(default)]
    pub reasoning: Option<String>,
    /// `default` (or none): the destination's own; `none`; or a shell's name.
    #[serde(default)]
    pub dest_shell: Option<String>,
    #[serde(default)]
    pub prompt: Option<String>,
}

fn compaction(name: &str) -> CmdResult<Compaction> {
    Ok(match name {
        "none" => Compaction::None,
        "portable" => Compaction::Portable {
            summary: SummarySource::SourceRecord,
            keep_from: KeepFrom::SourceBoundary,
        },
        "summary" => Compaction::Portable {
            summary: SummarySource::SourceModel(SummarizeOpts::default()),
            keep_from: KeepFrom::LastTurns(0),
        },
        other => {
            return Err(CmdError::refused(format!(
                "unknown compaction {other} (none, portable, summary)"
            )));
        }
    })
}

fn reasoning(name: Option<&str>) -> CmdResult<ReasoningPolicy> {
    Ok(match name {
        None | Some("drop_foreign") => ReasoningPolicy::DropForeign,
        Some("as_text") => ReasoningPolicy::AsText,
        Some("drop") => ReasoningPolicy::Drop,
        Some(other) => {
            return Err(CmdError::refused(format!(
                "unknown reasoning {other} (drop_foreign, as_text, drop)"
            )));
        }
    })
}

/// What a write into `dest` needs from the destination itself: its executable and version, its
/// protocol, the model its own configuration names, the shell it runs and the tools it offers in
/// `cwd`, and the project of `cwd` where the destination keeps one.
fn write_details_for(dest: &Location, cwd: Option<&str>) -> WriteDetails {
    let exe = dest.engine();
    let configured = dest::configured_model_in(dest.harness, &dest.roots);
    let project = cwd.map(Path::new);
    let project_id = cwd.and_then(|cwd| {
        mukade::harnesses::get(dest.harness)
            .destination()?
            .project_id(&exe, cwd, &dest.roots, mukade::cache::dir().as_deref())
            .ok()
            .flatten()
    });
    WriteDetails {
        dest_version: exe::version_of(&exe),
        dest_protocol: mukade::policy::destination_protocol(dest.harness),
        dest_shell: dest::shell_in(dest.harness, &dest.roots, project),
        offered: Some(mukade::tools::offered_in(
            dest.harness,
            &dest.roots,
            project,
        )),
        model_provider: configured.as_ref().map(|(provider, _)| provider.clone()),
        model: configured.and_then(|(_, model)| model),
        project_id,
        exe: Some(exe),
        ..Default::default()
    }
}

/// The `mukade-data` folder (`MUKADE_DATA_DIR`, else beside the executable).
fn data_dir() -> Option<PathBuf> {
    mukade::portable::data_dir()
}

fn save_manifest(receipt: &mukade::Receipt, dest: &Location) -> Option<String> {
    let dir = data_dir()?.join("manifests");
    std::fs::create_dir_all(&dir).ok()?;
    let name = format!("{}-{}.json", dest.harness.as_str(), receipt.dest_id);
    let path = dir.join(name.replace(['/', '\\'], "_"));
    std::fs::write(&path, serde_json::to_vec_pretty(&receipt.manifest).ok()?).ok()?;
    Some(path.display().to_string())
}

/// A written session's receipt, or the error; a write that happened before something else broke
/// is still a receipt, with that as its warning.
fn deliver(
    result: Result<mukade::Receipt, HopError>,
    dest_key: usize,
    dest: &Location,
) -> CmdResult<ReceiptView> {
    let (receipt, warning) = match result {
        Ok(receipt) => (receipt, None),
        Err(HopError::ResumeFailed { reason, receipt }) => (
            *receipt,
            Some(format!("the resume prompt failed: {reason}")),
        ),
        Err(HopError::ChildWriteFailed { reason, receipt }) => (
            *receipt,
            Some(format!(
                "the session was written; a subagent session was not: {reason}"
            )),
        ),
        Err(error) => return Err(error.into()),
    };
    let manifest = save_manifest(&receipt, dest);
    Ok(view::receipt(&receipt, dest_key, dest, manifest, warning))
}

fn prompt(text: Option<&str>) -> Option<ResumePrompt> {
    text.map(str::trim)
        .filter(|text| !text.is_empty())
        .map(|text| ResumePrompt::new(text.to_string()))
}

/// F6: read the source from its own installation, compact it there, write it into the
/// destination's stores (two installations of one harness keep theirs apart).
pub fn hop(shared: &Shared, args: &HopArgs) -> CmdResult<ReceiptView> {
    let (source, info) = shared.info(args.source, &args.id)?;
    let dest = shared.listing(args.dest)?.location;
    mukade::harnesses::destination(dest.harness)?;
    if args.all_branches && args.compaction == "summary" {
        return Err(CmdError::refused(
            "a summary with all branches: the summary stands for the branch the source resumes",
        ));
    }
    let selection = compaction(&args.compaction)?;
    let reasoning = reasoning(args.reasoning.as_deref())?;
    if let Some(why) = &info.unreadable {
        return Err(CmdError::refused(format!(
            "{} cannot be read: {why}",
            info.id
        )));
    }
    let reference = SessionRef::id(source.harness, info.id.clone());
    let session = if args.all_branches {
        mukade::hop::export_all_branches(&reference, &source.roots)?
    } else {
        mukade::export(&reference, &source.roots)?
    };
    let cwd = session.meta.cwd.clone().or(info.cwd.clone());
    let mut write = write_details_for(&dest, cwd.as_deref());
    match args.dest_shell.as_deref().map(str::trim) {
        None | Some("") | Some("default") => {}
        Some(name) if name.eq_ignore_ascii_case("none") => write.dest_shell = None,
        Some(name) => {
            write.dest_shell = Some(mukade::writer::plan::shell_of(name).ok_or_else(|| {
                CmdError::refused(format!(
                    "unknown shell {name} (bash, sh, zsh, dash, ksh, posix, powershell, pwsh, none)"
                ))
            })?);
        }
    }
    let (selected, summary) = mukade::hop::compact(&session, &selection, &source.roots)?;
    let opts = ImportOpts {
        cwd,
        roots: dest.roots.clone(),
        reasoning,
        write,
        resume: prompt(args.prompt.as_deref()),
        source_facts: SourceFacts::of(&session),
        compaction: selection.name().to_string(),
        summary,
        native_budget: None,
    };
    let result = if args.children {
        mukade::children::import_with_children(
            dest.harness,
            &selected,
            &session,
            &source.roots,
            &opts,
        )
    } else {
        mukade::import(dest.harness, &selected, &opts)
    };
    deliver(result, args.dest, &dest)
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Exported {
    pub path: String,
    pub messages: usize,
}

/// `<harness>-<id>.json`, the id made safe for a file name.
pub fn export_name(harness: &str, id: &str) -> String {
    let id: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("{harness}-{id}.json")
}

/// F7: the session's interchange document, at `to` or in `mukade-data/exports/`.
pub fn export(shared: &Shared, key: usize, id: &str, to: Option<&str>) -> CmdResult<Exported> {
    let (location, info) = shared.info(key, id)?;
    let session = read(&location, &info)?;
    let doc = mukade::hop::document(&session, None);
    let messages = doc
        .get("messages")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let path = match to.filter(|to| !to.trim().is_empty()) {
        Some(to) => PathBuf::from(to),
        None => {
            let dir = data_dir()
                .ok_or_else(|| CmdError::failed("no mukade-data folder beside the executable"))?
                .join("exports");
            dir.join(export_name(location.harness.as_str(), id))
        }
    };
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .map_err(|error| CmdError::failed(format!("{}: {error}", parent.display())))?;
    }
    let text = serde_json::to_string_pretty(&doc).map_err(|e| CmdError::failed(e.to_string()))?;
    std::fs::write(&path, text)
        .map_err(|error| CmdError::failed(format!("{}: {error}", path.display())))?;
    Ok(Exported {
        path: path.display().to_string(),
        messages,
    })
}

/// F8: a document file written into an installation as a new session.
pub fn import(
    shared: &Shared,
    document: &str,
    into: usize,
    prompt_text: Option<&str>,
) -> CmdResult<ReceiptView> {
    let dest = shared.listing(into)?.location;
    mukade::harnesses::destination(dest.harness)?;
    let path = Path::new(document.trim().trim_matches('"'));
    let text = std::fs::read_to_string(path)
        .map_err(|error| CmdError::not_found(format!("{}: {error}", path.display())))?;
    let doc: Value =
        serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|error| {
            CmdError::refused(format!("{}: not a JSON document: {error}", path.display()))
        })?;
    let session = mukade::hop::read_document(&doc)?;
    let opts = ImportOpts {
        cwd: session.meta.cwd.clone(),
        roots: dest.roots.clone(),
        reasoning: ReasoningPolicy::default(),
        write: write_details_for(&dest, session.meta.cwd.as_deref()),
        resume: prompt(prompt_text),
        source_facts: SourceFacts::of(&session),
        compaction: "none".to_string(),
        summary: None,
        native_budget: None,
    };
    deliver(
        mukade::import_document(dest.harness, &doc, &session, &opts),
        into,
        &dest,
    )
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Resumed {
    pub launched: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refusal: Option<String>,
    pub command: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}

/// What a resume would run: the refusal, or the interactive command and its folder.
pub fn resume_plan(shared: &Shared, key: usize, id: &str) -> CmdResult<(Resumed, Location)> {
    let (location, info) = shared.info(key, id)?;
    if let Some(why) = &info.unreadable {
        return Err(CmdError::refused(format!("{id} cannot be read: {why}")));
    }
    if let Some(refusal) = mukade::harnesses::get(location.harness).resume_refusal(&info) {
        return Ok((
            Resumed {
                launched: false,
                refusal: Some(refusal),
                command: Vec::new(),
                cwd: info.cwd.clone(),
            },
            location,
        ));
    }
    let command = mukade::catalog::interactive_resume(
        location.harness,
        &location.engine(),
        &info.id,
        Some(Path::new(&info.path)),
    );
    let cwd = info.cwd.clone().or_else(|| {
        read(&location, &info)
            .ok()
            .and_then(|session| session.meta.cwd)
    });
    Ok((
        Resumed {
            launched: false,
            refusal: None,
            command,
            cwd,
        },
        location,
    ))
}

/// F9: open a new terminal window running the interactive resume.
pub fn resume(shared: &Shared, key: usize, id: &str) -> CmdResult<Resumed> {
    let (mut plan, location) = resume_plan(shared, key, id)?;
    if plan.refusal.is_some() {
        return Ok(plan);
    }
    let env =
        crate::terminal::store_env(|command| location.roots.apply_to(location.harness, command));
    let cwd = plan
        .cwd
        .as_deref()
        .map(Path::new)
        .filter(|cwd| cwd.is_dir());
    crate::terminal::launch(&plan.command, cwd, &env).map_err(CmdError::failed)?;
    plan.launched = true;
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Args {
        Args::parse(&list.iter().map(|arg| arg.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn the_command_line_takes_turns_and_any_number_of_scans() {
        assert_eq!(
            args(&["app"]),
            Args {
                turns: 50,
                scan: vec![]
            }
        );
        let parsed = args(&["app", "--turns", "0", "--scan", "D:/apps", "--scan=E:/more"]);
        assert_eq!(parsed.turns, 0);
        assert_eq!(parsed.scan, ["D:/apps", "E:/more"]);
        assert_eq!(args(&["app", "--turns=7"]).turns, 7);
        assert_eq!(args(&["app", "--turns", "x"]).turns, 50);
    }

    #[test]
    fn an_export_name_is_safe_for_a_file() {
        assert_eq!(export_name("codex", "a/b:c"), "codex-a_b_c.json");
    }

    #[test]
    fn options_name_their_choices_and_refuse_others() {
        assert!(matches!(compaction("none"), Ok(Compaction::None)));
        assert_eq!(compaction("portable").unwrap().name(), "portable");
        assert_eq!(compaction("summary").unwrap().name(), "summary");
        assert_eq!(compaction("native").unwrap_err().kind, "refused");
        assert_eq!(reasoning(None).unwrap(), ReasoningPolicy::DropForeign);
        assert_eq!(reasoning(Some("as_text")).unwrap(), ReasoningPolicy::AsText);
        assert!(reasoning(Some("keep")).is_err());
    }

    #[test]
    fn children_are_split_into_subagents_and_forks() {
        let session = |id: &str, parent: Option<&str>, kind: Option<ParentKind>| SessionInfo {
            id: id.into(),
            parent: parent.map(String::from),
            parent_kind: kind,
            ..Default::default()
        };
        let sessions = [
            session("main", None, None),
            session("a", Some("main"), Some(ParentKind::spawn())),
            session("f", Some("main"), Some(ParentKind::fork())),
            session("other", Some("elsewhere"), Some(ParentKind::spawn())),
        ];
        let (subagents, forks) = children_of(&sessions, "main");
        assert_eq!(
            (subagents, forks),
            (vec!["a".to_string()], vec!["f".to_string()])
        );
    }

    /// Fixture stores in scratch: a Claude Code store with one session, listed, read and exported.
    #[test]
    fn a_fixture_store_is_listed_read_and_exported() {
        let scratch = crate::scratch::Scratch::new("store");
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../fixtures");
        let file = first_claude_fixture(&fixture).expect("a Claude Code fixture");
        let store = scratch.join(".claude");
        let project = store.join("projects").join("C--work");
        std::fs::create_dir_all(&project).unwrap();
        let name = file.file_name().unwrap();
        std::fs::copy(&file, project.join(name)).unwrap();

        let mut roots = StoreRoots::default();
        let claude = mukade::harnesses::all()
            .into_iter()
            .find(|harness| harness.id().as_str() == "claude_code")
            .unwrap();
        *claude.store_root_mut(&mut roots) = Some(store.clone());
        let shared = Shared::new(roots, args(&["app"]));
        let inventory = inventory(&shared, &[]);
        let key = inventory
            .installations
            .iter()
            .find(|installation| installation.harness == "claude_code")
            .unwrap()
            .key;
        let rows = sessions(&shared, key).unwrap();
        assert_eq!(rows.len(), 1, "{rows:?}");
        let id = rows[0].id.clone();
        let view = transcript(&shared, key, &id, 0).unwrap();
        assert!(!view.entries.is_empty());
        let first = &view.entries[0].id;
        assert!(!raw(&shared, key, &id, first).unwrap().is_empty());
        let to = scratch.join("out").join("doc.json");
        let exported = export(&shared, key, &id, Some(to.to_str().unwrap())).unwrap();
        assert!(exported.messages > 0);
        assert!(to.is_file());
        assert_eq!(
            transcript(&shared, key, "missing", 0).unwrap_err().kind,
            "not_found"
        );
    }

    /// A real hop, fixture to fixture: a Claude Code session written into every other writable
    /// harness's store in scratch, each with a receipt, a new id and a saved manifest.
    #[test]
    fn a_fixture_session_hops_into_every_writable_harness() {
        let scratch = crate::scratch::Scratch::new("hop");
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../fixtures");
        let file = first_claude_fixture(&fixture).expect("a Claude Code fixture");
        let mut roots = StoreRoots::default();
        for harness in mukade::harnesses::all() {
            let name = harness.id().as_str();
            let store = if mukade::catalog::store_is_file(harness.id()) {
                scratch.join(name).join("store.db")
            } else {
                scratch.join(name)
            };
            std::fs::create_dir_all(store.parent().unwrap()).unwrap();
            *harness.store_root_mut(&mut roots) = Some(store);
        }
        let claude = mukade::harnesses::all()
            .into_iter()
            .find(|harness| harness.id().as_str() == "claude_code")
            .unwrap();
        let project = claude
            .store_root(&roots)
            .unwrap()
            .join("projects")
            .join("C--work");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::copy(&file, project.join(file.file_name().unwrap())).unwrap();
        // SAFETY of the test: the manifest goes into scratch, not beside the test binary.
        unsafe { std::env::set_var("MUKADE_DATA_DIR", scratch.join("mukade-data")) };

        let shared = Shared::new(roots, args(&["app"]));
        let listed = inventory(&shared, &[]);
        let source = listed
            .installations
            .iter()
            .find(|installation| installation.harness == "claude_code")
            .unwrap()
            .key;
        let id = sessions(&shared, source).unwrap()[0].id.clone();
        let options = hop_options(&shared, source, &id).unwrap();
        assert!(!options.destinations.is_empty());
        let mut written = Vec::new();
        for dest in &options.destinations {
            let result = hop(
                &shared,
                &HopArgs {
                    source,
                    id: id.clone(),
                    dest: dest.key,
                    compaction: "none".into(),
                    children: false,
                    all_branches: false,
                    reasoning: None,
                    dest_shell: Some("posix".into()),
                    prompt: None,
                },
            );
            match result {
                Ok(receipt) => {
                    assert!(!receipt.dest_id.is_empty(), "{}", dest.label);
                    assert!(
                        receipt
                            .manifest_path
                            .as_deref()
                            .is_some_and(|p| Path::new(p).is_file())
                    );
                    written.push(dest.label.clone());
                }
                // A destination that needs its own binary to write (none is installed here)
                // refuses cleanly: nothing was written, and the window shows why.
                Err(error) => {
                    eprintln!("{} refused: {} {}", dest.label, error.kind, error.message);
                    assert_ne!(error.kind, "not_found", "{}: {}", dest.label, error.message);
                }
            }
        }
        assert!(!written.is_empty(), "no destination took the hop");
        let reloaded = inventory(&shared, &[]);
        let total: usize = reloaded.installations.iter().map(|i| i.count).sum();
        assert!(total > 1, "the written sessions are listed after a reload");
        eprintln!("hopped into: {written:?}");
    }

    fn first_claude_fixture(root: &Path) -> Option<PathBuf> {
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let mut entries: Vec<_> = std::fs::read_dir(&dir).ok()?.flatten().collect();
            entries.sort_by_key(|entry| entry.path());
            for entry in entries {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|ext| ext == "jsonl")
                    && path.to_string_lossy().contains("claude")
                    && mukade::reader::claude_code::read_file(&path)
                        .is_ok_and(|session| session.conversation().count() > 1)
                {
                    return Some(path);
                }
            }
        }
        None
    }
}
