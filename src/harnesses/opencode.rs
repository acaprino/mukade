//! OpenCode: one SQLite database, named by `OPENCODE_DB`.

use super::{Destination, Harness, InterchangeDocument, StoreStats};
use crate::hop::HopError;
use crate::model::{CanonicalTool, HarnessId, Kind, Opaque, Protocol, Session};
use crate::native_compaction::NativeCompaction;
use crate::policy::Clock;
use crate::reader::Reader;
use crate::roots::StoreRoots;
use crate::summary::{self, Reply, SummarizeOpts, SummaryCommand};
use crate::tools::{self, EditCall, GrepCall, Shell, ToolSet, Translation};
use crate::writer::{WriteOpts, Writer, media::Image, plan::Plan};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub(super) struct OpenCode;

impl Harness for OpenCode {
    fn id(&self) -> HarnessId {
        HarnessId::OpenCode
    }

    fn reader(&self) -> &'static dyn Reader {
        &crate::reader::opencode::Opencode
    }

    fn destination(&self) -> Option<&dyn Destination> {
        Some(self)
    }

    fn read_file(&self, path: &Path) -> anyhow::Result<Session> {
        crate::reader::opencode::read_file(path)
    }

    fn store_root<'a>(&self, roots: &'a StoreRoots) -> Option<&'a Path> {
        roots.opencode_db.as_deref()
    }

    fn store_root_mut<'a>(&self, roots: &'a mut StoreRoots) -> &'a mut Option<PathBuf> {
        &mut roots.opencode_db
    }

    /// The store is the database file: an existing folder means its `opencode.db`.
    fn set_store_root(&self, roots: &mut StoreRoots, root: &Path) {
        roots.opencode_db = Some(if root.is_dir() {
            root.join("opencode.db")
        } else {
            root.to_path_buf()
        });
    }

    /// OpenCode reads a relative `OPENCODE_DB` beneath its data directory, not the cwd:
    /// `$XDG_DATA_HOME/opencode/custom.db` (or the same fallback the catalog uses).
    fn resolve_store_path(&self, path: PathBuf, vars: &crate::catalog::Vars) -> PathBuf {
        if path.is_absolute() || self.store_is_in_memory(&path) {
            return path;
        }
        crate::catalog::store_default_with(self.id(), vars)
            .and_then(|default| default.parent().map(|dir| dir.join(&path)))
            .unwrap_or(path)
    }

    /// SQLite's `:memory:` database.
    fn store_is_in_memory(&self, root: &Path) -> bool {
        root.as_os_str() == ":memory:"
    }

    /// The database is passed, or the variable removed so the process does not inherit another
    /// store (L1). XDG variables must still be supplied by the caller when OpenCode needs them.
    fn apply_store_env(&self, roots: &StoreRoots, command: &mut std::process::Command) {
        let variable = crate::catalog::store_env(self.id());
        match self.store_root(roots) {
            Some(db) => command.env(variable, db),
            None => command.env_remove(variable),
        };
    }

    fn store_stats(&self, roots: &StoreRoots) -> StoreStats {
        let counted = match roots.opencode_db.as_deref() {
            Some(db) => crate::reader::opencode::store_counts(db),
            None => Err(anyhow::anyhow!("no OpenCode database to count")),
        };
        match counted {
            Ok(counts) => StoreStats {
                sessions: Ok(counts.sessions),
                forks: Some(Ok(counts.forks)),
            },
            Err(error) => {
                let error = format!("{error:#}");
                StoreStats {
                    sessions: Err(error.clone()),
                    forks: Some(Err(error)),
                }
            }
        }
    }

    // Every `run` writes to the store: the fork is deleted once the reply is in.
    fn summary_command(
        &self,
        session: &Session,
        opts: &SummarizeOpts,
        _scratch: &Path,
    ) -> Result<SummaryCommand, HopError> {
        let mut args: Vec<String> = [
            "run",
            "--standalone",
            "--session",
            &session.meta.id,
            "--fork",
            "--format",
            "json",
        ]
        .map(String::from)
        .to_vec();
        // A model is named only with its provider.
        args.extend(summary::model_args(
            "-m",
            opts.model.as_deref(),
            summary::own_model(session),
            |model| match model.provider.as_deref() {
                Some(provider) => vec!["-m".into(), format!("{provider}/{}", model.id)],
                None => Vec::new(),
            },
        ));
        let env = vec![("OPENCODE_CONFIG_CONTENT".into(), DENY_ALL.into())];
        summary::finish_command(session, opts, args, None, env)
    }

    /// The `text` events, joined.
    fn parse_summary_reply(
        &self,
        stdout: &str,
        _output_file: Option<&str>,
    ) -> Result<Reply, String> {
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
        Ok(Reply {
            text: texts.join("\n\n"),
            usage: None,
            fork: fork_of(stdout),
        })
    }

    /// The fork exists as soon as OpenCode names it: it goes whatever came of the run, and a
    /// failure says how to remove one that stayed.
    fn after_summary(
        &self,
        command: &SummaryCommand,
        roots: &StoreRoots,
        stdout: &str,
    ) -> Option<String> {
        fork_of(stdout).and_then(|fork| delete_fork(command, roots, &fork))
    }
}

impl Destination for OpenCode {
    fn writer(&self) -> &'static dyn Writer {
        &crate::writer::opencode::Opencode
    }

    /// OpenCode speaks whatever its configured provider speaks.
    fn protocol(&self) -> Option<Protocol> {
        None
    }

    /// OpenCode keeps a signature, an encrypted payload and the Chat envelope around them.
    fn payload_has_place(&self, opaque: &Opaque) -> bool {
        opaque.signature.is_some() || opaque.encrypted.is_some()
    }

    /// The receiving provider must understand the envelope OpenCode keeps. Native OpenAI
    /// ciphertext is never relabelled as OpenRouter ciphertext merely because both speak Responses.
    fn provider_reads_envelope(&self, opaque: &Opaque, provider: Option<&str>) -> bool {
        let Some(chat) = &opaque.chat else {
            return true;
        };
        if provider == Some("openrouter")
            && chat.details.iter().any(|d| {
                d.get("format").and_then(serde_json::Value::as_str)
                    == Some("openai-native-responses-v1")
            })
        {
            return false;
        }
        provider.is_some() && provider == chat.provider.as_deref()
            || crate::policy::scalar_chat_payload(opaque)
                && matches!(provider, Some("anthropic" | "openrouter"))
    }

    /// One database, no reserved file: a session id `ses_…`, an agent name, metadata as an object.
    fn validate_write(&self, opts: &WriteOpts) -> anyhow::Result<()> {
        if opts.dest_path.is_some() {
            anyhow::bail!("--dest-path is supported only for file-based destinations");
        }
        if let Some(id) = opts.dest_id.as_deref() {
            crate::writer::opencode::validate_session_id(id)?;
        }
        if let Some(agent) = opts.dest_agent.as_deref() {
            crate::writer::opencode::validate_agent(agent)?;
        }
        if let Some(metadata) = &opts.metadata {
            anyhow::ensure!(
                metadata.is_object(),
                "session metadata must be a JSON object"
            );
        }
        Ok(())
    }

    /// OpenCode ids are prefixed `ses_`.
    fn claims_id(&self, id: &str) -> bool {
        id.starts_with("ses_")
    }

    fn interchange_document(&self) -> Option<&'static dyn InterchangeDocument> {
        Some(&Document)
    }

    /// OpenCode's `model` is `<provider>/<model>`.
    ///
    /// A self-contained installation keeps its configuration beside its store: a database in an
    /// XDG data layout (`<root>/share/opencode/opencode.db`, a portable JanuraCode's) is
    /// configured by `<root>/config/opencode/opencode.json`. When such a file exists it alone
    /// answers, even without a model: another installation's model is not this one's. Otherwise
    /// OpenCode's process configuration applies (`OPENCODE_CONFIG`, then `XDG_CONFIG_HOME`).
    fn configured_model(&self, roots: &StoreRoots) -> Option<(String, Option<String>)> {
        let own = roots
            .opencode_db
            .as_deref()
            .and_then(opencode_config_beside)
            .filter(|path| path.is_file());
        opencode_model(&own.or_else(opencode_process_config)?)
    }

    fn project_id(
        &self,
        exe: &str,
        cwd: &str,
        roots: &StoreRoots,
        cache: Option<&Path>,
    ) -> anyhow::Result<Option<String>> {
        let _span = crate::timing::span("probe opencode project");
        Ok(crate::reader::opencode::project_id_for(exe, cwd, roots, cache))
    }

    /// The configuration's `shell`, then `$SHELL`, then PowerShell on Windows (OpenCode tries
    /// `pwsh` and `powershell` before Git Bash and `cmd`).
    fn default_shell(&self, roots: &StoreRoots, cwd: Option<&Path>) -> Option<Shell> {
        let var = |name: &str| std::env::var(name).ok();
        let home = crate::catalog::Vars::from_env().home;
        let host = Host {
            var: &var,
            home: home.as_deref(),
            windows: cfg!(windows),
        };
        opencode_shell(roots, cwd, &host)
    }

    fn offered(&self) -> ToolSet {
        ToolSet {
            names: &[
                "edit",
                "glob",
                "grep",
                "question",
                "read",
                "shell",
                "skill",
                "subagent",
                "webfetch",
                "websearch",
                "write",
                "execute",
            ],
        }
    }

    fn translate(
        &self,
        canonical: &CanonicalTool,
        _source_name: &str,
        input: &Value,
        dest_shell: Option<Shell>,
    ) -> Translation {
        match canonical {
            CanonicalTool::Bash | CanonicalTool::PowerShell => shell(canonical, input, dest_shell),
            CanonicalTool::Read => tools::read_by_path(input),
            CanonicalTool::Write => tools::write_by_path(input),
            CanonicalTool::Edit | CanonicalTool::MultiEdit => edit(canonical, input),
            CanonicalTool::Grep => grep(input),
            CanonicalTool::Glob => match tools::glob_input(input) {
                Ok(input) => Translation::Call {
                    name: "glob",
                    input,
                },
                Err(text) => text,
            },
            CanonicalTool::Ls => Translation::Text {
                why: "no ls tool offered",
            },
            CanonicalTool::WebFetch => web_fetch(input),
            CanonicalTool::WebSearch => match tools::query_of(input) {
                Ok(query) => Translation::Call {
                    name: "websearch",
                    input: tools::obj([("query", query)]),
                },
                Err(text) => text,
            },
            _ => tools::OTHER_CAPABILITY,
        }
    }

    fn accepted_keys(&self, canonical: &CanonicalTool) -> &'static [&'static str] {
        match canonical {
            CanonicalTool::Bash | CanonicalTool::PowerShell => tools::SHELL_KEYS_WITH_WORKDIR,
            CanonicalTool::Grep => &[
                "pattern",
                "path",
                "glob",
                "include",
                "-i",
                "ignoreCase",
                "caseSensitive",
                "literal",
                "head_limit",
                "limit",
                "output_mode",
            ],
            CanonicalTool::Glob => &["pattern", "path", "hidden", "limit"],
            _ => tools::common_keys(canonical),
        }
    }

    fn image_in_result(&self, image: &Image) -> Value {
        crate::writer::opencode::image_block(image)
    }

    fn image_in_user_turn(&self, image: &Image) -> Value {
        crate::writer::opencode::image_attachment(image)
    }
}

/// OpenCode's `run` has no switch for tools: its inline configuration denies every permission, so
/// the summarizing run cannot edit a file or run a command (live record, "Ephemeral runs").
const DENY_ALL: &str = r#"{"permission":{"*":"deny"}}"#;

/// How long deleting OpenCode's fork may take.
const DELETE_TIMEOUT: Duration = Duration::from_secs(30);

/// The session OpenCode forked for a run: the `sessionID` of the first event it printed, there as
/// soon as the fork exists, before any text.
pub(crate) fn fork_of(stdout: &str) -> Option<String> {
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

/// Delete the session OpenCode forked for the summary: `None` once it is gone, else the command
/// that removes it.
fn delete_fork(command: &SummaryCommand, roots: &StoreRoots, fork: &str) -> Option<String> {
    let args = ["session", "delete", fork, "--standalone"];
    let mut process = std::process::Command::new(crate::exe::spawnable(&command.program));
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

/// The configuration an OpenCode database in an XDG data layout has beside it:
/// `<root>/share/opencode/opencode.db` → `<root>/config/opencode/opencode.json`.
fn opencode_config_beside(database: &Path) -> Option<PathBuf> {
    let opencode = database.parent()?;
    let share = opencode.parent()?;
    let named = |path: &Path, name: &str| path.file_name().is_some_and(|n| n == name);
    (named(opencode, "opencode") && named(share, "share"))
        .then(|| share.parent())
        .flatten()
        .map(|root| root.join("config").join("opencode").join("opencode.json"))
}

/// The configuration this process's OpenCode reads.
fn opencode_process_config() -> Option<PathBuf> {
    std::env::var_os("OPENCODE_CONFIG")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(|dir| PathBuf::from(dir).join("opencode").join("opencode.json"))
        })
}

/// OpenCode's config names its model as `<provider>/<model>`.
fn opencode_model(config: &Path) -> Option<(String, Option<String>)> {
    let document: Value = serde_json::from_str(&std::fs::read_to_string(config).ok()?).ok()?;
    let (provider, model) = document.get("model")?.as_str()?.split_once('/')?;
    Some((provider.to_string(), Some(model.to_string())))
}

/// What OpenCode's shell lookup reads besides its files: a process variable by name, the home
/// folder and the platform. Tests state them instead of reading the machine.
struct Host<'a> {
    var: &'a dyn Fn(&str) -> Option<String>,
    home: Option<&'a Path>,
    windows: bool,
}

impl Host<'_> {
    /// A variable, unset when empty.
    fn var(&self, name: &str) -> Option<String> {
        (self.var)(name).filter(|value| !value.is_empty())
    }
}

/// The shell OpenCode runs in `cwd`: the configuration's `shell`, then `$SHELL`, then PowerShell
/// on Windows. A shell of no known dialect (`fish`, `nu`, `cmd`, another) answers nothing there,
/// and the next source is asked.
fn opencode_shell(roots: &StoreRoots, cwd: Option<&Path>, host: &Host) -> Option<Shell> {
    configured_shell(roots, cwd, host)
        .and_then(|name| tools::shell_named(&name))
        .or_else(|| host.var("SHELL").and_then(|name| tools::shell_named(&name)))
        .or_else(|| host.windows.then_some(Shell::PowerShell))
}

/// The `shell` the first source that sets one names, nearest first: `OPENCODE_CONFIG_CONTENT`;
/// the project files from `cwd` (an existing absolute folder) up to its git worktree;
/// `OPENCODE_CONFIG`; the global file.
fn configured_shell(roots: &StoreRoots, cwd: Option<&Path>, host: &Host) -> Option<String> {
    if let Some(shell) = host
        .var("OPENCODE_CONFIG_CONTENT")
        .and_then(|text| shell_in_jsonc(&text))
    {
        return Some(shell);
    }
    let project = super::project_dir(cwd)
        .into_iter()
        .flat_map(project_config_files);
    let named = host.var("OPENCODE_CONFIG").map(PathBuf::from);
    let global = global_config_files(roots, host);
    project
        .chain(named)
        .chain(global)
        .find_map(|file| shell_in_jsonc(&std::fs::read_to_string(file).ok()?))
}

/// The project files OpenCode reads, nearest folder first, from `cwd` up to the folder that holds
/// `.git` (its worktree), or the root.
fn project_config_files(cwd: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for folder in cwd.ancestors() {
        for name in [".opencode/opencode.jsonc", ".opencode/opencode.json"] {
            files.push(folder.join(name));
        }
        for name in ["opencode.jsonc", "opencode.json"] {
            files.push(folder.join(name));
        }
        if folder.join(".git").exists() {
            break;
        }
    }
    files
}

/// OpenCode's global file: the configuration beside a self-contained database when there is one,
/// else `$XDG_CONFIG_HOME/opencode/`, else `~/.config/opencode/`; `.jsonc` before `.json`.
fn global_config_files(roots: &StoreRoots, host: &Host) -> Vec<PathBuf> {
    let in_folder =
        |folder: &Path| vec![folder.join("opencode.jsonc"), folder.join("opencode.json")];
    let beside = roots
        .opencode_db
        .as_deref()
        .and_then(opencode_config_beside)
        .and_then(|file| file.parent().map(in_folder))
        .filter(|files| files.iter().any(|file| file.is_file()));
    if let Some(files) = beside {
        return files;
    }
    let folder = host
        .var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| host.home.map(|home| home.join(".config")));
    folder
        .map(|folder| in_folder(&folder.join("opencode")))
        .unwrap_or_default()
}

/// The non-empty `shell` string of a JSONC document, a byte-order mark before it or not.
fn shell_in_jsonc(text: &str) -> Option<String> {
    let document: Value =
        serde_json::from_str(&strip_jsonc(text.trim_start_matches('\u{feff}'))).ok()?;
    document
        .get("shell")?
        .as_str()
        .filter(|shell| !shell.trim().is_empty())
        .map(str::to_string)
}

/// JSONC as JSON: comments and trailing commas left out; strings kept as they are.
fn strip_jsonc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                out.push(c);
                while let Some(c) = chars.next() {
                    out.push(c);
                    match c {
                        '\\' => out.extend(chars.next()),
                        '"' => break,
                        _ => {}
                    }
                }
            }
            '/' if skip_comment(&mut chars) => {}
            ',' if closes_next(chars.clone()) => {}
            _ => out.push(c),
        }
    }
    out
}

/// After a `/`: the rest of a `//` or `/* */` comment that starts there is skipped.
fn skip_comment(chars: &mut std::iter::Peekable<std::str::Chars>) -> bool {
    match chars.peek() {
        Some('/') => {
            while chars.next_if(|&c| c != '\n').is_some() {}
            true
        }
        Some('*') => {
            chars.next();
            let mut last = '\0';
            for c in chars.by_ref() {
                if last == '*' && c == '/' {
                    break;
                }
                last = c;
            }
            true
        }
        _ => false,
    }
}

/// Whether what follows, past blanks and comments, closes an object or an array: the comma before
/// it is a trailing one.
fn closes_next(mut chars: std::iter::Peekable<std::str::Chars>) -> bool {
    while let Some(c) = chars.next() {
        match c {
            '/' if skip_comment(&mut chars) => {}
            c if c.is_whitespace() => {}
            '}' | ']' => return true,
            _ => return false,
        }
    }
    false
}

/// OpenCode's interchange format: what `opencode session import` takes.
struct Document;

impl InterchangeDocument for Document {
    /// An OpenCode session keeps each call to a tool OpenCode offers under its own name and input.
    fn source_tool_map(
        &self,
        source_harness: HarnessId,
        source: &Session,
    ) -> Vec<(String, String, Value, Value)> {
        if source_harness != HarnessId::OpenCode {
            return Vec::new();
        }
        let offered = crate::tools::offered(HarnessId::OpenCode);
        let mut map = Vec::new();
        for entry in source.conversation() {
            if let Kind::Step { calls, .. } = &entry.kind {
                for call in calls {
                    if offered.contains(&call.source_name) && call.input.is_object() {
                        map.push((
                            call.source_name.clone(),
                            call.source_name.clone(),
                            call.input.clone(),
                            call.input.clone(),
                        ));
                    }
                }
            }
        }
        map
    }

    /// Without a reserved id the document is `ses_preview`.
    fn document(
        &self,
        session: &Session,
        plan: &Plan,
        cwd: &str,
        opts: &WriteOpts,
        clock: &mut Clock,
    ) -> anyhow::Result<(Value, Option<NativeCompaction>)> {
        let id = opts.dest_id.as_deref().unwrap_or("ses_preview");
        crate::writer::opencode::validate_session_id(id)?;
        crate::writer::opencode::document_from_plan_report(session, plan, id, cwd, opts, clock)
    }
}

// ---------------------------------------------------------------------------------------------
// tools toward OpenCode

fn shell(canonical: &CanonicalTool, input: &Value, dest_shell: Option<Shell>) -> Translation {
    if let Some(text) = tools::other_dialect(canonical, dest_shell) {
        return text;
    }
    let Some(command) = tools::command_of(input) else {
        return Translation::Text { why: "no command" };
    };
    let mut pairs: Vec<(&str, Value)> = vec![("command", command)];
    if let Some(workdir) = tools::either(input, &["workdir", "cwd"]) {
        pairs.push(("workdir", workdir));
    }
    if let Some(timeout) = tools::arg(input, "timeout") {
        pairs.push(("timeout", timeout.clone()));
    }
    if tools::in_background(input) {
        pairs.push(("background", Value::Bool(true)));
    }
    Translation::Call {
        name: "shell",
        input: tools::obj_pairs(pairs),
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
        // OpenCode has no multi-edit, and one result cannot be split.
        Ok(EditCall::Several { .. }) => return tools::NO_MULTI_EDIT,
        Err(text) => return text,
    };
    let mut pairs: Vec<(&str, Value)> =
        vec![("path", path), ("oldString", old), ("newString", new)];
    if replace_all {
        pairs.push(("replaceAll", Value::Bool(true)));
    }
    Translation::Call {
        name: "edit",
        input: tools::obj_pairs(pairs),
    }
}

fn grep(input: &Value) -> Translation {
    let GrepCall {
        mut pairs,
        filter,
        limit,
        ignore_case,
        literal,
        ..
    } = match tools::grep_call(input) {
        Ok(grep) => grep,
        Err(text) => return text,
    };
    if let Some(filter) = filter {
        pairs.push(("include", filter));
    }
    if let Some(limit) = limit {
        pairs.push(("limit", limit));
    }
    if let Some(i) = ignore_case {
        pairs.push(("caseSensitive", Value::Bool(!i)));
    }
    if let Some(literal) = literal {
        pairs.push(("literal", literal));
    }
    Translation::Call {
        name: "grep",
        input: tools::obj_pairs(pairs),
    }
}

fn web_fetch(input: &Value) -> Translation {
    let url = match tools::url_of(input) {
        Ok(url) => url,
        Err(text) => return text,
    };
    let mut pairs: Vec<(&str, Value)> = vec![("url", url)];
    for key in ["format", "timeout"] {
        if let Some(v) = tools::arg(input, key) {
            pairs.push((key, v.clone()));
        }
    }
    Translation::Call {
        name: "webfetch",
        input: tools::obj_pairs(pairs),
    }
}

#[cfg(test)]
mod tests {
    use super::{Host, opencode_config_beside, opencode_shell, strip_jsonc};
    use crate::roots::StoreRoots;
    use crate::scratch::Scratch;
    use crate::tools::Shell;
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};

    #[test]
    fn only_an_xdg_data_layout_has_a_configuration_beside_it() {
        let root = PathBuf::from("install");
        assert_eq!(
            opencode_config_beside(&root.join("data/share/opencode/opencode.db")),
            Some(root.join("data/config/opencode/opencode.json"))
        );
        assert_eq!(opencode_config_beside(&root.join("store/opencode.db")), None);
    }

    #[test]
    fn jsonc_comments_and_trailing_commas_are_left_out_and_strings_kept() {
        let text = "{\n  // the shell\n  \"shell\": \"pwsh\", /* inline */\n  \"url\": \"https://x//y\",\n  \"say\": \"a /* b */, }\",\n  \"list\": [1, 2,],\n}\n";
        let value: serde_json::Value = serde_json::from_str(&strip_jsonc(text)).unwrap();
        assert_eq!(value["shell"], "pwsh");
        assert_eq!(value["url"], "https://x//y");
        assert_eq!(value["say"], "a /* b */, }");
        assert_eq!(value["list"], serde_json::json!([1, 2]));
        let escaped = r#"{"say": "a \" // b",}"#;
        let value: serde_json::Value = serde_json::from_str(&strip_jsonc(escaped)).unwrap();
        assert_eq!(value["say"], "a \" // b");
    }

    /// A scratch project inside a git worktree of its own, so the walk up stops inside it.
    fn scratch(tag: &str) -> Scratch {
        let dir = Scratch::new(&format!("opencode-shell-{tag}"));
        std::fs::create_dir_all(dir.join("repo/.git")).unwrap();
        std::fs::create_dir_all(dir.join("repo/app/src")).unwrap();
        dir
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// The shell OpenCode runs in `cwd`, with the variables `vars`, the home `home` and the
    /// platform `windows`.
    fn shell_with(
        roots: &StoreRoots,
        cwd: &Path,
        vars: &[(&str, &str)],
        home: &Path,
        windows: bool,
    ) -> Option<Shell> {
        let vars: HashMap<String, String> = vars
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect();
        let var = |name: &str| vars.get(name).cloned();
        opencode_shell(
            roots,
            Some(cwd),
            &Host {
                var: &var,
                home: Some(home),
                windows,
            },
        )
    }

    #[test]
    fn opencode_takes_the_nearest_configuration_that_names_a_shell() {
        let dir = scratch("order");
        let home = dir.join("home");
        let repo = dir.join("repo");
        let cwd = repo.join("app/src");
        let roots = StoreRoots::default();
        let shell = |vars: &[(&str, &str)]| shell_with(&roots, &cwd, vars, &home, false);

        assert_eq!(shell(&[]), None, "nothing names a shell off Windows");

        write(
            &home.join(".config/opencode/opencode.jsonc"),
            "{\n  // global\n  \"shell\": \"/bin/zsh\",\n}",
        );
        assert_eq!(shell(&[]), Some(Shell::Posix), "the global file");

        let xdg = dir.join("xdg");
        write(&xdg.join("opencode/opencode.json"), r#"{"shell":"pwsh"}"#);
        let xdg_var = xdg.to_str().unwrap();
        assert_eq!(
            shell(&[("XDG_CONFIG_HOME", xdg_var)]),
            Some(Shell::PowerShell),
            "XDG_CONFIG_HOME names the global folder"
        );

        let named = dir.join("named.json");
        write(&named, r#"{"shell":"pwsh"}"#);
        let named_var = named.to_str().unwrap();
        assert_eq!(
            shell(&[("OPENCODE_CONFIG", named_var)]),
            Some(Shell::PowerShell),
            "OPENCODE_CONFIG over the global file"
        );

        write(&repo.join("opencode.json"), r#"{"shell":"bash"}"#);
        assert_eq!(
            shell(&[("OPENCODE_CONFIG", named_var)]),
            Some(Shell::Posix),
            "a project file over OPENCODE_CONFIG"
        );

        write(&repo.join("app/opencode.json"), r#"{"shell":"pwsh"}"#);
        assert_eq!(shell(&[]), Some(Shell::PowerShell), "the nearer folder");

        write(&repo.join("app/opencode.jsonc"), r#"{"shell":"zsh",}"#);
        assert_eq!(shell(&[]), Some(Shell::Posix), ".jsonc before .json");

        write(
            &repo.join("app/.opencode/opencode.json"),
            r#"{"shell":"powershell"}"#,
        );
        assert_eq!(
            shell(&[]),
            Some(Shell::PowerShell),
            ".opencode/ before the folder's own file"
        );

        assert_eq!(
            shell(&[("OPENCODE_CONFIG_CONTENT", r#"{"shell":"sh"}"#)]),
            Some(Shell::Posix),
            "OPENCODE_CONFIG_CONTENT over every file"
        );
    }

    #[test]
    fn a_shell_of_no_known_dialect_passes_to_the_next_source() {
        let dir = scratch("unknown");
        let home = dir.join("home");
        let cwd = dir.join("repo/app/src");
        let roots = StoreRoots::default();
        let shell = |vars: &[(&str, &str)], windows| shell_with(&roots, &cwd, vars, &home, windows);

        assert_eq!(shell(&[("SHELL", "/bin/bash")], false), Some(Shell::Posix));
        assert_eq!(shell(&[], true), Some(Shell::PowerShell), "Windows");
        assert_eq!(shell(&[("SHELL", "")], false), None, "an empty $SHELL");
        assert_eq!(
            shell(&[("SHELL", "/usr/bin/fish")], true),
            Some(Shell::PowerShell),
            "$SHELL of no known dialect"
        );

        // The nearest file that sets `shell` is OpenCode's: a farther one is not read, and a
        // shell of no known dialect there goes on to $SHELL.
        write(
            &home.join(".config/opencode/opencode.json"),
            r#"{"shell":"bash"}"#,
        );
        write(&dir.join("repo/opencode.json"), r#"{"shell":"fish"}"#);
        assert_eq!(shell(&[], false), None, "the global bash is not OpenCode's");
        assert_eq!(
            shell(&[("SHELL", "/bin/zsh")], false),
            Some(Shell::Posix),
            "fish goes on to $SHELL"
        );
        write(&dir.join("repo/opencode.json"), r#"{"shell":"cmd.exe"}"#);
        assert_eq!(shell(&[], true), Some(Shell::PowerShell), "cmd goes on too");

        // A file that does not parse, or names no shell, sets nothing.
        write(&dir.join("repo/opencode.json"), "{ not json");
        assert_eq!(
            shell(&[], false),
            Some(Shell::Posix),
            "the global bash again"
        );
    }

    /// A project that is not an existing folder (a path from another machine) has no project
    /// files: walking up from it would reach files above any project.
    #[test]
    fn a_missing_project_is_not_walked_up() {
        let dir = Scratch::new("opencode-shell-missing");
        let home = dir.join("home");
        write(&dir.join("opencode.json"), r#"{"shell":"pwsh"}"#);
        let cwd = dir.join("elsewhere/app/src");
        assert_eq!(
            shell_with(&StoreRoots::default(), &cwd, &[], &home, false),
            None
        );
    }

    /// A configuration saved with a byte-order mark is read like any other.
    #[test]
    fn a_configuration_with_a_byte_order_mark_is_read() {
        let dir = scratch("bom");
        let home = dir.join("home");
        let cwd = dir.join("repo/app/src");
        write(
            &dir.join("repo/opencode.json"),
            "\u{feff}{\"shell\":\"pwsh\"}",
        );
        assert_eq!(
            shell_with(&StoreRoots::default(), &cwd, &[], &home, false),
            Some(Shell::PowerShell)
        );
    }

    #[test]
    fn the_walk_up_stops_at_the_git_worktree() {
        let dir = scratch("worktree");
        let home = dir.join("home");
        write(&dir.join("opencode.json"), r#"{"shell":"pwsh"}"#);
        let cwd = dir.join("repo/app/src");
        assert_eq!(
            shell_with(&StoreRoots::default(), &cwd, &[], &home, false),
            None,
            "a file above the worktree is not OpenCode's"
        );
    }

    /// A self-contained installation's configuration beside its database is its global file,
    /// whatever XDG_CONFIG_HOME names.
    #[test]
    fn a_self_contained_opencode_names_its_shell_beside_its_data() {
        let dir = scratch("beside");
        let home = dir.join("home");
        let data = dir.join("JanuraCode/data");
        write(
            &data.join("config/opencode/opencode.jsonc"),
            "{ \"shell\": \"pwsh\", }",
        );
        let xdg = dir.join("xdg");
        write(&xdg.join("opencode/opencode.json"), r#"{"shell":"bash"}"#);
        let roots = StoreRoots {
            opencode_db: Some(data.join("share/opencode/opencode.db")),
            ..Default::default()
        };
        let cwd = dir.join("repo/app/src");
        assert_eq!(
            shell_with(
                &roots,
                &cwd,
                &[("XDG_CONFIG_HOME", xdg.to_str().unwrap())],
                &home,
                false
            ),
            Some(Shell::PowerShell)
        );
    }
}
