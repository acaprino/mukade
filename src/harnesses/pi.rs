//! Pi: sessions under `PI_CODING_AGENT_DIR/sessions/`.

use super::{Destination, Harness, InterchangeDocument, StoreStats};
use crate::hop::HopError;
use crate::model::{CanonicalTool, HarnessId, Opaque, Protocol, Session};
use crate::reader::Reader;
use crate::roots::StoreRoots;
use crate::summary::{self, Reply, SummarizeOpts, SummaryCommand};
use crate::tools::{self, EditCall, GrepCall, Shell, ToolSet, Translation};
use crate::writer::{WriteOpts, Writer, media::Image};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub(super) struct Pi;

impl Harness for Pi {
    fn id(&self) -> HarnessId {
        HarnessId::Pi
    }

    fn reader(&self) -> &'static dyn Reader {
        &crate::reader::pi::Pi
    }

    fn destination(&self) -> Option<&dyn Destination> {
        Some(self)
    }

    fn read_file(&self, path: &Path) -> anyhow::Result<Session> {
        crate::reader::pi::read_file(path)
    }

    fn store_root<'a>(&self, roots: &'a StoreRoots) -> Option<&'a Path> {
        roots.pi_agent_dir.as_deref()
    }

    fn store_root_mut<'a>(&self, roots: &'a mut StoreRoots) -> &'a mut Option<PathBuf> {
        &mut roots.pi_agent_dir
    }

    fn apply_store_env(&self, roots: &StoreRoots, command: &mut std::process::Command) {
        let variable = crate::catalog::store_env(self.id());
        super::set_store_env(command, variable, self.store_root(roots));
    }

    /// Pi records a fork in the header of the session it forks into.
    fn store_stats(&self, roots: &StoreRoots) -> StoreStats {
        let counted = match roots.pi_agent_dir.as_deref() {
            Some(dir) => {
                crate::reader::pi::store_summary(dir).map_err(|error| format!("{error:#}"))
            }
            None => Err(format!(
                "{} is not set: no Pi store to list",
                crate::catalog::store_env(self.id())
            )),
        };
        match counted {
            Ok(summary) => StoreStats {
                sessions: Ok(summary.sessions),
                forks: Some(summary.forks.map_err(|error| error.to_string())),
            },
            Err(error) => StoreStats {
                sessions: Err(error.clone()),
                forks: Some(Err(error)),
            },
        }
    }

    /// Pi forks a session from a file, so it summarizes one from a file too.
    fn summarizes_from_file(&self) -> bool {
        true
    }

    // `--no-session` cannot be combined with `--fork`: the fork goes to this run's folder.
    fn summary_command(
        &self,
        session: &Session,
        opts: &SummarizeOpts,
        scratch: &Path,
    ) -> Result<SummaryCommand, HopError> {
        let file = session.meta.source_path.clone().ok_or_else(|| {
            HopError::Unsupported(
                "a Pi session is summarized from its own file, and this one was not read from a file"
                    .into(),
            )
        })?;
        let mut args = vec![
            "-p".into(),
            "--fork".into(),
            file,
            "--session-dir".into(),
            scratch.join("sessions").display().to_string(),
            "--no-tools".into(),
        ];
        // The provider goes apart: a model id may hold a slash itself.
        args.extend(summary::model_args(
            "--model",
            opts.model.as_deref(),
            summary::own_model(session),
            |model| match model.provider.as_deref() {
                Some(provider) => vec![
                    "--provider".into(),
                    provider.into(),
                    "--model".into(),
                    model.id.clone(),
                ],
                None => vec!["--model".into(), model.id.clone()],
            },
        ));
        summary::finish_command(session, opts, args, None, Vec::new())
    }

    /// Pi prints the summary on stdout.
    fn parse_summary_reply(
        &self,
        stdout: &str,
        _output_file: Option<&str>,
    ) -> Result<Reply, String> {
        Ok(Reply {
            text: stdout.to_string(),
            usage: None,
            fork: None,
        })
    }
}

impl Destination for Pi {
    fn writer(&self) -> &'static dyn Writer {
        &crate::writer::pi::Pi
    }

    /// Pi speaks whatever its configured provider speaks.
    fn protocol(&self) -> Option<Protocol> {
        None
    }

    /// A `thinkingSignature` is replayed, and an Anthropic encrypted payload as Pi's own
    /// `redacted: true` block. A Chat envelope is not kept: only one with a scalar conversion fits.
    fn payload_has_place(&self, opaque: &Opaque) -> bool {
        crate::policy::scalar_chat_payload(opaque)
            && (opaque.signature.is_some()
                || (opaque.encrypted.is_some() && opaque.protocol == Protocol::AnthropicMessages))
    }

    fn validate_write(&self, opts: &WriteOpts) -> anyhow::Result<()> {
        super::validate_file_write(self, opts)
    }

    fn interchange_document(&self) -> Option<&'static dyn InterchangeDocument> {
        None
    }

    /// Pi's `bash` tool runs `shellPath` when its settings name one, else a POSIX shell (Git Bash
    /// on Windows, `bash` or `sh` elsewhere). A `shellPath` of no known dialect names none.
    fn default_shell(&self, roots: &StoreRoots, cwd: Option<&Path>) -> Option<Shell> {
        match shell_path(roots.pi_agent_dir.as_deref(), cwd) {
            Some(path) => tools::shell_named(&path),
            None => Some(Shell::Posix),
        }
    }

    fn offered(&self) -> ToolSet {
        ToolSet {
            names: &["read", "bash", "edit", "write"],
        }
    }

    /// Pi offers `grep`, `find` and `ls` only when it is told to; a caller that knows it widens the
    /// offered set, and the calls take these shapes.
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
                    name: "find",
                    input,
                },
                Err(text) => text,
            },
            CanonicalTool::Ls => ls(input),
            CanonicalTool::WebFetch => match tools::url_of(input) {
                Ok(_) => Translation::Text {
                    why: "no webfetch tool offered",
                },
                Err(text) => text,
            },
            CanonicalTool::WebSearch => match tools::query_of(input) {
                Ok(_) => Translation::Text {
                    why: "web search is server-side or absent",
                },
                Err(text) => text,
            },
            _ => tools::OTHER_CAPABILITY,
        }
    }

    fn accepted_keys(&self, canonical: &CanonicalTool) -> &'static [&'static str] {
        match canonical {
            CanonicalTool::Glob => &["pattern", "path", "limit"],
            _ => tools::common_keys(canonical),
        }
    }

    /// A step that ended aborted or in error keeps that end as its `stopReason`.
    fn failed_steps_are_replies(&self) -> bool {
        false
    }

    fn image_in_result(&self, image: &Image) -> Value {
        crate::writer::pi::image_block(image)
    }
}

/// The `shellPath` Pi's settings name: `<cwd>/.pi/settings.json` (`cwd` an existing absolute
/// folder) over `<agent dir>/settings.json`, as Pi merges them. A file that does not parse, or has
/// no non-empty string there, names none.
fn shell_path(agent_dir: Option<&Path>, cwd: Option<&Path>) -> Option<String> {
    let project = super::project_dir(cwd).map(|cwd| cwd.join(".pi").join("settings.json"));
    let global = agent_dir.map(|dir| dir.join("settings.json"));
    [project, global].into_iter().flatten().find_map(|file| {
        let text = std::fs::read_to_string(file).ok()?;
        let settings: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
        settings
            .get("shellPath")?
            .as_str()
            .filter(|path| !path.trim().is_empty())
            .map(str::to_string)
    })
}

// ---------------------------------------------------------------------------------------------
// tools toward Pi

/// L12: Pi measures the timeout in **seconds**, the canonical form in milliseconds.
fn shell(canonical: &CanonicalTool, input: &Value, dest_shell: Option<Shell>) -> Translation {
    if let Some(text) = tools::other_dialect(canonical, dest_shell) {
        return text;
    }
    let Some(command) = tools::command_of(input) else {
        return Translation::Text { why: "no command" };
    };
    if tools::in_background(input) {
        return tools::NO_BACKGROUND;
    }
    let mut pairs: Vec<(&str, Value)> = vec![("command", command)];
    if let Some(ms) = tools::arg(input, "timeout").and_then(Value::as_u64) {
        pairs.push(("timeout", Value::from(ms.div_ceil(1000))));
    }
    Translation::Call {
        name: "bash",
        input: tools::obj_pairs(pairs),
    }
}

/// Pi's `edit` holds its changes in `edits`. "All occurrences" has no counterpart in it, so it
/// blocks translation (L13).
fn edit(canonical: &CanonicalTool, input: &Value) -> Translation {
    let (path, old, new, replace_all) = match tools::edit_call(canonical, input) {
        Ok(EditCall::One {
            path,
            old,
            new,
            replace_all,
        }) => (path, old, new, replace_all),
        Ok(EditCall::Several { path, entries }) => return multi_edit(path, entries),
        Err(text) => return text,
    };
    if replace_all {
        return Translation::Text {
            why: "replace_all has no counterpart in Pi",
        };
    }
    let edits = Value::Array(vec![tools::obj([("oldText", old), ("newText", new)])]);
    Translation::Call {
        name: "edit",
        input: tools::obj([("path", path), ("edits", edits)]),
    }
}

/// Several changes in one call, which only Pi's `edit` takes.
fn multi_edit(path: Value, entries: &[Value]) -> Translation {
    if entries.is_empty() {
        return Translation::Text { why: "no edits" };
    }
    let mut mapped = Vec::new();
    for entry in entries {
        if tools::flag(entry, "replace_all") || tools::flag(entry, "replaceAll") {
            return Translation::Text {
                why: "replace_all has no counterpart in Pi",
            };
        }
        let Some((old, new)) = tools::change_of(entry) else {
            return Translation::Text {
                why: "an edit without its old and new text",
            };
        };
        mapped.push(tools::obj([("oldText", old), ("newText", new)]));
    }
    Translation::Call {
        name: "edit",
        input: tools::obj([("path", path), ("edits", Value::Array(mapped))]),
    }
}

fn grep(input: &Value) -> Translation {
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
        input: tools::obj_pairs(pairs),
    }
}

fn ls(input: &Value) -> Translation {
    let mut pairs: Vec<(&str, Value)> = Vec::new();
    for key in ["path", "limit"] {
        if let Some(v) = tools::arg(input, key) {
            pairs.push((key, v.clone()));
        }
    }
    Translation::Call {
        name: "ls",
        input: tools::obj_pairs(pairs),
    }
}

#[cfg(test)]
mod tests {
    use super::shell_path;
    use crate::scratch::Scratch;
    use std::path::Path;

    fn put(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// The project's settings are read only in an absolute folder that exists: a relative one
    /// would be read against mukade's own working folder.
    #[test]
    fn a_relative_project_is_not_read() {
        let project = Scratch::in_working_dir("pi-relative");
        put(
            &project.join(".pi/settings.json"),
            r#"{"shellPath":"pwsh"}"#,
        );
        assert_eq!(shell_path(None, Some(&project)).as_deref(), Some("pwsh"));
        assert_eq!(shell_path(None, Some(&project.relative())), None);
    }

    /// A settings file saved with a byte-order mark is read like any other.
    #[test]
    fn a_settings_file_with_a_byte_order_mark_is_read() {
        let agent = Scratch::new("pi-bom");
        put(
            &agent.join("settings.json"),
            "\u{feff}{\"shellPath\":\"pwsh\"}",
        );
        assert_eq!(shell_path(Some(&agent), None).as_deref(), Some("pwsh"));
    }
}
