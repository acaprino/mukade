//! What the window does with a session besides reading it: hop it, export it, import a document,
//! resume it in a terminal. Plain functions over [`AppState`], so the tests call them without a
//! window; `commands` only moves them off the window's thread.
//!
//! Every per-harness answer comes from the library (`mukade::harnesses::get(..)`, `mukade::dest`,
//! `mukade::catalog`); nothing here branches on a harness (`tests/architecture.rs`).

use crate::error::{CmdError, CmdResult};
use crate::locations::{self, Location};
use crate::state::AppState;
use crate::view::{self, ReceiptView};
use mukade::dest;
use mukade::exe;
use mukade::hop::{
    Compaction, HopError, HopRequest, ImportOpts, KeepFrom, ResumePrompt, SessionRef, SourceFacts,
    SummarySource, WriteDetails,
};
use mukade::model::{ParentKind, Session};
use mukade::policy::ReasoningPolicy;
use mukade::reader::SessionInfo;
use mukade::summary::SummarizeOpts;
use mukade::tools::Shell;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// The listing's title, else the id.
fn label(info: &SessionInfo) -> String {
    info.title
        .clone()
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| info.id.clone())
}

/// A listed session that can be read: refused with the listing's reason otherwise.
fn readable(state: &AppState, key: &str, id: &str) -> CmdResult<(Location, SessionInfo)> {
    let location = state.location(key)?;
    let info = state.listed_session(key, id)?;
    if let Some(why) = &info.unreadable {
        return Err(CmdError::refused(format!("{id} cannot be read: {why}")));
    }
    Ok((location, info))
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Destination {
    pub key: String,
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

/// The titles of the listed sessions that came from `id`, subagents and forks apart.
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

pub fn hop_options(state: &AppState, key: &str, id: &str) -> CmdResult<HopOptions> {
    let (source, info) = readable(state, key, id)?;
    let cwd = info.cwd.as_deref().map(Path::new);
    let all = state.locations();
    let destinations = locations::destinations(&all, &source)
        .into_iter()
        .map(|location| Destination {
            key: location.key(),
            label: location.label.clone(),
            default_shell: dest::shell_in(location.harness, &location.roots, cwd).map(shell_name),
        })
        .collect();
    let (subagents, forks) = children_of(&state.listed(key).unwrap_or_default(), id);
    // Whether the reader keeps branches at all, as the library answers without reading anything.
    let all_branches_supported = mukade::hop::reads_all_branches(source.harness);
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
    pub source: String,
    pub id: String,
    pub dest: String,
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
fn deliver(result: Result<mukade::Receipt, HopError>, dest: &Location) -> CmdResult<ReceiptView> {
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
    Ok(view::receipt(
        &receipt,
        &dest.key(),
        dest,
        manifest,
        warning,
    ))
}

fn prompt(text: Option<&str>) -> Option<ResumePrompt> {
    text.map(str::trim)
        .filter(|text| !text.is_empty())
        .map(|text| ResumePrompt::new(text.to_string()))
}

/// F6: read the source from its own installation, compact it there, write it into the
/// destination's stores (two installations of one harness keep theirs apart). The library's
/// `hop_into` does the hop, so its refusals and its registry hold here as in the CLI and the TUI.
pub fn hop(state: &AppState, args: &HopArgs) -> CmdResult<ReceiptView> {
    let (source, info) = readable(state, &args.source, &args.id)?;
    let dest = state.location(&args.dest)?;
    let compaction = compaction(&args.compaction)?;
    let reasoning = reasoning(args.reasoning.as_deref())?;
    let reference = SessionRef::id(source.harness, info.id.clone());
    // The destination's facts depend on the folder: the listing's, else the session's own, read
    // only when the window holds no copy of it.
    let cwd = match info.cwd.clone() {
        Some(cwd) => Some(cwd),
        None => match state.cached_session(&args.source, &info.id) {
            Some(session) => session.meta.cwd.clone(),
            None => mukade::export(&reference, &source.roots)?.meta.cwd,
        },
    };
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
    let request = HopRequest {
        cwd,
        compaction,
        resume: prompt(args.prompt.as_deref()),
        reasoning,
        write,
        children: args.children,
        all_branches: args.all_branches,
        ..HopRequest::new(reference, dest.harness, source.roots.clone())
    };
    deliver(mukade::hop_into(&request, &dest.roots), &dest)
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
pub fn export(state: &AppState, key: &str, id: &str, to: Option<&str>) -> CmdResult<Exported> {
    let (location, _) = readable(state, key, id)?;
    let session: Session = match state.cached_session(key, id) {
        Some(session) => (*session).clone(),
        None => crate::sessions::read(&location, id)?,
    };
    let doc = mukade::hop::document(&session, None);
    let messages = doc
        .get("messages")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let path = match to.filter(|to| !to.trim().is_empty()) {
        Some(to) => PathBuf::from(to),
        None => data_dir()
            .ok_or_else(|| CmdError::failed("no mukade-data folder beside the executable"))?
            .join("exports")
            .join(export_name(location.harness.as_str(), id)),
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
    state: &AppState,
    document: &str,
    into: &str,
    prompt_text: Option<&str>,
) -> CmdResult<ReceiptView> {
    let dest = state.location(into)?;
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
        &dest,
    )
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Resumed {
    pub launched: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refusal: Option<String>,
    /// The interactive command launched, or with a refusal the headless one that continues it.
    pub command: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}

/// What a resume would run: the refusal and the command that continues it instead, or the
/// interactive command and its folder.
pub fn resume_plan(state: &AppState, key: &str, id: &str) -> CmdResult<(Resumed, Location)> {
    let (location, info) = readable(state, key, id)?;
    if let Some(refusal) = mukade::harnesses::get(location.harness).resume_refusal(&info) {
        let command = mukade::catalog::resume(
            location.harness,
            &location.engine(),
            &info.id,
            Some(Path::new(&info.path)),
        );
        return Ok((
            Resumed {
                launched: false,
                refusal: Some(refusal),
                command,
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
        crate::sessions::read(&location, &info.id)
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
pub fn resume(state: &AppState, key: &str, id: &str) -> CmdResult<Resumed> {
    let (mut plan, location) = resume_plan(state, key, id)?;
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
    use crate::scratch::Scratch;
    use crate::testing::{CLAUDE_FIXTURE, claude_store};
    use mukade::roots::StoreRoots;

    /// The state over `roots`, listed as `installations` lists it.
    fn listed(roots: StoreRoots) -> AppState {
        let state = AppState::new(roots);
        crate::commands::installations_for(&state, &[]);
        state
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

    #[test]
    fn the_branch_probe_answers_without_a_session() {
        let answers: Vec<bool> = mukade::harnesses::all()
            .into_iter()
            .map(|harness| mukade::hop::reads_all_branches(harness.id()))
            .collect();
        assert!(
            answers.iter().any(|&kept| kept),
            "some reader keeps branches"
        );
        assert!(
            answers.iter().any(|&kept| !kept),
            "some reader keeps one branch"
        );
    }

    #[test]
    fn a_listed_session_is_exported_to_the_path_asked() {
        let scratch = Scratch::new("export");
        let state = listed(claude_store(&scratch));
        let to = scratch.join("out").join("doc.json");
        let exported = export(
            &state,
            "claude_code",
            CLAUDE_FIXTURE,
            Some(to.to_str().unwrap()),
        )
        .unwrap();
        assert!(exported.messages > 0);
        assert!(to.is_file());
        assert_eq!(
            export(&state, "claude_code", "missing", None)
                .unwrap_err()
                .kind,
            "not_found"
        );
    }

    #[test]
    fn a_refused_resume_carries_the_command_that_continues_it() {
        let refusing = mukade::harnesses::all().into_iter().find(|harness| {
            harness
                .resume_refusal(&SessionInfo {
                    id: "s".into(),
                    ..Default::default()
                })
                .is_some()
        });
        let Some(harness) = refusing else { return };
        let state = AppState::new(StoreRoots::default());
        let key = harness.id().as_str();
        state.set_listed(
            key,
            vec![SessionInfo {
                id: "s1".into(),
                path: "s1.log".into(),
                ..Default::default()
            }],
        );
        let (plan, _) = resume_plan(&state, key, "s1").unwrap();
        assert!(plan.refusal.is_some());
        assert!(
            plan.command.iter().any(|part| part.contains("s1")),
            "{:?}",
            plan.command
        );
    }

    /// A real hop, fixture to fixture: the Claude Code fixture written into every other writable
    /// harness's store in scratch, each with a receipt, a new id and a saved manifest; the ones
    /// that need their own binary (none is installed here) refuse cleanly.
    #[test]
    fn a_fixture_session_hops_into_every_writable_harness() {
        let scratch = Scratch::new("hop");
        let mut roots = claude_store(&scratch);
        for harness in mukade::harnesses::all() {
            if harness.store_root(&roots).is_some() {
                continue;
            }
            let name = harness.id().as_str();
            let store = if mukade::catalog::store_is_file(harness.id()) {
                scratch.join(name).join("store.db")
            } else {
                scratch.join(name)
            };
            std::fs::create_dir_all(store.parent().unwrap()).unwrap();
            *harness.store_root_mut(&mut roots) = Some(store);
        }
        // SAFETY of the test: the manifest goes into scratch, not beside the test binary.
        unsafe { std::env::set_var("MUKADE_DATA_DIR", scratch.join("mukade-data")) };
        let state = listed(roots);
        let options = hop_options(&state, "claude_code", CLAUDE_FIXTURE).unwrap();
        assert!(!options.destinations.is_empty());
        let mut written = Vec::new();
        for dest in &options.destinations {
            let args = HopArgs {
                source: "claude_code".into(),
                id: CLAUDE_FIXTURE.into(),
                dest: dest.key.clone(),
                compaction: "none".into(),
                children: false,
                all_branches: false,
                reasoning: None,
                dest_shell: Some("posix".into()),
                prompt: None,
            };
            match hop(&state, &args) {
                Ok(receipt) => {
                    assert!(!receipt.dest_id.is_empty(), "{}", dest.label);
                    assert_eq!(receipt.dest_key, dest.key);
                    assert!(
                        receipt
                            .manifest_path
                            .as_deref()
                            .is_some_and(|p| Path::new(p).is_file())
                    );
                    written.push(dest.label.clone());
                }
                Err(error) => {
                    eprintln!("{} refused: {} {}", dest.label, error.kind, error.message);
                    assert_ne!(error.kind, "not_found", "{}: {}", dest.label, error.message);
                }
            }
        }
        assert!(!written.is_empty(), "no destination took the hop");
        // The window's hops go through the library's, so its registry remembers them.
        let registry = scratch.join("mukade-data").join("registry");
        let records = records_under(&registry);
        assert!(
            records >= written.len(),
            "{records} records for {} writes in {}",
            written.len(),
            registry.display()
        );
    }

    /// The records the registry files in `dir` and its folders hold (each file is a JSON list).
    fn records_under(dir: &Path) -> usize {
        std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| match entry.file_type() {
                Ok(kind) if kind.is_dir() => records_under(&entry.path()),
                Ok(_) => std::fs::read_to_string(entry.path())
                    .ok()
                    .and_then(|text| serde_json::from_str::<Vec<Value>>(&text).ok())
                    .map_or(0, |records| records.len()),
                Err(_) => 0,
            })
            .sum()
    }

    /// The library's refusal reaches the window: a summary from the source's model stands for the
    /// branch the source resumes, so it does not go with every branch.
    #[test]
    fn a_summary_with_every_branch_is_refused() {
        let scratch = Scratch::new("hop-branches");
        let state = listed(claude_store(&scratch));
        let args = HopArgs {
            source: "claude_code".into(),
            id: CLAUDE_FIXTURE.into(),
            dest: "claude_code".into(),
            compaction: "summary".into(),
            children: false,
            all_branches: true,
            reasoning: None,
            dest_shell: None,
            prompt: None,
        };
        let error = hop(&state, &args).unwrap_err();
        assert_eq!(error.kind, "refused", "{}", error.message);
        assert!(error.message.contains("every branch"), "{}", error.message);
    }
}
