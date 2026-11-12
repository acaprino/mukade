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

mod patch;

use crate::harnesses;
use crate::model::{CanonicalTool, HarnessId, ResultContent, ToolCall, ToolResult};
use crate::roots::StoreRoots;
use serde_json::{Map, Value};
use std::path::Path;

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

/// What each destination offers, for the versions the bench pinned. A harness that is no
/// destination offers nothing.
pub fn offered(harness: HarnessId) -> ToolSet {
    harnesses::get(harness)
        .destination()
        .map_or(ToolSet { names: &[] }, |destination| destination.offered())
}

/// What a destination offers in the installation `roots` name, for the project `cwd`: the pinned
/// list plus the tools its configuration turns on. A harness that is no destination offers nothing.
pub fn offered_in(harness: HarnessId, roots: &StoreRoots, cwd: Option<&Path>) -> ToolSet {
    harnesses::get(harness)
        .destination()
        .map_or(ToolSet { names: &[] }, |destination| {
            destination.offered_in(roots, cwd)
        })
}

/// The dialect of a shell named by its program or path, as a harness's configuration names it:
/// the base name, without `.exe`, in any case. `None` is a shell of no dialect mukade translates
/// toward (`cmd`, `fish`, `nu`, an unknown one) or no name.
pub fn shell_named(name: &str) -> Option<Shell> {
    let base = name.trim().rsplit(['/', '\\']).next().unwrap_or_default();
    let base = base.to_ascii_lowercase();
    match base.strip_suffix(".exe").unwrap_or(&base) {
        "bash" | "sh" | "zsh" | "dash" | "ksh" => Some(Shell::Posix),
        "pwsh" | "powershell" => Some(Shell::PowerShell),
        _ => None,
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

/// Translate with explicit bindings derived from the host's real tool catalog.
pub fn translate_with_map(
    call: &ToolCall,
    map: &[(String, String, Value, Value)],
) -> Option<(String, Value)> {
    if let Some((_, name, _, input)) = map.iter().find(|(source, _, original, _)| source == &call.source_name && original == &call.input) {
        return Some((name.clone(), input.clone()));
    }
    None
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
    // A Codex patch is read as the write or edit it makes; its recorded input stays the patch.
    let read_patch = match call.source_name.as_str() {
        "apply_patch" => patch::view(&call.input),
        _ => None,
    };
    let (canonical, input) = match &read_patch {
        Some((canonical, input)) => (canonical, input),
        None => (canonical, &call.input),
    };
    // The destination's adapter gives the call its own tool's shape.
    let Some(adapter) = harnesses::get(dest).destination() else {
        return Translation::Text {
            why: "not a destination",
        };
    };
    let out = adapter.translate(canonical, &call.source_name, input, dest_shell);
    // Tables §1.2: every argument the call used maps to the destination's tool, or has no effect
    // on the recorded result (§4.1); one with no counterpart makes the call not equivalent.
    if out.is_call() && !uses_only(input, adapter.accepted_keys(canonical)) {
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

/// Subagents, skills, questions, MCP servers and harness-internal tools are text form on purpose
/// (L14): the destination's agent types, skills and MCP servers are not the source's.
pub(crate) const OTHER_CAPABILITY: Translation = Translation::Text {
    why: "capability is not the destination's",
};

/// Several changes in one call, toward a destination whose edit makes one: one result cannot be
/// split.
pub(crate) const NO_MULTI_EDIT: Translation = Translation::Text {
    why: "no multi-edit equivalent",
};

/// A shell call in the background, toward a destination whose shell has no background mode.
pub(crate) const NO_BACKGROUND: Translation = Translation::Text {
    why: "background execution is not available",
};

/// The argument keys, in any harness's spelling, that a translation maps or may drop because they
/// do not change the recorded result (tables §4), where the destination's adapter names no others.
/// A call that used any other key is not equivalent.
pub(crate) fn common_keys(canonical: &CanonicalTool) -> &'static [&'static str] {
    use CanonicalTool as C;
    match canonical {
        // A shell that runs in the session's directory. The rest either maps or does not change
        // what the command printed.
        C::Bash | C::PowerShell => &[
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
        C::Read => &["file_path", "path", "offset", "limit"],
        C::Write => &["file_path", "path", "content"],
        C::Edit => &[
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
        C::MultiEdit => &["file_path", "path", "edits"],
        C::Grep => &[
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
        C::Glob => &["pattern", "path"],
        C::Ls => &["path", "limit"],
        C::WebFetch => &["url", "format", "timeout"],
        C::WebSearch => &["query"],
        _ => &[],
    }
}

/// A shell's keys where the shell takes a working directory (Codex's, OpenCode's); Claude's and
/// Pi's run in the session's.
pub(crate) const SHELL_KEYS_WITH_WORKDIR: &[&str] = &[
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
];

// ---------------------------------------------------------------------------------------------
// argument helpers

pub(crate) fn arg<'a>(input: &'a Value, key: &str) -> Option<&'a Value> {
    input.get(key).filter(|v| !v.is_null())
}

pub(crate) fn required(input: &Value, key: &str) -> Option<Value> {
    arg(input, key).cloned()
}

/// The first of several spellings of one argument.
pub(crate) fn either(input: &Value, keys: &[&str]) -> Option<Value> {
    keys.iter().find_map(|key| required(input, key))
}

pub(crate) fn flag(input: &Value, key: &str) -> bool {
    arg(input, key).and_then(Value::as_bool) == Some(true)
}

pub(crate) fn obj<const N: usize>(pairs: [(&str, Value); N]) -> Value {
    let mut map = Map::new();
    for (k, v) in pairs {
        map.insert(k.to_string(), v);
    }
    Value::Object(map)
}

pub(crate) fn obj_pairs(pairs: Vec<(&str, Value)>) -> Value {
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

/// The path argument, under either spelling: Claude's `file_path`, OpenCode's and Pi's `path`.
pub(crate) fn path_of(input: &Value) -> Option<Value> {
    either(input, &["file_path", "path"])
}

// ---------------------------------------------------------------------------------------------
// shell

/// The text form of a shell call toward a shell of another dialect, or of one not known: the
/// command string is never rewritten.
pub(crate) fn other_dialect(
    canonical: &CanonicalTool,
    dest_shell: Option<Shell>,
) -> Option<Translation> {
    let source_shell = if *canonical == CanonicalTool::PowerShell {
        Shell::PowerShell
    } else {
        Shell::Posix
    };
    (dest_shell != Some(source_shell)).then_some(Translation::Text {
        why: "shell dialect differs from the destination's",
    })
}

/// The command string, under either spelling: Claude's `command`, Codex's `cmd`.
pub(crate) fn command_of(input: &Value) -> Option<Value> {
    either(input, &["command", "cmd"])
}

/// Background, under either spelling: Claude's `run_in_background`, OpenCode's `background`.
pub(crate) fn in_background(input: &Value) -> bool {
    flag(input, "run_in_background") || flag(input, "background")
}

// ---------------------------------------------------------------------------------------------
// read / write

/// OpenCode's and Pi's `read`: `path`, `offset` and `limit`.
pub(crate) fn read_by_path(input: &Value) -> Translation {
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

/// OpenCode's and Pi's `write`: `path` and `content`.
pub(crate) fn write_by_path(input: &Value) -> Translation {
    match path_of(input) {
        Some(path) => Translation::Call {
            name: "write",
            input: obj([("path", path), ("content", required_content(input))]),
        },
        None => Translation::Text { why: "no path" },
    }
}

pub(crate) fn required_content(input: &Value) -> Value {
    arg(input, "content")
        .cloned()
        .unwrap_or_else(|| Value::String(String::new()))
}

// ---------------------------------------------------------------------------------------------
// edit

/// An Edit or MultiEdit call, read in any harness's spelling.
pub(crate) enum EditCall<'a> {
    /// One change; `replace_all` is "all occurrences".
    One {
        path: Value,
        old: Value,
        new: Value,
        replace_all: bool,
    },
    /// Several changes in one call, as the call lists them.
    Several { path: Value, entries: &'a [Value] },
}

/// One change of an edit, in any harness's spelling: Claude's `old_string`/`new_string`,
/// OpenCode's `oldString`/`newString`, a Pi `edits` entry's `oldText`/`newText`.
pub(crate) fn change_of(entry: &Value) -> Option<(Value, Value)> {
    let old = either(entry, &["old_string", "oldString", "oldText"])?;
    let new = either(entry, &["new_string", "newString", "newText"])?;
    Some((old, new))
}

/// Read an Edit or MultiEdit call; `Err` is its text form. "All occurrences" (`replace_all`,
/// OpenCode's `replaceAll`) is read for the destination to keep or refuse (L13). Pi's own `edit`
/// holds its changes in `edits`: one change is an ordinary edit everywhere, several are a
/// multi-edit.
pub(crate) fn edit_call<'a>(
    canonical: &CanonicalTool,
    input: &'a Value,
) -> Result<EditCall<'a>, Translation> {
    let Some(path) = path_of(input) else {
        return Err(Translation::Text { why: "no path" });
    };
    let entries = input.get("edits").and_then(Value::as_array);
    let single_pi_edit = *canonical == CanonicalTool::Edit && entries.is_some_and(|e| e.len() == 1);
    if (*canonical == CanonicalTool::MultiEdit || entries.is_some()) && !single_pi_edit {
        return Ok(EditCall::Several {
            path,
            entries: entries.map(Vec::as_slice).unwrap_or_default(),
        });
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
        return Err(Translation::Text {
            why: "no old_string",
        });
    };
    Ok(EditCall::One {
        path,
        old,
        new,
        replace_all,
    })
}

// ---------------------------------------------------------------------------------------------
// search

/// A grep call over the subset the destinations share: pattern, path, file filter, case, literal
/// and limit, plus context where the destination has it.
pub(crate) struct GrepCall {
    /// The pattern and the path, which every destination spells alike.
    pub(crate) pairs: Vec<(&'static str, Value)>,
    pub(crate) filter: Option<Value>,
    pub(crate) limit: Option<Value>,
    pub(crate) context: Option<Value>,
    pub(crate) ignore_case: Option<bool>,
    pub(crate) literal: Option<Value>,
}

/// Read a grep call in any harness's spelling; `Err` is its text form. Another `output_mode` and a
/// file `type` block it (tables §4.4, L13). Line numbers and `multiline` are blocked by the
/// destination's accepted keys; Claude Code's serve its own Grep, so its adapter refuses them.
pub(crate) fn grep_call(input: &Value) -> Result<GrepCall, Translation> {
    if let Some(mode) = arg(input, "output_mode").and_then(Value::as_str)
        && mode != "files_with_matches"
    {
        return Err(Translation::Text {
            why: "output_mode has no counterpart",
        });
    }
    if arg(input, "type").is_some() {
        return Err(Translation::Text {
            why: "a file `type` has no counterpart",
        });
    }
    let Some(pattern) = required(input, "pattern") else {
        return Err(Translation::Text { why: "no pattern" });
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
    let mut pairs: Vec<(&'static str, Value)> = vec![("pattern", pattern)];
    if let Some(path) = required(input, "path") {
        pairs.push(("path", path));
    }
    Ok(GrepCall {
        pairs,
        filter,
        limit,
        context,
        ignore_case,
        literal,
    })
}

/// A glob call's input, which every destination with a glob tool spells alike; `Err` is its text
/// form. OpenCode's `hidden` and `limit` and Pi's `limit` reach only the tools that have them: the
/// accepted keys turn any other use into text form.
pub(crate) fn glob_input(input: &Value) -> Result<Value, Translation> {
    let Some(pattern) = required(input, "pattern") else {
        return Err(Translation::Text { why: "no pattern" });
    };
    let mut pairs: Vec<(&str, Value)> = vec![("pattern", pattern)];
    if let Some(path) = arg(input, "path") {
        pairs.push(("path", path.clone()));
    }
    for key in ["hidden", "limit"] {
        if let Some(v) = arg(input, key) {
            pairs.push((key, v.clone()));
        }
    }
    Ok(obj_pairs(pairs))
}

// ---------------------------------------------------------------------------------------------
// web

/// A web fetch's `url`; `Err` is the text form of a call without one.
pub(crate) fn url_of(input: &Value) -> Result<Value, Translation> {
    required(input, "url").ok_or(Translation::Text { why: "no url" })
}

/// A web search's `query`; `Err` is the text form of a call without one.
pub(crate) fn query_of(input: &Value) -> Result<Value, Translation> {
    required(input, "query").ok_or(Translation::Text { why: "no query" })
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
