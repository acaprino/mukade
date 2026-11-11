# Mukade — harness sessions manager

Mukade moves a coding-agent session between **Claude Code, Codex CLI, OpenCode and Pi**, and the destination continues it as one of its own sessions.

<p align="center">
  <img src="docs/assets/branding/mukade-logo-oni.png" alt="Mukade from Oni beside the Mukade wordmark" width="480">
</p>

## What works, per harness

| | Claude Code | Codex CLI | OpenCode | Pi |
|---|:-:|:-:|:-:|:-:|
| `list` | ✅ | ✅ | ✅ | ✅ |
| `list` also shows sessions whose transcript is gone | ✅ | ✅ | ✅ | — |
| `dump` (export to a document) | ✅ | ✅ | ✅ | ✅ |
| `write` (import a document) | ✅ | ✅ | ✅ | ✅ |
| `hop` source / destination | ✅ / ✅ | ✅ / ✅ | ✅ / ✅ | ✅ / ✅ |
| `--compaction summary` (source's own model) | ✅ | ✅ ¹ | ✅ | ✅ |
| `--compaction native` (destination's own record) | ✅ | ✅ | ✅ | ✅ |
| `--all-branches` (abandoned turns too) | — | ✅ | — | — |
| TUI: browse, resume, hop, export, import | ✅ | ✅ | ✅ | ✅ |
| `live` (is it running now?) | ✅ ² | — | — | — |

¹ Checked against a mock only. ² Windows and Linux verified; macOS does not compare the start time.
Codex details (code mode, `apply_patch`, subagents): [How a hop works](#how-a-hop-works). Open checks: [next.md](next.md) (status board) and the [Windows checklist](docs/verification/windows-checklist.md) (step by step).

## Quick start

1. **Build** (needs a Rust toolchain):

   ```text
   git clone https://github.com/acaprino/mukade
   cd mukade
   build.bat            # Windows; on Linux: sh build.sh
   ```

   You get `target/release/mukade` and `target/release/mukade-tui` (`.exe` on Windows). Copy them anywhere.

2. **Point mukade at both stores** (PowerShell; on Linux or macOS use `export NAME=value`):

   ```powershell
   $env:CLAUDE_CONFIG_DIR   = Join-Path $HOME '.claude'
   $env:PI_CODING_AGENT_DIR = Join-Path $HOME '.pi\agent'
   ```

3. **See what is there:**

   ```text
   mukade harnesses           # what is installed, where its sessions are
   mukade list claude_code    # the sessions, one JSON line each
   ```

4. **Hop one session** into Pi:

   ```powershell
   mukade hop claude_code YOUR_CLAUDE_SESSION_ID pi --cwd 'C:\work\my-project' --manifest hop-manifest.json
   ```

5. **Read the receipt** it prints: `id` is the **new id**, `resume` is **how to open it** (run it from the project folder), `manifest` lists **what did not travel**. The source session is **never** changed.

Prefer a window? `mukade-tui`: `h` hop · `e` export · `i` import · `R` resume.

Use cases, per-harness examples and troubleshooting: the [usage guide](docs/usage.md).

## Library

mukade is a library first and a command line second: a task runner that hosts these harnesses wants the pipeline in-process, not behind a subprocess.

```text
cargo add mukade --git https://github.com/acaprino/mukade   # not on crates.io yet
cargo add serde_json                                        # used by the first example
```

**One hop, whole:**

```rust
use mukade::hop::{HopRequest, SessionRef, WriteDetails};
use mukade::model::HarnessId;
use mukade::roots::StoreRoots;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let receipt = mukade::hop(&HopRequest {
        source: SessionRef::id(HarnessId::ClaudeCode, "c45db7bc-542c-4201-89de-b590c6fdb797"),
        dest: HarnessId::Pi,
        cwd: None,                      // None: the project folder the source recorded
        compaction: Default::default(), // Compaction::None: the whole history
        resume: None,                   // Some(ResumePrompt) also runs the destination
        reasoning: Default::default(),  // ReasoningPolicy::DropForeign
        roots: StoreRoots::from_env(),  // the harnesses' own store variables
        write: WriteDetails::default(),
    })?;

    println!("{} {:?}", receipt.dest_id, receipt.resume_command);
    println!("{}", serde_json::to_string_pretty(&receipt.manifest)?);
    Ok(())
}
```

**Options:** native compaction, a resume prompt, interrupted work left out.

```rust
use mukade::hop::{Compaction, HopRequest, ResumePrompt, SessionRef, WriteDetails};
use mukade::model::HarnessId;
use mukade::native_compaction::NativeBudget;
use mukade::roots::StoreRoots;
use mukade::writer::Interrupted;
use std::time::Duration;

pub fn request() -> HopRequest {
    HopRequest {
        source: SessionRef::id(HarnessId::Codex, "YOUR_CODEX_THREAD_ID"),
        dest: HarnessId::Pi,
        cwd: Some(r"C:\work\my-project".into()),
        // The whole history, then Pi's own compaction record (spec §4.4). No model is called.
        compaction: Compaction::Native(NativeBudget {
            context: 200_000,        // the destination model's window, in tokens
            max_output_tokens: None, // None: 16000
            text_token_factor: None, // None: chars / 3
        }),
        // Runs Pi on the written session and keeps its reply (default timeout: 300 s).
        resume: Some(ResumePrompt::new("where were we?").with_timeout(Duration::from_secs(120))),
        reasoning: Default::default(),
        roots: StoreRoots::from_env(),
        write: WriteDetails {
            interrupted: Interrupted::Drop, // default: Interrupted::Keep
            ..Default::default()
        },
    }
}
```

`Interrupted::Drop` leaves out the work of the turns the source never finished (killed, or interrupted by the user) and keeps those turns' prompts. The manifest then states `options.interrupted`, counts `dropped_interrupted_steps` and names the loss. `Session::interrupted_work()` gives the same answer without writing anything: one flag per entry, set on the steps and results of the interrupted turns.

**The two halves, through the document `mukade dump` and `mukade write` use:**

```rust
use mukade::hop::{self, ImportOpts, SessionRef, SourceFacts, WriteDetails};
use mukade::model::HarnessId;
use mukade::roots::StoreRoots;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let roots = StoreRoots::from_env();
    let source = SessionRef::id(HarnessId::Codex, "YOUR_CODEX_THREAD_ID");
    let session = mukade::export(&source, &roots)?;
    let work = session.interrupted_work(); // one flag per entry; nothing is written

    let doc = hop::document(&session, None); // what `mukade dump` prints
    let session = hop::read_document(&doc)?; // what `mukade write` reads

    let opts = ImportOpts {
        cwd: None,
        roots,
        reasoning: Default::default(),
        write: WriteDetails::default(),
        resume: None,
        source_facts: SourceFacts::of(&session),
        compaction: "none".into(),
        summary: None,
        native_budget: None,
    };
    let receipt = mukade::import_document(HarnessId::Pi, &doc, &session, &opts)?;
    println!("{} ({} interrupted entries)", receipt.location, work.iter().filter(|w| **w).count());
    Ok(())
}
```

**Entry points** (module `mukade::hop`; `hop`, `hop_into`, `export`, `import`, `import_document` are also at the crate root):

| call | does |
|---|---|
| `hop(&HopRequest)` | the whole pipeline: read, compact, plan, check, write, report |
| `hop_into(&HopRequest, &StoreRoots)` | the same, the destination written into stores of its own (two installations of one harness cannot share one `StoreRoots`) |
| `export(&SessionRef, &roots)` · `export_path(harness, &path)` | read a session from its store · from a file |
| `export_all_branches` · `export_path_all_branches` | the same, with every branch the source keeps (Codex only) |
| `import(dest, &session, &ImportOpts)` | write a session you already hold |
| `document` · `read_document` · `import_document` | the JSON document `mukade dump` and `mukade write` use |
| `compact(&session, &Compaction, &roots)` | the session a compaction policy hands to the writer, with the summary the source's model wrote when one is asked for (`summary::summarize` runs it) |

`Receipt` carries `dest_id`, `location`, `resume_command` (the native resume command), `manifest` and `resume_output` (the destination's reply, when a resume prompt was given). When the write succeeds but the resume fails, `HopError::ResumeFailed` holds the written session's receipt and manifest, so a caller can recover without blindly creating another session.

**Harness processes:**

| call | does |
|---|---|
| `exe::resolve(harness, override)` | the binary to spawn: explicit override → `MUKADE_*_EXE` → `PATH` → well-known install locations → the bare command name |
| `detect::detect_store(dir)` | which harness stores a directory holds, with the evidence |
| `detect::scan_installations(dir, depth)` | every installation below a folder (`mukade scan`) |
| `exe::run_within(&mut command, timeout)` | runs a harness process with a deadline |

`WriteDetails::exe` carries a destination-side override, which the resume command also names. Every harness process mukade starts runs with a deadline, except the interactive resume of `mukade-tui`. At the deadline mukade stops the process **with everything it started** (an MCP server, a shell):

- Windows: `taskkill /T`.
- Linux and macOS: by walking the process tree (`/proc`, or `ps`).
- A process that detached itself from the tree is not found.
- mukade stops waiting for output pipes one second after the process exits.
- A timeout too large for the clock sets no deadline: mukade waits for the process to exit.

## Command line

| command | prints |
|---|---|
| `mukade harnesses` | text for a person, no options: each harness's executable, store, version, session/fork counts and lineage support. Store reads are read-only; discovered executables are asked `--version` |
| `mukade list <harness> [options]` | one JSON line per session: `id`, `title`, `cwd`, `timestamp`, `location`, `archived`, `unreadable`, `live` ([the list output](docs/usage.md#the-list-output)) |
| `mukade intents` | the editable summary prompts (name, path, builtin/custom) as JSON; reads no session, starts no harness |
| `mukade dump <harness> <session-id> [options]` | the interchange document on stdout, or in `--output <file>` (UTF-8) |
| `mukade document opencode <src-harness> <session-id> [options]` | the OpenCode import document a hop would write, with its counts and losses; writes nothing |
| `mukade write <harness> <document.json> [options]` | a checked write: the receipt (id, location, resume, manifest) |
| `mukade hop <src-harness> <session-id> <dst-harness> [options]` | read and write in one process: the receipt |
| `mukade detect <dir>` | which harness stores the directory holds, with the evidence (JSON) |
| `mukade scan <dir> [--depth <n>]` | every installation below the directory as JSON (default depth 8), portable and custom ones included, with the executable kept beside each store |

Harness names: `claude_code` | `codex` | `opencode` | `pi`. `mukade --help` prints every option.

**`list` sees every project.** It also reports the sessions a harness still names after it lost their transcripts:

- Codex threads that only remain as rows in `state_<n>.sqlite`;
- Claude Code sessions that only remain as prompts in `history.jsonl`;
- OpenCode sessions in the other `opencode*.db` files beside the selected one.

Those rows carry an `unreadable` reason, and `dump`/`hop` refuse them (exit 2). `live` is `true` for a Claude Code session whose process is running. `--archived false` leaves out the sessions the harness marked archived (default `true`; the same for every harness).

The binaries' install locations, each harness's default store on Windows, macOS and Linux, the store layouts and the lineage each records are one data file, [`catalog/harnesses.json`](catalog/harnesses.json) ([harness catalog](docs/reference/harness-catalog.md)). It is embedded in the executable, and `mukade harnesses` reads it.

### Stores

A `hop` reads the source and writes the destination in one process: set **both** harnesses' store variables.

| harness | variable | root |
|---|---|---|
| Claude Code | `CLAUDE_CONFIG_DIR` | explicit root required |
| Codex CLI | `CODEX_HOME` | explicit root required |
| OpenCode | `OPENCODE_DB`, with its `XDG_*` | listing requires `OPENCODE_DB` or `--root` and opens the database read-only; other operations can use OpenCode's configured store |
| Pi | `PI_CODING_AGENT_DIR` | explicit root required |

An empty variable is no root. Every OpenCode command that touches the store (`session export`, `import`, `run`, or a project probe) carries `--standalone`: without it, its CLI hands the work to a background service started with the user's own environment. An explicit `OPENCODE_DB` is passed to those commands. Temporary OpenCode import documents are removed on success and on failure.

**Naming a store per command:**

- `--root <dir>` (non-empty) points one harness's root at that directory: for a hop, the source's. For OpenCode, a directory means its `opencode.db`.
- With `--root`, the harness can be omitted from `list`, `dump` and a `hop` source. It is inferred from the directory's own layout when exactly one harness is recognized, and refused when none or several are. `mukade detect <dir>` prints the same answer with its evidence.
- `--dest-root <dir>` names a hop's destination store or installation folder; its executable writes the session unless `--<harness>-exe` names another.
- `--root` and `--dest-root` accept a folder `mukade scan` walks, when it holds exactly one installation (of the harness named, when one is).

**Detection needs concrete files:** session records (`projects/*/*.jsonl`, `sessions/**/rollout-*.jsonl`, `sessions/--*--/*.jsonl`), Codex's `session_index.jsonl`, Pi's `models.json`, or OpenCode's `opencode.db` (that file can also be passed directly). An empty directory tree infers nothing. Another harness's SQLite state (Codex's `state_*.sqlite`) is not OpenCode.

### Executables

Resolution: `--claude-exe` / `--codex-exe` / `--opencode-exe` / `--pi-exe` → `MUKADE_CLAUDE_EXE` / `MUKADE_CODEX_EXE` / `MUKADE_OPENCODE_EXE` / `MUKADE_PI_EXE` → `PATH` → the well-known install locations → the plain command name. So an install kept in its own folder, with its own config, can be selected explicitly. An empty flag falls back to the default resolution.

- For a hop, set the destination's store variable in the environment; `--root` selects the source store.
- A `--resume-prompt` runs the destination with its store variable set to the root the session was written to. A resume command you run later by hand needs that variable set too.
- The executable override also names the binary in the receipt's resume command. Without one, the command names the bare binary, which mukade itself looks up on `PATH` (`.cmd` launchers included) when it runs it.
- For OpenCode as a source, select a custom executable with `MUKADE_OPENCODE_EXE`.

### Options worth knowing

| option | on | effect |
|---|---|---|
| `--interrupted keep\|drop` | `hop`, `write`, `document` | `drop` leaves out what the agent did in the turns the source never finished (killed, or interrupted by the user) and keeps their prompts. `keep` (default) carries it, so a session with no finished turn is still exported whole. The manifest states the mode and counts `dropped_interrupted_steps` ([interrupted turns](docs/usage.md#a-session-with-interrupted-turns)) |
| `--without-tools` | `hop`, `write`, `document` | omits recorded tool calls and results; the manifest counts the omissions |
| `--all-branches` | `dump`, `hop` | a Codex thread whole, the abandoned turns included |
| `--expect-sha256 <hash>` | `hop` | refuses a source whose captured fingerprint differs from the caller's. It compares the complete source selected by that read; it does not freeze an earlier prefix of a file that has since grown |
| `--dest-id <id>` | `hop`, `write`, `document` | a reserved destination session ID. Into OpenCode, it is checked in the explicitly selected database after the import |
| `--dest-agent <name>`, `--metadata-file <path>` | OpenCode destination only | its primary agent; a JSON object stored as session metadata, checked in the explicitly selected database after the import |

```powershell
mukade hop codex YOUR_CODEX_THREAD_ID pi --cwd 'C:\work\my-project' --interrupted drop
```

### When a resume or an import goes wrong

- A destination write that succeeds with a failed resume: the receipt is printed on stdout and `--manifest` saved before mukade exits with code 5. Once a session is written, its receipt is printed even when the manifest file cannot be saved.
- The resume runs from the session's working directory. A timeout stops the harness, including one started through a Windows `.cmd` launcher.
- A resume prompt that starts with `-`, or one with line breaks for a `.cmd` launcher, is refused before anything is written.
- An external import whose outcome cannot be confirmed returns code 5 and names the reserved ID: reconcile it before retrying.

### Exit codes

| code | meaning | `HopError` |
|---|---|---|
| 0 | done | — |
| 1 | a command-line error, or any other error that is not a `HopError` | — |
| 2 | refused with a reason, nothing written: the session is not there, the request is not supported, the `--expect-sha256` fingerprint differs | `NotFound`, `Unsupported`, `SourceChanged` |
| 4 | failed and nothing changed: the plan would be refused by a provider, the write failed, the summary failed | `StrictCheckFailed`, `WriteFailed`, `SummaryFailed` |
| 5 | failed and recovery is needed: the source could not be read, an external import has an uncertain outcome, the resume failed after the write | `VerifyFailed`, `WriteUncertain`, `ResumeFailed` |

## How a hop works

No proxy in the middle, no re-prompting the history, no third-party converter.

| step | what happens |
|---|---|
| read | the source's own store is parsed into one model, keeping every original record verbatim (`Entry::raw`) |
| plan | tool calls are translated to what the destination actually offers; what cannot be translated travels as text, never dropped |
| check | the plan is held against the provider rules that make a real API refuse a request; nothing is written if it would be refused |
| write | the destination's native files: Claude's `projects/<dir>/<uuid>.jsonl`, Codex's `rollout-*.jsonl`, OpenCode's document plus `session import`, Pi's `sessions/--<dir>--/<stamp>_<uuid>.jsonl` |
| report | a manifest states the source's fingerprint, the destination, the counts and **every loss** |

**Three rules hold everywhere:**

- The destination session is always **new**; the source is never touched.
- The order is the harness's **native order**, never timestamps.
- **No loss is silent:** a dropped orphan result, a reasoning block left behind, a tool call written as text, an abandoned branch of a rewound session all appear in the manifest.

A session that branched (a Claude Code rewind, Pi's `/tree`) travels as the branch the source harness itself would resume.

**Claude Code histories** are rebuilt from the selected branch and its dependencies, even when compaction, split responses or parallel tool results were flushed out of file order. Records without a UUID are recovered when their response, call or compaction boundary identifies them unambiguously. Missing parents, cycles, conflicting identifiers and records that cannot be placed safely are refused, not guessed. Automatic compaction and rewind on real sources remain unverified.

**Codex sessions:**

- Code-mode `exec` scripts are read as the calls they wrap (L10): 725 of 981 real scripts ([census](docs/verification/2026-11-11-codex-code-mode.md)). The other 256 (variables, `Promise.allSettled`, `ALL_TOOLS`) stay one `exec` call in text form.
- `apply_patch` becomes a write (one added file), an edit (one hunk) or a multi-edit toward Pi (several hunks). Several files, a delete or a move travel as text.
- A subagent thread records its parent (`meta.parent`) but travels as its own session.

## Compaction and reasoning between hops

| `--compaction` | the destination gets | library |
|---|---|---|
| `none` (default) | the whole selected history | `Compaction::None` |
| `portable` | the handoff line and the source's own summary, then the tail from the boundary the source's compaction recorded | `Compaction::Portable` |
| `summary` | a handoff summary the source harness's own model writes now, with a prompt of mukade's own (`--intent <name>`), in a run that saves nothing in the source's store | `Compaction::Portable` with `SummarySource::SourceModel` |
| `native` | the whole history plus the destination's own deterministic compaction record (spec §4.4); no model is called. Needs `--context-window <n>` | `Compaction::Native(NativeBudget)` |

- `dump` exports a `summary` and `write` imports it later.
- `--compaction native` works on `hop` and `document`, not on `dump`.
- `Compaction::DestinationNative` keeps the whole history and leaves the destination's own compaction to the caller.

**Reasoning** is a payload only its own provider protocol can replay. Each block is carried natively (the destination's protocol matches and the block still has its signature or encrypted payload), written as labelled text, or dropped. The manifest says which. `--reasoning drop_foreign` (default) | `as_text` | `drop`.

## Sessions TUI

`mukade-tui` is a separate native terminal explorer and manager for the same stores: the four harnesses and their sessions in one window, a transcript, interactive resume (`R`), hop (`h`), export to a document (`e`) and import of a document (`i`). It is built only on request, so the `mukade` CLI stays a single binary with its own dependency list.

```text
cargo run -p mukade-tui
cargo run -p mukade-tui -- --turns 20    # transcripts open on the last 20 user turns
cargo run -p mukade-tui -- --check       # harnesses and session counts, then exit
```

Keys, the hop dialog and examples: [mukade-tui/README.md](mukade-tui/README.md).

## One executable, nothing to install

`build.bat` (Windows) or `sh build.sh` (Linux) builds the whole workspace: `target/release/mukade.exe` (or `mukade`) and the terminal browser `target/release/mukade-tui.exe` beside it. Copy the executable anywhere (a USB stick included) and run it.

- The Windows executable includes the Musashi icon and links the C runtime in, so it needs no redistributable.
- There is no installer, no configuration file and no mode to switch on.

**What it writes beside the executable:**

| path | what |
|---|---|
| `intent-prompts/` | the editable summary prompts, copied on first start; an existing file is never replaced (`mukade intents` lists them) |
| `mukade-data/cache/` | the versions and OpenCode project ids learned from harness binaries, and the Claude Code listing cache |
| `mukade-data/tmp/` | the document an OpenCode import needs, and the folder of a `--compaction summary` run (Codex's last message, Pi's fork); each removed when its work ends. A killed run's leftovers are swept an hour later |
| `mukade-data/exports/` | the documents the TUI's `e` writes |

- `mukade-data` is made the first time something is written to it.
- By default nothing goes to the user profile, the application-data folders, the registry or the system's temporary folder. Delete the executable's folder and nothing is left.
- Where the folder cannot be made (an executable in a read-only place), the cache is simply not kept, and an OpenCode import or a summary says why it cannot run.
- A host can set `MUKADE_DATA_DIR` (and `MUKADE_INTENT_DIR`) to an absolute writable folder. A relative or empty override is refused, never silently redirected.
- An automated test runs the real binary with all of those places pointed at an empty sandbox and asserts it stays empty.

## Speed

| source session | hop | before the speed work |
|---|---|---|
| 1 MB | 30–35 ms | 83 ms |
| 10 MB | 185–240 ms | 556 ms |
| 30 MB | 440–630 ms | 1657 ms |

The harnesses' own stores are JSON Lines and SQLite and are read and written as they are, so the cost was never the format: it was copies, hashing and asking a harness for its version. `--timing` shows the time phase by phase on stderr. Details: [Speed](docs/usage.md#9-speed).

## Tests and fixtures

`cargo test` runs against real sessions of the four harnesses kept under `fixtures/` and writes only to scratch directories. The fixtures are recorded runs with synthetic content: no user's conversation is in this repository. `cargo test --workspace --no-fail-fast` also tests the TUI.

## Documentation

**Use it:**

- [Usage guide](docs/usage.md): setup, use cases, receipt and manifest, per-harness examples, troubleshooting.
- [mukade-tui/README.md](mukade-tui/README.md): the terminal UI.
- [next.md](next.md): what is done, what is left to verify, known limits.

**Harness pages** (what it is, install and launch, store, compaction, tools, plugins and hooks, how mukade reads and writes it):

- Supported: [Claude Code](docs/harnesses/claude-code.md), [Codex CLI](docs/harnesses/codex.md), [OpenCode](docs/harnesses/opencode.md), [Pi](docs/harnesses/pi.md).
- No support yet (mukade neither reads nor writes them): [Hermes Agent](docs/harnesses/hermes-agent.md), [DeepSeek Harness](docs/harnesses/deepseek-harness.md); read-first plan proposed in [ADR-0002](docs/adr/ADR-0002-hermes-and-deepseek-harness-read-first.md).

**Reference:**

- [Session interchange specification](docs/session-interchange-spec.md).
- [Tool conversion tables](docs/tool-conversion-tables.md): the one place that says how a tool call of one harness becomes a call of another.
- [Harness session persistence reference](docs/reference/harness-session-persistence-reference.md): secondary research (2026-10-27) on 24 harnesses' session stores, restructured on 2026-11-11. For the four supported harnesses, the harness pages and the specification come first.
- [Pi verification](docs/verification/pi-session-and-reasoning-verification.md): mukade-written sessions against Pi 1.2.0 and 1.4.0.

Everything else (verification records, decisions, archived designs): the [documentation index](docs/README.md).

## License

MIT — see [LICENSE](LICENSE).
