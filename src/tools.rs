//! Canonical → destination tool translation (docs/tool-conversion-tables.md, spec L11–L14).
//!
//! A call is translated into the destination's equivalent tool **only** when the destination
//! offers that tool and every argument the call used maps with the same meaning; otherwise it is
//! carried in **text form**, which is the one shape every provider accepts:
//!
//! ```text
//! [tool call · <source tool name> · id <call id> · not available in <dest>]
//! <key>: <value>
//! ```
//! ```text
//! [tool result · id <call id>]<error flag>
//! <result text, verbatim>
//! ```
//!
//! Never synthesize a call the source did not make: a read is not rewritten as a shell command,
//! and a missing tool is not emulated with another one.

use crate::model::{CanonicalTool, HarnessId, ResultContent, ToolCall, ToolResult};
use serde_json::{Map, Value};

/// The shell dialect a call's command string was written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Posix,
    PowerShell,
}

/// The tools a destination offers in the bench (tool-conversion-tables §2, pinned versions).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolSet {
    pub names: &'static [&'static str],
}

impl ToolSet {
    pub fn contains(&self, name: &str) -> bool {
        self.names.contains(&name)
    }
}

/// What each destination offers, for the versions the bench pinned.
pub fn offered(harness: HarnessId) -> ToolSet {
    match harness {
        HarnessId::ClaudeCode => ToolSet {
            names: &[
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
            ],
        },
        HarnessId::Codex => ToolSet {
            names: &[
                "exec_command",
                "write_stdin",
                "request_user_input",
                "view_image",
                "get_goal",
                "create_goal",
                "update_goal",
                "web_search",
            ],
        },
        HarnessId::OpenCode => ToolSet {
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
        },
        HarnessId::Pi => ToolSet {
            names: &["read", "bash", "edit", "write"],
        },
    }
}

/// The outcome of translating one call.
#[derive(Debug, Clone, PartialEq)]
pub enum Translation {
    /// The destination offers an equivalent tool.
    Call { name: &'static str, input: Value },
    /// No equivalent: the call travels as assistant text. `why` is recorded, not shown.
    Text { why: &'static str },
}

impl Translation {
    pub fn is_call(&self) -> bool {
        matches!(self, Translation::Call { .. })
    }
}

/// Translate one canonical call for a destination.
///
/// `dest_shell` is the shell the destination runs commands with, when the caller knows it
/// (Claude records it as `<environment_context><shell>`, the harness config states it). A shell
/// call is translated only when the two dialects match: the command string is never rewritten.
/// Translate one canonical call for a destination, using the destination's offered set for the
/// pinned version.
pub fn translate(call: &ToolCall, dest: HarnessId, dest_shell: Option<Shell>) -> Translation {
    translate_with(call, dest, dest_shell, offered(dest))
}

/// The same, against an offered set the caller states. The destination's set is per version,
/// settings and model (tables §6): Pi offers `grep`, `find` and `ls` only when it is told to, so a
/// caller that knows the destination's configuration can widen the set instead of losing the call
/// to text form.
pub fn translate_with(
    call: &ToolCall,
    dest: HarnessId,
    dest_shell: Option<Shell>,
    offered: ToolSet,
) -> Translation {
    let Some(canonical) = &call.canonical else {
        return Translation::Text {
            why: "no canonical form",
        };
    };
    let input = &call.input;
    let out = match canonical {
        CanonicalTool::Bash | CanonicalTool::PowerShell => {
            shell(canonical, input, dest, dest_shell)
        }
        CanonicalTool::Read => match dest {
            HarnessId::ClaudeCode => read_claude(input),
            HarnessId::OpenCode => read_opencode(input),
            HarnessId::Pi => read_pi(input),
            HarnessId::Codex => Translation::Text {
                why: "Codex offers no read tool",
            },
        },
        CanonicalTool::Write => match dest {
            HarnessId::ClaudeCode => write_claude(input),
            HarnessId::OpenCode => write_oc_pi(input, "write"),
            HarnessId::Pi => write_oc_pi(input, "write"),
            HarnessId::Codex => Translation::Text {
                why: "Codex offers no apply_patch here",
            },
        },
        CanonicalTool::Edit | CanonicalTool::MultiEdit => edit(canonical, input, dest),
        CanonicalTool::Grep => grep(&call.source_name, input, dest),
        CanonicalTool::Glob => glob(input, dest),
        CanonicalTool::Ls => match dest {
            HarnessId::Pi => call_pi_ls(input),
            _ => Translation::Text {
                why: "no ls tool offered",
            },
        },
        CanonicalTool::WebFetch => web_fetch(input, dest),
        CanonicalTool::WebSearch => web_search(input, dest),
        // Subagents, skills, questions, MCP servers and harness-internal tools are text form on
        // purpose (L14): the destination's agent types, skills and MCP servers are not the source's.
        _ => Translation::Text {
            why: "capability is not the destination's",
        },
    };
    // Tables §1.2: every argument the call used maps to the destination's tool, or has no effect
    // on the recorded result (§4.1); one with no counterpart makes the call not equivalent.
    if out.is_call() && !uses_only(input, accepted_keys(canonical, dest)) {
        return Translation::Text {
            why: "an argument has no counterpart",
        };
    }
    // L11's safety net: never emit a name the destination does not offer.
    match &out {
        Translation::Call { name, .. } if !offered.contains(name) => Translation::Text {
            why: "the destination's offered set does not contain it",
        },
        _ => out,
    }
}

/// The argument keys, in any harness's spelling, that a translation into `dest` maps or may
/// drop because they do not change the recorded result (tables §4). A call that used any other
/// key is not equivalent.
fn accepted_keys(canonical: &CanonicalTool, dest: HarnessId) -> &'static [&'static str] {
    use CanonicalTool as C;
    use HarnessId as H;
    match (canonical, dest) {
        // Codex's and OpenCode's shells take a working directory; Claude's and Pi's run in the
        // session's. The rest either maps or does not change what the command printed.
        (C::Bash | C::PowerShell, H::Codex | H::OpenCode) => &[
            "command",
            "cmd",
            "workdir",
            "cwd",
            "timeout",
            "description",
            "justification",
            "run_in_background",
            "background",
            "yield_time_ms",
            "max_output_tokens",
            "shell",
            "login",
            "tty",
            "sandbox_permissions",
            "with_escalated_permissions",
            "prefix_rule",
            "dangerouslyDisableSandbox",
        ],
        (C::Bash | C::PowerShell, _) => &[
            "command",
            "cmd",
            "timeout",
            "description",
            "justification",
            "run_in_background",
            "background",
            "yield_time_ms",
            "max_output_tokens",
            "shell",
            "login",
            "tty",
            "sandbox_permissions",
            "with_escalated_permissions",
            "prefix_rule",
            "dangerouslyDisableSandbox",
        ],
        (C::Read, H::ClaudeCode) => &[
            "file_path",
            "path",
            "offset",
            "limit",
            "pages",
            "allow_large",
        ],
        (C::Read, _) => &["file_path", "path", "offset", "limit"],
        (C::Write, _) => &["file_path", "path", "content"],
        (C::Edit, _) => &[
            "file_path",
            "path",
            "old_string",
            "new_string",
            "oldString",
            "newString",
            "replace_all",
            "replaceAll",
            "edits",
        ],
        (C::MultiEdit, _) => &["file_path", "path", "edits"],
        (C::Grep, H::ClaudeCode) => &[
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
        (C::Grep, H::OpenCode) => &[
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
        (C::Grep, _) => &[
            "pattern",
            "path",
            "glob",
            "include",
            "-i",
            "ignoreCase",
            "caseSensitive",
            "literal",
            "-C",
            "context",
            "head_limit",
            "limit",
            "output_mode",
        ],
        (C::Glob, H::OpenCode) => &["pattern", "path", "hidden", "limit"],
        (C::Glob, H::Pi) => &["pattern", "path", "limit"],
        (C::Glob, _) => &["pattern", "path"],
        (C::Ls, _) => &["path", "limit"],
        (C::WebFetch, H::ClaudeCode) => &["url", "prompt"],
        (C::WebFetch, _) => &["url", "format", "timeout"],
        (C::WebSearch, H::ClaudeCode) => &["query", "allowed_domains", "blocked_domains"],
        (C::WebSearch, _) => &["query"],
        _ => &[],
    }
}

// ---------------------------------------------------------------------------------------------
// argument helpers

fn arg<'a>(input: &'a Value, key: &str) -> Option<&'a Value> {
    input.get(key).filter(|v| !v.is_null())
}

fn required(input: &Value, key: &str) -> Option<Value> {
    arg(input, key).cloned()
}

/// The first of several spellings of one argument.
fn either(input: &Value, keys: &[&str]) -> Option<Value> {
    keys.iter().find_map(|key| required(input, key))
}

fn flag(input: &Value, key: &str) -> bool {
    arg(input, key).and_then(Value::as_bool) == Some(true)
}

fn obj<const N: usize>(pairs: [(&str, Value); N]) -> Value {
    let mut map = Map::new();
    for (k, v) in pairs {
        map.insert(k.to_string(), v);
    }
    Value::Object(map)
}

/// The whole input, when the call used nothing but these keys.
fn uses_only(input: &Value, allowed: &[&str]) -> bool {
    match input {
        Value::Object(map) => map.keys().all(|k| allowed.contains(&k.as_str())),
        _ => true,
    }
}

// ---------------------------------------------------------------------------------------------
// shell

fn shell(
    canonical: &CanonicalTool,
    input: &Value,
    dest: HarnessId,
    dest_shell: Option<Shell>,
) -> Translation {
    let source_shell = if *canonical == CanonicalTool::PowerShell {
        Shell::PowerShell
    } else {
        Shell::Posix
    };
    if dest == HarnessId::ClaudeCode {
        // Claude offers both tools; a PowerShell call needs the PowerShell tool.
        return match source_shell {
            Shell::Posix => translate_claude_shell(input, "Bash"),
            Shell::PowerShell => translate_claude_shell(input, "PowerShell"),
        };
    }
    if dest_shell != Some(source_shell) {
        return Translation::Text {
            why: "shell dialect differs from the destination's",
        };
    }
    match dest {
        HarnessId::Codex => translate_codex_shell(input),
        HarnessId::OpenCode => translate_opencode_shell(input),
        HarnessId::Pi => translate_pi_shell(input),
        HarnessId::ClaudeCode => unreachable!(),
    }
}

/// Background, under either spelling: Claude's `run_in_background`, OpenCode's `background`.
fn in_background(input: &Value) -> bool {
    flag(input, "run_in_background") || flag(input, "background")
}

fn translate_claude_shell(input: &Value, tool: &'static str) -> Translation {
    let Some(command) = command_of(input) else {
        return Translation::Text { why: "no command" };
    };
    let mut pairs: Vec<(&str, Value)> = vec![("command", command)];
    if let Some(t) = arg(input, "timeout") {
        pairs.push(("timeout", t.clone()));
    }
    if let Some(d) = either(input, &["description", "justification"]) {
        pairs.push(("description", d));
    }
    if in_background(input) {
        pairs.push(("run_in_background", Value::Bool(true)));
    }
    if let Some(unsandboxed) = arg(input, "dangerouslyDisableSandbox") {
        pairs.push(("dangerouslyDisableSandbox", unsandboxed.clone()));
    }
    Translation::Call {
        name: tool,
        input: obj_pairs(pairs),
    }
}

fn translate_codex_shell(input: &Value) -> Translation {
    let Some(command) = command_of(input) else {
        return Translation::Text { why: "no command" };
    };
    if in_background(input) {
        // Codex has no background flag; it keeps a session with `write_stdin` instead.
        return Translation::Text {
            why: "background execution is not available",
        };
    }
    let mut pairs: Vec<(&str, Value)> = vec![("cmd", command)];
    if let Some(workdir) = either(input, &["workdir", "cwd"]) {
        pairs.push(("workdir", workdir));
    }
    if let Some(justification) = either(input, &["justification", "description"]) {
        pairs.push(("justification", justification));
    }
    // `timeout` is dropped: Codex has no timeout for exec_command and the recorded result does
    // not depend on it (tables §4.1). `yield_time_ms` is not a timeout and is not emitted.
    Translation::Call {
        name: "exec_command",
        input: obj_pairs(pairs),
    }
}

fn translate_opencode_shell(input: &Value) -> Translation {
    let Some(command) = command_of(input) else {
        return Translation::Text { why: "no command" };
    };
    let mut pairs: Vec<(&str, Value)> = vec![("command", command)];
    if let Some(workdir) = either(input, &["workdir", "cwd"]) {
        pairs.push(("workdir", workdir));
    }
    if let Some(timeout) = arg(input, "timeout") {
        pairs.push(("timeout", timeout.clone()));
    }
    if in_background(input) {
        pairs.push(("background", Value::Bool(true)));
    }
    Translation::Call {
        name: "shell",
        input: obj_pairs(pairs),
    }
}

/// L12: Pi measures the timeout in **seconds**, the canonical form in milliseconds.
fn translate_pi_shell(input: &Value) -> Translation {
    let Some(command) = command_of(input) else {
        return Translation::Text { why: "no command" };
    };
    if in_background(input) {
        return Translation::Text {
            why: "background execution is not available",
        };
    }
    let mut pairs: Vec<(&str, Value)> = vec![("command", command)];
    if let Some(ms) = arg(input, "timeout").and_then(Value::as_u64) {
        pairs.push(("timeout", Value::from(ms.div_ceil(1000))));
    }
    Translation::Call {
        name: "bash",
        input: obj_pairs(pairs),
    }
}

/// The command string, under either spelling: Claude's `command`, Codex's `cmd`.
fn command_of(input: &Value) -> Option<Value> {
    either(input, &["command", "cmd"])
}

/// The path argument, under either spelling: Claude's `file_path`, OpenCode's and Pi's `path`.
fn path_of(input: &Value) -> Option<Value> {
    either(input, &["file_path", "path"])
}

// ---------------------------------------------------------------------------------------------
// read

fn read_opencode(input: &Value) -> Translation {
    if arg(input, "pages").is_some() || arg(input, "allow_large").is_some() {
        return Translation::Text {
            why: "pages/allow_large has no counterpart",
        };
    }
    let Some(path) = path_of(input) else {
        return Translation::Text { why: "no path" };
    };
    let mut pairs: Vec<(&str, Value)> = vec![("path", path)];
    for key in ["offset", "limit"] {
        if let Some(v) = arg(input, key) {
            pairs.push((key, v.clone()));
        }
    }
    Translation::Call {
        name: "read",
        input: obj_pairs(pairs),
    }
}

fn read_pi(input: &Value) -> Translation {
    let out = read_opencode(input);
    match out {
        Translation::Call { input, .. } => Translation::Call {
            name: "read",
            input,
        },
        other => other,
    }
}

fn read_claude(input: &Value) -> Translation {
    // A Claude destination keeps its own `Read`: `file_path`, not `path`.
    let Some(path) = path_of(input) else {
        return Translation::Text { why: "no path" };
    };
    let mut pairs: Vec<(&str, Value)> = vec![("file_path", path)];
    for key in ["offset", "limit", "pages", "allow_large"] {
        if let Some(v) = arg(input, key) {
            pairs.push((key, v.clone()));
        }
    }
    Translation::Call {
        name: "Read",
        input: obj_pairs(pairs),
    }
}

// ---------------------------------------------------------------------------------------------
// write / edit

fn write_claude(input: &Value) -> Translation {
    match path_of(input) {
        Some(path) => Translation::Call {
            name: "Write",
            input: obj([("file_path", path), ("content", required_content(input))]),
        },
        None => Translation::Text { why: "no path" },
    }
}

fn write_oc_pi(input: &Value, tool: &'static str) -> Translation {
    match path_of(input) {
        Some(path) => Translation::Call {
            name: tool,
            input: obj([("path", path), ("content", required_content(input))]),
        },
        None => Translation::Text { why: "no path" },
    }
}

fn required_content(input: &Value) -> Value {
    arg(input, "content")
        .cloned()
        .unwrap_or_else(|| Value::String(String::new()))
}

/// One change of an edit, in any harness's spelling: Claude's `old_string`/`new_string`,
/// OpenCode's `oldString`/`newString`, a Pi `edits` entry's `oldText`/`newText`.
fn change_of(entry: &Value) -> Option<(Value, Value)> {
    let old = either(entry, &["old_string", "oldString", "oldText"])?;
    let new = either(entry, &["new_string", "newString", "newText"])?;
    Some((old, new))
}

/// Edit and MultiEdit. "All occurrences" (`replace_all`, OpenCode's `replaceAll`) has no
/// counterpart in Pi or Codex's one-hunk patch, so it blocks translation there (L13). Pi's own
/// `edit` holds its changes in `edits`: one change is an ordinary edit everywhere, several are a
/// multi-edit.
fn edit(canonical: &CanonicalTool, input: &Value, dest: HarnessId) -> Translation {
    let Some(path) = path_of(input) else {
        return Translation::Text { why: "no path" };
    };
    let entries = input.get("edits").and_then(Value::as_array);
    let single_pi_edit = *canonical == CanonicalTool::Edit && entries.is_some_and(|e| e.len() == 1);
    if (*canonical == CanonicalTool::MultiEdit || entries.is_some()) && !single_pi_edit {
        return multi_edit(path, entries.map(Vec::as_slice).unwrap_or_default(), dest);
    }
    let (change, replace_all) = match entries {
        Some(entries) => (change_of(&entries[0]), flag(&entries[0], "replace_all")),
        None => (
            change_of(input).or_else(|| {
                // A plain edit may leave the new text out: it deletes the old one.
                either(input, &["old_string", "oldString"])
                    .map(|old| (old, Value::String(String::new())))
            }),
            flag(input, "replace_all") || flag(input, "replaceAll"),
        ),
    };
    let Some((old, new)) = change else {
        return Translation::Text {
            why: "no old_string",
        };
    };

    match dest {
        HarnessId::OpenCode => {
            let mut pairs: Vec<(&str, Value)> =
                vec![("path", path), ("oldString", old), ("newString", new)];
            if replace_all {
                pairs.push(("replaceAll", Value::Bool(true)));
            }
            Translation::Call {
                name: "edit",
                input: obj_pairs(pairs),
            }
        }
        HarnessId::Pi => {
            if replace_all {
                return Translation::Text {
                    why: "replace_all has no counterpart in Pi",
                };
            }
            let edits = Value::Array(vec![obj([("oldText", old), ("newText", new)])]);
            Translation::Call {
                name: "edit",
                input: obj([("path", path), ("edits", edits)]),
            }
        }
        HarnessId::ClaudeCode => {
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
                input: obj_pairs(pairs),
            }
        }
        HarnessId::Codex => {
            // The patch form (tables §4.3) is not written yet: the pinned Codex of the bench does
            // not offer `apply_patch` at all, and a form no destination has accepted is not worth
            // guessing at. The call travels as text until a fixture proves it.
            Translation::Text {
                why: "Codex's apply_patch form is not written yet",
            }
        }
    }
}

/// Several changes in one call. Only Pi's `edit` takes a list; OpenCode has no multi-edit, Claude
/// 2.1.311 no MultiEdit and Codex's patch is one hunk, and one result cannot be split.
fn multi_edit(path: Value, entries: &[Value], dest: HarnessId) -> Translation {
    if dest != HarnessId::Pi {
        return Translation::Text {
            why: "no multi-edit equivalent",
        };
    }
    if entries.is_empty() {
        return Translation::Text { why: "no edits" };
    }
    let mut mapped = Vec::new();
    for entry in entries {
        if flag(entry, "replace_all") || flag(entry, "replaceAll") {
            return Translation::Text {
                why: "replace_all has no counterpart in Pi",
            };
        }
        let Some((old, new)) = change_of(entry) else {
            return Translation::Text {
                why: "an edit without its old and new text",
            };
        };
        mapped.push(obj([("oldText", old), ("newText", new)]));
    }
    Translation::Call {
        name: "edit",
        input: obj([("path", path), ("edits", Value::Array(mapped))]),
    }
}

// ---------------------------------------------------------------------------------------------
// search

fn grep(source_name: &str, input: &Value, dest: HarnessId) -> Translation {
    // A Claude source keeps its own call: every argument is Claude's.
    if dest == HarnessId::ClaudeCode && source_name == "Grep" {
        return Translation::Call {
            name: "Grep",
            input: input.clone(),
        };
    }
    // Translated only when the call used the shared subset: pattern, path, file filter, case,
    // literal and limit, plus context where the destination has it. Another `output_mode`,
    // line numbers and `multiline` block it (tables §4.4, L13).
    if let Some(mode) = arg(input, "output_mode").and_then(Value::as_str)
        && mode != "files_with_matches"
    {
        return Translation::Text {
            why: "output_mode has no counterpart",
        };
    }
    if arg(input, "type").is_some() {
        return Translation::Text {
            why: "a file `type` has no counterpart",
        };
    }
    let Some(pattern) = required(input, "pattern") else {
        return Translation::Text { why: "no pattern" };
    };
    let filter = either(input, &["glob", "include"]);
    let limit = either(input, &["head_limit", "limit"]);
    let context = either(input, &["context", "-C"]);
    let ignore_case = arg(input, "-i")
        .or_else(|| arg(input, "ignoreCase"))
        .and_then(Value::as_bool)
        .or_else(|| {
            arg(input, "caseSensitive")
                .and_then(Value::as_bool)
                .map(|s| !s)
        });
    let literal = arg(input, "literal").cloned();
    let mut pairs: Vec<(&str, Value)> = vec![("pattern", pattern)];
    if let Some(path) = required(input, "path") {
        pairs.push(("path", path));
    }

    match dest {
        HarnessId::ClaudeCode => {
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
                input: obj_pairs(pairs),
            }
        }
        HarnessId::OpenCode => {
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
                input: obj_pairs(pairs),
            }
        }
        HarnessId::Pi => {
            if let Some(filter) = filter {
                pairs.push(("glob", filter));
            }
            if let Some(i) = ignore_case {
                pairs.push(("ignoreCase", Value::Bool(i)));
            }
            if let Some(literal) = literal {
                pairs.push(("literal", literal));
            }
            if let Some(context) = context {
                pairs.push(("context", context));
            }
            if let Some(limit) = limit {
                pairs.push(("limit", limit));
            }
            Translation::Call {
                name: "grep",
                input: obj_pairs(pairs),
            }
        }
        HarnessId::Codex => Translation::Text {
            why: "no grep tool offered",
        },
    }
}

fn glob(input: &Value, dest: HarnessId) -> Translation {
    let Some(pattern) = required(input, "pattern") else {
        return Translation::Text { why: "no pattern" };
    };
    let mut pairs: Vec<(&str, Value)> = vec![("pattern", pattern)];
    if let Some(path) = arg(input, "path") {
        pairs.push(("path", path.clone()));
    }
    let name = match dest {
        HarnessId::ClaudeCode => "Glob",
        HarnessId::OpenCode => "glob",
        HarnessId::Pi => "find",
        HarnessId::Codex => {
            return Translation::Text {
                why: "no glob tool offered",
            };
        }
    };
    // OpenCode's `hidden` and `limit` and Pi's `limit` reach only the tools that have them; the
    // accepted keys turn any other use into text form.
    for key in ["hidden", "limit"] {
        if let Some(v) = arg(input, key) {
            pairs.push((key, v.clone()));
        }
    }
    Translation::Call {
        name,
        input: obj_pairs(pairs),
    }
}

fn web_fetch(input: &Value, dest: HarnessId) -> Translation {
    let Some(url) = required(input, "url") else {
        return Translation::Text { why: "no url" };
    };
    match dest {
        // Claude's WebFetch needs the prompt it answers.
        HarnessId::ClaudeCode => match required(input, "prompt") {
            Some(prompt) => Translation::Call {
                name: "WebFetch",
                input: obj([("url", url), ("prompt", prompt)]),
            },
            None => Translation::Text { why: "no prompt" },
        },
        HarnessId::OpenCode => {
            let mut pairs: Vec<(&str, Value)> = vec![("url", url)];
            for key in ["format", "timeout"] {
                if let Some(v) = arg(input, key) {
                    pairs.push((key, v.clone()));
                }
            }
            Translation::Call {
                name: "webfetch",
                input: obj_pairs(pairs),
            }
        }
        _ => Translation::Text {
            why: "no webfetch tool offered",
        },
    }
}

fn web_search(input: &Value, dest: HarnessId) -> Translation {
    let Some(query) = required(input, "query") else {
        return Translation::Text { why: "no query" };
    };
    match dest {
        HarnessId::ClaudeCode => {
            let mut pairs: Vec<(&str, Value)> = vec![("query", query)];
            for key in ["allowed_domains", "blocked_domains"] {
                if let Some(v) = arg(input, key) {
                    pairs.push((key, v.clone()));
                }
            }
            Translation::Call {
                name: "WebSearch",
                input: obj_pairs(pairs),
            }
        }
        HarnessId::OpenCode => Translation::Call {
            name: "websearch",
            input: obj([("query", query)]),
        },
        _ => Translation::Text {
            why: "web search is server-side or absent",
        },
    }
}

fn call_pi_ls(input: &Value) -> Translation {
    let mut pairs: Vec<(&str, Value)> = Vec::new();
    for key in ["path", "limit"] {
        if let Some(v) = arg(input, key) {
            pairs.push((key, v.clone()));
        }
    }
    Translation::Call {
        name: "ls",
        input: obj_pairs(pairs),
    }
}

fn obj_pairs(pairs: Vec<(&str, Value)>) -> Value {
    let mut map = Map::new();
    for (k, v) in pairs {
        map.insert(k.to_string(), v);
    }
    Value::Object(map)
}

/// Whether a call's input used nothing but the keys this crate knows how to map, outside the
/// rules above. Kept for callers that want to be strict about unknown arguments.
pub fn uses_only_known_keys(call: &ToolCall, allowed: &[&str]) -> bool {
    uses_only(&call.input, allowed)
}

// ---------------------------------------------------------------------------------------------
// text form

/// The assistant-text form of a call that has no equivalent (spec §1 policy 4).
pub fn text_form_call(call: &ToolCall, dest: HarnessId) -> String {
    text_form(call, &format!("not available in {}", dest.as_str()))
}

/// The text form of a call the source never answered: the destination may offer the tool, but a
/// call without its result would be refused (§7, L7).
pub fn text_form_unanswered(call: &ToolCall) -> String {
    text_form(call, "no result in the source")
}

fn text_form(call: &ToolCall, note: &str) -> String {
    let mut out = format!(
        "[tool call · {} · id {} · {note}]\n",
        call.source_name, call.id
    );
    match &without_encrypted(&call.input) {
        Value::Object(map) if !map.is_empty() => {
            for (k, v) in map {
                out.push_str(&format!("{k}: {}\n", one_line(v)));
            }
            out.pop();
        }
        Value::Null => {}
        other => out.push_str(&one_line(other)),
    }
    out
}

/// The text form of a result whose call travelled as text.
pub fn text_form_result(result: &ToolResult) -> String {
    let flag = if result.is_error { " (error)" } else { "" };
    format!(
        "[tool result · id {}]{}\n{}",
        result.call_id,
        flag,
        result.content.text()
    )
}

/// A token of OpenAI's encryption (Fernet: `gAAAAA` and base64url) is at least this long.
const MIN_ENCRYPTED: usize = 100;

/// `value` with every token the source's provider encrypted replaced by a placeholder: only that
/// provider can read it, so written into the text of another harness it is only noise (Codex
/// Desktop's subagent messages travel whole in such tokens).
fn without_encrypted(value: &Value) -> Value {
    match value {
        Value::String(s) if s.contains("gAAAAA") => Value::String(replace_encrypted(s)),
        Value::Array(items) => Value::Array(items.iter().map(without_encrypted).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), without_encrypted(v)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn replace_encrypted(text: &str) -> String {
    let token_char = |c: char| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '=');
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("gAAAAA") {
        let len = rest[start..]
            .find(|c: char| !token_char(c))
            .unwrap_or(rest.len() - start);
        let starts_a_word = rest[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !token_char(c));
        out.push_str(&rest[..start]);
        if starts_a_word && len >= MIN_ENCRYPTED {
            out.push_str(&format!(
                "<encrypted by the source's provider, {len} chars>"
            ));
        } else {
            out.push_str(&rest[start..start + len]);
        }
        rest = &rest[start + len..];
    }
    out.push_str(rest);
    out
}

fn one_line(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// The content of a result as the destination should receive it, unchanged.
pub fn content_blocks(result: &ToolResult) -> Vec<Value> {
    match &result.content {
        ResultContent::Text(t) => vec![serde_json::json!({"type": "text", "text": t})],
        ResultContent::Blocks(blocks) => blocks.clone(),
    }
}
