//! Codex CLI: rollouts under `CODEX_HOME/sessions/`.

use super::{Destination, Harness, InterchangeDocument, StoreStats};
use crate::hop::HopError;
use crate::model::{CanonicalTool, HarnessId, Opaque, Protocol, Session};
use crate::reader::Reader;
use crate::roots::StoreRoots;
use crate::summary::{self, Reply, SummarizeOpts, SummaryCommand};
use crate::tools::{self, EditCall, Shell, ToolSet, Translation};
use crate::writer::{WriteOpts, Writer, media::Image};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub(super) struct Codex;

impl Harness for Codex {
    fn id(&self) -> HarnessId {
        HarnessId::Codex
    }

    fn reader(&self) -> &'static dyn Reader {
        &crate::reader::codex::Codex
    }

    fn destination(&self) -> Option<&dyn Destination> {
        Some(self)
    }

    fn read_file(&self, path: &Path) -> anyhow::Result<Session> {
        crate::reader::codex::read_file(path)
    }

    fn chat_mcp_target(
        &self,
        vars: &crate::catalog::Vars,
        server: &crate::chat::connect::Server,
    ) -> Option<crate::chat::connect::Target> {
        let folder = |name: &str| {
            std::env::var_os(name)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        };
        let path = folder("CODEX_HOME")
            .or_else(|| vars.home.as_ref().map(|home| home.join(".codex")))?
            .join("config.toml");
        Some(crate::chat::connect::Target {
            path,
            format: crate::chat::connect::Format::Toml,
            tables: vec!["mcp_servers"],
            // A wait may last 300 s: the tool timeout sits above it.
            entry: serde_json::json!({"url": server.endpoint, "http_headers": server.headers(), "tool_timeout_sec": 330}),
        })
    }

    /// Codex reads the same hook output as Claude Code.
    fn chat_hook_output(
        &self,
        event: crate::chat::adapter::Event,
        delivery: &crate::chat::adapter::Delivery,
    ) -> Option<String> {
        crate::chat::adapter::hook_json(event, delivery)
    }

    /// `codex -c …` for one run: the MCP entry and the three hooks are configuration overrides,
    /// and the credential travels in the environment (`env_http_headers`), not on the command
    /// line. Codex on Windows runs a hook with PowerShell, which needs the call operator.
    ///
    /// Codex runs a hook only once the user has trusted it (`/hooks`); trust follows the hook's
    /// content, which is the same for every launch, so it is given once.
    fn chat_launch(
        &self,
        kit: &crate::chat::adapter::Kit,
    ) -> Result<Option<std::process::Command>, crate::bus::ChatError> {
        let exe = crate::exe::resolve(self.id(), None);
        let mut command = std::process::Command::new(crate::exe::spawnable(&exe));
        let mcp = [
            ("mcp_servers.mukade.url".to_string(), toml_text(&kit.server.endpoint)),
            (
                "mcp_servers.mukade.env_http_headers".to_string(),
                "{Authorization=\"MUKADE_CHAT_AUTHORIZATION\",\"X-Mukade-Service-Id\"=\"MUKADE_CHAT_SERVICE_ID\"}".into(),
            ),
            ("mcp_servers.mukade.tool_timeout_sec".to_string(), "330".into()),
        ];
        for (key, value) in mcp
            .into_iter()
            .chain(chat_hooks(kit).into_iter().map(|(k, v, _)| (k, v)))
        {
            command.arg("-c").arg(format!("{key}={value}"));
        }
        command
            .env(
                "MUKADE_CHAT_AUTHORIZATION",
                format!("Bearer {}", kit.server.token),
            )
            .env("MUKADE_CHAT_SERVICE_ID", &kit.server.service_id)
            .args(&kit.args);
        eprintln!(
            "mukade: Codex runs mukade's three hooks only once they are trusted: run \
             `mukade chat trust codex` once (or trust them with /hooks in Codex); until then this \
             instance does not join the room."
        );
        Ok(Some(command))
    }

    /// Trust exactly mukade's three hooks, through Codex's own app-server: `hooks/list` names
    /// each hook with its key and content hash, and `config/value/write` records the hash under
    /// `hooks.state` in the user's `config.toml`, as `/hooks` does. No other hook is touched.
    /// Trust follows content, and the content holds this mukade executable's path.
    fn chat_trust(
        &self,
        kit: &crate::chat::adapter::Kit,
        dry_run: bool,
    ) -> Result<Option<serde_json::Value>, crate::bus::ChatError> {
        use crate::chat::adapter::{Rpc, StdioRpc};
        use serde_json::json;
        let hooks = chat_hooks(kit);
        let exe = crate::exe::resolve(self.id(), None);
        let mut command = std::process::Command::new(crate::exe::spawnable(&exe));
        command.arg("app-server");
        for (key, value, _) in &hooks {
            command.arg("-c").arg(format!("{key}={value}"));
        }
        let mut rpc = StdioRpc::spawn(command)?;
        rpc.call(
            "initialize",
            json!({"clientInfo": {"name": "mukade", "version": env!("CARGO_PKG_VERSION")}}),
        )?;
        rpc.notify("initialized")?;
        let cwd = std::env::current_dir().unwrap_or_default();
        let mut report = trust_hooks(&mut rpc, &hooks, &cwd, dry_run)?;
        report["exe"] = json!(kit.exe);
        Ok(Some(report))
    }

    fn user_origin(&self, raw: Option<&serde_json::Value>) -> crate::reader::origin::Origin {
        use crate::reader::origin::Origin;
        let Some(raw) = raw else {
            return Origin::Unknown;
        };
        {
            let Some(payload) = raw.get("payload") else {
                return Origin::Unknown;
            };
            if payload
                .pointer("/internal_chat_message_metadata_passthrough/content_item_metadata")
                .and_then(|v| v.as_array())
                .is_some_and(|items| {
                    items.iter().any(|item| {
                        item.pointer("/provenance/type").and_then(|v| v.as_str()) == Some("harness")
                    })
                })
            {
                return Origin::Injected("harness_context");
            }
            if payload.get("role").and_then(|v| v.as_str()) == Some("user")
                || payload.get("type").and_then(|v| v.as_str()) == Some("user_message")
            {
                Origin::Typed
            } else {
                Origin::Unknown
            }
        }
    }

    fn store_root<'a>(&self, roots: &'a StoreRoots) -> Option<&'a Path> {
        roots.codex_home.as_deref()
    }

    fn store_root_mut<'a>(&self, roots: &'a mut StoreRoots) -> &'a mut Option<PathBuf> {
        &mut roots.codex_home
    }

    fn apply_store_env(&self, roots: &StoreRoots, command: &mut std::process::Command) {
        let variable = crate::catalog::store_env(self.id());
        super::set_store_env(command, variable, self.store_root(roots));
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

    fn reads_all_branches(&self) -> bool {
        true
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
}

impl Destination for Codex {
    fn writer(&self) -> &'static dyn Writer {
        &crate::writer::codex::Codex
    }

    fn protocol(&self) -> Option<Protocol> {
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

    /// Codex runs PowerShell on Windows (`pwsh`, else `powershell`) and the login shell elsewhere,
    /// a POSIX one (`zsh`, `bash`, `sh`); no configuration key changes it.
    fn default_shell(&self, _roots: &StoreRoots, _cwd: Option<&Path>) -> Option<Shell> {
        Some(platform_shell(cfg!(windows)))
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

    fn image_in_result(&self, image: &Image) -> Value {
        crate::writer::codex::image_block(image)
    }
}

/// The shell Codex runs on Windows (`windows`) or elsewhere.
fn platform_shell(windows: bool) -> Shell {
    if windows {
        Shell::PowerShell
    } else {
        Shell::Posix
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
        return tools::NO_BACKGROUND;
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
        Ok(EditCall::Several { .. }) => tools::NO_MULTI_EDIT,
        Err(text) => text,
    }
}

#[cfg(test)]
mod tests {
    use super::{Shell, platform_shell, top_level_string};

    #[test]
    fn codex_runs_powershell_on_windows_only() {
        assert_eq!(platform_shell(true), Shell::PowerShell);
        assert_eq!(platform_shell(false), Shell::Posix);
    }

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

/// A TOML string, quoted and escaped, for a `-c key=value` override.
fn toml_text(text: &str) -> String {
    toml_edit::Value::from(text).to_string().trim().to_string()
}

/// mukade's three chat hooks as Codex `-c` overrides: (key, TOML value, the command line). They
/// are the same for every launch, so trust given once holds. Codex on Windows runs a hook with
/// PowerShell, which needs the call operator.
fn chat_hooks(kit: &crate::chat::adapter::Kit) -> Vec<(String, String, String)> {
    [
        ("hooks.SessionStart", "session-start", 30),
        ("hooks.UserPromptSubmit", "prompt", 30),
        ("hooks.Stop", "stop", kit.stop_timeout),
    ]
    .into_iter()
    .map(|(key, event, timeout)| {
        let line = if cfg!(windows) {
            kit.powershell_hook_command("codex", event)
        } else {
            kit.hook_command("codex", event)
        };
        let value = format!(
            "[{{hooks=[{{type=\"command\",command={},timeout={timeout}}}]}}]",
            toml_text(&line)
        );
        (key.to_string(), value, line)
    })
    .collect()
}

/// Trust, through Codex's own server, the hooks of `hooks` that Codex lists as session flags.
/// Every one of them must be listed, or nothing is written; one already trusted is left as it is;
/// any other hook, of the user or of a plugin, is neither read nor written.
fn trust_hooks(
    rpc: &mut dyn crate::chat::adapter::Rpc,
    hooks: &[(String, String, String)],
    cwd: &std::path::Path,
    dry_run: bool,
) -> Result<Value, crate::bus::ChatError> {
    use serde_json::json;
    let ours =
        |rpc: &mut dyn crate::chat::adapter::Rpc| -> Result<Vec<Value>, crate::bus::ChatError> {
            let listed = rpc.call("hooks/list", json!({"cwds": [cwd]}))?;
            Ok(listed["data"]
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|entry| entry["hooks"].as_array().cloned().unwrap_or_default())
                .filter(|hook| {
                    hook["source"] == "sessionFlags"
                        && hooks
                            .iter()
                            .any(|(_, _, line)| hook["command"].as_str() == Some(line.as_str()))
                })
                .collect())
        };
    let before = ours(rpc)?;
    if before.len() != hooks.len() {
        return Err(crate::bus::ChatError {
            code: "unsupported".into(),
            message: format!(
                "Codex listed {} of mukade's {} hooks; nothing was trusted",
                before.len(),
                hooks.len()
            ),
            retryable: false,
            ack_batch_id: None,
        });
    }
    let mut wrote = false;
    for hook in &before {
        if dry_run || hook["trustStatus"] == "trusted" {
            continue;
        }
        let key = hook["key"].as_str().unwrap_or_default();
        rpc.call(
            "config/value/write",
            json!({"keyPath": "hooks.state", "mergeStrategy": "upsert",
                   "value": {key: {"trusted_hash": hook["currentHash"]}}}),
        )?;
        wrote = true;
    }
    let after = if wrote { ours(rpc)? } else { before.clone() };
    let report: Vec<Value> = before
        .iter()
        .zip(&after)
        .map(|(was, now)| {
            json!({"event": was["eventName"], "key": was["key"],
                   "before": was["trustStatus"], "after": now["trustStatus"]})
        })
        .collect();
    let trusted =
        after.len() == hooks.len() && after.iter().all(|hook| hook["trustStatus"] == "trusted");
    Ok(json!({
        "status": if dry_run { "dry_run" } else if trusted { "trusted" } else { "not_trusted" },
        "hooks": report,
    }))
}

#[cfg(test)]
mod trust_tests {
    use super::trust_hooks;
    use crate::bus::ChatError;
    use serde_json::{Value, json};

    /// Codex's server as far as trust goes: listed hooks, and the writes it was sent.
    struct FakeCodex {
        hooks: Vec<Value>,
        writes: Vec<Value>,
    }

    impl crate::chat::adapter::Rpc for FakeCodex {
        fn call(&mut self, method: &str, params: Value) -> Result<Value, ChatError> {
            match method {
                "hooks/list" => Ok(json!({"data": [{"hooks": self.hooks.clone()}]})),
                "config/value/write" => {
                    for (key, state) in params["value"].as_object().unwrap() {
                        for hook in &mut self.hooks {
                            if hook["key"] == key.as_str()
                                && hook["currentHash"] == state["trusted_hash"]
                            {
                                hook["trustStatus"] = json!("trusted");
                            }
                        }
                    }
                    self.writes.push(params);
                    Ok(json!({}))
                }
                other => panic!("unexpected call {other}"),
            }
        }
    }

    fn ours() -> Vec<(String, String, String)> {
        ["start", "prompt", "stop"]
            .iter()
            .map(|event| {
                (
                    format!("hooks.{event}"),
                    String::new(),
                    format!("mukade {event}"),
                )
            })
            .collect()
    }

    fn listed(command: &str, source: &str, trust: &str, n: usize) -> Value {
        json!({"key": format!("flags:{n}"), "source": source, "command": command,
               "trustStatus": trust, "currentHash": format!("sha256:{n}"), "eventName": command})
    }

    fn codex(trust: &str) -> FakeCodex {
        let mut hooks: Vec<Value> = ["start", "prompt", "stop"]
            .iter()
            .enumerate()
            .map(|(n, event)| listed(&format!("mukade {event}"), "sessionFlags", trust, n))
            .collect();
        // A user's own hook, untrusted, and a plugin hook with mukade's command line.
        hooks.push(listed("their own", "sessionFlags", "untrusted", 7));
        hooks.push(listed("mukade stop", "plugin", "untrusted", 8));
        FakeCodex {
            hooks,
            writes: Vec::new(),
        }
    }

    #[test]
    fn trusts_each_untrusted_hook_of_mukade_with_its_listed_hash() {
        let mut fake = codex("untrusted");
        let report = trust_hooks(&mut fake, &ours(), std::path::Path::new("."), false).unwrap();
        assert_eq!(report["status"], "trusted", "{report}");
        let keys: Vec<Value> = fake.writes.iter().map(|w| w["value"].clone()).collect();
        assert_eq!(
            keys,
            vec![
                json!({"flags:0": {"trusted_hash": "sha256:0"}}),
                json!({"flags:1": {"trusted_hash": "sha256:1"}}),
                json!({"flags:2": {"trusted_hash": "sha256:2"}}),
            ]
        );
        assert!(
            fake.writes
                .iter()
                .all(|w| w["keyPath"] == "hooks.state" && w["mergeStrategy"] == "upsert")
        );
        assert_eq!(
            fake.hooks[3]["trustStatus"], "untrusted",
            "the user's own hook stays"
        );
        assert_eq!(
            fake.hooks[4]["trustStatus"], "untrusted",
            "a plugin's hook stays"
        );
        assert!(
            report["hooks"]
                .as_array()
                .unwrap()
                .iter()
                .all(|h| h["before"] == "untrusted" && h["after"] == "trusted")
        );
    }

    #[test]
    fn a_dry_run_writes_nothing() {
        let mut fake = codex("untrusted");
        let report = trust_hooks(&mut fake, &ours(), std::path::Path::new("."), true).unwrap();
        assert_eq!(report["status"], "dry_run");
        assert!(fake.writes.is_empty());
    }

    #[test]
    fn hooks_already_trusted_are_not_written_again() {
        let mut fake = codex("trusted");
        let report = trust_hooks(&mut fake, &ours(), std::path::Path::new("."), false).unwrap();
        assert_eq!(report["status"], "trusted");
        assert!(fake.writes.is_empty());
    }

    #[test]
    fn a_hook_that_codex_does_not_list_writes_nothing() {
        let mut fake = codex("untrusted");
        fake.hooks.remove(2);
        let error = trust_hooks(&mut fake, &ours(), std::path::Path::new("."), false).unwrap_err();
        assert_eq!(error.code, "unsupported");
        assert!(fake.writes.is_empty());
    }

    #[test]
    fn a_hash_codex_refuses_reports_not_trusted() {
        let mut fake = codex("modified");
        // Codex keeps the hook as it was: the hash written is not the one it holds.
        for hook in &mut fake.hooks {
            hook["currentHash"] = json!("sha256:other");
        }
        let mut stubborn = Stubborn(fake);
        let report = trust_hooks(&mut stubborn, &ours(), std::path::Path::new("."), false).unwrap();
        assert_eq!(report["status"], "not_trusted", "{report}");
    }

    /// A Codex that accepts the writes but never changes its answer.
    struct Stubborn(FakeCodex);

    impl crate::chat::adapter::Rpc for Stubborn {
        fn call(&mut self, method: &str, params: Value) -> Result<Value, ChatError> {
            if method == "config/value/write" {
                self.0.writes.push(params);
                return Ok(json!({}));
            }
            self.0.call(method, params)
        }
    }
}
