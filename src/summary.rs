//! A summary of a session written by the source harness's own model (design
//! `docs/archive/design/source-model-summary.md`).
//!
//! mukade holds no API key and calls no provider: the summary is asked of the source harness's own
//! binary, with its own credentials and configuration, in a run that saves nothing in its store.
//! What comes back travels like the source's own compaction does (§4.2): one handoff message, then
//! the turns the caller chose to keep.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::hop::HopError;
use crate::model::{HarnessId, Kind, ModelRef, Session};
use crate::roots::StoreRoots;

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
    /// The prompt to ask with; `None` uses [`SUMMARY_PROMPT`] (the generic intent).
    pub prompt: Option<String>,
    /// The intent the prompt came from, when it was selected by name.
    pub intent: Option<String>,
}

impl Default for SummarizeOpts {
    fn default() -> Self {
        SummarizeOpts {
            model: None,
            timeout: Duration::from_secs(300),
            exe: None,
            prompt: None,
            intent: None,
        }
    }
}

/// What the source harness's model wrote, and what is known about the run that wrote it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Summary {
    pub text: String,
    pub harness: HarnessId,
    pub harness_version: Option<String>,
    /// The model the run was told to use. `None`: none was named, so the harness's default
    /// answered (OpenCode when the session recorded a model without its provider).
    pub model: Option<String>,
    /// The tokens or cost the harness reported, as compact JSON.
    pub usage: Option<String>,
    /// The command that removes what the run could not clean up (an OpenCode fork it failed to delete).
    pub leftover: Option<String>,
    /// The intent prompt used, when the run was given one instead of [`SUMMARY_PROMPT`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
}

/// A summary as a hop used it: the text, and how many source entries it stands for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SummaryFacts {
    pub summary: Summary,
    pub left_out: usize,
}

/// What the source's model is asked (one line: a Windows `.cmd` launcher cannot take a line break,
/// and the characters `"%^&|<>!` are not safe there).
pub const SUMMARY_PROMPT: &str = "Write a handoff summary of this whole session for another coding agent that will continue the work from your summary alone. Do not call tools and do not continue the task: reply only with the summary, in the language the user wrote in. Cover: (1) the goal and what the user asked for, quoting the user where wording matters; (2) the current state, what is done and what is half done; (3) the decisions taken and why, and the approaches tried and dropped; (4) the files read or changed and how, and the commands and tests run with their results; (5) open problems, unanswered questions, and the next step. Keep it as short as it can be while a new agent could continue the work from it alone.";

/// One summarizing run of a source harness: what to start, from where, and where its reply lands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryCommand {
    pub program: String,
    pub args: Vec<String>,
    /// The session's own directory when it still exists; else the run starts from mukade's.
    pub cwd: Option<PathBuf>,
    /// Codex writes its last message to a file (`-o`) rather than only to stdout.
    pub output_file: Option<PathBuf>,
    /// Variables the run needs set (OpenCode's inline configuration).
    pub env: Vec<(String, String)>,
}

/// The command that asks `session`'s own harness for a summary and saves nothing in its store
/// (live record `docs/verification/source-model-summary-live.md`, "Final commands"). `scratch` is the
/// folder of this run, for what a harness has to write somewhere: Codex's last message, Pi's fork.
pub fn summary_command(
    session: &Session,
    opts: &SummarizeOpts,
    scratch: &Path,
) -> Result<SummaryCommand, HopError> {
    crate::harnesses::get(session.meta.harness).summary_command(session, opts, scratch)
}

/// The rest of a harness's summarizing command, the same for all: its program, the prompt as the
/// last argument, and the directory the session was in. `args` is what the harness adds before the
/// prompt.
pub(crate) fn finish_command(
    session: &Session,
    opts: &SummarizeOpts,
    mut args: Vec<String>,
    output_file: Option<PathBuf>,
    env: Vec<(String, String)>,
) -> Result<SummaryCommand, HopError> {
    let prompt = opts.prompt.as_deref().unwrap_or(SUMMARY_PROMPT);
    let program = crate::exe::resolve(session.meta.harness, opts.exe.as_deref());
    // A Windows `.cmd` shim cannot receive every argument. Native executables take any text; a
    // batch launcher is checked before it is spawned, never silently given a mangled prompt.
    if crate::exe::is_batch(&program) && !batch_safe(prompt) {
        return Err(HopError::Unsupported(format!(
            "the summary prompt cannot be passed through the batch launcher {program}: it must be \
             one line of ASCII letters, digits, spaces and .,;:() ; name the harness's native \
             executable, or edit the intent prompt in the intent-prompts folder to that subset"
        )));
    }
    args.push(prompt.into());
    Ok(SummaryCommand {
        program,
        args,
        cwd: session
            .meta
            .cwd
            .as_deref()
            .map(PathBuf::from)
            .filter(|dir| dir.is_dir()),
        output_file,
        env,
    })
}

/// The prompt a Windows `.cmd` launcher can take: one line, no `"%^&|<>!` and no leading dash.
fn batch_safe(prompt: &str) -> bool {
    !prompt.is_empty()
        && !prompt.starts_with('-')
        && !prompt.contains(['\n', '\r'])
        && prompt
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || " .,;:()".contains(c))
}

/// The model the conversation was last having: that of its last step, else the last the session
/// recorded. A subagent's model is recorded too, and a killed or failed Claude reply is recorded as
/// `<synthetic>`, which no harness can be asked for.
pub(crate) fn own_model(session: &Session) -> Option<&ModelRef> {
    let real = |model: &&ModelRef| !model.id.is_empty() && model.id != "<synthetic>";
    session
        .entries
        .iter()
        .rev()
        .find_map(|entry| match &entry.kind {
            Kind::Step { model, .. } => model.as_ref().filter(real),
            _ => None,
        })
        .or_else(|| session.meta.models.iter().rev().find(real))
}

/// The model flags: the one named, else the session's own last model, spelled by `recorded_args`.
pub(crate) fn model_args(
    flag: &str,
    named: Option<&str>,
    recorded: Option<&ModelRef>,
    recorded_args: impl FnOnce(&ModelRef) -> Vec<String>,
) -> Vec<String> {
    if let Some(model) = named {
        return vec![flag.into(), model.into()];
    }
    match recorded.filter(|model| !model.id.is_empty()) {
        Some(model) => recorded_args(model),
        None => Vec::new(),
    }
}

/// What a summarizing run answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub text: String,
    /// The tokens or cost the harness reported, as compact JSON.
    pub usage: Option<String>,
    /// The session OpenCode forked for the run, to be deleted.
    pub fork: Option<String>,
}

/// Read the summary where `harness` puts it: Claude Code's JSON `result`, the file Codex wrote
/// (`output_file` is its content), Pi's stdout, OpenCode's `text` events. An empty summary, or one
/// the harness itself marks as an error, is no summary.
pub fn parse_reply(
    harness: HarnessId,
    stdout: &str,
    output_file: Option<&str>,
) -> Result<Reply, String> {
    let mut reply = crate::harnesses::get(harness).parse_summary_reply(stdout, output_file)?;
    reply.text = reply.text.trim().to_string();
    if reply.text.is_empty() {
        return Err("the model returned no summary".into());
    }
    Ok(reply)
}

/// The session OpenCode forked for a run: the `sessionID` of the first event it printed, there as
/// soon as the fork exists, before any text.
pub fn opencode_fork(stdout: &str) -> Option<String> {
    crate::harnesses::opencode_fork(stdout)
}

/// Ask `session`'s own harness for a summary of it (design 2026-10-28). The run saves nothing in
/// the source's store: OpenCode's fork is deleted afterwards, and named in [`Summary::leftover`]
/// when it could not be. This run's folder under `mukade-data/tmp` is removed whatever happens.
pub fn summarize(
    session: &Session,
    roots: &StoreRoots,
    opts: &SummarizeOpts,
) -> Result<Summary, HopError> {
    let base =
        crate::portable::temp_dir().map_err(|error| HopError::SummaryFailed(error.to_string()))?;
    sweep_stale_runs(&base);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|age| age.as_nanos())
        .unwrap_or_default();
    // The pid names the run's owner for the sweep; the counter keeps two runs of one process
    // (two threads of a library caller) apart.
    static RUNS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let run = RUNS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let scratch = base.join(format!("summary-{}-{nanos}-{run}", std::process::id()));
    std::fs::create_dir_all(&scratch).map_err(|error| {
        HopError::SummaryFailed(format!("could not make {}: {error}", scratch.display()))
    })?;
    let result = summarize_in(session, roots, opts, &scratch);
    let _ = std::fs::remove_dir_all(&scratch);
    result
}

fn summarize_in(
    session: &Session,
    roots: &StoreRoots,
    opts: &SummarizeOpts,
    scratch: &Path,
) -> Result<Summary, HopError> {
    let harness = session.meta.harness;
    let command = summary_command(session, opts, scratch)?;
    // The prompt is long and the same every time: a failure names it by its version.
    let shown = {
        let mut words = vec![command.program.clone()];
        words.extend(command.args[..command.args.len() - 1].iter().cloned());
        words.push(format!("<summary prompt {SUMMARY_PROMPT_VERSION}>"));
        words.join(" ")
    };
    let mut process = Command::new(crate::exe::spawnable(&command.program));
    process.args(&command.args);
    if let Some(cwd) = &command.cwd {
        process.current_dir(cwd);
    }
    roots.apply_to(harness, &mut process);
    process.envs(command.env.iter().map(|(key, value)| (key, value)));
    let finished = crate::exe::run_within(&mut process, opts.timeout)
        .map_err(|error| HopError::SummaryFailed(format!("{shown}: {error}")))?;
    let stdout = String::from_utf8_lossy(&finished.stdout);
    let written = command
        .output_file
        .as_deref()
        .and_then(|file| std::fs::read(file).ok())
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned());
    let parsed = parse_reply(harness, &stdout, written.as_deref());
    // OpenCode's fork exists as soon as it names it: it goes whatever came of the run, and a
    // failure says how to remove one that stayed.
    let leftover = crate::harnesses::get(harness).after_summary(&command, roots, &stdout);
    let failed = |reason: String| {
        HopError::SummaryFailed(match &leftover {
            Some(command) => {
                format!("{reason} (the run left a fork in the source store: run {command})")
            }
            None => reason,
        })
    };
    let reply = match (finished.status, parsed) {
        (None, _) => {
            return Err(failed(format!(
                "{shown} did not finish within {:?}",
                opts.timeout
            )));
        }
        (Some(status), _) if !status.success() => {
            return Err(failed(format!(
                "{shown} exited with {status}: {}",
                String::from_utf8_lossy(&finished.stderr).trim()
            )));
        }
        (Some(_), Err(reason)) => return Err(failed(format!("{shown}: {reason}"))),
        (Some(_), Ok(reply)) => reply,
    };
    // The model the run was told to use: the one named, else the session's own when the
    // harness could spell it. Unnamed (OpenCode with a model recorded without its provider), the
    // harness's default answered, and which one that is is not known.
    let model = opts.model.clone().or_else(|| {
        own_model(session)
            .map(|model| model.id.clone())
            .filter(|id| names_model(&command.args, id))
    });
    Ok(Summary {
        text: reply.text,
        harness,
        harness_version: crate::exe::version_of(&command.program),
        model,
        usage: reply.usage,
        leftover,
        prompt: opts
            .prompt
            .as_ref()
            .map(|_| opts.intent.clone().unwrap_or_else(|| "custom".into())),
    })
}

/// Whether `args` name the model `id`: as an argument of its own, or as `<provider>/<id>`.
fn names_model(args: &[String], id: &str) -> bool {
    args.iter()
        .any(|arg| arg == id || arg.strip_suffix(id).is_some_and(|head| head.ends_with('/')))
}

/// The folders of runs a killed process left behind, removed once they are an hour old.
fn sweep_stale_runs(base: &Path) {
    let Ok(entries) = std::fs::read_dir(base) else {
        return;
    };
    let now = std::time::SystemTime::now();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        // A run whose process still runs is not stale, however long it takes.
        let owner_runs = name
            .strip_prefix("summary-")
            .and_then(|rest| rest.split('-').next())
            .and_then(|pid| pid.parse::<u32>().ok())
            .is_some_and(|pid| {
                pid == std::process::id()
                    || matches!(
                        crate::process::inspect(pid),
                        crate::process::Process::Running { .. }
                    )
            });
        let stale = name.starts_with("summary-")
            && !owner_runs
            && entry
                .metadata()
                .ok()
                .filter(|meta| meta.is_dir())
                .and_then(|meta| meta.modified().ok())
                .and_then(|modified| now.duration_since(modified).ok())
                .is_some_and(|age| age > crate::portable::STALE_AFTER);
        if stale {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    /// The model a run names is its own argument or the end of `<provider>/<id>`; a prompt that
    /// merely mentions it names none.
    #[test]
    fn a_model_is_named_by_its_argument() {
        let args = |list: &[&str]| list.iter().map(|a| a.to_string()).collect::<Vec<_>>();
        assert!(super::names_model(&args(&["--model", "gpt-9"]), "gpt-9"));
        assert!(super::names_model(&args(&["-m", "openai/gpt-9"]), "gpt-9"));
        assert!(!super::names_model(&args(&["run", "about gpt-9"]), "gpt-9"));
        assert!(!super::names_model(
            &args(&["-m", "openai/xgpt-9"]),
            "gpt-9"
        ));
    }

    /// An old run folder is swept unless the process named in it still runs: a long summary is
    /// never removed from under the run that owns it.
    #[cfg(unix)]
    #[test]
    fn the_sweep_spares_the_folder_of_a_running_process() {
        let base = std::env::temp_dir().join(format!("mukade-sweep-{}", uuid::Uuid::new_v4()));
        let old = std::time::SystemTime::now() - 2 * crate::portable::STALE_AFTER;
        let make = |name: &str| {
            let dir = base.join(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::File::open(&dir)
                .unwrap()
                .set_modified(old)
                .unwrap();
            dir
        };
        let mine = make(&format!("summary-{}-1-0", std::process::id()));
        // Pid 0 is never a process a summary runs in.
        let gone = make("summary-0-1-0");
        super::sweep_stale_runs(&base);
        assert!(mine.exists(), "this process still runs");
        assert!(!gone.exists(), "nobody runs it any more");
        let _ = std::fs::remove_dir_all(&base);
    }
}
