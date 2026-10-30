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
use serde_json::{Value, json};

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

/// OpenCode's `run` has no switch for tools: its inline configuration denies every permission, so
/// the summarizing run cannot edit a file or run a command (live record, "Ephemeral runs").
const OPENCODE_DENY_ALL: &str = r#"{"permission":{"*":"deny"}}"#;

/// How long deleting OpenCode's fork may take.
const DELETE_TIMEOUT: Duration = Duration::from_secs(30);

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
    let harness = session.meta.harness;
    let id = session.meta.id.clone();
    let model = model_args(harness, opts.model.as_deref(), own_model(session));
    let mut output_file = None;
    let mut args: Vec<String> = match harness {
        // `--tools` takes several values: before `--resume`, so it cannot swallow the prompt.
        HarnessId::ClaudeCode => [
            "-p",
            "--tools",
            "",
            "--resume",
            &id,
            "--no-session-persistence",
            "--output-format",
            "json",
        ]
        .map(String::from)
        .to_vec(),
        // `exec resume --ephemeral` still appends to the rollout it resumes; a fork does not.
        HarnessId::Codex => {
            let file = scratch.join("last-message.txt");
            let args = vec![
                "exec".into(),
                "fork".into(),
                id,
                "--ephemeral".into(),
                "--skip-git-repo-check".into(),
                "-c".into(),
                "sandbox_mode=\"read-only\"".into(),
                "-o".into(),
                file.display().to_string(),
            ];
            output_file = Some(file);
            args
        }
        // `--no-session` cannot be combined with `--fork`: the fork goes to this run's folder.
        HarnessId::Pi => {
            let file = session.meta.source_path.clone().ok_or_else(|| {
                HopError::Unsupported(
                    "a Pi session is summarized from its own file, and this one was not read from a file"
                        .into(),
                )
            })?;
            vec![
                "-p".into(),
                "--fork".into(),
                file,
                "--session-dir".into(),
                scratch.join("sessions").display().to_string(),
                "--no-tools".into(),
            ]
        }
        // Every `run` writes to the store: the fork is deleted once the reply is in.
        HarnessId::OpenCode => [
            "run",
            "--standalone",
            "--session",
            &id,
            "--fork",
            "--format",
            "json",
        ]
        .map(String::from)
        .to_vec(),
    };
    args.extend(model);
    let prompt = opts.prompt.as_deref().unwrap_or(SUMMARY_PROMPT);
    let program = crate::exe::resolve(harness, opts.exe.as_deref());
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
        env: match harness {
            HarnessId::OpenCode => {
                vec![("OPENCODE_CONFIG_CONTENT".into(), OPENCODE_DENY_ALL.into())]
            }
            _ => Vec::new(),
        },
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
fn own_model(session: &Session) -> Option<&ModelRef> {
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

/// The model flags: the one named, else the session's own last model. Pi gets the provider apart
/// (a model id may hold a slash itself); OpenCode names a model only with its provider.
fn model_args(harness: HarnessId, named: Option<&str>, recorded: Option<&ModelRef>) -> Vec<String> {
    let flag = match harness {
        HarnessId::ClaudeCode | HarnessId::Pi => "--model",
        HarnessId::Codex | HarnessId::OpenCode => "-m",
    };
    if let Some(model) = named {
        return vec![flag.into(), model.into()];
    }
    let Some(model) = recorded.filter(|model| !model.id.is_empty()) else {
        return Vec::new();
    };
    match (harness, model.provider.as_deref()) {
        (HarnessId::Pi, Some(provider)) => vec![
            "--provider".into(),
            provider.into(),
            flag.into(),
            model.id.clone(),
        ],
        (HarnessId::OpenCode, Some(provider)) => {
            vec![flag.into(), format!("{provider}/{}", model.id)]
        }
        (HarnessId::OpenCode, None) => Vec::new(),
        _ => vec![flag.into(), model.id.clone()],
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
    let mut reply = Reply {
        text: String::new(),
        usage: None,
        fork: None,
    };
    match harness {
        HarnessId::ClaudeCode => {
            let result: Value = stdout
                .lines()
                .rev()
                .find_map(|line| serde_json::from_str(line.trim()).ok())
                .or_else(|| serde_json::from_str(stdout.trim()).ok())
                .ok_or_else(|| "Claude Code printed no JSON result".to_string())?;
            let subtype = result.get("subtype").and_then(Value::as_str).unwrap_or("");
            let text = result.get("result").and_then(Value::as_str).unwrap_or("");
            if result.get("is_error").and_then(Value::as_bool) == Some(true) || subtype != "success"
            {
                return Err(format!(
                    "Claude Code answered with an error ({}): {}",
                    if subtype.is_empty() {
                        "no subtype"
                    } else {
                        subtype
                    },
                    text.trim()
                ));
            }
            reply.text = text.to_string();
            let usage = json!({
                "total_cost_usd": result.get("total_cost_usd"),
                "usage": result.get("usage"),
                "modelUsage": result.get("modelUsage"),
            });
            reply.usage = Some(usage.to_string());
        }
        HarnessId::Codex => reply.text = output_file.unwrap_or("").to_string(),
        HarnessId::Pi => reply.text = stdout.to_string(),
        HarnessId::OpenCode => {
            let mut texts = Vec::new();
            for event in stdout
                .lines()
                .filter_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
            {
                match event.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        if let Some(text) = event.pointer("/part/text").and_then(Value::as_str) {
                            texts.push(text.to_string());
                        }
                    }
                    // A model that reached for a tool was working, not summarizing.
                    Some("tool_use") => {
                        return Err(format!(
                            "the model called a tool ({}) instead of only summarizing",
                            event
                                .pointer("/part/tool")
                                .and_then(Value::as_str)
                                .unwrap_or("?")
                        ));
                    }
                    Some("error") => {
                        let error = event.get("error").unwrap_or(&event);
                        let message = error
                            .pointer("/data/message")
                            .or_else(|| error.get("message"))
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                            .unwrap_or_else(|| error.to_string());
                        return Err(format!("OpenCode reported an error: {message}"));
                    }
                    _ => {}
                }
            }
            reply.fork = opencode_fork(stdout);
            reply.text = texts.join("\n\n");
        }
    }
    reply.text = reply.text.trim().to_string();
    if reply.text.is_empty() {
        return Err("the model returned no summary".into());
    }
    Ok(reply)
}

/// The session OpenCode forked for a run: the `sessionID` of the first event it printed, there as
/// soon as the fork exists, before any text.
pub fn opencode_fork(stdout: &str) -> Option<String> {
    stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
        .find_map(|event| {
            event
                .get("sessionID")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
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
    let scratch = base.join(format!("summary-{}-{nanos}", std::process::id()));
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
    let leftover = match harness {
        HarnessId::OpenCode => {
            opencode_fork(&stdout).and_then(|fork| delete_opencode_fork(&command, roots, &fork))
        }
        _ => None,
    };
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
    let model = opts
        .model
        .clone()
        .or_else(|| own_model(session).map(|model| model.id.clone()));
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

/// Delete the session OpenCode forked for the summary: `None` once it is gone, else the command
/// that removes it.
fn delete_opencode_fork(
    command: &SummaryCommand,
    roots: &StoreRoots,
    fork: &str,
) -> Option<String> {
    let args = ["session", "delete", fork, "--standalone"];
    let mut process = Command::new(crate::exe::spawnable(&command.program));
    process.args(args);
    if let Some(cwd) = &command.cwd {
        process.current_dir(cwd);
    }
    roots.apply_to(HarnessId::OpenCode, &mut process);
    let deleted = crate::exe::run_within(&mut process, DELETE_TIMEOUT)
        .ok()
        .and_then(|finished| finished.status)
        .is_some_and(|status| status.success());
    (!deleted).then(|| format!("{} {}", command.program, args.join(" ")))
}

/// The folders of runs a killed process left behind, removed once they are an hour old.
fn sweep_stale_runs(base: &Path) {
    let Ok(entries) = std::fs::read_dir(base) else {
        return;
    };
    let now = std::time::SystemTime::now();
    for entry in entries.flatten() {
        let stale = entry.file_name().to_string_lossy().starts_with("summary-")
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
