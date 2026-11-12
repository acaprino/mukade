//! The Hermes Agent (0.23.1) writer: through Hermes's own import, never by writing `state.db`.
//!
//! `hermes sessions import --from claude <file>` reads a Claude Code session file and has the
//! gateway (the only writer of `state.db`) store it. What that import keeps decides the shape:
//!
//! - user and assistant rows only: text blocks, `tool_use` blocks as `[ran tool: <name>]`, and
//!   nothing of the results (`tool_result` blocks are empty to it). So the session is planned
//!   with no tool offered: every call is text form, its arguments and its result in the text of
//!   the turns, and the Claude rows are written for that plan;
//! - consecutive turns of one role are merged, and a user turn that opens with a context tag
//!   (`<system-reminder>`, `<environment_context>`, ...) is dropped as injected context;
//! - the working directory is kept only when it is a folder here, so a session whose folder is not
//!   one is written with a folder that is nowhere;
//! - Hermes allocates the session id and titles the session itself (`Imported from Claude Code:
//!   <first line>`), and records no model.
//!
//! The Claude file is mukade's own temporary file, removed whatever came of the import.

use super::opencode::{ImportDocument, write_private};
use super::{WriteOpts, Writer, Written, plan};
use crate::model::{HarnessId, Session};
use crate::policy::Clock;
use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

/// How long `hermes sessions import` may take before it is stopped (it starts the gateway).
const IMPORT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(180);

/// The Hermes Agent writer.
#[derive(Debug, Clone, Copy, Default)]
pub struct Hermes;

impl Writer for Hermes {
    fn harness(&self) -> HarnessId {
        HarnessId::Hermes
    }

    fn write_planned(
        &self,
        session: &Session,
        plan: &plan::Plan,
        opts: &WriteOpts,
    ) -> Result<Written> {
        crate::harnesses::destination(HarnessId::Hermes)?.validate_write(opts)?;
        let home = opts
            .roots
            .hermes_home
            .clone()
            .context("HERMES_HOME is not set: nowhere to write the Hermes session")?;
        let state_db = home.join("state.db").to_string_lossy().into_owned();

        let temp = crate::portable::temp_dir()?;
        crate::portable::sweep_stale(
            &temp,
            "mukade-hermes-",
            ".jsonl",
            crate::portable::STALE_AFTER,
        );
        let name = uuid::Uuid::new_v4().simple().to_string();
        // The folder the rows name: Hermes keeps it only when it exists, so a session whose folder
        // is not one here names a folder that is nowhere.
        let cwd = opts
            .cwd
            .clone()
            .or_else(|| session.meta.cwd.clone())
            .filter(|cwd| Path::new(cwd).is_dir())
            .unwrap_or_else(|| {
                temp.join(format!("mukade-no-folder-{name}"))
                    .to_string_lossy()
                    .into_owned()
            });
        let mut clock = Clock::new(opts.now.as_deref());
        let rows = super::claude_code::rows(
            plan,
            session,
            &uuid::Uuid::new_v4().to_string(),
            &cwd,
            opts,
            &mut clock,
        );
        let mut text = String::new();
        // The import reads user and assistant rows only; Hermes titles the session itself.
        for row in rows
            .iter()
            .filter(|row| matches!(row["type"].as_str(), Some("user" | "assistant")))
        {
            text.push_str(&serde_json::to_string(row).context("serialize a record")?);
            text.push('\n');
        }
        let file = temp.join(format!("mukade-hermes-{name}.jsonl"));
        // The guard exists before the file does: a write that fails half-way leaves nothing behind.
        let temporary = ImportDocument(file.clone());
        write_private(&file, text.as_bytes())
            .with_context(|| format!("write {}", file.display()))?;

        let exe = opts
            .exe
            .clone()
            .unwrap_or_else(|| crate::exe::resolve(HarnessId::Hermes, None));
        let mut command = Command::new(&exe);
        command.args(import_args(&file));
        // Hermes is Python: its confirmation line has a check mark, which a pipe's default
        // encoding on Windows cannot print.
        command.env("PYTHONIOENCODING", "utf-8");
        opts.roots.apply_to(HarnessId::Hermes, &mut command);
        let unknown = |reason: String| -> anyhow::Error {
            super::WriteOutcomeUnknown {
                id: "(allocated by Hermes)".into(),
                location: Some(state_db.clone()),
                reason,
            }
            .into()
        };
        let output = crate::exe::run_within(&mut command, IMPORT_TIMEOUT)
            .map_err(|error| unknown(format!("could not complete the import process: {error}")))?;
        let Some(status) = output.status else {
            return Err(unknown(format!(
                "hermes sessions import did not finish within {IMPORT_TIMEOUT:?}"
            )));
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !status.success() {
            return Err(unknown(format!(
                "hermes sessions import failed ({status}): {}",
                stderr.trim()
            )));
        }
        drop(temporary);
        let Some(id) = imported_id(&stdout) else {
            return Err(unknown(format!(
                "hermes did not confirm an import: {} {}",
                stdout.trim(),
                stderr.trim()
            )));
        };
        Ok(Written {
            records: Vec::new(),
            id: id.clone(),
            location: state_db,
            resume: crate::catalog::resume(HarnessId::Hermes, &exe, &id, None),
        })
    }
}

/// The arguments of the import: the file is read as a Claude Code session, and named by its path
/// so the import does not open its picker.
pub fn import_args(file: &Path) -> Vec<String> {
    vec![
        "sessions".into(),
        "import".into(),
        "--from".into(),
        "claude".into(),
        file.display().to_string(),
    ]
}

/// The id Hermes allocated, from its confirmation line: `✓ Imported Claude Code session as <id>`.
pub fn imported_id(stdout: &str) -> Option<String> {
    const MARKER: &str = "Imported Claude Code session as ";
    stdout.lines().find_map(|line| {
        let id = line.split_once(MARKER)?.1.split_whitespace().next()?;
        Some(id.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_confirmation_line_names_the_id() {
        assert_eq!(
            imported_id("✓ Imported Claude Code session as 20261112_094615_fb3768\n  Continue it with:  hermes --resume 20261112_094615_fb3768\n").as_deref(),
            Some("20261112_094615_fb3768")
        );
        // A console that could not print the check mark leaves the rest of the line.
        assert_eq!(
            imported_id("? Imported Claude Code session as local-abc\n").as_deref(),
            Some("local-abc")
        );
        assert_eq!(imported_id("Error: file not found: x\n"), None);
        assert_eq!(imported_id("Imported Codex CLI session as x\n"), None);
        assert_eq!(imported_id("Imported Claude Code session as \n"), None);
    }

    #[test]
    fn the_file_is_named_so_the_import_opens_no_picker() {
        assert_eq!(
            import_args(Path::new("a.jsonl")),
            ["sessions", "import", "--from", "claude", "a.jsonl"]
        );
    }
}
