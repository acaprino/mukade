//! Claude Code: sessions under `CLAUDE_CONFIG_DIR/projects/`.

use super::{Destination, Harness, InterchangeDocument, StoreStats};
use crate::hop::HopError;
use crate::model::{CanonicalTool, HarnessId, Opaque, Protocol, Session};
use crate::reader::Reader;
use crate::roots::StoreRoots;
use crate::summary::{self, Reply, SummarizeOpts, SummaryCommand};
use crate::tools::{self, EditCall, GrepCall, Shell, ToolSet, Translation};
use crate::writer::{WriteOpts, Writer, media::Image};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub(super) struct ClaudeCode;

impl Harness for ClaudeCode {
    fn id(&self) -> HarnessId {
        HarnessId::ClaudeCode
    }

    fn reader(&self) -> &'static dyn Reader {
        &crate::reader::claude_code::ClaudeCode
    }

    fn destination(&self) -> Option<&dyn Destination> {
        Some(self)
    }

    fn read_file(&self, path: &Path) -> anyhow::Result<Session> {
        crate::reader::claude_code::read_file(path)
    }

    fn store_root<'a>(&self, roots: &'a StoreRoots) -> Option<&'a Path> {
        roots.claude_config_dir.as_deref()
    }

    fn store_root_mut<'a>(&self, roots: &'a mut StoreRoots) -> &'a mut Option<PathBuf> {
        &mut roots.claude_config_dir
    }

    fn apply_store_env(&self, roots: &StoreRoots, command: &mut std::process::Command) {
        let variable = crate::catalog::store_env(self.id());
        super::set_store_env(command, variable, self.store_root(roots));
    }

    fn store_stats(&self, roots: &StoreRoots) -> StoreStats {
        super::listed_stats(self, roots)
    }

    // `--tools` takes several values: before `--resume`, so it cannot swallow the prompt.
    fn summary_command(
        &self,
        session: &Session,
        opts: &SummarizeOpts,
        _scratch: &Path,
    ) -> Result<SummaryCommand, HopError> {
        let mut args: Vec<String> = [
            "-p",
            "--tools",
            "",
            "--resume",
            &session.meta.id,
            "--no-session-persistence",
            "--output-format",
            "json",
        ]
        .map(String::from)
        .to_vec();
        args.extend(summary::model_args(
            "--model",
            opts.model.as_deref(),
            summary::own_model(session),
            |model| vec!["--model".into(), model.id.clone()],
        ));
        summary::finish_command(session, opts, args, None, Vec::new())
    }

    /// The JSON `result`, read from the last line that parses.
    fn parse_summary_reply(
        &self,
        stdout: &str,
        _output_file: Option<&str>,
    ) -> Result<Reply, String> {
        let result: Value = stdout
            .lines()
            .rev()
            .find_map(|line| serde_json::from_str(line.trim()).ok())
            .or_else(|| serde_json::from_str(stdout.trim()).ok())
            .ok_or_else(|| "Claude Code printed no JSON result".to_string())?;
        let subtype = result.get("subtype").and_then(Value::as_str).unwrap_or("");
        let text = result.get("result").and_then(Value::as_str).unwrap_or("");
        if result.get("is_error").and_then(Value::as_bool) == Some(true) || subtype != "success" {
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
        let usage = json!({
            "total_cost_usd": result.get("total_cost_usd"),
            "usage": result.get("usage"),
            "modelUsage": result.get("modelUsage"),
        });
        Ok(Reply {
            text: text.to_string(),
            usage: Some(usage.to_string()),
            fork: None,
        })
    }
}

impl Destination for ClaudeCode {
    fn writer(&self) -> &'static dyn Writer {
        &crate::writer::claude_code::ClaudeCode
    }

    fn protocol(&self) -> Option<Protocol> {
        Some(Protocol::AnthropicMessages)
    }

    /// A signature is written as `thinking`, an encrypted payload as `redacted_thinking`. A Chat
    /// envelope is not kept: only one with a scalar conversion fits.
    fn payload_has_place(&self, opaque: &Opaque) -> bool {
        crate::policy::scalar_chat_payload(opaque)
            && (opaque.signature.is_some() || opaque.encrypted.is_some())
    }

    fn validate_write(&self, opts: &WriteOpts) -> anyhow::Result<()> {
        super::validate_file_write(self, opts)
    }

    fn interchange_document(&self) -> Option<&'static dyn InterchangeDocument> {
        None
    }

    fn offered(&self) -> ToolSet {
        ToolSet { names: PINNED }
    }

    /// The pinned list, plus `PowerShell` where this installation runs it.
    fn offered_in(&self, roots: &StoreRoots, cwd: Option<&Path>) -> ToolSet {
        let var = std::env::var(POWERSHELL_VAR).ok();
        offered_for(var.as_deref(), roots, cwd)
    }

    /// Claude offers both shells and runs either, whatever the session's shell.
    fn translate(
        &self,
        canonical: &CanonicalTool,
        source_name: &str,
        input: &Value,
        _dest_shell: Option<Shell>,
    ) -> Translation {
        match canonical {
            // A PowerShell call needs the PowerShell tool.
            CanonicalTool::Bash => shell(input, "Bash"),
            CanonicalTool::PowerShell => shell(input, "PowerShell"),
            CanonicalTool::Read => read(input),
            CanonicalTool::Write => write(input),
            CanonicalTool::Edit | CanonicalTool::MultiEdit => edit(canonical, input),
            CanonicalTool::Grep => grep(source_name, input),
            CanonicalTool::Glob => match tools::glob_input(input) {
                Ok(input) => Translation::Call {
                    name: "Glob",
                    input,
                },
                Err(text) => text,
            },
            CanonicalTool::Ls => Translation::Text {
                why: "no ls tool offered",
            },
            CanonicalTool::WebFetch => web_fetch(input),
            CanonicalTool::WebSearch => web_search(input),
            _ => tools::OTHER_CAPABILITY,
        }
    }

    fn accepted_keys(&self, canonical: &CanonicalTool) -> &'static [&'static str] {
        match canonical {
            CanonicalTool::Read => &[
                "file_path",
                "path",
                "offset",
                "limit",
                "pages",
                "allow_large",
            ],
            CanonicalTool::Grep => &[
                "pattern",
                "path",
                "glob",
                "include",
                "type",
                "output_mode",
                "-i",
                "ignoreCase",
                "caseSensitive",
                "-n",
                "-A",
                "-B",
                "-C",
                "context",
                "head_limit",
                "limit",
                "offset",
                "multiline",
            ],
            CanonicalTool::WebFetch => &["url", "prompt"],
            CanonicalTool::WebSearch => &["query", "allowed_domains", "blocked_domains"],
            _ => tools::common_keys(canonical),
        }
    }

    /// The writer writes a result's text: its other blocks are dropped and counted.
    fn flattens_result_blocks(&self) -> bool {
        true
    }

    fn image_in_result(&self, image: &Image) -> Value {
        crate::writer::claude_code::image_block(image)
    }
}

// ---------------------------------------------------------------------------------------------
// the offered set

/// The tools of the pinned version, followed by `extra`.
macro_rules! claude_tools {
    ($($extra:literal),*) => {
        &[
            "Agent",
            "Bash",
            "CronCreate",
            "CronDelete",
            "CronList",
            "Edit",
            "EnterWorktree",
            "ExitWorktree",
            "Glob",
            "Grep",
            "ListAgents",
            "Monitor",
            "NotebookEdit",
            "Read",
            "ReportFindings",
            "ScheduleWakeup",
            "SendMessage",
            "Skill",
            "TaskStop",
            "WebFetch",
            "WebSearch",
            "Workflow",
            "Write",
            $($extra),*
        ]
    };
}

/// The tools the pinned Claude Code offers in the bench.
const PINNED: &[&str] = claude_tools!();

/// The same, where the installation runs its `PowerShell` tool.
const WITH_POWERSHELL: &[&str] = claude_tools!("PowerShell");

/// The variable that turns the `PowerShell` tool on (`1`) or off (`0`).
const POWERSHELL_VAR: &str = "CLAUDE_CODE_USE_POWERSHELL_TOOL";

/// How much of a transcript's end is read for its environment snapshot.
const TRANSCRIPT_TAIL: u64 = 1 << 20;

/// How many of the newest transcripts are tried for one.
const TRANSCRIPTS_TRIED: usize = 5;

/// The offered set, with `var` the process's value of [`POWERSHELL_VAR`].
fn offered_for(var: Option<&str>, roots: &StoreRoots, cwd: Option<&Path>) -> ToolSet {
    let config = roots.claude_config_dir.as_deref();
    let names = if powershell_enabled(var, config, cwd) {
        WITH_POWERSHELL
    } else {
        PINNED
    };
    ToolSet { names }
}

/// Whether the installation configured in `config` runs its `PowerShell` tool in the project
/// `cwd`; the first answer wins: the process variable `var`; the same variable under `env` in the
/// settings, nearest first; the environment snapshot of its newest transcripts. Else off.
fn powershell_enabled(var: Option<&str>, config: Option<&Path>, cwd: Option<&Path>) -> bool {
    let settings = [
        cwd.map(|cwd| cwd.join(".claude").join("settings.local.json")),
        cwd.map(|cwd| cwd.join(".claude").join("settings.json")),
        config.map(|config| config.join("settings.json")),
    ];
    switch(var)
        .or_else(|| {
            settings
                .into_iter()
                .flatten()
                .find_map(|file| settings_switch(&file))
        })
        .or_else(|| config.and_then(transcript_evidence))
        .unwrap_or(false)
}

/// The variable's value read as on (`1`) or off (`0`); anything else is no answer.
fn switch(value: Option<&str>) -> Option<bool> {
    match value?.trim() {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

/// The variable under `env` in a settings file, as a string or a number.
fn settings_switch(file: &Path) -> Option<bool> {
    let settings: Value = serde_json::from_str(&std::fs::read_to_string(file).ok()?).ok()?;
    let value = settings.get("env")?.get(POWERSHELL_VAR)?;
    match value {
        Value::String(text) => switch(Some(text)),
        Value::Number(number) => switch(Some(&number.to_string())),
        _ => None,
    }
}

/// What the newest transcripts under `<config>/projects/*/` say: the last environment snapshot of
/// the newest one that has one, on when its shell starts with `PowerShell`.
fn transcript_evidence(config: &Path) -> Option<bool> {
    newest_transcripts(&config.join("projects"), TRANSCRIPTS_TRIED)
        .iter()
        .find_map(|file| snapshot_shell(&tail(file, TRANSCRIPT_TAIL)?))
        .map(|shell| shell.starts_with("PowerShell"))
}

/// The `limit` newest `.jsonl` files directly inside the project folders of `projects`.
fn newest_transcripts(projects: &Path, limit: usize) -> Vec<PathBuf> {
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(projects)
        .into_iter()
        .flatten()
        .flatten()
        .flat_map(|project| {
            std::fs::read_dir(project.path())
                .into_iter()
                .flatten()
                .flatten()
        })
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "jsonl"))
        .filter_map(|entry| {
            let metadata = entry.metadata().ok().filter(|m| m.is_file())?;
            Some((metadata.modified().ok()?, entry.path()))
        })
        .collect();
    files.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    files
        .into_iter()
        .take(limit)
        .map(|(_, path)| path)
        .collect()
}

/// The last `limit` bytes of a file, from the first whole line in them.
fn tail(file: &Path, limit: u64) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut handle = std::fs::File::open(file).ok()?;
    let length = handle.metadata().ok()?.len();
    let start = length.saturating_sub(limit);
    handle.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    handle.read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    Some(match start {
        0 => text.into_owned(),
        _ => text
            .split_once('\n')
            .map_or_else(String::new, |(_, rest)| rest.to_string()),
    })
}

/// The shell the last environment snapshot in `text` names: a line
/// `{"type":"attachment","attachment":{"type":"environment","snapshot":{"shell":…}}}`.
fn snapshot_shell(text: &str) -> Option<String> {
    text.lines()
        .rev()
        .filter(|line| line.contains("\"environment\""))
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find_map(|record| {
            let attachment = record.get("attachment")?;
            (record.get("type")?.as_str()? == "attachment"
                && attachment.get("type")?.as_str()? == "environment")
                .then(|| {
                    attachment
                        .pointer("/snapshot/shell")?
                        .as_str()
                        .map(str::to_string)
                })
                .flatten()
        })
}

// ---------------------------------------------------------------------------------------------
// tools toward Claude Code

fn shell(input: &Value, tool: &'static str) -> Translation {
    let Some(command) = tools::command_of(input) else {
        return Translation::Text { why: "no command" };
    };
    let mut pairs: Vec<(&str, Value)> = vec![("command", command)];
    if let Some(t) = tools::arg(input, "timeout") {
        pairs.push(("timeout", t.clone()));
    }
    if let Some(d) = tools::either(input, &["description", "justification"]) {
        pairs.push(("description", d));
    }
    if tools::in_background(input) {
        pairs.push(("run_in_background", Value::Bool(true)));
    }
    if let Some(unsandboxed) = tools::arg(input, "dangerouslyDisableSandbox") {
        pairs.push(("dangerouslyDisableSandbox", unsandboxed.clone()));
    }
    Translation::Call {
        name: tool,
        input: tools::obj_pairs(pairs),
    }
}

/// Claude's own `Read`: `file_path`, not `path`.
fn read(input: &Value) -> Translation {
    let Some(path) = tools::path_of(input) else {
        return Translation::Text { why: "no path" };
    };
    let mut pairs: Vec<(&str, Value)> = vec![("file_path", path)];
    for key in ["offset", "limit", "pages", "allow_large"] {
        if let Some(v) = tools::arg(input, key) {
            pairs.push((key, v.clone()));
        }
    }
    Translation::Call {
        name: "Read",
        input: tools::obj_pairs(pairs),
    }
}

fn write(input: &Value) -> Translation {
    match tools::path_of(input) {
        Some(path) => Translation::Call {
            name: "Write",
            input: tools::obj([
                ("file_path", path),
                ("content", tools::required_content(input)),
            ]),
        },
        None => Translation::Text { why: "no path" },
    }
}

fn edit(canonical: &CanonicalTool, input: &Value) -> Translation {
    let (path, old, new, replace_all) = match tools::edit_call(canonical, input) {
        Ok(EditCall::One {
            path,
            old,
            new,
            replace_all,
        }) => (path, old, new, replace_all),
        // Claude 2.1.311 has no MultiEdit, and one result cannot be split.
        Ok(EditCall::Several { .. }) => return tools::NO_MULTI_EDIT,
        Err(text) => return text,
    };
    let mut pairs: Vec<(&str, Value)> = vec![
        ("file_path", path),
        ("old_string", old),
        ("new_string", new),
    ];
    if replace_all {
        pairs.push(("replace_all", Value::Bool(true)));
    }
    Translation::Call {
        name: "Edit",
        input: tools::obj_pairs(pairs),
    }
}

fn grep(source_name: &str, input: &Value) -> Translation {
    // A Claude source keeps its own call: every argument is Claude's.
    if source_name == "Grep" {
        return Translation::Call {
            name: "Grep",
            input: input.clone(),
        };
    }
    // The accepted keys are Claude's own; a translated call carries only what is written below,
    // so line context, an offset, multiline and unnumbered lines have no counterpart (tables §1.2).
    let dropped = ["-A", "-B", "offset"]
        .iter()
        .any(|key| tools::arg(input, key).is_some())
        || tools::arg(input, "multiline").and_then(Value::as_bool) == Some(true)
        || tools::arg(input, "-n").and_then(Value::as_bool) == Some(false);
    if dropped {
        return Translation::Text {
            why: "an argument has no counterpart",
        };
    }
    let GrepCall {
        mut pairs,
        filter,
        limit,
        context,
        ignore_case,
        literal,
    } = match tools::grep_call(input) {
        Ok(grep) => grep,
        Err(text) => return text,
    };
    // OpenCode and Pi print the matching lines with their numbers.
    if literal.is_some() {
        return Translation::Text {
            why: "Claude's Grep has no literal search",
        };
    }
    if let Some(filter) = filter {
        pairs.push(("glob", filter));
    }
    if let Some(i) = ignore_case {
        pairs.push(("-i", Value::Bool(i)));
    }
    if let Some(context) = context {
        pairs.push(("-C", context));
    }
    if let Some(limit) = limit {
        pairs.push(("head_limit", limit));
    }
    pairs.push(("output_mode", Value::String("content".into())));
    pairs.push(("-n", Value::Bool(true)));
    Translation::Call {
        name: "Grep",
        input: tools::obj_pairs(pairs),
    }
}

/// Claude's WebFetch needs the prompt it answers.
fn web_fetch(input: &Value) -> Translation {
    let url = match tools::url_of(input) {
        Ok(url) => url,
        Err(text) => return text,
    };
    match tools::required(input, "prompt") {
        Some(prompt) => Translation::Call {
            name: "WebFetch",
            input: tools::obj([("url", url), ("prompt", prompt)]),
        },
        None => Translation::Text { why: "no prompt" },
    }
}

fn web_search(input: &Value) -> Translation {
    let query = match tools::query_of(input) {
        Ok(query) => query,
        Err(text) => return text,
    };
    let mut pairs: Vec<(&str, Value)> = vec![("query", query)];
    for key in ["allowed_domains", "blocked_domains"] {
        if let Some(v) = tools::arg(input, key) {
            pairs.push((key, v.clone()));
        }
    }
    Translation::Call {
        name: "WebSearch",
        input: tools::obj_pairs(pairs),
    }
}

#[cfg(test)]
mod tests {
    use super::{PINNED, WITH_POWERSHELL, offered_for, powershell_enabled, snapshot_shell};
    use crate::roots::StoreRoots;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime};

    const POWERSHELL_SHELL: &str = "PowerShell (primary); Bash tool also available for POSIX scripts — each takes its own syntax.";

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mukade-claude-powershell-{tag}-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(dir.join("config")).unwrap();
        std::fs::create_dir_all(dir.join("work")).unwrap();
        dir
    }

    fn put(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// A settings file whose `env` sets the variable to `value`.
    fn settings(value: &str) -> String {
        serde_json::json!({"env": {"CLAUDE_CODE_USE_POWERSHELL_TOOL": value}}).to_string()
    }

    /// One transcript line as Claude Code records its environment snapshot.
    fn snapshot(shell: &str) -> String {
        serde_json::json!({
            "parentUuid": "p",
            "isSidechain": false,
            "attachment": {
                "type": "environment",
                "snapshot": {"platform": "win32", "shell": shell, "isGitRepo": false}
            },
            "type": "attachment",
            "uuid": "u"
        })
        .to_string()
    }

    fn user_line() -> String {
        serde_json::json!({"type": "user", "message": {"role": "user", "content": "hi"}})
            .to_string()
    }

    /// A transcript of `lines` under `<config>/projects/<project>/`, modified `age` seconds after
    /// the epoch: a larger age is a newer file.
    fn transcript(config: &Path, project: &str, name: &str, lines: &[String], age: u64) {
        let path = config.join("projects").join(project).join(name);
        put(&path, &(lines.join("\n") + "\n"));
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(age))
            .unwrap();
    }

    #[test]
    fn the_extended_list_is_the_pinned_one_plus_powershell() {
        let mut extended: Vec<&str> = WITH_POWERSHELL.to_vec();
        extended.retain(|name| *name != "PowerShell");
        assert_eq!(extended, PINNED);
        assert!(!PINNED.contains(&"PowerShell"));
        assert_eq!(WITH_POWERSHELL.len(), PINNED.len() + 1);
    }

    #[test]
    fn the_process_variable_decides_first() {
        let dir = scratch("variable");
        let (config, work) = (dir.join("config"), dir.join("work"));
        put(&work.join(".claude/settings.local.json"), &settings("1"));
        assert!(!powershell_enabled(Some("0"), Some(&config), Some(&work)));
        put(&work.join(".claude/settings.local.json"), &settings("0"));
        assert!(powershell_enabled(Some("1"), Some(&config), Some(&work)));
        // Empty, or a value other than 1 and 0, is no answer: the settings decide.
        assert!(!powershell_enabled(Some(""), Some(&config), Some(&work)));
        assert!(!powershell_enabled(Some("yes"), Some(&config), Some(&work)));
        assert!(powershell_enabled(Some("1"), None, None));
    }

    #[test]
    fn the_settings_nearest_the_project_decide_next() {
        let dir = scratch("settings");
        let (config, work) = (dir.join("config"), dir.join("work"));
        let enabled = || powershell_enabled(None, Some(&config), Some(&work));

        assert!(!enabled(), "no settings and no transcript");
        put(&config.join("settings.json"), &settings("1"));
        assert!(enabled(), "the configuration folder's settings");
        put(&work.join(".claude/settings.json"), &settings("0"));
        assert!(
            !enabled(),
            "the project's settings over the configuration's"
        );
        put(&work.join(".claude/settings.local.json"), r#"{"env":{}}"#);
        assert!(
            !enabled(),
            "a local file without the key leaves the project's"
        );
        put(&work.join(".claude/settings.local.json"), &settings("1"));
        assert!(enabled(), "the local settings over the project's");
        put(&work.join(".claude/settings.local.json"), "{ not json");
        assert!(!enabled(), "a file that does not parse says nothing");
    }

    #[test]
    fn the_settings_decide_before_the_transcripts() {
        let dir = scratch("settings-first");
        let (config, work) = (dir.join("config"), dir.join("work"));
        transcript(&config, "p", "a.jsonl", &[snapshot(POWERSHELL_SHELL)], 100);
        assert!(powershell_enabled(None, Some(&config), Some(&work)));
        put(&config.join("settings.json"), &settings("0"));
        assert!(!powershell_enabled(None, Some(&config), Some(&work)));
    }

    #[test]
    fn the_newest_transcript_with_a_snapshot_is_the_evidence() {
        let dir = scratch("transcripts");
        let config = dir.join("config");
        let enabled = || powershell_enabled(None, Some(&config), None);

        transcript(
            &config,
            "p1",
            "old.jsonl",
            &[snapshot(POWERSHELL_SHELL)],
            100,
        );
        assert!(enabled(), "PowerShell (primary)");
        transcript(&config, "p2", "newer.jsonl", &[snapshot("bash")], 200);
        assert!(!enabled(), "the newest transcript names bash");
        // The last snapshot of a file wins.
        transcript(
            &config,
            "p2",
            "newest.jsonl",
            &[snapshot("bash"), user_line(), snapshot("PowerShell")],
            300,
        );
        assert!(enabled(), "the newest file's last snapshot");
        // A newer file without a snapshot passes to the next one; a subagent's file below a
        // project folder is not a transcript of its own.
        transcript(&config, "p3", "plain.jsonl", &[user_line()], 400);
        transcript(
            &config,
            "p3/subagents",
            "agent.jsonl",
            &[snapshot("zsh (login shell: bash)")],
            500,
        );
        assert!(enabled(), "skipped to the newest file with a snapshot");
    }

    #[test]
    fn at_most_five_transcripts_are_tried() {
        let dir = scratch("five");
        let config = dir.join("config");
        transcript(
            &config,
            "p",
            "evidence.jsonl",
            &[snapshot("PowerShell")],
            100,
        );
        for age in 0..4 {
            let name = format!("plain-{age}.jsonl");
            transcript(&config, "p", &name, &[user_line()], 200 + age);
        }
        assert!(powershell_enabled(None, Some(&config), None), "the fifth");
        transcript(&config, "p", "plain-4.jsonl", &[user_line()], 300);
        assert!(!powershell_enabled(None, Some(&config), None), "a sixth");
    }

    #[test]
    fn only_the_last_mebibyte_of_a_transcript_is_read() {
        let dir = scratch("tail");
        let config = dir.join("config");
        transcript(&config, "p", "older.jsonl", &[snapshot("bash")], 100);
        let filler = serde_json::json!({"type": "user", "message": {"content": "x".repeat(1000)}})
            .to_string();
        let mut lines = vec![snapshot("PowerShell")];
        lines.extend(std::iter::repeat_n(filler, 1100));
        transcript(&config, "p", "long.jsonl", &lines, 200);
        assert!(
            !powershell_enabled(None, Some(&config), None),
            "the snapshot more than 1 MiB from the end is not read; the older file decides"
        );
    }

    #[test]
    fn a_snapshot_names_its_shell() {
        let text = [snapshot("bash"), user_line(), snapshot(POWERSHELL_SHELL)].join("\n");
        assert_eq!(snapshot_shell(&text).as_deref(), Some(POWERSHELL_SHELL));
        assert_eq!(snapshot_shell(&user_line()), None);
        // A partial first line, as a tail read starts, is skipped.
        assert_eq!(snapshot_shell("ell\":\"x\"}}}\n"), None);
    }

    #[test]
    fn powershell_joins_the_offered_set_when_enabled() {
        let roots = StoreRoots::default();
        assert_eq!(offered_for(Some("1"), &roots, None).names, WITH_POWERSHELL);
        assert_eq!(offered_for(None, &roots, None).names, PINNED);
    }
}
