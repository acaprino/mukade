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
            let source_shell = if *canonical == CanonicalTool::PowerShell {
                Shell::PowerShell
            } else {
                Shell::Posix
            };
            if dest == HarnessId::ClaudeCode {
                // Claude offers both tools; a PowerShell call needs the PowerShell tool.
                match source_shell {
                    Shell::Posix => translate_claude_shell(input, "Bash"),
                    Shell::PowerShell => translate_claude_shell(input, "PowerShell"),
                }
            } else {
                if dest_shell != Some(source_shell) {
                    return Translation::Text {
                        why: "shell dialect differs from the destination's",
                    };
                }
                match dest {
                    HarnessId::Codex => translate_codex_shell(input, source_shell),
                    HarnessId::OpenCode => translate_opencode_shell(input),
                    HarnessId::Pi => translate_pi_shell(input),
                    HarnessId::ClaudeCode => unreachable!(),
                }
            }
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
        CanonicalTool::Grep => grep(input, dest),
        CanonicalTool::Glob => glob(input, dest),
        CanonicalTool::Ls => match dest {
            HarnessId::Pi => call_pi_ls(input),
            _ => Translation::Text {
                why: "no ls tool offered",
            },
        },
        CanonicalTool::WebFetch => match dest {
            HarnessId::OpenCode => match required(input, "url") {
                Some(url) => Translation::Call {
                    name: "webfetch",
                    input: obj([("url", url)]),
                },
                None => Translation::Text { why: "no url" },
            },
            _ => Translation::Text {
                why: "no webfetch tool offered",
            },
        },
        CanonicalTool::WebSearch => match dest {
            HarnessId::OpenCode => match required(input, "query") {
                Some(query) => Translation::Call {
                    name: "websearch",
                    input: obj([("query", query)]),
                },
                None => Translation::Text { why: "no query" },
            },
            _ => Translation::Text {
                why: "web search is server-side or absent",
            },
        },
        // Subagents, skills, questions, MCP servers and harness-internal tools are text form on
        // purpose (L14): the destination's agent types, skills and MCP servers are not the source's.
        _ => Translation::Text {
            why: "capability is not the destination's",
        },
    };
    // L11's safety net: never emit a name the destination does not offer.
    match &out {
        Translation::Call { name, .. } if !offered.contains(name) => Translation::Text {
            why: "the destination's offered set does not contain it",
        },
        _ => out,
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

fn translate_claude_shell(input: &Value, tool: &'static str) -> Translation {
    let Some(command) = command_of(input) else {
        return Translation::Text { why: "no command" };
    };
    if arg(input, "run_in_background").and_then(Value::as_bool) == Some(true) {
        return Translation::Text {
            why: "background execution is not available",
        };
    }
    let mut pairs: Vec<(&str, Value)> = vec![("command", command)];
    if let Some(t) = arg(input, "timeout") {
        pairs.push(("timeout", t.clone()));
    }
    if let Some(d) = arg(input, "description") {
        pairs.push(("description", d.clone()));
    }
    Translation::Call {
        name: tool,
        input: obj_pairs(pairs),
    }
}

fn translate_codex_shell(input: &Value, _source: Shell) -> Translation {
    let Some(command) = command_of(input) else {
        return Translation::Text { why: "no command" };
    };
    if arg(input, "run_in_background").and_then(Value::as_bool) == Some(true) {
        // Codex has no background flag; it keeps a session with `write_stdin` instead.
        return Translation::Text {
            why: "background execution is not available",
        };
    }
    let mut pairs: Vec<(&str, Value)> = vec![("cmd", command)];
    if let Some(workdir) = arg(input, "workdir").or_else(|| arg(input, "cwd")) {
        pairs.push(("workdir", workdir.clone()));
    }
    if let Some(justification) = arg(input, "description") {
        pairs.push(("justification", justification.clone()));
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
    if let Some(workdir) = arg(input, "workdir").or_else(|| arg(input, "cwd")) {
        pairs.push(("workdir", workdir.clone()));
    }
    if let Some(timeout) = arg(input, "timeout") {
        pairs.push(("timeout", timeout.clone()));
    }
    if arg(input, "run_in_background").and_then(Value::as_bool) == Some(true) {
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
    if arg(input, "run_in_background").and_then(Value::as_bool) == Some(true) {
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
    required(input, "command").or_else(|| required(input, "cmd"))
}

/// The path argument, under either spelling: Claude's `file_path`, OpenCode's and Pi's `path`.
fn path_of(input: &Value) -> Option<Value> {
    required(input, "file_path").or_else(|| required(input, "path"))
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
    for key in ["offset", "limit"] {
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
    match required(input, "file_path") {
        Some(path) => Translation::Call {
            name: "Write",
            input: obj([("file_path", path), ("content", required_content(input))]),
        },
        None => Translation::Text {
            why: "no file_path",
        },
    }
}

fn write_oc_pi(input: &Value, tool: &'static str) -> Translation {
    // `file_path` from a Claude source, `path` from OpenCode/Pi.
    let path = required(input, "file_path").or_else(|| required(input, "path"));
    match path {
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

/// Edit and MultiEdit. `replace_all: true` has no counterpart in Pi or Codex's one-hunk patch, so
/// it blocks translation there (L13).
fn edit(canonical: &CanonicalTool, input: &Value, dest: HarnessId) -> Translation {
    let multi = *canonical == CanonicalTool::MultiEdit;
    let path = required(input, "file_path").or_else(|| required(input, "path"));
    let Some(path) = path else {
        return Translation::Text { why: "no path" };
    };
    let replace_all = arg(input, "replace_all").and_then(Value::as_bool) == Some(true);

    if multi {
        let edits = input
            .get("edits")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        return match dest {
            HarnessId::Pi => {
                let mapped: Vec<Value> = edits
                    .iter()
                    .map(|e| {
                        obj([
                            ("oldText", required(e, "old_string").unwrap_or(Value::Null)),
                            ("newText", required(e, "new_string").unwrap_or(Value::Null)),
                        ])
                    })
                    .collect();
                Translation::Call {
                    name: "edit",
                    input: obj([("path", path), ("edits", Value::Array(mapped))]),
                }
            }
            // OpenCode has no multi-edit and Claude 2.1.311 has no MultiEdit; Codex's patch is
            // one hunk. One result cannot be split, so it stays text form.
            _ => Translation::Text {
                why: "no multi-edit equivalent",
            },
        };
    }

    let Some(old) = required(input, "old_string").or_else(|| required(input, "oldString")) else {
        return Translation::Text {
            why: "no old_string",
        };
    };
    let new = required(input, "new_string")
        .or_else(|| required(input, "newString"))
        .unwrap_or(Value::String(String::new()));

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

// ---------------------------------------------------------------------------------------------
// search

fn grep(input: &Value, dest: HarnessId) -> Translation {
    // Translated only when the call used the shared subset: pattern, path, file filter, case and
    // limit. `output_mode`, context lines and `multiline` block it (tables §4.4, L13).
    if let Some(mode) = arg(input, "output_mode").and_then(Value::as_str)
        && mode != "files_with_matches"
    {
        return Translation::Text {
            why: "output_mode has no counterpart",
        };
    }
    if arg(input, "-n").is_some() || arg(input, "multiline").is_some() {
        return Translation::Text {
            why: "output_mode flags have no counterpart",
        };
    }
    let has_context = ["-A", "-B", "-C", "context"]
        .iter()
        .any(|k| arg(input, k).is_some());
    let Some(pattern) = required(input, "pattern") else {
        return Translation::Text { why: "no pattern" };
    };
    let case = arg(input, "-i").and_then(Value::as_bool);

    match dest {
        HarnessId::OpenCode => {
            if has_context {
                return Translation::Text {
                    why: "context lines have no counterpart in OpenCode",
                };
            }
            if arg(input, "type").is_some() {
                return Translation::Text {
                    why: "a file `type` has no counterpart",
                };
            }
            let mut pairs: Vec<(&str, Value)> = vec![("pattern", pattern)];
            for (from, to) in [
                ("path", "path"),
                ("glob", "include"),
                ("head_limit", "limit"),
            ] {
                if let Some(v) = arg(input, from) {
                    pairs.push((to, v.clone()));
                }
            }
            if let Some(i) = case {
                pairs.push(("caseSensitive", Value::Bool(!i)));
            }
            Translation::Call {
                name: "grep",
                input: obj_pairs(pairs),
            }
        }
        HarnessId::Pi => {
            if arg(input, "type").is_some() {
                return Translation::Text {
                    why: "the file filter has no counterpart",
                };
            }
            let mut pairs: Vec<(&str, Value)> = vec![("pattern", pattern)];
            for key in ["path", "context", "limit"] {
                let from = if key == "limit" { "head_limit" } else { key };
                if let Some(v) = arg(input, from) {
                    pairs.push((key, v.clone()));
                }
            }
            Translation::Call {
                name: "grep",
                input: obj_pairs(pairs),
            }
        }
        _ => Translation::Text {
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
    match dest {
        HarnessId::OpenCode => Translation::Call {
            name: "glob",
            input: obj_pairs(pairs),
        },
        HarnessId::Pi => Translation::Call {
            name: "find",
            input: obj_pairs(pairs),
        },
        _ => Translation::Text {
            why: "no glob tool offered",
        },
    }
}

fn call_pi_ls(input: &Value) -> Translation {
    let mut pairs: Vec<(&str, Value)> = Vec::new();
    if let Some(path) = arg(input, "path") {
        pairs.push(("path", path.clone()));
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
    let mut out = format!(
        "[tool call · {} · id {} · not available in {}]\n",
        call.source_name,
        call.id,
        dest.as_str()
    );
    match &call.input {
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
