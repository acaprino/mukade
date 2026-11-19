//! A separate observation model: never serialize the interchange model to an observer.
use super::secrets;
use crate::{
    Kind, Session,
    model::{CanonicalTool, ResultContent},
    reader::origin::{Origin, origin_of},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Version of the permitted-field projection and its full-field fingerprint.
pub const PROJECTION_VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewKind {
    Prompt,
    Injected,
    ShellCommand,
    Reply,
    ToolUse,
    ToolOutcome,
    Notice,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewItem {
    pub id: String,
    pub kind: ViewKind,
    pub text: String,
    pub full_len: usize,
    pub fingerprint: String,
}

/// Terminal-safe text. Redact the complete value before escaping or cutting it.
pub(crate) fn safe(text: &str) -> String {
    secrets::mask(text)
        .chars()
        .flat_map(|c| {
            if c.is_control() || matches!(c,'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}') {
                c.escape_default().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect()
}
fn item(id: String, kind: ViewKind, source: &str, limit: usize) -> ViewItem {
    let full_len = source.chars().count();
    let masked = safe(source);
    let fingerprint = crate::sha256::sha256_hex(
        format!("{PROJECTION_VERSION}\0{id}\0{kind:?}\0{masked}\0{full_len}").as_bytes(),
    );
    let text = if masked.chars().count() > limit {
        format!(
            "{}… ({full_len} characters in all)",
            masked.chars().take(limit).collect::<String>()
        )
    } else {
        masked
    };
    ViewItem {
        id,
        kind,
        text,
        full_len,
        fingerprint,
    }
}

pub(super) fn legacy_fingerprint(item: &ViewItem) -> String {
    crate::sha256::sha256_hex(
        format!("{:?}\0{}\0{}", item.kind, item.text, item.full_len).as_bytes(),
    )
}
fn field<'a>(input: &'a Value, names: &[&str]) -> Option<&'a str> {
    names
        .iter()
        .find_map(|name| input.get(name).and_then(Value::as_str))
}

fn size_view(bytes: usize, scalars: usize) -> String {
    format!("{bytes} UTF-8 bytes / {scalars} Unicode scalars")
}

/// Sizes describe supplied text, not the eventual file delta (replace-all can match
/// multiple times). Only counts enter the projection; the bodies never do.
fn change_view(input: &Value, write: bool) -> Option<String> {
    let mut fields: Vec<String> = field(input, &["file_path", "path"])
        .map(str::to_owned)
        .into_iter()
        .collect();
    if write {
        if let Some(content) = field(input, &["content"]) {
            fields.push(format!(
                "content: {}",
                size_view(content.len(), content.chars().count())
            ));
        }
    } else {
        let edits = input.get("edits").and_then(Value::as_array);
        if let Some(edits) = edits {
            fields.push(format!("{} edits", edits.len()));
        }
        let entries: Vec<&Value> = edits
            .map(|rows| rows.iter().collect())
            .unwrap_or_else(|| vec![input]);
        for (label, names) in [
            ("old", ["old_string", "oldString", "oldText"]),
            ("new", ["new_string", "newString", "newText"]),
        ] {
            let mut bytes = 0;
            let mut scalars = 0;
            let mut known = 0;
            for entry in &entries {
                if let Some(text) = field(entry, &names) {
                    bytes += text.len();
                    scalars += text.chars().count();
                    known += 1;
                }
            }
            if known > 0 {
                let qualifier = if known == entries.len() {
                    String::new()
                } else {
                    format!(" ({known}/{} known)", entries.len())
                };
                fields.push(format!("{label}{qualifier}: {}", size_view(bytes, scalars)));
            }
        }
    }
    (!fields.is_empty()).then(|| fields.join("; "))
}

/// Shell text has no universal safe parser. Only a simple invocation of a known command
/// contributes arguments. Script interpreters, writes and compound syntax contribute a name.
fn command_view(command: &str) -> String {
    let masked = secrets::mask(command);
    let first = masked.lines().next().unwrap_or("").trim();
    let executable = first.split_whitespace().next().unwrap_or("shell command");
    let subcommand = first.split_whitespace().nth(1);
    // Wrappers can carry programs even without shell metacharacters. Admit only
    // known subcommands; package runners and arbitrary aliases show their name.
    let known = match executable.to_ascii_lowercase().as_str() {
        "git" => matches!(
            subcommand,
            Some("status" | "diff" | "show" | "log" | "ls-files" | "rev-parse" | "branch" | "grep")
        ),
        "cargo" => matches!(
            subcommand,
            Some(
                "build"
                    | "check"
                    | "test"
                    | "fmt"
                    | "clippy"
                    | "clean"
                    | "tree"
                    | "metadata"
                    | "version"
            )
        ),
        "rg" | "grep" | "ls" | "cat" | "pwd" | "rustc" | "get-content" | "get-childitem"
        | "get-location" => true,
        _ => false,
    };
    let simple = known && !first.chars().any(|c| "'\"|;&<>`$(){}!=".contains(c));
    let mut result = if simple {
        first.to_string()
    } else {
        let name = if executable
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-/\\:".contains(c))
        {
            executable
        } else {
            "shell command"
        };
        format!("{name} [arguments/body hidden]")
    };
    let hidden = masked.lines().count().saturating_sub(1);
    if hidden > 0 {
        result.push_str(&format!(" [+{hidden} more lines hidden]"));
    }
    result
}

pub fn build(session: &Session) -> Vec<ViewItem> {
    let mut out = Vec::new();
    for entry in &session.entries {
        match &entry.kind {
            Kind::User { text, .. } => {
                let (kind, text, limit) = match origin_of(session.meta.harness, entry) {
                    Origin::Typed => (ViewKind::Prompt, text.clone(), 1200),
                    Origin::ShellCommand { command } => {
                        (ViewKind::ShellCommand, command_view(&command), 160)
                    }
                    Origin::Injected(kind) => (
                        ViewKind::Injected,
                        format!("[{kind} entry, content not shown]"),
                        160,
                    ),
                    Origin::Unknown => (
                        ViewKind::Injected,
                        "[unknown entry, content not shown]".into(),
                        160,
                    ),
                };
                out.push(item(entry.id.clone(), kind, &text, limit));
            }
            Kind::Step { text, calls, .. } => {
                if !text.is_empty() {
                    out.push(item(
                        entry.id.clone(),
                        ViewKind::Reply,
                        &text.join("\n"),
                        1200,
                    ));
                }
                for (i, call) in calls.iter().enumerate() {
                    let detail = match call.canonical.as_ref() {
                        Some(CanonicalTool::Bash | CanonicalTool::PowerShell) => {
                            field(&call.input, &["command", "cmd"]).map(command_view)
                        }
                        Some(CanonicalTool::Read | CanonicalTool::Ls) => {
                            field(&call.input, &["file_path", "path"]).map(str::to_string)
                        }
                        Some(CanonicalTool::Write) => change_view(&call.input, true),
                        Some(CanonicalTool::Edit | CanonicalTool::MultiEdit) => {
                            change_view(&call.input, false)
                        }
                        Some(CanonicalTool::Grep) => field(&call.input, &["pattern"]).map(|p| {
                            format!("{p} {}", field(&call.input, &["path"]).unwrap_or(""))
                        }),
                        Some(CanonicalTool::Glob) => {
                            field(&call.input, &["pattern"]).map(str::to_string)
                        }
                        Some(CanonicalTool::WebFetch) => {
                            field(&call.input, &["url"]).map(str::to_string)
                        }
                        Some(CanonicalTool::WebSearch) => {
                            field(&call.input, &["query"]).map(str::to_string)
                        }
                        _ => None,
                    };
                    let text = detail
                        .map(|d| format!("{}: {d}", call.source_name))
                        .unwrap_or_else(|| call.source_name.clone());
                    out.push(item(
                        format!("{}#tool:{i}", entry.id),
                        ViewKind::ToolUse,
                        &text,
                        160,
                    ));
                }
            }
            Kind::Results { results } => {
                for (i, result) in results.iter().enumerate() {
                    let size = match &result.content {
                        ResultContent::Text(s) => s.chars().count(),
                        ResultContent::Blocks(v) => serde_json::to_string(v)
                            .map(|s| s.chars().count())
                            .unwrap_or(0),
                    };
                    let text = format!(
                        "{} ({size} characters)",
                        if result.is_error { "error" } else { "ok" }
                    );
                    out.push(item(
                        format!("{}#result:{i}", entry.id),
                        ViewKind::ToolOutcome,
                        &text,
                        160,
                    ));
                }
            }
            Kind::Compaction { .. } => out.push(item(
                entry.id.clone(),
                ViewKind::Notice,
                "[the conversation was compacted here]",
                160,
            )),
            Kind::Meta { .. } => {}
        }
    }
    out
}
