//! The library's command line: `list`, `dump`, `write` and `hop` as a binary, for scripts, CI
//! and any caller that would rather not write Rust to move a session.
//!
//! ```text
//! mukade list  <harness>
//! mukade dump  <harness> <session-id>            â†’ the document on stdout
//! mukade write <harness> <document.json>         â†’ {"id","location","resume"} on stdout
//! mukade detect <dir>                            â†’ which harness stores the directory holds
//! ```
//!
//! `dump` and `write` are a pair: what `dump` prints is what `write` takes. Store roots come from
//! the harnesses' own variables (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `OPENCODE_DB`,
//! `PI_CODING_AGENT_DIR`), so every run stays inside the scratch home the caller set.

use crate::hop::{
    Compaction, HopError, ImportOpts, KeepFrom, Receipt, SessionRef, SourceFacts, SummarySource,
    WriteDetails,
};
use crate::model::{HarnessId, Protocol};
use crate::policy::ReasoningPolicy;
use crate::roots::StoreRoots;
use crate::summary::SummarizeOpts;
use crate::wire;
use crate::writer::WriteOpts;
use anyhow::{Context, Result};
use serde_json::json;

pub const USAGE: &str = "\
mukade harnesses
mukade list  <harness> [options]
mukade intents
mukade dump  <harness> <session-id> [options]
mukade document opencode <src-harness> <session-id> [options]
mukade write <harness> <document.json> [options]
mukade hop   <src-harness> <session-id> <dst-harness> [options]
mukade detect <dir>

`harnesses` lists every supported harness with the executable and store mukade finds on this
machine, its version, session/fork counts and lineage support. Store reads are read-only; discovered
executables are asked for `--version`.

`intents` lists the editable summary prompts (name, path, builtin/custom) as JSON, without reading a
session or starting a harness.

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
  --dest-shell <s>         bash | sh | zsh | powershell | pwsh â€” the destination's shell
  --model-provider <id>    the destination's provider id
  --model <id>             the destination's model id
  --project-id <id>        the destination's project (OpenCode requires one)
  --dest-id <id>           a reserved destination session ID
  --dest-path <path>       a reserved file for a file-based destination
  --dest-agent <name>      the destination's primary agent (OpenCode only)
  --metadata-file <path>   a JSON object to store as session metadata (OpenCode only)
  --tool-map <path>        explicit source tool bindings from the destination catalog
  --without-tools          omit historical tool calls and results (hop and write)
  --expect-sha256 <hash>   refuse a hop when the captured source fingerprint changed
  --reasoning <mode>       drop_foreign (default) | as_text | drop
  --compaction <mode>      none (default) | portable (the source's own summary, then its tail) |
                           summary (the source's model writes one now; dump and hop)
  --keep-turns <n>         summary: also keep the last n user turns verbatim (default 0)
  --intent <name>          summary: the prompt in intent-prompts/<name>.md (default: generic);
                           it implies --compaction summary
  --summary-model <id>     summary: the model to ask (default: the session's own)
  --summary-timeout <s>    summary: deadline of the summarizing run (default 300)
  --resume-prompt <text>   resume in the destination and keep its reply in the receipt
  --from-file <path>       read the source from a file instead of its store
  --all-branches           dump, hop: carry every branch the source keeps (a Codex thread whose
                           later turns were redone: the abandoned turns too); default: only the
                           branch the harness resumes
  --manifest <path>        also write the manifest there
  --output <path>          dump: write the document to this file (UTF-8) instead of stdout
  --timing                 print where the time went, phase by phase, on stderr
  --root <dir>             the store directory to use, and to infer the harness from
  --claude-exe <path>      the claude binary to spawn
  --codex-exe <path>       the codex binary to spawn
  --opencode-exe <path>    the opencode binary to spawn
  --pi-exe <path>          the pi binary to spawn

Executable resolution: --<harness>-exe â†’ MUKADE_CLAUDE_EXE / MUKADE_CODEX_EXE /
MUKADE_OPENCODE_EXE / MUKADE_PI_EXE â†’ PATH â†’ the well-known install locations â†’ the plain
command name. --root selects the
source store of a hop; a portable destination needs its store variable set in the environment
and its --<harness>-exe. A source OpenCode read spawns the binary its own resolution finds
(MUKADE_OPENCODE_EXE wins).
";

/// Parse and run the command line. `args` excludes the program name.
pub fn run(args: &[String]) -> Result<()> {
    let (command, rest) = args.split_first().context(USAGE)?;
    if command == "--identity" {
        anyhow::ensure!(rest.is_empty(), "--identity takes no arguments");
        println!("{}", json!({ "name":"mukade", "version":env!("CARGO_PKG_VERSION"),
            "commit":option_env!("MUKADE_SOURCE_COMMIT"), "target": if cfg!(all(windows, target_arch="x86_64", target_env="msvc")) { "x86_64-pc-windows-msvc" } else { "other" } }));
        return Ok(());
    }
    if matches!(command.as_str(), "-h" | "--help" | "help") {
        print!("{USAGE}");
        return Ok(());
    }
    if std::env::var_os("MUKADE_DATA_DIR")
        .is_some_and(|path| !std::path::Path::new(&path).is_absolute())
    {
        anyhow::bail!("MUKADE_DATA_DIR must be absolute");
    }
    // The default intent prompts live beside the executable and are editable: they are copied
    // once, and a file already there is never replaced.
    if let Err(error) = crate::intent::install_defaults() {
        eprintln!("mukade: {error}");
    }
    let mut positional: Vec<&str> = Vec::new();
    let mut flags = Flags::default();
    let mut i = 0;
    while i < rest.len() {
        let arg = rest[i].as_str();
        match arg {
            "--cwd" => {
                // The destination files the session by its directory: a relative one is resolved
                // against where the command runs.
                let cwd = next(rest, &mut i)?;
                flags.cwd = Some(
                    std::path::absolute(&cwd)
                        .map(|path| path.to_string_lossy().into_owned())
                        .unwrap_or(cwd),
                );
            }
            "--dest-version" => flags.dest_version = Some(next(rest, &mut i)?),
            "--dest-protocol" => flags.dest_protocol = Some(next(rest, &mut i)?),
            "--dest-shell" => flags.dest_shell = Some(next(rest, &mut i)?),
            "--model-provider" => flags.model_provider = Some(next(rest, &mut i)?),
            "--model" => flags.model = Some(next(rest, &mut i)?),
            "--project-id" => flags.project_id = Some(next(rest, &mut i)?),
            "--dest-id" => flags.dest_id = Some(next(rest, &mut i)?),
            "--dest-path" => flags.dest_path = Some(next(rest, &mut i)?),
            "--dest-agent" => flags.dest_agent = Some(next(rest, &mut i)?),
            "--metadata-file" => flags.metadata_file = Some(next(rest, &mut i)?),
            "--tool-map" => flags.tool_map = Some(next(rest, &mut i)?),
            "--without-tools" => flags.without_tools = true,
            "--expect-sha256" => flags.expect_sha256 = Some(next(rest, &mut i)?),
            "--compaction" => flags.compaction = Some(next(rest, &mut i)?),
            "--keep-turns" => flags.keep_turns = Some(next(rest, &mut i)?),
            "--intent" => flags.intent = Some(next(rest, &mut i)?),
            "--summary-model" => flags.summary_model = Some(next(rest, &mut i)?),
            "--summary-timeout" => flags.summary_timeout = Some(next(rest, &mut i)?),
            "--resume-prompt" => flags.resume_prompt = Some(next(rest, &mut i)?),
            "--from-file" => flags.from_file = Some(next(rest, &mut i)?),
            "--all-branches" => flags.all_branches = true,
            "--manifest" => flags.manifest = Some(next(rest, &mut i)?),
            "--output" => flags.output = Some(next(rest, &mut i)?),
            "--timing" => crate::timing::enable(),
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

    if flags.without_tools && !matches!(command.as_str(), "hop" | "write" | "document") {
        anyhow::bail!("--without-tools is supported only by hop, write and document");
    }
    if flags.intent.is_some() && !matches!(command.as_str(), "dump" | "hop") {
        anyhow::bail!("--intent is supported only by dump and hop");
    }
    if flags.intent.is_some()
        && flags
            .compaction
            .as_deref()
            .is_some_and(|mode| mode != "summary")
    {
        anyhow::bail!(
            "--intent goes with --compaction summary; it cannot be combined with another compaction"
        );
    }
    // An intent is a summary prompt: naming one selects the summary transport by itself.
    let summary = flags.compaction.as_deref() == Some("summary") || flags.intent.is_some();
    // Compaction is chosen on the source, before a document exists: `write` imports a document
    // `dump` already compacted, so a compaction flag there would be silently ignored.
    if (flags.compaction.is_some()
        || flags.keep_turns.is_some()
        || flags.summary_model.is_some()
        || flags.summary_timeout.is_some())
        && !matches!(command.as_str(), "dump" | "hop")
    {
        anyhow::bail!(
            "--compaction, --keep-turns, --summary-model and --summary-timeout are supported only by dump and hop"
        );
    }
    if !summary
        && (flags.keep_turns.is_some()
            || flags.summary_model.is_some()
            || flags.summary_timeout.is_some())
    {
        anyhow::bail!(
            "--keep-turns, --summary-model and --summary-timeout go with --compaction summary or --intent"
        );
    }
    if let Some(hash) = flags.expect_sha256.as_deref() {
        anyhow::ensure!(command == "hop", "--expect-sha256 is supported only by hop");
        anyhow::ensure!(
            hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "--expect-sha256 must be a 64-digit SHA-256"
        );
    }
    if flags.tool_map.is_some() && !matches!(command.as_str(), "hop" | "write" | "document") {
        anyhow::bail!("--tool-map is supported only by hop, write and document");
    }
    if (flags.dest_id.is_some() || flags.dest_path.is_some() || flags.dest_agent.is_some() || flags.metadata_file.is_some())
        && !matches!(command.as_str(), "hop" | "write" | "document")
    {
        anyhow::bail!(
            "--dest-id, --dest-path, --dest-agent and --metadata-file are supported only by hop, write and document"
        );
    }
    if let Some(id) = flags.dest_id.as_deref() {
        crate::writer::validate_id_syntax(id)?;
    }
    if let Some(agent) = flags.dest_agent.as_deref() {
        crate::writer::opencode::validate_agent(agent)?;
    }
    if command == "harnesses" {
        no_more(&positional, 0)?;
        anyhow::ensure!(flags == Flags::default(), "harnesses takes no options");
        print_harnesses()?;
        return Ok(());
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
    if command == "intents" {
        no_more(&positional, 0)?;
        anyhow::ensure!(flags == Flags::default(), "intents takes no options");
        let directory = crate::intent::prompt_dir().map_err(anyhow::Error::msg)?;
        let prompts: Vec<serde_json::Value> = crate::intent::list()
            .map_err(anyhow::Error::msg)?
            .into_iter()
            .map(|entry| {
                json!({
                    "name": entry.name,
                    "path": entry.path.display().to_string(),
                    "builtin": entry.builtin,
                    "custom": entry.custom,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "directory": directory.display().to_string(),
                "default": crate::intent::DEFAULT_INTENT,
                "prompts": prompts,
            }))?
        );
        return Ok(());
    }

    // An empty root would turn every store path into a relative one, writing beside the cwd.
    if flags
        .root
        .as_deref()
        .is_some_and(|root| root.trim().is_empty())
    {
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
    let operands: &[&str] = if explicit.is_some() {
        &positional[1..]
    } else {
        &positional
    };
    let mut roots = StoreRoots::from_env();
    // A hop's --root selects only the source, including when both ends use the same harness.
    let destination_roots = roots.clone();
    let harness = match explicit {
        Some(harness) => harness,
        None => {
            let root = flags.root.as_deref().expect("inference needs --root");
            let found = crate::detect::detect_store(std::path::Path::new(root));
            match found.len() {
                0 => anyhow::bail!(
                    "no harness store recognized in {root}: state the harness explicitly"
                ),
                1 => found[0].harness,
                _ => anyhow::bail!(
                    "{root} holds more than one harness's store ({}): state the harness explicitly",
                    found
                        .iter()
                        .map(|d| d.harness.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
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
            no_more(operands, 0)?;
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
                        "archived": info.archived,
                    })
                );
            }
        }
        "dump" => {
            if flags.from_file.is_some() {
                no_more(operands, 0)?;
            } else {
                no_more(operands, 1)?;
            }
            let compaction = flags.compaction(harness)?;
            let session = match (flags.from_file.as_deref(), flags.all_branches) {
                (Some(path), false) => {
                    crate::hop::export_path(harness, std::path::Path::new(path))
                }
                (Some(path), true) => {
                    crate::hop::export_path_all_branches(harness, std::path::Path::new(path))
                }
                (None, all) => {
                    let id = operands.first().context("dump needs a session id")?;
                    let source = SessionRef::id(harness, *id);
                    if all {
                        crate::hop::export_all_branches(&source, &roots)
                    } else {
                        crate::hop::export(&source, &roots)
                    }
                }
            }
            .map_err(anyhow::Error::new)?;
            // What `dump` exports is what `hop` would write: `write` imports it later.
            let (selection, summary) =
                crate::hop::compact(&session, &compaction, &roots).map_err(anyhow::Error::new)?;
            let mut doc = wire::to_document(&selection);
            // Who wrote the summary travels with it, for the manifest `write` makes later.
            if let Some(facts) = &summary {
                if let Some(command) = &facts.summary.leftover {
                    eprintln!("the summarizing run left a fork in the source store: run {command}");
                }
                doc["summary"] = serde_json::to_value(facts)?;
            }
            let doc = serde_json::to_string_pretty(&doc)?;
            match flags.output.as_deref() {
                // Written as UTF-8 by the program itself: a shell pipe re-encodes the text with
                // the console's code page.
                Some(path) => std::fs::write(path, doc).with_context(|| format!("write {path}"))?,
                None => println!("{doc}"),
            }
        }
        "write" => {
            no_more(operands, 1)?;
            let path = operands.first().context("write needs a document file")?;
            let text = std::fs::read_to_string(path).with_context(|| format!("read {path}"))?;
            let doc: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))
                .with_context(|| format!("parse {path}"))?;
            let session = wire::from_document(&doc)?;
            let mut opts = flags.import_opts(session.meta.harness, harness, &session, &roots)?;
            // A document `dump --compaction summary` made says who wrote its summary.
            if let Some(facts) = doc
                .get("summary")
                .and_then(|facts| serde_json::from_value(facts.clone()).ok())
            {
                opts.summary = Some(facts);
                opts.compaction = "summary".into();
            }
            // A document holds no source records: its fingerprint is that of its own messages.
            opts.source_facts.content_sha256 = crate::sha256::records_sha256(
                doc.get("messages")
                    .and_then(serde_json::Value::as_array)
                    .map(|messages| messages.iter())
                    .into_iter()
                    .flatten(),
            );
            deliver(
                crate::hop::import(harness, &session, &opts),
                flags.manifest.as_deref(),
            )?;
        }
        "document" => {
            no_more(operands, 2)?;
            let source_harness: HarnessId = operands.first().context("document needs a source harness")?.parse()?;
            let dest = harness;
            if dest != HarnessId::OpenCode {
                anyhow::bail!("document currently supports only an OpenCode destination");
            }
            let source = match (flags.from_file.as_deref(), flags.all_branches) {
                (Some(path), false) => crate::hop::export_path(source_harness, std::path::Path::new(path)),
                (Some(path), true) => crate::hop::export_path_all_branches(source_harness, std::path::Path::new(path)),
                (None, all) => {
                    let id = operands.get(1).context("document needs a session id")?;
                    let source = SessionRef::id(source_harness, *id);
                    if all { crate::hop::export_all_branches(&source, &roots) } else { crate::hop::export(&source, &roots) }
                }
            }.map_err(anyhow::Error::new)?;
            anyhow::ensure!(source.conversation().any(|entry| matches!(entry.kind, crate::model::Kind::Step { .. })), "noCompleteTurn");
            let mut opts = flags.write_opts(dest, &source, &destination_roots)?;
            if source_harness == HarnessId::OpenCode {
                for entry in source.conversation() {
                    if let crate::model::Kind::Step { calls, .. } = &entry.kind {
                        for call in calls {
                            if crate::tools::offered(dest).contains(&call.source_name) && call.input.is_object() {
                                opts.tool_map.push((call.source_name.clone(), call.source_name.clone(), call.input.clone(), call.input.clone()));
                            }
                        }
                    }
                }
            }
            let mut clock = crate::policy::Clock::new(opts.now.as_deref());
            let id = opts.dest_id.as_deref().unwrap_or("ses_preview");
            crate::writer::opencode::validate_session_id(id)?;
            let plan = crate::writer::plan::plan(&source, dest, &opts);
            let violations = crate::strict::check_plan(&plan, opts.dest_protocol.as_ref());
            anyhow::ensure!(violations.is_empty(), "the document would violate the provider contract: {}", violations.join("; "));
            let document = crate::writer::opencode::document_from_plan(&source, &plan, id, opts.cwd.as_deref().context("a working directory is required")?, &opts, &mut clock)?;
            println!("{}", json!({
                "document": document,
                "turns": source.conversation().filter(|entry| matches!(entry.kind, crate::model::Kind::User { .. })).count(),
                "interrupted_last_turn": source.interrupted_last_turn(),
                "counts": {
                    "messages_in": plan.counts.messages_in,
                    "messages_out": plan.counts.messages_out,
                    "tools_translated": plan.counts.tools_translated,
                    "tools_text_form": plan.counts.tools_text_form,
                    "tools_omitted": plan.counts.tools_omitted,
                },
                "losses": crate::manifest::Manifest::losses_for(&plan.counts, source.interrupted_last_turn(), false),
            }));
        }
        "hop" => {
            no_more(operands, 2)?;
            let dest: HarnessId = operands
                .get(1)
                .context("hop needs a destination harness")?
                .parse()?;
            if dest != HarnessId::OpenCode
                && (flags.dest_agent.is_some() || flags.metadata_file.is_some())
            {
                anyhow::bail!(
                    "--dest-agent and --metadata-file are supported only for an OpenCode destination"
                );
            }
            // The source is read once, by the same path the library uses: a session that is not
            // there is refused (exit 2), one that cannot be read needs recovery (exit 5).
            let selection = flags.compaction(harness)?;
            let source = match (flags.from_file.as_deref(), flags.all_branches) {
                (Some(path), false) => crate::hop::export_path(harness, std::path::Path::new(path)),
                (Some(path), true) => {
                    crate::hop::export_path_all_branches(harness, std::path::Path::new(path))
                }
                (None, all) => {
                    let id = operands.first().context("hop needs a session id")?;
                    let source = SessionRef::id(harness, *id);
                    if all {
                        crate::hop::export_all_branches(&source, &roots)
                    } else {
                        crate::hop::export(&source, &roots)
                    }
                }
            }
            .map_err(anyhow::Error::new)?;
            if let Some(expected) = flags.expect_sha256.as_deref() {
                let actual = SourceFacts::of(&source).content_sha256;
                if !expected.eq_ignore_ascii_case(&actual) {
                    return Err(anyhow::Error::new(HopError::SourceChanged {
                        expected: expected.to_string(),
                        actual,
                    }));
                }
            }
            // Compaction (including a source-model summary) uses the source store. The writer and
            // optional resume use the destination's environment, never the source override.
            let mut opts = flags.import_opts(harness, dest, &source, &destination_roots)?;
            let (selected, summary) =
                crate::hop::compact(&source, &selection, &roots).map_err(anyhow::Error::new)?;
            opts.summary = summary;
            let result = crate::hop::import(dest, &selected, &opts);
            deliver(result, flags.manifest.as_deref())?;
        }
        other => anyhow::bail!("unknown command {other}\n\n{USAGE}"),
    }
    Ok(())
}

struct HarnessStoreInfo {
    path: Option<std::path::PathBuf>,
    origin: &'static str,
    state: String,
    sessions: String,
    forks: String,
}

/// The stores the inventory looks at: the harness's own variable when this process sets a
/// non-empty one, else the catalog's default location for this platform. Read-only: the library's
/// explicit store contract is unchanged, and no other command reads these defaults.
fn inventory_roots() -> StoreRoots {
    use std::path::PathBuf;
    let vars = crate::catalog::Vars::from_env();
    let pick = |harness: HarnessId| {
        std::env::var_os(crate::catalog::store_env(harness))
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(|| crate::catalog::store_default_with(harness, &vars))
    };
    let opencode_db = pick(HarnessId::OpenCode).map(|path| {
        if path.is_absolute() || path.as_os_str() == ":memory:" {
            return path;
        }
        // OpenCode resolves a relative OPENCODE_DB beneath its data directory, not mukade's cwd:
        // `$XDG_DATA_HOME/opencode/custom.db` (or the same fallback the catalog uses).
        crate::catalog::store_default_with(HarnessId::OpenCode, &vars)
            .and_then(|default| default.parent().map(|dir| dir.join(&path)))
            .unwrap_or(path)
    });
    StoreRoots {
        claude_config_dir: pick(HarnessId::ClaudeCode),
        codex_home: pick(HarnessId::Codex),
        opencode_db,
        pi_agent_dir: pick(HarnessId::Pi),
    }
}

fn store_path(harness: HarnessId, roots: &StoreRoots) -> Option<&std::path::Path> {
    match harness {
        HarnessId::ClaudeCode => roots.claude_config_dir.as_deref(),
        HarnessId::Codex => roots.codex_home.as_deref(),
        HarnessId::OpenCode => roots.opencode_db.as_deref(),
        HarnessId::Pi => roots.pi_agent_dir.as_deref(),
    }
}

fn store_env_name(harness: HarnessId) -> &'static str {
    crate::catalog::store_env(harness)
}

/// What the catalog says about lineage, in one line: the fields when the store records them, or
/// that it does not and why.
fn lineage_info(harness: HarnessId) -> String {
    let lineage = &crate::catalog::entry(harness).lineage;
    if lineage.recorded {
        format!("recorded: {}", lineage.fields.join(", "))
    } else {
        format!("not recorded ({})", lineage.note)
    }
}

fn inspect_store(harness: HarnessId, roots: &StoreRoots) -> HarnessStoreInfo {
    use std::path::Path;
    let path = store_path(harness, roots).map(Path::to_path_buf);
    let explicit = std::env::var_os(store_env_name(harness))
        .is_some_and(|value| !value.is_empty());
    let origin = if explicit { "environment" } else { "default" };
    let missing = || HarnessStoreInfo {
        path: path.clone(),
        origin,
        state: "not found".into(),
        sessions: "unknown".into(),
        forks: if matches!(harness, HarnessId::ClaudeCode | HarnessId::Codex) {
            "unknown (not recorded)".into()
        } else {
            "unknown".into()
        },
    };
    let Some(path) = path.clone() else {
        return missing();
    };
    if harness == HarnessId::OpenCode && path.as_os_str() == ":memory:" {
        return HarnessStoreInfo {
            path: Some(path),
            origin,
            state: "non-persistent (:memory:)".into(),
            sessions: "unknown".into(),
            forks: "unknown".into(),
        };
    }
    if !path.exists() {
        return missing();
    }
    let detections = crate::detect::detect_store(&path);
    let recognized = detections.iter().any(|detection| detection.harness == harness);
    if !recognized && !explicit {
        return HarnessStoreInfo {
            path: Some(path),
            origin,
            state: "present, layout not recognized".into(),
            sessions: "unknown".into(),
            forks: if matches!(harness, HarnessId::ClaudeCode | HarnessId::Codex) {
                "unknown (not recorded)".into()
            } else {
                "unknown".into()
            },
        };
    }
    let reader = match crate::reader::for_harness(harness) {
        Ok(reader) => reader,
        Err(error) => {
            return HarnessStoreInfo {
                path: Some(path),
                origin,
                state: format!("read error: {error}"),
                sessions: "unknown".into(),
                forks: "unknown".into(),
            };
        }
    };
    let (session_count, fork_count) = if harness == HarnessId::OpenCode {
        match crate::reader::opencode::store_counts(&path) {
            Ok(counts) => (Ok(counts.sessions), Some(Ok(counts.forks))),
            Err(error) => {
                let error = format!("{error:#}");
                (
                    Err(anyhow::anyhow!(error.clone())),
                    Some(Err(anyhow::anyhow!(error))),
                )
            }
        }
    } else if harness == HarnessId::Pi {
        match roots.pi_agent_dir.as_deref().map(crate::reader::pi::store_summary) {
            Some(Ok(summary)) => (Ok(summary.sessions), Some(summary.forks)),
            Some(Err(error)) => {
                let error = format!("{error:#}");
                (
                    Err(anyhow::anyhow!(error.clone())),
                    Some(Err(anyhow::anyhow!(error))),
                )
            }
            None => {
                let error = "PI_CODING_AGENT_DIR is not set: no Pi store to list".to_string();
                (
                    Err(anyhow::anyhow!(error.clone())),
                    Some(Err(anyhow::anyhow!(error))),
                )
            }
        }
    } else {
        (reader.list(roots).map(|sessions| sessions.len()), None)
    };
    let state = match &session_count {
        Ok(_) => "recognized".to_string(),
        Err(error) => format!(
            "recognized, read error: {}",
            error.to_string().replace(['\r', '\n'], " ")
        ),
    };
    let sessions = session_count
        .map(|count| count.to_string())
        .unwrap_or_else(|_| "unknown".into());
    let forks = match fork_count {
        Some(Ok(count)) => count.to_string(),
        Some(Err(error)) => format!(
            "unknown ({})",
            error.to_string().replace(['\r', '\n'], " ")
        ),
        None => "unknown (not recorded)".into(),
    };
    HarnessStoreInfo {
        path: Some(path),
        origin,
        state,
        sessions,
        forks,
    }
}

fn print_harnesses() -> Result<()> {
    use crate::exe::Source;
    let roots = inventory_roots();
    println!(
        "mukade harnesses - local inventory; store reads are read-only (catalog {}, {} supported)",
        crate::catalog::catalog().schema,
        HarnessId::ALL.len()
    );
    for harness in HarnessId::ALL {
        let entry = crate::catalog::entry(harness);
        let (binary, source) = crate::exe::resolve_source(harness, None);
        let status = match source {
            Source::Override | Source::Env => "configured",
            Source::Path | Source::WellKnown => "installed",
            Source::Name => "not found",
        };
        let version = if source == Source::Name {
            "unknown".to_string()
        } else {
            crate::exe::version_of_in(&binary, None).unwrap_or_else(|| "unknown".into())
        };
        let source = match source {
            Source::Override => entry.binary.flag.as_str(),
            Source::Env => entry.binary.env.as_str(),
            Source::Path => "PATH",
            Source::WellKnown => "catalog location",
            Source::Name => "not on PATH, no catalog location",
        };
        let store = inspect_store(harness, &roots);
        println!("\n{} ({}) - {}", harness.as_str(), entry.display, status);
        println!("  binary:   {} [{}]", binary, source);
        println!("  version:  {}", version);
        println!(
            "  store:    {} [{}; {}]",
            store
                .path
                .as_deref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "unknown".into()),
            store_env_name(harness),
            store.origin
        );
        println!("  status:   {}", store.state);
        println!("  sessions: {}", store.sessions);
        println!("  forks:    {}", store.forks);
        println!("  lineage:  {}", lineage_info(harness));
        println!("  layout:   {}", crate::catalog::layout(harness).join(" | "));
    }
    Ok(())
}

/// Hand back a written session: its receipt on stdout first, so a caller always learns what was
/// written, then the manifest file when one was asked for. A resume that failed after the write
/// still has both (Â§5), and keeps its own exit code.
fn deliver(result: Result<Receipt, HopError>, manifest: Option<&str>) -> Result<()> {
    let (receipt, failure) = match result {
        Ok(receipt) => (receipt, None),
        Err(HopError::ResumeFailed { reason, receipt }) => {
            let written = (*receipt).clone();
            (written, Some(HopError::ResumeFailed { reason, receipt }))
        }
        Err(other) => return Err(anyhow::Error::new(other)),
    };
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
    let saved = match manifest {
        Some(path) => std::fs::write(path, serde_json::to_string_pretty(&receipt.manifest)?)
            .with_context(|| {
                format!(
                    "the session was written ({}), but its manifest could not be saved to {path}",
                    receipt.location
                )
            }),
        None => Ok(()),
    };
    match failure {
        Some(error) => {
            if let Err(unsaved) = saved {
                eprintln!("{unsaved:#}");
            }
            Err(anyhow::Error::new(error))
        }
        None => saved,
    }
}

/// The value of the flag at `i`. A value that is itself an option means the flag was given none.
fn next(rest: &[String], i: &mut usize) -> Result<String> {
    *i += 1;
    let flag = &rest[*i - 1];
    match rest.get(*i) {
        Some(value) if !value.starts_with("--") => Ok(value.clone()),
        Some(value) => anyhow::bail!("{flag} needs a value, not the option {value}"),
        None => anyhow::bail!("{flag} needs a value"),
    }
}

/// No positional argument is left unread: a stray one is a mistake, not something to ignore.
fn no_more(operands: &[&str], expected: usize) -> Result<()> {
    match operands.get(expected) {
        Some(extra) => anyhow::bail!("unexpected argument {extra}\n\n{USAGE}"),
        None => Ok(()),
    }
}

#[derive(Default, PartialEq, Eq)]
struct Flags {
    cwd: Option<String>,
    dest_version: Option<String>,
    dest_protocol: Option<String>,
    dest_shell: Option<String>,
    model_provider: Option<String>,
    model: Option<String>,
    project_id: Option<String>,
    dest_id: Option<String>,
    dest_path: Option<String>,
    dest_agent: Option<String>,
    metadata_file: Option<String>,
    tool_map: Option<String>,
    without_tools: bool,
    expect_sha256: Option<String>,
    reasoning: Option<String>,
    compaction: Option<String>,
    keep_turns: Option<String>,
    intent: Option<String>,
    summary_model: Option<String>,
    summary_timeout: Option<String>,
    resume_prompt: Option<String>,
    from_file: Option<String>,
    all_branches: bool,
    manifest: Option<String>,
    output: Option<String>,
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
        let write = self.write_opts(dest, session, roots)?;
        Ok(ImportOpts {
            cwd: self.cwd.clone().or_else(|| session.meta.cwd.clone()),
            roots: roots.clone(),
            reasoning: write.reasoning,
            source_facts: SourceFacts {
                harness: source_harness.as_str().to_string(),
                ..SourceFacts::of(session)
            },
            write: WriteDetails {
                dest_version: write.dest_version,
                dest_protocol: write.dest_protocol,
                dest_shell: write.dest_shell,
                offered: write.offered,
                model_provider: write.model_provider,
                model: write.model,
                project_id: write.project_id,
                dest_id: write.dest_id,
                dest_path: write.dest_path,
                dest_agent: write.dest_agent,
                metadata: write.metadata,
                tool_map: write.tool_map,
                without_tools: write.without_tools,
                now: write.now,
                exe: write.exe,
            },
            resume: self
                .resume_prompt
                .clone()
                .map(crate::hop::ResumePrompt::new),
            compaction: self.compaction.clone().unwrap_or_else(|| "none".into()),
            summary: None,
        })
    }

    fn write_opts(
        &self,
        harness: HarnessId,
        session: &crate::model::Session,
        roots: &StoreRoots,
    ) -> Result<WriteOpts> {
        if harness != HarnessId::OpenCode
            && (self.dest_agent.is_some() || self.metadata_file.is_some())
        {
            anyhow::bail!(
                "--dest-agent and --metadata-file are supported only for an OpenCode destination"
            );
        }
        if let Some(id) = self.dest_id.as_deref() {
            if harness == HarnessId::OpenCode {
                crate::writer::opencode::validate_session_id(id)?;
            } else {
                uuid::Uuid::parse_str(id).map_err(|_| anyhow::anyhow!("invalid destination UUID"))?;
            }
        }
        if let Some(agent) = self.dest_agent.as_deref() {
            crate::writer::opencode::validate_agent(agent)?;
        }
        let metadata = match self.metadata_file.as_deref() {
            None => None,
            Some(file) => {
                let info =
                    std::fs::metadata(file).with_context(|| format!("metadata file {file}"))?;
                anyhow::ensure!(
                    info.is_file() && info.len() <= 1_048_576,
                    "metadata file must be a regular file no larger than 1 MiB"
                );
                let value: serde_json::Value = serde_json::from_slice(&std::fs::read(file)?)
                    .with_context(|| format!("parse metadata file {file}"))?;
                anyhow::ensure!(value.is_object(), "session metadata must be a JSON object");
                Some(value)
            }
        };
        let tool_map = match self.tool_map.as_deref() {
            None => Vec::new(),
            Some(file) => {
                let info = std::fs::metadata(file).with_context(|| format!("tool map {file}"))?;
                anyhow::ensure!(info.is_file() && info.len() <= 1_048_576, "tool map must be a regular file no larger than 1 MiB");
                let entries: Vec<(String, String, serde_json::Value, serde_json::Value)> = serde_json::from_slice(&std::fs::read(file)?)
                    .with_context(|| format!("parse tool map {file}"))?;
                anyhow::ensure!(entries.len() <= 4096 && entries.iter().all(|(source, destination, original, mapped)|
                    !source.is_empty() && !destination.is_empty() && source.len() <= 256 && destination.len() <= 256
                    && original.is_object() && mapped.is_object()), "invalid tool map");
                entries
            }
        };
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
            Some(name) => match crate::writer::plan::shell_of(name) {
                Some(shell) => Some(shell),
                None => {
                    anyhow::bail!("unknown --dest-shell {name} (bash, sh, zsh, powershell, pwsh)")
                }
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
        // chain â€” PATH, then the well-known install locations â€” so a default install is found
        // with no flag at all.
        let exe_override = self
            .exe_flag(harness)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
            .or_else(|| {
                std::env::var(crate::exe::env_var_name(harness))
                    .ok()
                    .filter(|v| !v.is_empty())
            });
        let resolved_exe = crate::exe::resolve(harness, self.exe_flag(harness));
        // The destination's own version stamp, when the caller did not state it: L7 wants the
        // destination's `cli_version`, and Codex refuses a rollout without one.
        let dest_version = self.dest_version.clone().or_else(|| {
            let _span = crate::timing::span("probe destination version");
            crate::exe::version_of(&resolved_exe)
        });
        let cwd = self.cwd.clone().or_else(|| session.meta.cwd.clone());
        // OpenCode's interchange format requires the destination's own project id; the destination
        // is the only one that knows it, so it is asked.
        let project_id = match self.project_id.clone() {
            Some(id) => Some(id),
            None if harness == HarnessId::OpenCode => cwd
                .as_deref()
                .and_then(|cwd| opencode_project_id(&resolved_exe, cwd, roots)),
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
            dest_id: self.dest_id.clone(),
            dest_path: self.dest_path.as_ref().map(std::path::PathBuf::from),
            dest_agent: self.dest_agent.clone(),
            metadata,
            tool_map,
            without_tools: self.without_tools,
            reasoning,
            title: session.meta.title.clone(),
            now: None,
            exe: exe_override,
        })
    }

    /// The compaction the flags ask for, refused before anything is read or run when it cannot
    /// work: a summary stands for the branch the source resumes, and a source outside its store can
    /// be summarized only by Pi, which forks a session from its file.
    fn compaction(&self, source: HarnessId) -> Result<Compaction> {
        let mode = self
            .compaction
            .as_deref()
            .or(self.intent.as_ref().map(|_| "summary"));
        let summary = mode == Some("summary");
        if !summary
            && (self.keep_turns.is_some()
                || self.summary_model.is_some()
                || self.summary_timeout.is_some())
        {
            anyhow::bail!(
                "--keep-turns, --summary-model and --summary-timeout go with --compaction summary or --intent"
            );
        }
        match mode {
            None | Some("none") => Ok(Compaction::None),
            Some("portable") => Ok(Compaction::Portable {
                summary: SummarySource::SourceRecord,
                keep_from: KeepFrom::SourceBoundary,
            }),
            Some("summary") => {
                let keep_turns = match self.keep_turns.as_deref() {
                    Some(n) => n
                        .parse::<usize>()
                        .with_context(|| format!("--keep-turns {n}: not a number of turns"))?,
                    None => 0,
                };
                let timeout = match self.summary_timeout.as_deref() {
                    Some(s) => match s.parse::<u64>() {
                        Ok(seconds) if seconds > 0 => std::time::Duration::from_secs(seconds),
                        _ => anyhow::bail!("--summary-timeout {s}: not a number of seconds"),
                    },
                    None => SummarizeOpts::default().timeout,
                };
                if self.all_branches {
                    return Err(HopError::Unsupported(
                        "--compaction summary with --all-branches: the summary stands for the branch the \
                         source resumes"
                            .into(),
                    )
                    .into());
                }
                if self.from_file.is_some() && source != HarnessId::Pi {
                    return Err(HopError::Unsupported(format!(
                        "--compaction summary with --from-file: {} summarizes a session from its own \
                         store, by id; only Pi forks one from a file",
                        source.as_str()
                    ))
                    .into());
                }
                let prompt =
                    crate::intent::resolve(self.intent.as_deref()).map_err(anyhow::Error::msg)?;
                let intent = prompt.name.clone();
                Ok(Compaction::Portable {
                    summary: SummarySource::SourceModel(SummarizeOpts {
                        model: self.summary_model.clone(),
                        timeout,
                        exe: self.exe_flag(source).map(str::to_owned),
                        prompt: Some(prompt.text),
                        intent: Some(intent),
                    }),
                    keep_from: KeepFrom::LastTurns(keep_turns),
                })
            }
            Some(other) => anyhow::bail!("unknown --compaction {other} (none, portable, summary)"),
        }
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
            roots.opencode_db = Some(if root.is_dir() {
                root.join("opencode.db")
            } else {
                root
            })
        }
        HarnessId::Pi => roots.pi_agent_dir = Some(root),
    }
}

/// The project the destination already files sessions of this directory under. The destination is
/// asked from **inside that directory**: the CLI scopes its session list to the project it is run
/// in, so the question has to be asked where the session will live.
fn opencode_project_id(exe: &str, cwd: &str, roots: &StoreRoots) -> Option<String> {
    let _span = crate::timing::span("probe opencode project");
    opencode_project_id_in(exe, cwd, roots, crate::cache::dir().as_deref())
}

/// [`opencode_project_id`] with the cache folder stated. The answer is kept for this binary, this
/// database and this directory: finding it takes a process start per session it has to look at.
fn opencode_project_id_in(
    exe: &str,
    cwd: &str,
    roots: &StoreRoots,
    cache: Option<&std::path::Path>,
) -> Option<String> {
    let program = std::path::PathBuf::from(crate::exe::spawnable(exe));
    let key = crate::cache::identity(&program).map(|identity| {
        let database = roots
            .opencode_db
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        format!("project|{identity}|{database}|{}", normalise_directory(cwd))
    });
    if let (Some(dir), Some(key)) = (cache, &key)
        && let Some(known) = crate::cache::get(dir, key)
    {
        return Some(known);
    }
    let found = ask_opencode_project(exe, cwd, roots)?;
    if let (Some(dir), Some(key)) = (cache, &key) {
        crate::cache::put(dir, key, &found);
    }
    Some(found)
}

fn ask_opencode_project(exe: &str, cwd: &str, roots: &StoreRoots) -> Option<String> {
    let directory = std::path::Path::new(cwd);
    // Asked of the store the session will be written to, not the caller's own.
    let store = Some((HarnessId::OpenCode, roots));
    let list = run_probe(exe, &["session", "list", "--standalone"], directory, store)
        .map(|out| String::from_utf8_lossy(&out).into_owned())?;
    for id in list
        .lines()
        .filter_map(|line| line.split('\t').next())
        .filter(|id| id.starts_with("ses_"))
        .take(5)
    {
        let Some(doc) = run_json(
            exe,
            &["session", "export", id, "--standalone"],
            directory,
            store,
        ) else {
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

fn normalise_directory(directory: &str) -> String {
    directory
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

fn same_directory(a: &str, b: &str) -> bool {
    normalise_directory(a) == normalise_directory(b)
}

/// How long a question to the destination's binary (its version, its sessions) may take; a probe
/// that does not answer leaves the fact unknown instead of hanging the command.
const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// The stdout of a probe that succeeded within [`PROBE_TIMEOUT`], run against `store` when given.
fn run_probe(
    exe: &str,
    args: &[&str],
    cwd: &std::path::Path,
    store: Option<(HarnessId, &StoreRoots)>,
) -> Option<Vec<u8>> {
    let mut command = std::process::Command::new(crate::exe::spawnable(exe));
    command.args(args).current_dir(cwd);
    if let Some((harness, roots)) = store {
        roots.apply_to(harness, &mut command);
    }
    let output = crate::exe::run_within(&mut command, PROBE_TIMEOUT).ok()?;
    output
        .status
        .is_some_and(|status| status.success())
        .then_some(output.stdout)
}

fn run_json(
    exe: &str,
    args: &[&str],
    cwd: &std::path::Path,
    store: Option<(HarnessId, &StoreRoots)>,
) -> Option<serde_json::Value> {
    serde_json::from_slice(&run_probe(exe, args, cwd, store)?).ok()
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
    top_level_string(&text, "model_provider")
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
        return Some(value.to_string()).filter(|v| !v.is_empty());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{opencode_project_id_in, top_level_string};
    use crate::roots::StoreRoots;

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

    /// The project id OpenCode files a directory under takes one process start per session it has
    /// to look at: it is asked once per binary, database and directory.
    #[test]
    fn the_opencode_project_of_a_directory_is_asked_once() {
        let dir = std::env::temp_dir().join(format!("mukade-project-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let work = dir.join("work");
        std::fs::create_dir_all(&work).unwrap();
        let (count, document) = (dir.join("count.txt"), dir.join("export.json"));
        std::fs::write(
            &document,
            serde_json::json!({"info": {"location": {"directory": work.to_string_lossy()},
                                        "projectID": "proj_work"}})
            .to_string(),
        )
        .unwrap();
        // `session list` prints one session; `session export` prints its document.
        let launcher = if cfg!(windows) {
            let path = dir.join("fakeopencode.cmd");
            std::fs::write(
                &path,
                format!(
                    "@echo off\r\necho x>>\"{}\"\r\nif \"%2\"==\"list\" (echo ses_one	title) else type \"{}\"\r\n",
                    count.display(),
                    document.display()
                ),
            )
            .unwrap();
            path
        } else {
            let path = dir.join("fakeopencode");
            std::fs::write(
                &path,
                format!(
                    "#!/bin/sh\necho x >> '{}'\nif [ \"$2\" = list ]; then printf 'ses_one\\ttitle\\n'; else cat '{}'; fi\n",
                    count.display(),
                    document.display()
                ),
            )
            .unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
            path
        };
        let exe = launcher.to_string_lossy().into_owned();
        let cache = dir.join("cache");
        let roots = StoreRoots::default();
        let cwd = work.to_string_lossy().into_owned();
        let runs = || std::fs::read_to_string(&count).map_or(0, |t| t.lines().count());
        let first = opencode_project_id_in(&exe, &cwd, &roots, Some(&cache));
        assert_eq!(first.as_deref(), Some("proj_work"));
        let asked = runs();
        assert!(asked >= 2, "a list and an export: {asked}");
        assert_eq!(
            opencode_project_id_in(&exe, &cwd, &roots, Some(&cache)).as_deref(),
            Some("proj_work")
        );
        assert_eq!(runs(), asked, "the second answer came from the cache");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
