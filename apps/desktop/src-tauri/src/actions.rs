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
use mukade::handoff;
use mukade::hop::{
    Compaction, HopError, HopRequest, ImportOpts, KeepFrom, ResumePrompt, SessionRef, SourceFacts,
    SummarySource, WriteDetails,
};
use mukade::model::{ParentKind, Session};
use mukade::native_compaction::NativeBudget;
use mukade::policy::ReasoningPolicy;
use mukade::reader::SessionInfo;
use mukade::summary::SummarizeOpts;
use mukade::tools::Shell;
use mukade::work::{Work, work_in};
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
    /// The model the destination's own configuration names, when it names one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
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
    /// The intents a summary can be asked with (the prompt folder's names), the default first.
    pub intents: Vec<String>,
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
            model: dest::configured_model_in(location.harness, &location.roots)
                .and_then(|(_, model)| model),
        })
        .collect();
    let (subagents, forks) = children_of(&state.listed(key).unwrap_or_default(), id);
    // Whether the reader keeps branches at all, as the library answers without reading anything.
    let all_branches_supported = mukade::hop::reads_all_branches(source.harness);
    Ok(HopOptions {
        source_label: source.label,
        title: label(&info),
        destinations,
        compactions: vec!["none", "portable", "summary", "native"],
        intents: intents(),
        subagents,
        forks,
        all_branches_supported,
        live: info.live,
    })
}

/// The names of the summary intents, the default one first; only the default when the prompt
/// folder cannot be listed.
fn intents() -> Vec<String> {
    let default = mukade::intent::DEFAULT_INTENT.to_string();
    let mut names: Vec<String> = mukade::intent::list()
        .unwrap_or_default()
        .into_iter()
        .map(|entry| entry.name)
        .filter(|name| *name != default)
        .collect();
    names.insert(0, default);
    names
}

#[derive(Debug, Clone, Default, Deserialize)]
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
    /// Summary: the intent its prompt is taken from (none: the default one).
    #[serde(default)]
    pub intent: Option<String>,
    /// Summary: the model asked (none: the session's own last model).
    #[serde(default)]
    pub summary_model: Option<String>,
    /// Summary: the last user turns that travel whole after it.
    #[serde(default)]
    pub keep_turns: Option<usize>,
    /// Native: the destination model's context window, in tokens. Required with it.
    #[serde(default)]
    pub context_window: Option<u64>,
    /// Native: the destination model's output limit, in tokens.
    #[serde(default)]
    pub max_output_tokens: Option<u64>,
    /// The destination's model and provider, instead of the ones its configuration names.
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub model_provider: Option<String>,
    /// Write the conversation without its tool calls and results.
    #[serde(default)]
    pub without_tools: bool,
    /// `keep` (or none) or `drop`: the work of the turns the source never finished.
    #[serde(default)]
    pub interrupted: Option<String>,
}

/// A text option that was filled in.
fn given(text: &Option<String>) -> Option<String> {
    text.as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn compaction(args: &HopArgs) -> CmdResult<Compaction> {
    Ok(match args.compaction.as_str() {
        "none" => Compaction::None,
        "portable" => Compaction::Portable {
            summary: SummarySource::SourceRecord,
            keep_from: KeepFrom::SourceBoundary,
        },
        "summary" => {
            let prompt =
                mukade::intent::resolve(given(&args.intent).as_deref()).map_err(CmdError::refused)?;
            Compaction::Portable {
                summary: SummarySource::SourceModel(SummarizeOpts {
                    model: given(&args.summary_model),
                    intent: Some(prompt.name),
                    prompt: Some(prompt.text),
                    ..SummarizeOpts::default()
                }),
                keep_from: KeepFrom::LastTurns(args.keep_turns.unwrap_or(0)),
            }
        }
        // The caller states the window: Mukade never reads another product's catalog.
        "native" => match args.context_window {
            Some(context) if context > 0 => Compaction::Native(NativeBudget {
                context,
                max_output_tokens: args.max_output_tokens,
                text_token_factor: None,
            }),
            _ => {
                return Err(CmdError::refused(
                    "native compaction needs the destination model's context window, in tokens",
                ));
            }
        },
        other => {
            return Err(CmdError::refused(format!(
                "unknown compaction {other} (none, portable, summary, native)"
            )));
        }
    })
}

fn interrupted(name: Option<&str>) -> CmdResult<mukade::writer::Interrupted> {
    Ok(match name {
        None | Some("keep") => mukade::writer::Interrupted::Keep,
        Some("drop") => mukade::writer::Interrupted::Drop,
        Some(other) => {
            return Err(CmdError::refused(format!(
                "unknown interrupted {other} (keep, drop)"
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
/// `hop_into` does the hop, so its refusals and its registry hold here as in the CLI.
pub fn hop(state: &AppState, args: &HopArgs) -> CmdResult<ReceiptView> {
    let (source, info) = readable(state, &args.source, &args.id)?;
    let dest = state.location(&args.dest)?;
    let compaction = compaction(args)?;
    let interrupted = interrupted(args.interrupted.as_deref())?;
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
    if let Some(model) = given(&args.model) {
        write.model = Some(model);
    }
    if let Some(provider) = given(&args.model_provider) {
        write.model_provider = Some(provider);
    }
    write.without_tools = args.without_tools;
    write.interrupted = interrupted;
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

/// The folder of a session that has one: refused with the library's reason otherwise.
fn folder_of(info: &SessionInfo) -> CmdResult<PathBuf> {
    match handoff::refusal(info) {
        Some(why) => Err(CmdError::refused(why)),
        None => Ok(PathBuf::from(info.cwd.as_deref().unwrap_or_default())),
    }
}

/// The git work in a listed session's folder, read-only. Unreadable sessions are the point of
/// this, so it does not ask for a readable one.
pub fn work(state: &AppState, key: &str, id: &str) -> CmdResult<Work> {
    state.location(key)?;
    let info = state.listed_session(key, id)?;
    Ok(work_in(&folder_of(&info)?))
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct HandoffOptions {
    /// Why this session cannot be handed off; the other fields are then empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refusal: Option<String>,
    /// The locations of a harness a handoff can start.
    pub destinations: Vec<Destination>,
    /// The composed note, for the person to edit.
    pub note: String,
    pub cwd: Option<String>,
}

/// The folder of the editable template, beside the executable.
fn template_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.join(handoff::FOLDER))
}

/// What a handoff offers for a listed session: its destinations and the note, or why not.
pub fn handoff_options(state: &AppState, key: &str, id: &str) -> CmdResult<HandoffOptions> {
    handoff_options_in(state, key, id, template_dir().as_deref())
}

/// [`handoff_options`] with the template folder stated.
fn handoff_options_in(
    state: &AppState,
    key: &str,
    id: &str,
    templates: Option<&Path>,
) -> CmdResult<HandoffOptions> {
    let location = state.location(key)?;
    let info = state.listed_session(key, id)?;
    if let Some(why) = handoff::refusal(&info) {
        return Ok(HandoffOptions {
            refusal: Some(why),
            destinations: Vec::new(),
            note: String::new(),
            cwd: None,
        });
    }
    let folder = folder_of(&info)?;
    let note = handoff::note(
        &handoff::template(templates),
        location.harness,
        &info,
        &work_in(&folder),
    );
    let starts = handoff::destinations();
    let destinations = state
        .locations()
        .into_iter()
        .filter(|candidate| starts.contains(&candidate.harness))
        .map(|candidate| Destination {
            key: candidate.key(),
            label: candidate.label.clone(),
            default_shell: None,
            model: None,
        })
        .collect();
    Ok(HandoffOptions {
        refusal: None,
        destinations,
        note,
        cwd: Some(folder.display().to_string()),
    })
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct HandedOff {
    pub launched: bool,
    /// The command the destination's harness runs, its first prompt pointing at the note.
    pub command: Vec<String>,
    pub note_path: String,
    pub cwd: String,
}

/// What a handoff would do, after saving the note under `data`: the command and the destination
/// it runs in.
fn handoff_plan(
    state: &AppState,
    key: &str,
    id: &str,
    dest: &str,
    note: &str,
    data: Option<&Path>,
) -> CmdResult<(HandedOff, Location)> {
    let source = state.location(key)?;
    let info = state.listed_session(key, id)?;
    let folder = folder_of(&info)?;
    if !folder.is_dir() {
        return Err(CmdError::refused(format!(
            "{} does not exist",
            folder.display()
        )));
    }
    let dest = state.location(dest)?;
    if !handoff::destinations().contains(&dest.harness) {
        return Err(CmdError::refused(format!(
            "{} cannot start from a handoff note",
            dest.label
        )));
    }
    let data =
        data.ok_or_else(|| CmdError::refused("no mukade-data folder beside the executable"))?;
    let path = handoff::note_path(data, source.harness, &info.id);
    handoff::save(note, &path)
        .map_err(|error| CmdError::failed(format!("{}: {error}", path.display())))?;
    let prompt = handoff::first_prompt(&path);
    let command = mukade::catalog::interactive_start(dest.harness, &dest.engine(), &prompt)
        .ok_or_else(|| {
            CmdError::refused(format!("{} cannot start from a handoff note", dest.label))
        })?;
    Ok((
        HandedOff {
            launched: false,
            command,
            note_path: path.display().to_string(),
            cwd: folder.display().to_string(),
        },
        dest,
    ))
}

/// Saves `note` as edited, then opens `dest`'s harness on it in a terminal, in the session's
/// folder.
pub fn handoff(
    state: &AppState,
    key: &str,
    id: &str,
    dest: &str,
    note: &str,
) -> CmdResult<HandedOff> {
    let (mut handed, dest) = handoff_plan(state, key, id, dest, note, data_dir().as_deref())?;
    let env = crate::terminal::store_env(|command| dest.roots.apply_to(dest.harness, command));
    crate::terminal::launch(&handed.command, Some(Path::new(&handed.cwd)), &env)
        .map_err(CmdError::failed)?;
    handed.launched = true;
    Ok(handed)
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
        let named = |name: &str, context_window| HopArgs {
            compaction: name.into(),
            context_window,
            ..Default::default()
        };
        assert!(matches!(
            compaction(&named("none", None)),
            Ok(Compaction::None)
        ));
        assert_eq!(
            compaction(&named("portable", None)).unwrap().name(),
            "portable"
        );
        assert_eq!(
            compaction(&named("summary", None)).unwrap().name(),
            "summary"
        );
        // Native needs the destination model's window: nobody guesses it.
        assert_eq!(
            compaction(&named("native", None)).unwrap_err().kind,
            "refused"
        );
        assert_eq!(
            compaction(&named("native", Some(200_000))).unwrap().name(),
            "native"
        );
        assert_eq!(
            compaction(&named("other", None)).unwrap_err().kind,
            "refused"
        );
        assert_eq!(
            interrupted(Some("drop")).unwrap(),
            mukade::writer::Interrupted::Drop
        );
        assert!(interrupted(Some("other")).is_err());
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
                dest_shell: Some("posix".into()),
                ..Default::default()
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
        check_import_recovery(&state, &scratch);
    }

    /// Reuse the isolated stores and manifest directory from the fixture hop.
    fn check_import_recovery(state: &AppState, scratch: &Scratch) {
        use crate::state::{WriteKind, WriteOutcome};
        let target = state.location("pi").unwrap();
        let before = crate::sessions::list(&target).unwrap().len();
        let file = scratch.join("import.json");
        let path = file.to_str().unwrap();
        export(state, "claude_code", CLAUDE_FIXTURE, Some(path)).unwrap();

        // Hold the scheduled import until the replacement renderer has attempted to resubmit.
        let mut scheduled = None;
        let pending = state
            .start_write(WriteKind::Import, |id| scheduled = Some(id))
            .unwrap();
        let replacement = state.clone();
        assert_eq!(
            replacement
                .start_write(WriteKind::Import, |_| panic!("duplicate import"))
                .unwrap(),
            pending
        );
        let id = scheduled.expect("the first request scheduled a worker");
        state.finish_write(id, import(state, path, "pi", None));
        let completed = replacement.write_status().unwrap();
        assert_eq!(completed.id, pending.id);
        let WriteOutcome::Completed { receipt } = &completed.outcome else {
            panic!("import did not complete: {completed:?}");
        };
        let sessions = crate::sessions::list(&target).unwrap();
        assert_eq!(sessions.len(), before + 1);
        assert!(sessions.iter().any(|row| row.id == receipt.dest_id));
        assert_eq!(
            replacement
                .start_write(WriteKind::Hop, |_| panic!("unacknowledged import"))
                .unwrap(),
            completed
        );
        replacement.acknowledge_write(id);
        assert!(state.write_status().is_none());
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
            ..Default::default()
        };
        let error = hop(&state, &args).unwrap_err();
        assert_eq!(error.kind, "refused", "{}", error.message);
        assert!(error.message.contains("every branch"), "{}", error.message);
    }

    /// A state whose Codex location lists one session that cannot be read (its conversation lives
    /// on a server), with `folder` as its project.
    fn hosted(folder: Option<&Path>) -> AppState {
        let state = AppState::new(StoreRoots::default());
        state.set_listed(
            "codex",
            vec![SessionInfo {
                id: "h1".into(),
                title: Some("Port the parser".into()),
                cwd: folder.map(|folder| folder.display().to_string()),
                updated: Some("2026-11-01T10:00:00Z".into()),
                unreadable: Some("its conversation lives on the ChatGPT servers".into()),
                ..Default::default()
            }],
        );
        state
    }

    /// A project folder in `scratch`, and the state that lists a hosted chat in it.
    fn project(scratch: &Scratch) -> (PathBuf, AppState) {
        let folder = scratch.join("project");
        std::fs::create_dir_all(&folder).unwrap();
        let state = hosted(Some(folder.as_path()));
        (folder, state)
    }

    #[test]
    fn the_work_of_an_unreadable_session_is_reported() {
        let scratch = Scratch::new("work-hosted");
        let (folder, state) = project(&scratch);
        let found = work(&state, "codex", "h1").unwrap();
        assert_eq!(found.folder, folder);
        // Not a repository: nothing to list, and the reason is said.
        assert!(found.repositories.is_empty());
        let note = found.note.expect("a note");
        assert!(note.contains("no git repository"), "{note}");
    }

    #[test]
    fn the_work_of_a_session_without_a_folder_is_refused_with_the_reason() {
        let state = hosted(None);
        let error = work(&state, "codex", "h1").unwrap_err();
        assert_eq!(error.kind, "refused");
        assert!(error.message.contains("no folder"), "{}", error.message);
        assert_eq!(work(&state, "codex", "nope").unwrap_err().kind, "not_found");
    }

    #[test]
    fn the_handoff_offers_the_locations_of_claude_code_and_codex() {
        let scratch = Scratch::new("handoff-options");
        let (folder, state) = project(&scratch);
        let options = handoff_options_in(&state, "codex", "h1", None).unwrap();
        assert_eq!(options.refusal, None);
        let keys: Vec<&str> = options
            .destinations
            .iter()
            .map(|destination| destination.key.as_str())
            .collect();
        assert_eq!(keys, ["claude_code", "codex"]);
        assert!(options.note.contains("Port the parser"), "{}", options.note);
        assert_eq!(options.cwd, Some(folder.display().to_string()));
    }

    #[test]
    fn a_session_without_a_folder_has_no_handoff_options_but_a_reason() {
        let state = hosted(None);
        let options = handoff_options_in(&state, "codex", "h1", None).unwrap();
        assert!(
            options
                .refusal
                .as_deref()
                .is_some_and(|why| why.contains("no folder")),
            "{options:?}"
        );
        assert!(options.destinations.is_empty());
        assert!(options.note.is_empty());
        assert_eq!(options.cwd, None);
    }

    #[test]
    fn a_handoff_toward_a_harness_without_a_start_command_is_refused() {
        let scratch = Scratch::new("handoff-pi");
        let (_, state) = project(&scratch);
        let data = scratch.join("data");
        let error =
            handoff_plan(&state, "codex", "h1", "pi", "note", Some(data.as_path())).unwrap_err();
        assert_eq!(error.kind, "refused", "{}", error.message);
        // Nothing was saved.
        assert!(!data.exists());
    }

    #[test]
    fn a_handoff_saves_the_edited_note_and_points_the_destination_at_it() {
        let scratch = Scratch::new("handoff-plan");
        let (folder, state) = project(&scratch);
        let data = scratch.join("data");
        let (handed, dest) = handoff_plan(
            &state,
            "codex",
            "h1",
            "claude_code",
            "my edited note",
            Some(data.as_path()),
        )
        .unwrap();
        assert_eq!(dest.key(), "claude_code");
        assert!(!handed.launched);
        let saved = mukade::handoff::note_path(&data, mukade::model::HarnessId::Codex, "h1");
        assert_eq!(handed.note_path, saved.display().to_string());
        assert_eq!(std::fs::read_to_string(&saved).unwrap(), "my edited note");
        assert_eq!(handed.cwd, folder.display().to_string());
        assert_eq!(
            handed.command.last().map(String::as_str),
            Some(mukade::handoff::first_prompt(&saved).as_str())
        );
    }

    #[test]
    fn a_handoff_needs_the_folder_and_a_data_folder() {
        let scratch = Scratch::new("handoff-needs");
        let (_, state) = project(&scratch);
        let data = scratch.join("data");
        let error = handoff_plan(&state, "codex", "h1", "claude_code", "n", None).unwrap_err();
        assert_eq!(error.kind, "refused", "{}", error.message);
        let gone = hosted(Some(scratch.join("gone").as_path()));
        let error = handoff_plan(
            &gone,
            "codex",
            "h1",
            "claude_code",
            "n",
            Some(data.as_path()),
        )
        .unwrap_err();
        assert_eq!(error.kind, "refused");
        assert!(
            error.message.contains("does not exist"),
            "{}",
            error.message
        );
        let none = hosted(None);
        let error = handoff_plan(
            &none,
            "codex",
            "h1",
            "claude_code",
            "n",
            Some(data.as_path()),
        )
        .unwrap_err();
        assert_eq!(error.kind, "refused");
        assert!(!data.exists());
    }
}
