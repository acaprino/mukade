//! The library's command line: `list`, `dump`, `write` and `hop` as a binary, for scripts, CI
//! and any caller that would rather not write Rust to move a session.
//!
//! ```text
//! mukade list  <harness>
//! mukade dump  <harness> <session-id>            → the document on stdout
//! mukade write <harness> <document.json>         → {"id","location","resume"} on stdout
//! mukade detect <dir>                            → which harness stores the directory holds
//! ```
//!
//! `dump` and `write` are a pair: what `dump` prints is what `write` takes. Store roots come from
//! the harnesses' own variables (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `OPENCODE_DB`,
//! `PI_CODING_AGENT_DIR`), so every run stays inside the scratch home the caller set.

use crate::hop::{
    Compaction, HopError, HopRequest, ImportOpts, KeepFrom, SessionRef, SourceFacts, SummarySource,
    WriteDetails,
};
use crate::model::{HarnessId, Protocol};
use crate::policy::ReasoningPolicy;
use crate::roots::StoreRoots;
use crate::tools::Shell;
use crate::wire;
use crate::writer::WriteOpts;
use anyhow::{Context, Result};
use serde_json::json;

pub const USAGE: &str = "\
mukade list  <harness> [options]
mukade dump  <harness> <session-id> [options]
mukade write <harness> <document.json> [options]
mukade hop   <src-harness> <session-id> <dst-harness> [options]
mukade detect <dir>

harnesses: claude_code | codex | opencode | pi

A `hop` reads the source and writes the destination in one process: set **both** harnesses' store
variables (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `OPENCODE_DB` with its `XDG_*`, `PI_CODING_AGENT_DIR`).
An unset variable means that harness's configured store; Claude Code, Codex and Pi need an explicit root.

`detect` prints which harness stores a directory holds, with the evidence: session records, Pi's
models.json or OpenCode's opencode.db (that file can be passed directly; other SQLite files, e.g.
Codex's state_*.sqlite, are not OpenCode). An empty directory tree infers nothing. With
`--root <dir>` (non-empty) the harness may be omitted from `list`, `dump` and a `hop` source: it is
inferred from the directory's own layout when exactly one harness is recognized. `--root` names the
source store for a hop, the only store otherwise; for OpenCode a directory means its `opencode.db`.

options:
  --cwd <dir>              the project the destination files the session under
  --dest-version <v>       the destination's own version stamp
  --dest-protocol <p>      anthropic-messages | openai-responses | openai-chat
  --dest-shell <s>         bash | powershell — the shell the destination runs commands with
  --model-provider <id>    the destination's provider id
  --model <id>             the destination's model id
  --project-id <id>        the destination's project (OpenCode requires one)
  --reasoning <mode>       drop_foreign (default) | as_text | drop
  --compaction <mode>      none (default) | portable (the source's own summary, then its tail)
  --resume-prompt <text>   resume in the destination and keep its reply in the receipt
  --from-file <path>       read the source from a file instead of its store
  --manifest <path>        also write the manifest there
  --root <dir>             the store directory to use, and to infer the harness from
  --claude-exe <path>      the claude binary to spawn
  --codex-exe <path>       the codex binary to spawn
  --opencode-exe <path>    the opencode binary to spawn
  --pi-exe <path>          the pi binary to spawn

Executable resolution: --<harness>-exe → MUKADE_CLAUDE_EXE / MUKADE_CODEX_EXE /
MUKADE_OPENCODE_EXE / MUKADE_PI_EXE → PATH → the well-known install locations → the plain
command name. A portable install with its own config needs only --root and its --<harness>-exe;
a source OpenCode read spawns the binary its own resolution finds (MUKADE_OPENCODE_EXE wins).
";

/// Parse and run the command line. `args` excludes the program name.
pub fn run(args: &[String]) -> Result<()> {
    let (command, rest) = args.split_first().context(USAGE)?;
    let mut positional: Vec<&str> = Vec::new();
    let mut flags = Flags::default();
    let mut i = 0;
    while i < rest.len() {
        let arg = rest[i].as_str();
        match arg {
            "--cwd" => flags.cwd = Some(next(rest, &mut i)?),
            "--dest-version" => flags.dest_version = Some(next(rest, &mut i)?),
            "--dest-protocol" => flags.dest_protocol = Some(next(rest, &mut i)?),
            "--dest-shell" => flags.dest_shell = Some(next(rest, &mut i)?),
            "--model-provider" => flags.model_provider = Some(next(rest, &mut i)?),
            "--model" => flags.model = Some(next(rest, &mut i)?),
            "--project-id" => flags.project_id = Some(next(rest, &mut i)?),
            "--compaction" => flags.compaction = Some(next(rest, &mut i)?),
            "--resume-prompt" => flags.resume_prompt = Some(next(rest, &mut i)?),
            "--from-file" => flags.from_file = Some(next(rest, &mut i)?),
            "--manifest" => flags.manifest = Some(next(rest, &mut i)?),
            "--reasoning" => flags.reasoning = Some(next(rest, &mut i)?),
            "--root" => flags.root = Some(next(rest, &mut i)?),
            "--claude-exe" => flags.claude_exe = Some(next(rest, &mut i)?),
            "--codex-exe" => flags.codex_exe = Some(next(rest, &mut i)?),
            "--opencode-exe" => flags.opencode_exe = Some(next(rest, &mut i)?),
            "--pi-exe" => flags.pi_exe = Some(next(rest, &mut i)?),
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(());
            }
            other if other.starts_with("--") => anyhow::bail!("unknown option {other}\n\n{USAGE}"),
            other => positional.push(other),
        }
        i += 1;
    }

    if command == "detect" {
        let path = positional.first().context("detect needs a directory")?;
        let found = crate::detect::detect_store(std::path::Path::new(path));
        let candidates: Vec<serde_json::Value> = found
            .iter()
            .map(|d| json!({"harness": d.harness.as_str(), "evidence": d.evidence}))
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({"path": path, "candidates": candidates}))?
        );
        return Ok(());
    }

    // An empty root would turn every store path into a relative one, writing beside the cwd.
    if flags.root.as_deref().is_some_and(|root| root.trim().is_empty()) {
        anyhow::bail!("--root needs a non-empty directory");
    }

    // The harness is explicit when the first positional names one. With --root it may be omitted
    // from list/dump/hop: the store's own layout says which harness it is, when exactly one does.
    // `write` always needs it: a destination store can legitimately be empty.
    let inferred_allowed = flags.root.is_some() && command != "write";
    let explicit = match positional.first() {
        Some(first) => match first.parse::<HarnessId>() {
            Ok(harness) => Some(harness),
            Err(_) if inferred_allowed => None,
            Err(error) => return Err(error),
        },
        None if inferred_allowed => None,
        None => return Err(anyhow::anyhow!(USAGE)),
    };
    let operands: &[&str] = if explicit.is_some() { &positional[1..] } else { &positional };
    let mut roots = StoreRoots::from_env();
    let harness = match explicit {
        Some(harness) => harness,
        None => {
            let root = flags.root.as_deref().expect("inference needs --root");
            let found = crate::detect::detect_store(std::path::Path::new(root));
            match found.len() {
                0 => anyhow::bail!("no harness store recognized in {root}: state the harness explicitly"),
                1 => found[0].harness,
                _ => anyhow::bail!(
                    "{root} holds more than one harness's store ({}): state the harness explicitly",
                    found.iter().map(|d| d.harness.as_str()).collect::<Vec<_>>().join(", ")
                ),
            }
        }
    };
    // The root override concerns the store the command reads; for a hop that is the source, and
    // the destination keeps its own environment.
    if let Some(root) = flags.root.as_deref() {
        apply_root(&mut roots, harness, std::path::Path::new(root));
    }
    match command.as_str() {
        "list" => {
            let reader = crate::reader::for_harness(harness)?;
            for info in reader.list(&roots)? {
                println!(
                    "{}",
                    json!({
                        "id": info.id,
                        "title": info.title,
                        "cwd": info.cwd,
                        "timestamp": info.updated,
                        "location": info.path,
                    })
                );
            }
        }
        "dump" => {
            let id = operands.first().context("dump needs a session id")?;
            let reader = crate::reader::for_harness(harness)?;
            let session = reader.read(id, &roots)?;
            let doc = wire::to_document(&session);
            println!("{}", serde_json::to_string_pretty(&doc)?);
        }
        "write" => {
            let path = operands.first().context("write needs a document file")?;
            let text = std::fs::read_to_string(path).with_context(|| format!("read {path}"))?;
            let doc: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))
                .with_context(|| format!("parse {path}"))?;
            let session = wire::from_document(&doc)?;
            let opts = flags.import_opts(harness, harness, &session, &roots)?;
            let receipt =
                crate::hop::import(harness, &session, &opts).map_err(anyhow::Error::new)?;
            if let Some(path) = flags.manifest.as_deref() {
                std::fs::write(path, serde_json::to_string_pretty(&receipt.manifest)?)
                    .with_context(|| format!("write {path}"))?;
            }
            println!(
                "{}",
                json!({
                    "id": receipt.dest_id,
                    "location": receipt.location,
                    "resume": receipt.resume_command,
                    "manifest": receipt.manifest,
                    "resume_output": receipt.resume_output,
                })
            );
        }
        "hop" => {
            let dest: HarnessId = operands
                .get(1)
                .context("hop needs a destination harness")?
                .parse()?;
            let source = match flags.from_file.as_deref() {
                Some(path) => crate::hop::export_path(harness, std::path::Path::new(path))
                    .map_err(anyhow::Error::new)?,
                None => {
                    let id = operands.first().context("hop needs a session id")?;
                    let reader = crate::reader::for_harness(harness)?;
                    reader
                        .read(id, &roots)
                        .map_err(|error| HopError::NotFound(format!("{error:#}")))?
                }
            };
            let opts = flags.import_opts(harness, dest, &source, &roots)?;
            let selection = match flags.compaction.as_deref() {
                None | Some("none") => Compaction::None,
                Some("portable") => Compaction::Portable {
                    summary: SummarySource::SourceRecord,
                    keep_from: KeepFrom::SourceBoundary,
                },
                Some(other) => anyhow::bail!("unknown --compaction {other} (none, portable)"),
            };
            let request = HopRequest {
                source: SessionRef {
                    harness,
                    id: operands.first().copied().unwrap_or_default().to_string(),
                },
                dest,
                cwd: opts.cwd.clone(),
                compaction: selection.clone(),
                resume: flags
                    .resume_prompt
                    .clone()
                    .map(crate::hop::ResumePrompt::new),
                reasoning: opts.reasoning,
                roots: roots.clone(),
                write: opts.write.clone(),
            };
            let receipt = match flags.from_file.as_deref() {
                // A session that came from a file is already in hand: only the write is left.
                Some(_) => {
                    let import_opts = crate::hop::ImportOpts {
                        source_facts: SourceFacts::of(&source),
                        ..opts.clone()
                    };
                    crate::hop::hop_session(dest, &source, &selection, &import_opts)
                }
                None => crate::hop::hop(&request),
            }
            .map_err(anyhow::Error::new)?;
            if let Some(path) = flags.manifest.as_deref() {
                std::fs::write(path, serde_json::to_string_pretty(&receipt.manifest)?)
                    .with_context(|| format!("write {path}"))?;
            }
            println!(
                "{}",
                json!({
                    "id": receipt.dest_id,
                    "location": receipt.location,
                    "resume": receipt.resume_command,
                    "manifest": receipt.manifest,
                    "resume_output": receipt.resume_output,
                })
            );
        }
        other => anyhow::bail!("unknown command {other}\n\n{USAGE}"),
    }
    Ok(())
}

fn next(rest: &[String], i: &mut usize) -> Result<String> {
    *i += 1;
    rest.get(*i)
        .cloned()
        .with_context(|| format!("{} needs a value", rest[*i - 1]))
}

#[derive(Default)]
struct Flags {
    cwd: Option<String>,
    dest_version: Option<String>,
    dest_protocol: Option<String>,
    dest_shell: Option<String>,
    model_provider: Option<String>,
    model: Option<String>,
    project_id: Option<String>,
    reasoning: Option<String>,
    compaction: Option<String>,
    resume_prompt: Option<String>,
    from_file: Option<String>,
    manifest: Option<String>,
    root: Option<String>,
    claude_exe: Option<String>,
    codex_exe: Option<String>,
    opencode_exe: Option<String>,
    pi_exe: Option<String>,
}

impl Flags {
    /// Everything an import needs, from the flags plus the destination's own configuration.
    fn import_opts(
        &self,
        source_harness: HarnessId,
        dest: HarnessId,
        session: &crate::model::Session,
        roots: &StoreRoots,
    ) -> Result<ImportOpts> {
        let write = self.write_opts(dest, session)?;
        Ok(ImportOpts {
            cwd: self.cwd.clone().or_else(|| session.meta.cwd.clone()),
            roots: roots.clone(),
            reasoning: write.reasoning,
            source_facts: SourceFacts {
                harness: source_harness.as_str().to_string(),
                version: Some(session.meta.harness_version.clone()).filter(|v| !v.is_empty()),
                session_id: session.meta.id.clone(),
                content_sha256: crate::sha256::records_sha256(
                    &session
                        .entries
                        .iter()
                        .flat_map(|entry| entry.raw.clone())
                        .collect::<Vec<_>>(),
                ),
            },
            write: WriteDetails {
                dest_version: write.dest_version,
                dest_protocol: write.dest_protocol,
                dest_shell: write.dest_shell,
                offered: write.offered,
                model_provider: write.model_provider,
                model: write.model,
                project_id: write.project_id,
                now: write.now,
                exe: write.exe,
            },
            resume: self
                .resume_prompt
                .clone()
                .map(crate::hop::ResumePrompt::new),
            compaction: self.compaction.clone().unwrap_or_else(|| "none".into()),
        })
    }

    fn write_opts(&self, harness: HarnessId, session: &crate::model::Session) -> Result<WriteOpts> {
        let reasoning = match self.reasoning.as_deref() {
            None | Some("drop_foreign") => ReasoningPolicy::DropForeign,
            Some("as_text") => ReasoningPolicy::AsText,
            Some("drop") => ReasoningPolicy::Drop,
            Some(other) => {
                anyhow::bail!("unknown --reasoning {other} (drop_foreign, as_text, drop)")
            }
        };
        let dest_protocol = match self.dest_protocol.as_deref() {
            Some(name) => Some(Protocol::from_provider(name)),
            None => crate::policy::destination_protocol(harness),
        };
        let dest_shell = match self.dest_shell.as_deref() {
            Some(name) => match name.to_ascii_lowercase().as_str() {
                "bash" | "sh" | "posix" => Some(Shell::Posix),
                "powershell" | "pwsh" => Some(Shell::PowerShell),
                other => anyhow::bail!("unknown --dest-shell {other} (bash, powershell)"),
            },
            None => None,
        };
        // What the *destination* is configured to use, when the caller did not state it. OpenCode's
        // interchange format refuses a model reference without a provider, and Codex stamps the
        // provider its own config names, so both are read from the destination's own files.
        let configured = match harness {
            HarnessId::OpenCode => opencode_config_model(),
            HarnessId::Codex => codex_config_provider().map(|provider| (provider, None)),
            _ => None,
        };
        let model_provider = self
            .model_provider
            .clone()
            .or_else(|| configured.as_ref().map(|(provider, _)| provider.clone()));
        let model = self
            .model
            .clone()
            .or_else(|| configured.as_ref().and_then(|(_, model)| model.clone()));
        // The binary to spawn. An explicit override (the harness's flag, or its MUKADE_*_EXE)
        // travels with the write so the resume command names it too; probes resolve the whole
        // chain — PATH, then the well-known install locations — so a default install is found
        // with no flag at all. For OpenCode an override also avoids the npm shim some Windows
        // installs put on PATH, which would append a second `--standalone`.
        let exe_override = self
            .exe_flag(harness)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
            .or_else(|| std::env::var(crate::exe::env_var_name(harness)).ok().filter(|v| !v.is_empty()));
        let resolved_exe = crate::exe::resolve(harness, self.exe_flag(harness));
        // The destination's own version stamp, when the caller did not state it: L7 wants the
        // destination's `cli_version`, and Codex refuses a rollout without one.
        let dest_version = self
            .dest_version
            .clone()
            .or_else(|| destination_version(&resolved_exe));
        let cwd = self.cwd.clone().or_else(|| session.meta.cwd.clone());
        // OpenCode's interchange format requires the destination's own project id; the destination
        // is the only one that knows it, so it is asked.
        let project_id = match self.project_id.clone() {
            Some(id) => Some(id),
            None if harness == HarnessId::OpenCode => cwd
                .as_deref()
                .and_then(|cwd| opencode_project_id(&resolved_exe, cwd)),
            None => None,
        };
        Ok(WriteOpts {
            roots: StoreRoots::from_env(),
            cwd,
            dest_version,
            dest_protocol,
            dest_shell,
            offered: None,
            model_provider,
            model,
            project_id,
            reasoning,
            title: session.meta.title.clone(),
            now: None,
            exe: exe_override,
        })
    }

    /// The executable override stated for one harness, if any.
    fn exe_flag(&self, harness: HarnessId) -> Option<&str> {
        match harness {
            HarnessId::ClaudeCode => self.claude_exe.as_deref(),
            HarnessId::Codex => self.codex_exe.as_deref(),
            HarnessId::OpenCode => self.opencode_exe.as_deref(),
            HarnessId::Pi => self.pi_exe.as_deref(),
        }
    }
}

/// Point one harness's root at an explicit directory, e.g. a portable install's own home.
/// OpenCode's root is the database file: a directory means its `opencode.db`.
fn apply_root(roots: &mut StoreRoots, harness: HarnessId, root: &std::path::Path) {
    let root = root.to_path_buf();
    match harness {
        HarnessId::ClaudeCode => roots.claude_config_dir = Some(root),
        HarnessId::Codex => roots.codex_home = Some(root),
        HarnessId::OpenCode => {
            roots.opencode_db = Some(if root.is_dir() { root.join("opencode.db") } else { root })
        }
        HarnessId::Pi => roots.pi_agent_dir = Some(root),
    }
}

/// The destination's own version, asked of the destination's binary (already resolved by
/// `crate::exe`). No model is involved: this is `--version`, and a failure simply leaves the
/// version unknown.
fn destination_version(exe: &str) -> Option<String> {
    let cwd = std::env::current_dir().ok()?;
    let text = run_text(exe, &["--version"], &cwd)?;
    first_version(&text)
}

/// The first `x.y.z` token of a line like `codex-cli 0.164.0` or `2.1.4`.
fn first_version(text: &str) -> Option<String> {
    text.split_whitespace().find_map(|token| {
        let token = token.trim_start_matches('v');
        let parts: Vec<&str> = token.split('.').collect();
        let numeric = parts.len() >= 2
            && parts
                .iter()
                .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()));
        numeric.then(|| token.to_string())
    })
}

/// The project the destination already files sessions of this directory under. The destination is
/// asked from **inside that directory**: the CLI scopes its session list to the project it is run
/// in, so the question has to be asked where the session will live.
fn opencode_project_id(exe: &str, cwd: &str) -> Option<String> {
    let directory = std::path::Path::new(cwd);
    let list = run_text(exe, &["session", "list", "--standalone"], directory)?;
    for id in list
        .lines()
        .filter_map(|line| line.split('\t').next())
        .filter(|id| id.starts_with("ses_"))
        .take(5)
    {
        let Some(doc) = run_json(exe, &["session", "export", id, "--standalone"], directory) else {
            continue;
        };
        let found = doc
            .pointer("/info/location/directory")
            .and_then(serde_json::Value::as_str);
        if found.is_some_and(|found| same_directory(found, cwd)) {
            return doc
                .pointer("/info/projectID")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned);
        }
    }
    None
}

fn same_directory(a: &str, b: &str) -> bool {
    let normalise = |s: &str| {
        s.replace('\\', "/")
            .trim_end_matches('/')
            .to_ascii_lowercase()
    };
    normalise(a) == normalise(b)
}

fn run_text(exe: &str, args: &[&str], cwd: &std::path::Path) -> Option<String> {
    let output = std::process::Command::new(exe)
        .args(args)
        .current_dir(cwd)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

fn run_json(exe: &str, args: &[&str], cwd: &std::path::Path) -> Option<serde_json::Value> {
    let output = std::process::Command::new(exe)
        .args(args)
        .current_dir(cwd)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    serde_json::from_slice(&output.stdout).ok()
}

/// OpenCode's own config names its model as `<provider>/<model>`; that is where the destination's
/// provider id comes from when the caller does not state one.
fn opencode_config_model() -> Option<(String, Option<String>)> {
    let path = std::env::var_os("OPENCODE_CONFIG")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("XDG_CONFIG_HOME").map(|d| {
                std::path::PathBuf::from(d)
                    .join("opencode")
                    .join("opencode.json")
            })
        })?;
    let doc: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    let (provider, model) = doc.get("model")?.as_str()?.split_once('/')?;
    Some((provider.to_string(), Some(model.to_string())))
}

/// Codex's `config.toml` states `model_provider = "<id>"`.
fn codex_config_provider() -> Option<String> {
    let home = std::env::var_os("CODEX_HOME").map(std::path::PathBuf::from)?;
    let text = std::fs::read_to_string(home.join("config.toml")).ok()?;
    text.lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("model_provider"))
        .and_then(|rest| rest.split_once('='))
        .map(|(_, value)| value.trim().trim_matches('"').to_string())
        .filter(|provider| !provider.is_empty())
}
