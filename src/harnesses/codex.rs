//! Codex CLI: rollouts under `CODEX_HOME/sessions/`.

use super::{Harness, InterchangeDocument, StoreStats};
use crate::hop::HopError;
use crate::model::{CanonicalTool, HarnessId, Opaque, Protocol, Session};
use crate::reader::Reader;
use crate::roots::StoreRoots;
use crate::summary::{self, Reply, SummarizeOpts, SummaryCommand};
use crate::tools::{self, EditCall, Shell, ToolSet, Translation};
use crate::writer::{WriteOpts, Writer};
use serde_json::Value;
use std::path::Path;

pub(super) struct Codex;

impl Harness for Codex {
    fn id(&self) -> HarnessId {
        HarnessId::Codex
    }

    fn reader(&self) -> &'static dyn Reader {
        &crate::reader::codex::Codex
    }

    fn writer(&self) -> Option<&'static dyn Writer> {
        Some(&crate::writer::codex::Codex)
    }

    fn read_file(&self, path: &Path) -> anyhow::Result<Session> {
        crate::reader::codex::read_file(path)
    }

    fn store_root<'a>(&self, roots: &'a StoreRoots) -> Option<&'a Path> {
        roots.codex_home.as_deref()
    }

    fn set_store_root(&self, roots: &mut StoreRoots, root: &Path) {
        roots.codex_home = Some(root.to_path_buf());
    }

    fn apply_store_env(&self, roots: &StoreRoots, command: &mut std::process::Command) {
        let variable = crate::catalog::store_env(self.id());
        super::set_store_env(command, variable, self.store_root(roots));
    }

    fn destination_protocol(&self) -> Option<Protocol> {
        Some(Protocol::OpenaiResponses)
    }

    /// Only `encrypted_content`. A Chat envelope is not kept: only one with a scalar conversion
    /// fits.
    fn payload_has_place(&self, opaque: &Opaque) -> bool {
        crate::policy::scalar_chat_payload(opaque) && opaque.encrypted.is_some()
    }

    fn validate_write(&self, opts: &WriteOpts) -> anyhow::Result<()> {
        super::validate_file_write(self, opts)
    }

    fn interchange_document(&self) -> Option<&'static dyn InterchangeDocument> {
        None
    }

    /// `model_provider` at the top level of `config.toml`. Codex names no model there.
    fn configured_model(&self, roots: &StoreRoots) -> Option<(String, Option<String>)> {
        let home = roots.codex_home.as_deref()?;
        let text = std::fs::read_to_string(home.join("config.toml")).ok()?;
        top_level_string(&text, "model_provider").map(|provider| (provider, None))
    }

    fn store_stats(&self, roots: &StoreRoots) -> StoreStats {
        super::listed_stats(self, roots)
    }

    /// A paginated thread is read whole: its base file, abandoned turns included, then its active
    /// page.
    fn read_all_branches(&self, path: &Path) -> Option<anyhow::Result<Session>> {
        Some(crate::reader::codex::read_file_with(
            path,
            crate::reader::codex::Branches::All,
        ))
    }

    fn read_all_branches_by_id(
        &self,
        id: &str,
        roots: &StoreRoots,
    ) -> Option<anyhow::Result<Session>> {
        Some(crate::reader::codex::read_with(
            id,
            roots,
            crate::reader::codex::Branches::All,
        ))
    }

    // `exec resume --ephemeral` still appends to the rollout it resumes; a fork does not.
    fn summary_command(
        &self,
        session: &Session,
        opts: &SummarizeOpts,
        scratch: &Path,
    ) -> Result<SummaryCommand, HopError> {
        let file = scratch.join("last-message.txt");
        let mut args = vec![
            "exec".into(),
            "fork".into(),
            session.meta.id.clone(),
            "--ephemeral".into(),
            "--skip-git-repo-check".into(),
            "-c".into(),
            "sandbox_mode=\"read-only\"".into(),
            "-o".into(),
            file.display().to_string(),
        ];
        args.extend(summary::model_args(
            "-m",
            opts.model.as_deref(),
            summary::own_model(session),
            |model| vec!["-m".into(), model.id.clone()],
        ));
        summary::finish_command(session, opts, args, Some(file), Vec::new())
    }

    /// The file Codex wrote its last message to.
    fn parse_summary_reply(
        &self,
        _stdout: &str,
        output_file: Option<&str>,
    ) -> Result<Reply, String> {
        Ok(Reply {
            text: output_file.unwrap_or("").to_string(),
            usage: None,
            fork: None,
        })
    }

    fn offered(&self) -> ToolSet {
        ToolSet {
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
        }
    }

    /// The pinned Codex runs commands and nothing else of the file and search tools; it searches
    /// the web server-side. The reason a call would not translate anywhere comes first.
    fn translate(
        &self,
        canonical: &CanonicalTool,
        _source_name: &str,
        input: &Value,
        dest_shell: Option<Shell>,
    ) -> Translation {
        match canonical {
            CanonicalTool::Bash | CanonicalTool::PowerShell => shell(canonical, input, dest_shell),
            CanonicalTool::Read => Translation::Text {
                why: "Codex offers no read tool",
            },
            CanonicalTool::Write => Translation::Text {
                why: "Codex offers no apply_patch here",
            },
            CanonicalTool::Edit | CanonicalTool::MultiEdit => edit(canonical, input),
            CanonicalTool::Grep => match tools::grep_call(input) {
                Ok(_) => Translation::Text {
                    why: "no grep tool offered",
                },
                Err(text) => text,
            },
            CanonicalTool::Glob => match tools::glob_input(input) {
                Ok(_) => Translation::Text {
                    why: "no glob tool offered",
                },
                Err(text) => text,
            },
            CanonicalTool::Ls => Translation::Text {
                why: "no ls tool offered",
            },
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
            CanonicalTool::Bash | CanonicalTool::PowerShell => tools::SHELL_KEYS_WITH_WORKDIR,
            _ => tools::common_keys(canonical),
        }
    }

    /// The writer writes a result's text: its other blocks are dropped and counted.
    fn flattens_result_blocks(&self) -> bool {
        true
    }
}

/// A top-level `key = "value"` of a TOML file: before any `[table]`, the key exactly (not
/// `model_providers.x`), with a trailing `# comment` left out.
fn top_level_string(text: &str, key: &str) -> Option<String> {
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            return None;
        }
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        if name.trim() != key {
            continue;
        }
        let value = value.trim();
        let value = match value.strip_prefix('"') {
            Some(quoted) => quoted.split('"').next().unwrap_or_default(),
            None => value.split('#').next().unwrap_or_default().trim(),
        };
        return Some(value.to_string()).filter(|value| !value.is_empty());
    }
    None
}

// ---------------------------------------------------------------------------------------------
// tools toward Codex

fn shell(canonical: &CanonicalTool, input: &Value, dest_shell: Option<Shell>) -> Translation {
    if let Some(text) = tools::other_dialect(canonical, dest_shell) {
        return text;
    }
    let Some(command) = tools::command_of(input) else {
        return Translation::Text { why: "no command" };
    };
    if tools::in_background(input) {
        // Codex has no background flag; it keeps a session with `write_stdin` instead.
        return Translation::Text {
            why: "background execution is not available",
        };
    }
    let mut pairs: Vec<(&str, Value)> = vec![("cmd", command)];
    if let Some(workdir) = tools::either(input, &["workdir", "cwd"]) {
        pairs.push(("workdir", workdir));
    }
    if let Some(justification) = tools::either(input, &["justification", "description"]) {
        pairs.push(("justification", justification));
    }
    // `timeout` is dropped: Codex has no timeout for exec_command and the recorded result does
    // not depend on it (tables §4.1). `yield_time_ms` is not a timeout and is not emitted.
    Translation::Call {
        name: "exec_command",
        input: tools::obj_pairs(pairs),
    }
}

fn edit(canonical: &CanonicalTool, input: &Value) -> Translation {
    match tools::edit_call(canonical, input) {
        // The patch form (tables §4.3) is not written yet: the pinned Codex of the bench does not
        // offer `apply_patch` at all, and a form no destination has accepted is not worth
        // guessing at. The call travels as text until a fixture proves it.
        Ok(EditCall::One { .. }) => Translation::Text {
            why: "Codex's apply_patch form is not written yet",
        },
        // Codex's patch is one hunk, and one result cannot be split.
        Ok(EditCall::Several { .. }) => Translation::Text {
            why: "no multi-edit equivalent",
        },
        Err(text) => text,
    }
}

#[cfg(test)]
mod tests {
    use super::top_level_string;

    /// Codex's own provider is the top-level key: not a provider table's field, not a profile's,
    /// and not the comment after it.
    #[test]
    fn the_codex_provider_is_the_top_level_key_only() {
        let config = "model = \"gpt\"\nmodel_provider = \"azure\" # the work account\n\n[model_providers.azure]\nname = \"Azure\"\n";
        assert_eq!(
            top_level_string(config, "model_provider").as_deref(),
            Some("azure")
        );
        let nested = "[profiles.fast]\nmodel_provider = \"openai\"\n";
        assert_eq!(top_level_string(nested, "model_provider"), None);
        let dotted = "model_providers.azure.name = \"Azure\"\n";
        assert_eq!(top_level_string(dotted, "model_provider"), None);
    }
}
