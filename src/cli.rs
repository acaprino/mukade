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
//! `PI_CODING_AGENT_DIR`, `HERMES_HOME`, `DSH_HOME`), so every run stays inside the scratch home
//! the caller set.

use crate::hop::{
    Compaction, HopError, ImportOpts, KeepFrom, Receipt, SessionRef, SourceFacts, SummarySource,
    WriteDetails,
};
use crate::model::{HarnessId, ParentKind, Protocol};
use crate::native_compaction::NativeBudget;
use crate::policy::ReasoningPolicy;
use crate::roots::StoreRoots;
use crate::summary::SummarizeOpts;
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
mukade scan  <dir> [--depth <n>]
mukade chat  start|status|stop|<operation> [options]    (mukade chat --help)

`harnesses` lists every supported harness with the executable and store mukade finds on this
machine, its version, session/fork counts and lineage support. Store reads are read-only; discovered
executables are asked for `--version`.

`intents` lists the editable summary prompts (name, path, builtin/custom) as JSON, without reading a
session or starting a harness.

harnesses: claude_code | codex | opencode | pi | hermes | deepseek_harness

A `hop` reads the source and writes the destination in one process: set **both** harnesses' store
variables (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `OPENCODE_DB` with its `XDG_*`, `PI_CODING_AGENT_DIR`,
`HERMES_HOME`, `DSH_HOME`). An unset variable means that harness's configured store; Claude Code,
Codex, Pi, Hermes and DeepSeek Harness need an explicit root.

`detect` prints which harness stores a directory holds, with the evidence: session records, Pi's
models.json, Hermes's state.db or OpenCode's opencode.db (that file can be passed directly; other
SQLite files, e.g. Codex's state_*.sqlite, are not OpenCode). An empty directory tree infers
nothing. With `--root <dir>` (non-empty) the harness may be omitted from `list`, `dump` and a `hop`
source: it is inferred from the directory's own layout when exactly one harness is recognized.
`--root` names the source store for a hop, the only store otherwise; for OpenCode a directory means
its `opencode.db`.

`scan` walks a directory (default 8 folder levels, `--depth <n>` to change it) and prints every
harness installation below it as JSON, portable and custom ones included: the harness, the store as
its variable names it, the executable kept beside the store (or null) and the evidence. It never
follows links, skips node_modules, .git and target, and sets `truncated` when the depth or
its folder budget stopped it. `--root` and `--dest-root` accept such a folder too: when the path is
not a store itself, it must hold exactly one installation (of the harness named, when one is).
`--dest-root` names a hop's destination installation; its executable writes the session unless
--<harness>-exe names another.

options:
  --cwd <dir>              the project the destination files the session under
  --dest-version <v>       the destination's own version stamp
  --dest-protocol <p>      anthropic-messages | openai-responses | openai-chat
  --dest-shell <s>         the shell the destination runs: bash, sh, zsh, dash, ksh, posix,
                           powershell, pwsh (a path to one too), or none: no dialect, so shell
                           calls into Codex, OpenCode and Pi stay text. Default: the one its
                           configuration or platform names
  --model-provider <id>    the destination's provider id
  --model <id>             the destination's model id
  --project-id <id>        the destination's project (OpenCode requires one)
  --dest-id <id>           a reserved destination session ID
  --dest-path <path>       a reserved file for a file-based destination
  --dest-agent <name>      the destination's primary agent (OpenCode only)
  --metadata-file <path>   a JSON object to store as session metadata (OpenCode only)
  --tool-map <path>        explicit source tool bindings from the destination catalog
  --without-tools          omit historical tool calls and results (hop and write)
  --interrupted <mode>     keep (default) | drop: leave out what the agent did in the turns the
                           source never finished (killed or interrupted by the user), with the
                           tool calls and results of those turns; their prompts stay
                           (hop, write and document)
  --expect-sha256 <hash>   refuse a hop when the captured source fingerprint changed
  --reasoning <mode>       drop_foreign (default) | as_text | drop
  --compaction <mode>      none (default) | portable (the source's own summary, then its tail) |
                           summary (the source's model writes one now; dump and hop) |
                           native (whole history plus the destination's own deterministic
                           compaction record; document and hop)
  --keep-turns <n>         summary: also keep the last n user turns verbatim (default 0)
  --context-window <n>     native: the destination model's context window, in tokens
  --max-output-tokens <n>  native: the destination model's output limit (default 16000)
  --text-token-factor <f>  native: the Gateway's text-token multiplier (default: chars/3)
  --intent <name>          summary: the prompt in intent-prompts/<name>.md (default: generic);
                           it implies --compaction summary
  --summary-model <id>     summary: the model to ask (default: the session's own)
  --summary-timeout <s>    summary: deadline of the summarizing run (default 300)
  --resume-prompt <text>   resume in the destination and keep its reply in the receipt
  --from-file <path>       read the source from a file instead of its store
  --all-branches           dump, hop, document: carry every branch the source keeps as a branch
                           (a Claude Code or Pi tree, a Codex thread's redone pages and rolled-back
                           turns); a destination that keeps one conversation leaves them out and
                           counts them; default: only the branch the harness resumes
  --archived <bool>        list: true (default) includes the sessions the harness marked archived,
                           false leaves them out; the same for every harness
  --children               list: also the subagent sessions (Claude Code, Codex, OpenCode, Hermes,
                           DeepSeek Harness); a row
                           with a parent names it and its kind, spawn or fork
                           dump: the subagent sessions too, in the document's `children`
                           hop: write them too, each as a session of its own after the session,
                           linked where the destination has a link (OpenCode, Claude Code)
  --manifest <path>        also write the manifest there
  --output <path>          dump: write the document to this file (UTF-8) instead of stdout
  --timing                 print where the time went, phase by phase, on stderr
  --root <dir>             the store directory to use, and to infer the harness from
  --dest-root <dir>        hop: the destination's store or installation folder
  --depth <n>              scan: how many folder levels to descend (default 8)
  --claude-exe <path>      the claude binary to spawn
  --codex-exe <path>       the codex binary to spawn
  --opencode-exe <path>    the opencode binary to spawn
  --pi-exe <path>          the pi binary to spawn
  --hermes-exe <path>      the hermes binary to spawn
  --dsh-exe <path>         the dsh binary to spawn

Executable resolution: --<harness>-exe â†’ MUKADE_CLAUDE_EXE / MUKADE_CODEX_EXE /
MUKADE_OPENCODE_EXE / MUKADE_PI_EXE / MUKADE_HERMES_EXE / MUKADE_DSH_EXE â†’ PATH â†’ the
well-known install locations â†’ the plain command name. --root selects the
source store of a hop; a portable destination needs its store variable set in the environment
and its --<harness>-exe. A source OpenCode read spawns the binary its own resolution finds
(MUKADE_OPENCODE_EXE wins).
";

/// Parse and run the command line. `args` excludes the program name.
pub mod chat;

pub fn run(args: &[String]) -> Result<()> {
    let (command, rest) = args.split_first().context(USAGE)?;
    // Before anything is installed or read: `chat status` creates no file.
    if command == "chat" {
        return chat::run(rest);
    }
    if command == "--identity" {
        anyhow::ensure!(rest.is_empty(), "--identity takes no arguments");
        println!(
            "{}",
            json!({ "name":"mukade", "version":env!("CARGO_PKG_VERSION"),
            "commit":option_env!("MUKADE_SOURCE_COMMIT"), "target": if cfg!(all(windows, target_arch="x86_64", target_env="msvc")) { "x86_64-pc-windows-msvc" } else { "other" } })
        );
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
            "--interrupted" => flags.interrupted = Some(next(rest, &mut i)?),
            "--expect-sha256" => flags.expect_sha256 = Some(next(rest, &mut i)?),
            "--compaction" => flags.compaction = Some(next(rest, &mut i)?),
            "--keep-turns" => flags.keep_turns = Some(next(rest, &mut i)?),
            "--context-window" => flags.context_window = Some(next(rest, &mut i)?),
            "--max-output-tokens" => flags.max_output_tokens = Some(next(rest, &mut i)?),
            "--text-token-factor" => flags.text_token_factor = Some(next(rest, &mut i)?),
            "--intent" => flags.intent = Some(next(rest, &mut i)?),
            "--summary-model" => flags.summary_model = Some(next(rest, &mut i)?),
            "--summary-timeout" => flags.summary_timeout = Some(next(rest, &mut i)?),
            "--resume-prompt" => flags.resume_prompt = Some(next(rest, &mut i)?),
            "--from-file" => flags.from_file = Some(next(rest, &mut i)?),
            "--all-branches" => flags.all_branches = true,
            "--archived" => flags.archived = Some(next(rest, &mut i)?),
            "--children" => flags.children = true,
            "--manifest" => flags.manifest = Some(next(rest, &mut i)?),
            "--output" => flags.output = Some(next(rest, &mut i)?),
            "--timing" => crate::timing::enable(),
            "--reasoning" => flags.reasoning = Some(next(rest, &mut i)?),
            "--root" => flags.root = Some(next(rest, &mut i)?),
            "--dest-root" => flags.dest_root = Some(next(rest, &mut i)?),
            "--depth" => flags.depth = Some(next(rest, &mut i)?),
            other if exe_flag_harness(other).is_some() => {
                let harness = exe_flag_harness(other).expect("checked by the guard");
                let value = next(rest, &mut i)?;
                *flags.exe_slot(harness) = Some(value);
            }
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(());
            }
            other if other.starts_with("--") => anyhow::bail!("unknown option {other}\n\n{USAGE}"),
            other => positional.push(other),
        }
        i += 1;
    }

    if flags.archived.is_some() && command != "list" {
        anyhow::bail!("--archived is supported only by list");
    }
    if flags.children && !matches!(command.as_str(), "list" | "dump" | "hop") {
        anyhow::bail!("--children is supported only by list, dump and hop");
    }
    if flags.children && flags.from_file.is_some() && command != "list" {
        anyhow::bail!(
            "--children finds subagent sessions in the store: it cannot go with --from-file"
        );
    }
    // Every harness alike: the sessions its store marks archived are kept or left out.
    let with_archived = match flags.archived.as_deref() {
        None | Some("true") => true,
        Some("false") => false,
        Some(other) => anyhow::bail!("--archived takes true or false, not {other:?}"),
    };
    if flags.without_tools && !matches!(command.as_str(), "hop" | "write" | "document") {
        anyhow::bail!("--without-tools is supported only by hop, write and document");
    }
    if flags.interrupted.is_some() && !matches!(command.as_str(), "hop" | "write" | "document") {
        anyhow::bail!("--interrupted is supported only by hop, write and document");
    }
    // A mistyped mode is refused here, before a store is read.
    flags.interrupted()?;
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
    // `dump` already compacted, so a compaction flag there would be silently ignored. `document`
    // may ask Mukade for the OpenCode-native boundary it will write itself.
    let native_document = command == "document" && flags.compaction.as_deref() == Some("native");
    if (flags.compaction.is_some()
        || flags.keep_turns.is_some()
        || flags.summary_model.is_some()
        || flags.summary_timeout.is_some())
        && !matches!(command.as_str(), "dump" | "hop")
        && !native_document
    {
        anyhow::bail!(
            "--compaction, --keep-turns, --summary-model and --summary-timeout are supported only by dump and hop"
        );
    }
    if flags.compaction.as_deref() == Some("native") && command == "dump" {
        anyhow::bail!("--compaction native needs a destination writer: use document or hop");
    }
    let native_budget_flags = flags.context_window.is_some()
        || flags.max_output_tokens.is_some()
        || flags.text_token_factor.is_some();
    if native_budget_flags && !matches!(command.as_str(), "document" | "hop") {
        anyhow::bail!(
            "--context-window, --max-output-tokens and --text-token-factor are supported only by document and hop"
        );
    }
    if native_budget_flags && flags.compaction.as_deref() != Some("native") {
        anyhow::bail!(
            "--context-window, --max-output-tokens and --text-token-factor go with --compaction native"
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
    if (flags.dest_id.is_some()
        || flags.dest_path.is_some()
        || flags.dest_agent.is_some()
        || flags.metadata_file.is_some())
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
    if flags.depth.is_some() && command != "scan" {
        anyhow::bail!("--depth is supported only by scan");
    }
    if command == "scan" {
        let path = positional.first().context("scan needs a directory")?;
        no_more(&positional, 1)?;
        anyhow::ensure!(
            flags
                == Flags {
                    depth: flags.depth.clone(),
                    ..Flags::default()
                },
            "scan takes only --depth"
        );
        let depth = match flags.depth.as_deref() {
            Some(depth) => depth
                .parse::<usize>()
                .with_context(|| format!("--depth {depth}: not a number of folder levels"))?,
            None => crate::detect::DEFAULT_SCAN_DEPTH,
        };
        let scan = crate::detect::scan_installations(std::path::Path::new(path), depth);
        let installations: Vec<serde_json::Value> =
            scan.installations.iter().map(installation_json).collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "path": path,
                "depth": depth,
                "visited": scan.visited,
                "truncated": scan.truncated,
                "installations": installations,
            }))?
        );
        return Ok(());
    }
    if flags.dest_root.is_some() && command != "hop" {
        anyhow::bail!(
            "--dest-root is supported only by hop: it names the destination's installation"
        );
    }
    if flags
        .dest_root
        .as_deref()
        .is_some_and(|root| root.trim().is_empty())
    {
        anyhow::bail!("--dest-root needs a non-empty directory");
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
    // --root names a store, or a folder that holds one installation; without a harness in the
    // arguments, the installation's own layout says which harness it is.
    let rooted = match flags.root.as_deref() {
        Some(root) => locate_root(root, explicit, "--root")?,
        None => None,
    };
    let harness = explicit
        .or(rooted.as_ref().map(|found| found.harness))
        .context(USAGE)?;
    // The root override concerns the store the command reads; for a hop that is the source, and
    // the destination keeps its own environment.
    if let Some(root) = flags.root.as_deref() {
        let store = rooted.as_ref().map_or_else(
            || std::path::PathBuf::from(root),
            |found| found.store.clone(),
        );
        roots.set_root(harness, &store);
    }
    // `write`'s root is its destination: the engine kept beside that store imports into it.
    let write_engine = rooted
        .and_then(|found| found.exe)
        .map(|exe| exe.to_string_lossy().into_owned());
    match command.as_str() {
        "list" => {
            no_more(operands, 0)?;
            let reader = crate::reader::for_harness(harness)?;
            let sessions = if flags.children {
                reader.list_all(&roots)?
            } else {
                reader.list(&roots)?
            };
            for info in sessions
                .into_iter()
                .filter(|info| with_archived || !info.archived)
            {
                let mut row = json!({
                    "id": info.id,
                    "title": info.title,
                    "cwd": info.cwd,
                    "timestamp": info.updated,
                    "location": info.path,
                    "archived": info.archived,
                    "unreadable": info.unreadable,
                    "live": info.live,
                });
                // Only a session with a parent has the two keys: a root's row is as it was.
                if let (Some(parent), Some(object)) = (info.parent, row.as_object_mut()) {
                    object.insert("parent".into(), json!(parent));
                    object.insert(
                        "kind".into(),
                        json!(info.parent_kind.as_ref().map(ParentKind::name)),
                    );
                }
                println!("{row}");
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
                (Some(path), false) => crate::hop::export_path(harness, std::path::Path::new(path)),
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
            if let Some(command) = summary
                .as_ref()
                .and_then(|facts| facts.summary.leftover.as_ref())
            {
                eprintln!("the summarizing run left a fork in the source store: run {command}");
            }
            // Who wrote the summary travels with it, for the manifest `write` makes later.
            let doc = if flags.children {
                crate::children::document(&selection, summary.as_ref(), &roots)
                    .map_err(anyhow::Error::new)?
            } else {
                crate::hop::document(&selection, summary.as_ref())
            };
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
            crate::harnesses::destination(harness)?;
            let path = operands.first().context("write needs a document file")?;
            let text = std::fs::read_to_string(path).with_context(|| format!("read {path}"))?;
            let doc: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))
                .with_context(|| format!("parse {path}"))?;
            let session = crate::wire::from_document(&doc)?;
            if let Some(children) = doc["children"].as_array().filter(|c| !c.is_empty()) {
                eprintln!(
                    "the document holds {} subagent session(s); write imports the session alone (hop --children writes them)",
                    children.len()
                );
            }
            let opts = flags.import_opts(
                session.meta.harness,
                harness,
                &session,
                &roots,
                write_engine.as_deref(),
            )?;
            deliver(
                crate::hop::import_document(harness, &doc, &session, &opts),
                flags.manifest.as_deref(),
            )?;
        }
        "document" => {
            no_more(operands, 2)?;
            let source_harness: HarnessId = operands
                .first()
                .context("document needs a source harness")?
                .parse()?;
            let dest = harness;
            let Some(interchange) = crate::harnesses::destination(dest)?.interchange_document()
            else {
                anyhow::bail!("document currently supports only an OpenCode destination");
            };
            let source = match (flags.from_file.as_deref(), flags.all_branches) {
                (Some(path), false) => {
                    crate::hop::export_path(source_harness, std::path::Path::new(path))
                }
                (Some(path), true) => {
                    crate::hop::export_path_all_branches(source_harness, std::path::Path::new(path))
                }
                (None, all) => {
                    let id = operands.get(1).context("document needs a session id")?;
                    let source = SessionRef::id(source_harness, *id);
                    if all {
                        crate::hop::export_all_branches(&source, &roots)
                    } else {
                        crate::hop::export(&source, &roots)
                    }
                }
            }
            .map_err(anyhow::Error::new)?;
            anyhow::ensure!(
                source
                    .conversation()
                    .any(|entry| matches!(entry.kind, crate::model::Kind::Step { .. })),
                "noCompleteTurn"
            );
            let mut opts = flags.write_opts(dest, &source, &destination_roots, None)?;
            opts.tool_map
                .extend(interchange.source_tool_map(source_harness, &source));
            let mut clock = crate::policy::Clock::new(opts.now.as_deref());
            let plan = crate::writer::plan::plan(&source, dest, &opts);
            let violations = crate::strict::check_plan(&plan, opts.dest_protocol.as_ref());
            anyhow::ensure!(
                violations.is_empty(),
                "the document would violate the provider contract: {}",
                violations.join("; ")
            );
            let (document, native_compaction) = interchange.document(
                &source,
                &plan,
                opts.cwd
                    .as_deref()
                    .context("a working directory is required")?,
                &opts,
                &mut clock,
            )?;
            println!(
                "{}",
                json!({
                    "document": document,
                    "native_compaction": native_compaction.map(|boundary| json!({
                        "keep_id": boundary.keep_id,
                        "summary_chars": boundary.summary.chars().count(),
                        "estimated_tokens": boundary.estimated_tokens,
                        "projected_tokens": boundary.projected_tokens,
                        "summarized_messages": boundary.summarized_messages,
                        "summarized_user_messages": boundary.summarized_user_messages,
                        "budget": boundary.budget,
                        "context_window": opts.native_budget.as_ref().map(|budget| budget.context),
                        "fits": boundary.fits,
                    })),
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
                })
            );
        }
        "hop" => {
            no_more(operands, 2)?;
            let dest: HarnessId = operands
                .get(1)
                .context("hop needs a destination harness")?
                .parse()?;
            crate::harnesses::destination(dest)?;
            flags.check_write_flags(dest)?;
            let selection = flags.compaction(harness)?;
            // The writer and optional resume use the destination's environment, or the
            // installation --dest-root names, never the source override. A --dest-path is held to
            // that store before the source is read: a refused hop runs nothing, a summarizing
            // model included.
            let (destination_roots, engine) = hop_destination(&flags, dest, destination_roots)?;
            flags.check_dest_path(dest, &destination_roots)?;
            // The source is read once, by the same path the library uses: a session that is not
            // there is refused (exit 2), one that cannot be read needs recovery (exit 5).
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
            let mut opts = flags.import_opts(
                harness,
                dest,
                &source,
                &destination_roots,
                engine.as_deref(),
            )?;
            // Compaction (including a source-model summary) uses the source store.
            let (selected, summary) =
                crate::hop::compact(&source, &selection, &roots).map_err(anyhow::Error::new)?;
            opts.summary = summary;
            let result = if flags.children {
                crate::children::import_with_children(dest, &selected, &source, &roots, &opts)
            } else {
                crate::hop::import(dest, &selected, &opts)
            };
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
    let mut roots = StoreRoots::default();
    for harness in crate::harnesses::all() {
        let id = harness.id();
        *harness.store_root_mut(&mut roots) = std::env::var_os(crate::catalog::store_env(id))
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(|| crate::catalog::store_default_with(id, &vars))
            .map(|path| harness.resolve_store_path(path, &vars));
    }
    roots
}

fn store_path(harness: HarnessId, roots: &StoreRoots) -> Option<&std::path::Path> {
    crate::harnesses::get(harness).store_root(roots)
}

fn store_env_name(harness: HarnessId) -> &'static str {
    crate::catalog::store_env(harness)
}

/// The harness whose catalog `binary.flag` is `arg`.
fn exe_flag_harness(arg: &str) -> Option<HarnessId> {
    HarnessId::ALL
        .into_iter()
        .find(|harness| crate::catalog::binary_flag(*harness) == arg)
}

/// A harness's place in [`Flags::exes`].
fn exe_slot_index(harness: HarnessId) -> usize {
    HarnessId::ALL
        .iter()
        .position(|candidate| *candidate == harness)
        .expect("every harness is in HarnessId::ALL")
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
    let explicit = std::env::var_os(store_env_name(harness)).is_some_and(|value| !value.is_empty());
    let origin = if explicit { "environment" } else { "default" };
    let missing = || HarnessStoreInfo {
        path: path.clone(),
        origin,
        state: "not found".into(),
        sessions: "unknown".into(),
        forks: if !crate::catalog::lineage_recorded(harness) {
            "unknown (not recorded)".into()
        } else {
            "unknown".into()
        },
    };
    let Some(path) = path.clone() else {
        return missing();
    };
    if crate::harnesses::get(harness).store_is_in_memory(&path) {
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
    let recognized = detections
        .iter()
        .any(|detection| detection.harness == harness);
    if !recognized && !explicit {
        return HarnessStoreInfo {
            path: Some(path),
            origin,
            state: "present, layout not recognized".into(),
            sessions: "unknown".into(),
            forks: if !crate::catalog::lineage_recorded(harness) {
                "unknown (not recorded)".into()
            } else {
                "unknown".into()
            },
        };
    }
    let stats = crate::harnesses::get(harness).store_stats(roots);
    let oneline = |error: &str| error.replace(['\r', '\n'], " ");
    let state = match &stats.sessions {
        Ok(_) => "recognized".to_string(),
        Err(error) => format!("recognized, read error: {}", oneline(error)),
    };
    let sessions = stats
        .sessions
        .map(|count| count.to_string())
        .unwrap_or_else(|_| "unknown".into());
    let forks = match stats.forks {
        Some(Ok(count)) => count.to_string(),
        Some(Err(error)) => format!("unknown ({})", oneline(&error)),
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
        println!(
            "  layout:   {}",
            crate::catalog::layout(harness).join(" | ")
        );
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
        Err(HopError::ChildWriteFailed { reason, receipt }) => {
            let written = (*receipt).clone();
            (
                written,
                Some(HopError::ChildWriteFailed { reason, receipt }),
            )
        }
        Err(other) => return Err(anyhow::Error::new(other)),
    };
    // One line per subagent session written, so a person sees where each went.
    for child in receipt.manifest["children"]
        .as_array()
        .into_iter()
        .flatten()
    {
        eprintln!(
            "child {} -> {} ({})",
            child["source_id"].as_str().unwrap_or("?"),
            child["dest_id"].as_str().unwrap_or("?"),
            match child["link"].as_str() {
                Some("native") => "linked to its parent",
                _ => "not linked: the destination has no link",
            }
        );
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
    interrupted: Option<String>,
    expect_sha256: Option<String>,
    reasoning: Option<String>,
    compaction: Option<String>,
    keep_turns: Option<String>,
    context_window: Option<String>,
    max_output_tokens: Option<String>,
    text_token_factor: Option<String>,
    intent: Option<String>,
    summary_model: Option<String>,
    summary_timeout: Option<String>,
    resume_prompt: Option<String>,
    from_file: Option<String>,
    all_branches: bool,
    archived: Option<String>,
    children: bool,
    manifest: Option<String>,
    output: Option<String>,
    root: Option<String>,
    dest_root: Option<String>,
    depth: Option<String>,
    /// The `--<harness>-exe` values, in [`HarnessId::ALL`] order.
    exes: [Option<String>; HarnessId::ALL.len()],
}

impl Flags {
    /// `--interrupted`: whether the work of the turns the source never finished travels.
    fn interrupted(&self) -> Result<crate::writer::Interrupted> {
        match self.interrupted.as_deref() {
            None | Some("keep") => Ok(crate::writer::Interrupted::Keep),
            Some("drop") => Ok(crate::writer::Interrupted::Drop),
            Some(other) => anyhow::bail!("unknown --interrupted {other} (keep, drop)"),
        }
    }

    /// Everything an import needs, from the flags plus the destination's own configuration.
    fn import_opts(
        &self,
        source_harness: HarnessId,
        dest: HarnessId,
        session: &crate::model::Session,
        roots: &StoreRoots,
        engine: Option<&str>,
    ) -> Result<ImportOpts> {
        let write = self.write_opts(dest, session, roots, engine)?;
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
                interrupted: write.interrupted,
                now: write.now,
                exe: write.exe,
            },
            resume: self
                .resume_prompt
                .clone()
                .map(crate::hop::ResumePrompt::new),
            compaction: self.compaction.clone().unwrap_or_else(|| "none".into()),
            summary: None,
            native_budget: match self.compaction.as_deref() {
                Some("native") => Some(self.native_budget()?),
                _ => None,
            },
        })
    }

    /// The destination's own rules ([`crate::harnesses::Destination::validate_write`]) on the reserved
    /// id, the agent and the metadata the flags ask for, before a store is read or a process
    /// started. The metadata file is read later: here only its presence counts.
    fn check_write_flags(&self, harness: HarnessId) -> Result<()> {
        crate::harnesses::destination(harness)?.validate_write(&WriteOpts {
            dest_id: self.dest_id.clone(),
            dest_agent: self.dest_agent.clone(),
            metadata: self
                .metadata_file
                .as_ref()
                .map(|_| serde_json::Value::Object(serde_json::Map::new())),
            ..WriteOpts::default()
        })
    }

    /// `--dest-path` held to the destination's own rules in `roots`, the store the write goes to:
    /// the refusal the import gives (exit 2), given before the source is read.
    fn check_dest_path(&self, harness: HarnessId, roots: &StoreRoots) -> Result<()> {
        let Some(path) = self.dest_path.as_deref() else {
            return Ok(());
        };
        crate::harnesses::destination(harness)?
            .validate_write(&WriteOpts {
                roots: roots.clone(),
                dest_path: Some(std::path::PathBuf::from(path)),
                ..WriteOpts::default()
            })
            .map_err(|error| anyhow::Error::new(HopError::Unsupported(format!("{error:#}"))))
    }

    /// `engine` is the executable the destination installation keeps beside its store: an
    /// explicit `--<harness>-exe` wins over it, and it wins over `MUKADE_*_EXE` and `PATH`.
    fn write_opts(
        &self,
        harness: HarnessId,
        session: &crate::model::Session,
        roots: &StoreRoots,
        engine: Option<&str>,
    ) -> Result<WriteOpts> {
        self.check_write_flags(harness)?;
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
                anyhow::ensure!(
                    info.is_file() && info.len() <= 1_048_576,
                    "tool map must be a regular file no larger than 1 MiB"
                );
                let entries: Vec<(String, String, serde_json::Value, serde_json::Value)> =
                    serde_json::from_slice(&std::fs::read(file)?)
                        .with_context(|| format!("parse tool map {file}"))?;
                anyhow::ensure!(
                    entries.len() <= 4096
                        && entries
                            .iter()
                            .all(|(source, destination, original, mapped)| !source.is_empty()
                                && !destination.is_empty()
                                && source.len() <= 256
                                && destination.len() <= 256
                                && original.is_object()
                                && mapped.is_object()),
                    "invalid tool map"
                );
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
        let cwd = self.cwd.clone().or_else(|| session.meta.cwd.clone());
        let project = cwd.as_deref().map(std::path::Path::new);
        // The flag, else the shell the destination's own configuration or platform names.
        let dest_shell = match self.dest_shell.as_deref() {
            Some(name) if name.trim().eq_ignore_ascii_case("none") => None,
            Some(name) => match crate::writer::plan::shell_of(name) {
                Some(shell) => Some(shell),
                None => anyhow::bail!(
                    "unknown --dest-shell {name} (bash, sh, zsh, dash, ksh, posix, powershell, pwsh, none)"
                ),
            },
            None => crate::dest::shell_in(harness, roots, project),
        };
        let offered = crate::tools::offered_in(harness, roots, project);
        // What the *destination* is configured to use, when the caller did not state it. OpenCode's
        // interchange format refuses a model reference without a provider, and Codex stamps the
        // provider its own config names, so both are read from the destination's own files.
        let configured = crate::dest::configured_model_in(harness, roots);
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
        let stated_exe = self.exe_flag(harness).filter(|v| !v.is_empty()).or(engine);
        let exe_override = stated_exe.map(str::to_string).or_else(|| {
            std::env::var(crate::exe::env_var_name(harness))
                .ok()
                .filter(|v| !v.is_empty())
        });
        let resolved_exe = crate::exe::resolve(harness, stated_exe);
        // The destination's own version stamp, when the caller did not state it: L7 wants the
        // destination's `cli_version`, and Codex refuses a rollout without one.
        let dest_version = self.dest_version.clone().or_else(|| {
            let _span = crate::timing::span("probe destination version");
            crate::exe::version_of(&resolved_exe)
        });
        // OpenCode's interchange format requires the destination's own project id; the destination
        // is the only one that knows it, so it is asked.
        let project_id = match self.project_id.clone() {
            Some(id) => Some(id),
            None => match cwd.as_deref() {
                Some(cwd) => destination_project_id(harness, &resolved_exe, cwd, roots)?,
                None => None,
            },
        };
        Ok(WriteOpts {
            roots: StoreRoots::from_env(),
            cwd,
            dest_version,
            dest_protocol,
            dest_shell,
            offered: Some(offered),
            model_provider,
            model,
            project_id,
            dest_id: self.dest_id.clone(),
            dest_path: self.dest_path.as_ref().map(std::path::PathBuf::from),
            dest_agent: self.dest_agent.clone(),
            metadata,
            tool_map,
            without_tools: self.without_tools,
            interrupted: self.interrupted()?,
            reasoning,
            title: session.meta.title.clone(),
            now: None,
            exe: exe_override,
            native_budget: match self.compaction.as_deref() {
                Some("native") => Some(self.native_budget()?),
                _ => None,
            },
            subagents: Default::default(),
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
            Some("native") => self.native_budget().map(Compaction::Native),
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
                if self.from_file.is_some() && !crate::harnesses::get(source).summarizes_from_file()
                {
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
            Some(other) => {
                anyhow::bail!("unknown --compaction {other} (none, portable, summary, native)")
            }
        }
    }

    /// The destination-model facts an OpenCode-native boundary needs. The caller supplies them;
    /// Mukade never reads another product's catalog and never guesses a window.
    fn native_budget(&self) -> Result<NativeBudget> {
        let context = match self.context_window.as_deref() {
            Some(value) => value
                .parse::<u64>()
                .with_context(|| format!("--context-window {value}: not a number of tokens"))?,
            None => anyhow::bail!("--context-window is required with --compaction native"),
        };
        anyhow::ensure!(context > 0, "--context-window must be greater than zero");
        let max_output_tokens =
            match self.max_output_tokens.as_deref() {
                Some(value) => Some(value.parse::<u64>().with_context(|| {
                    format!("--max-output-tokens {value}: not a number of tokens")
                })?),
                None => None,
            };
        let text_token_factor = match self.text_token_factor.as_deref() {
            Some(value) => {
                let factor = value
                    .parse::<f64>()
                    .with_context(|| format!("--text-token-factor {value}: not a number"))?;
                Some(
                    crate::native_compaction::TokenFactor::new(factor)
                        .map_err(anyhow::Error::msg)?,
                )
            }
            None => None,
        };
        Ok(NativeBudget {
            context,
            max_output_tokens,
            text_token_factor,
        })
    }

    /// The executable override stated for one harness, if any.
    fn exe_flag(&self, harness: HarnessId) -> Option<&str> {
        self.exes[exe_slot_index(harness)].as_deref()
    }

    fn exe_slot(&mut self, harness: HarnessId) -> &mut Option<String> {
        &mut self.exes[exe_slot_index(harness)]
    }
}

/// Resolve a `--root` or `--dest-root` to the installation it names. A path that names nothing is
/// still a store when the harness is stated (a destination store can legitimately be empty);
/// without a harness, nothing recognized is refused, and so is a choice between several.
fn locate_root(
    root: &str,
    harness: Option<HarnessId>,
    flag: &str,
) -> Result<Option<crate::detect::Installation>> {
    use crate::detect::Located;
    match crate::detect::locate_installation(std::path::Path::new(root), harness) {
        Located::One(found) => Ok(Some(found)),
        Located::Nothing if harness.is_some() => Ok(None),
        Located::Nothing => {
            anyhow::bail!("no harness store recognized in {root}: state the harness explicitly")
        }
        Located::Several(found) => anyhow::bail!(
            "{root} holds more than one harness installation: state the harness, or pass one of \
             these stores as {flag}:\n{}",
            found
                .iter()
                .map(|found| format!("  {} {}", found.harness.as_str(), found.store.display()))
                .collect::<Vec<_>>()
                .join("\n")
        ),
    }
}

/// Where a hop writes: the destination's environment, or the installation `--dest-root` names,
/// with the engine kept beside its store.
fn hop_destination(
    flags: &Flags,
    dest: HarnessId,
    mut roots: StoreRoots,
) -> Result<(StoreRoots, Option<String>)> {
    let Some(dest_root) = flags.dest_root.as_deref() else {
        return Ok((roots, None));
    };
    let found = locate_root(dest_root, Some(dest), "--dest-root")?;
    let store = found.as_ref().map_or_else(
        || std::path::PathBuf::from(dest_root),
        |found| found.store.clone(),
    );
    roots.set_root(dest, &store);
    let engine = found
        .and_then(|found| found.exe)
        .or_else(|| crate::detect::engine_beside_store(dest, &store))
        .map(|exe| exe.to_string_lossy().into_owned());
    Ok((roots, engine))
}

/// One installation as `scan` prints it.
fn installation_json(found: &crate::detect::Installation) -> serde_json::Value {
    json!({
        "harness": found.harness.as_str(),
        "store": found.store.display().to_string(),
        "exe": found.exe.as_ref().map(|exe| exe.display().to_string()),
        "evidence": found.evidence,
    })
}

/// The project the destination already files sessions of this directory under, for a harness
/// that files them under one. The destination is asked from **inside that directory**: the CLI
/// scopes its session list to the project it is run in, so the question has to be asked where the
/// session will live.
fn destination_project_id(
    harness: HarnessId,
    exe: &str,
    cwd: &str,
    roots: &StoreRoots,
) -> Result<Option<String>> {
    crate::harnesses::destination(harness)?.project_id(
        exe,
        cwd,
        roots,
        crate::cache::dir().as_deref(),
    )
}

#[cfg(test)]
mod tests {
    use crate::model::HarnessId;
    use crate::roots::StoreRoots;

    /// The project of `cwd` in the OpenCode adapter, with the cache folder stated.
    fn opencode_project_id_in(
        exe: &str,
        cwd: &str,
        roots: &StoreRoots,
        cache: Option<&std::path::Path>,
    ) -> Option<String> {
        crate::harnesses::destination(HarnessId::OpenCode)
            .unwrap()
            .project_id(exe, cwd, roots, cache)
            .unwrap()
    }

    /// The project id OpenCode files a directory under takes one process start per session it has
    /// to look at: it is asked once per binary, database and directory.
    #[test]
    fn the_opencode_project_of_a_directory_is_asked_once() {
        let dir = crate::scratch::Scratch::new("project");
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
    }
}
