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
Open checks: [next.md](next.md) (status board), [Windows checklist](docs/verification/windows-checklist.md) (step by step).

## Quick start

1. **Build** (needs a Rust toolchain):

   ```text
   git clone https://github.com/acaprino/mukade
   cd mukade
   build.bat            # Windows; on Linux: sh build.sh
   ```

   You get `target/release/mukade` and `target/release/mukade-tui` (`.exe` on Windows). Copy them anywhere.

2. **Point mukade at both stores** (PowerShell; elsewhere `export NAME=value`):

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

Prefer a window? `mukade-tui`: `h` hop · `e` export · `i` import · `R` resume. More: the [usage guide](docs/usage.md).

## Library

mukade is a library first and a command line second: a task runner hosting these harnesses wants the pipeline in-process, not behind a subprocess.

```text
cargo add mukade --git https://github.com/acaprino/mukade   # not on crates.io yet
cargo add serde_json                                        # used by the example
```

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

Fields that change the result, all from the types in `mukade::hop`:

- **`compaction`**: `Compaction::None` (default, the whole history); `Compaction::Native(NativeBudget { context, max_output_tokens: None, text_token_factor: None })` (the whole history plus the destination's own compaction record, spec §4.4, no model called; `None` means 16000 and chars / 3); `Compaction::Portable` (with `SummarySource::SourceModel`, a summary the source's model writes); `Compaction::DestinationNative` (the whole history; the destination's own compaction is the caller's).
- **`resume`**: `Some(ResumePrompt::new("where were we?").with_timeout(Duration::from_secs(120)))` runs the destination on the written session and keeps its reply (default timeout 300 s).
- **`write`**: `WriteDetails { interrupted: Interrupted::Drop, ..Default::default() }` (`mukade::writer::Interrupted`; default `Keep`) leaves out the work of turns the source never finished (killed, or interrupted by the user) and keeps their prompts. The manifest states `options.interrupted`, counts `dropped_interrupted_steps` and names the loss. `Session::interrupted_work()` gives the same answer without writing: one flag per entry, set on the steps and results of those turns.

**Entry points** (module `mukade::hop`; `hop`, `hop_into`, `export`, `import`, `import_document` are also at the crate root):

| call | does |
|---|---|
| `hop(&HopRequest)` | the whole pipeline: read, compact, plan, check, write, report |
| `hop_into(&HopRequest, &StoreRoots)` | the same, into stores of its own for the destination (two installations of one harness cannot share one `StoreRoots`) |
| `export(&SessionRef, &roots)` · `export_path(harness, &path)` | read a session from its store · from a file |
| `export_all_branches` · `export_path_all_branches` | the same with every branch the source keeps (Codex only) |
| `import(dest, &session, &ImportOpts)` | write a session you already hold |
| `document(&session, None)` · `read_document(&doc)` · `import_document(dest, &doc, &session, &opts)` | the JSON document `mukade dump` prints and `mukade write` reads; `ImportOpts` carries `cwd`, `roots`, `reasoning`, `write`, `resume`, `source_facts` (`SourceFacts::of(&session)`), `compaction`, `summary`, `native_budget` |
| `compact(&session, &Compaction, &roots)` | the session a policy hands to the writer, with the source model's summary when asked for (`summary::summarize` runs it) |

`Receipt` carries `dest_id`, `location`, `resume_command`, `manifest` and `resume_output` (the destination's reply to a resume prompt). When the write succeeds but the resume fails, `HopError::ResumeFailed` holds the written session's receipt and manifest, so a caller recovers without creating another session.

**Harness processes:**

| call | does |
|---|---|
| `exe::resolve(harness, override)` | the binary to spawn (order: [usage §7](docs/usage.md#7-stores-and-executables)) |
| `detect::detect_store(dir)` | which harness stores a directory holds, with the evidence |
| `detect::scan_installations(dir, depth)` | every installation below a folder (`mukade scan`) |
| `exe::run_within(&mut command, timeout)` | runs a harness process with a deadline |

`WriteDetails::exe` is a destination-side override, which the resume command also names. Every harness process has a deadline (not `mukade-tui`'s interactive resume); at it mukade stops the process with everything it started ([usage §7](docs/usage.md#7-stores-and-executables)). It stops waiting for output pipes one second after the process exits.

## Command line

| command | prints |
|---|---|
| `mukade harnesses` | text for a person: each harness's executable, store, version, session/fork counts, lineage support (store reads are read-only; discovered executables are asked `--version`) |
| `mukade list <harness> [options]` | one JSON line per session: `id`, `title`, `cwd`, `timestamp`, `location`, `archived`, `unreadable`, `live` ([the list output](docs/usage.md#the-list-output)) |
| `mukade intents` | the editable summary prompts (name, path, builtin/custom) as JSON; no session read, no harness started |
| `mukade dump <harness> <session-id> [options]` | the interchange document on stdout, or in `--output <file>` (UTF-8) |
| `mukade document opencode <src-harness> <session-id> [options]` | the OpenCode import document a hop would write, with counts and losses; writes nothing |
| `mukade write <harness> <document.json> [options]` | a checked write: the receipt (id, location, resume, manifest) |
| `mukade hop <src-harness> <session-id> <dst-harness> [options]` | read and write in one process: the receipt |
| `mukade detect <dir>` | which harness stores the directory holds, with the evidence (JSON) |
| `mukade scan <dir> [--depth <n>]` | every installation below the directory as JSON (default depth 8), portable and custom ones included, with the executable beside each store |

Harness names: `claude_code` | `codex` | `opencode` | `pi`. `mukade --help` prints every option; which command takes which: [usage §3](docs/usage.md#3-commands). Detection needs concrete session files (listed on each harness page; an empty tree infers nothing).

**`list` sees every project**, and the sessions a harness still names after losing their transcripts: Codex threads that only remain as rows in `state_<n>.sqlite`, Claude Code sessions that only remain as prompts in `history.jsonl`, OpenCode sessions in the other `opencode*.db` files beside the selected one. Those rows carry an `unreadable` reason, and `dump`/`hop` refuse them (exit 2). `live` is `true` for a Claude Code session whose process is running. `--archived false` leaves out archived sessions (default `true`).

### Stores

A `hop` reads the source and writes the destination in one process: set **both** harnesses' variables. An empty variable is no root.

| harness | variable | root |
|---|---|---|
| Claude Code | `CLAUDE_CONFIG_DIR` | explicit root required |
| Codex CLI | `CODEX_HOME` | explicit root required |
| OpenCode | `OPENCODE_DB`, with its `XDG_*` | listing requires `OPENCODE_DB` or `--root`; other operations can use OpenCode's configured store |
| Pi | `PI_CODING_AGENT_DIR` | explicit root required |

`--root`, `--dest-root`, root inference, OpenCode's `--standalone` and the executable overrides (`--<harness>-exe`, `MUKADE_<HARNESS>_EXE`, `PATH`, install locations; an empty flag falls back to the default): [usage §7](docs/usage.md#7-stores-and-executables). Install locations and default stores per OS are one data file, [`catalog/harnesses.json`](catalog/harnesses.json), embedded in the executable ([harness catalog](docs/reference/harness-catalog.md)).

### Options worth knowing

| option | on | effect |
|---|---|---|
| `--interrupted keep\|drop` | `hop`, `write`, `document` | `drop` leaves out the work of unfinished turns ([interrupted turns](docs/usage.md#a-session-with-interrupted-turns)) |
| `--without-tools` | same | omits recorded tool calls and results; the manifest counts them |
| `--all-branches` | `dump`, `hop` | a Codex thread whole, abandoned turns included |
| `--expect-sha256 <hash>` | `hop` | refuses a source whose captured fingerprint differs; compares the complete selected source |
| `--dest-id <id>` | `hop`, `write`, `document` | a reserved destination session ID |
| `--dest-agent <name>`, `--metadata-file <path>` | OpenCode destination | its primary agent; JSON session metadata ([usage §6](docs/usage.md#6-host-and-automation-options)) |

**When a resume goes wrong:** a destination write that succeeds with a failed resume prints the receipt on stdout and saves `--manifest` before exit 5; a written session always has its receipt printed, even if the manifest file cannot be saved. A resume prompt starting with `-`, or one with line breaks for a `.cmd` launcher, is refused before anything is written. An external import whose outcome cannot be confirmed returns 5 and names the reserved ID: reconcile it before retrying ([troubleshooting](docs/usage.md#12-troubleshooting)).
### Exit codes

| code | meaning |
|---|---|
| 0 | done |
| 1 | a command-line or other error |
| 2 | refused with a reason, nothing written |
| 4 | failed, nothing changed |
| 5 | failed, recovery needed |

Which `HopError` gives which: [usage §11](docs/usage.md#11-exit-codes), [spec §1.2](docs/session-interchange-spec.md#12-rust-surface).

## How a hop works

No proxy, no re-prompting the history, no third-party converter.

| step | what happens |
|---|---|
| read | the source's own store is parsed into one model, keeping every original record verbatim (`Entry::raw`) |
| plan | tool calls are translated to what the destination offers; what cannot be translated travels as text, never dropped |
| check | the plan is held against the provider rules a real API refuses requests by; nothing is written if it would be refused |
| write | the destination's native files: Claude's `projects/<dir>/<uuid>.jsonl`, Codex's `rollout-*.jsonl`, OpenCode's document plus `session import`, Pi's `sessions/--<dir>--/<stamp>_<uuid>.jsonl` |
| report | a manifest states the source's fingerprint, the destination, the counts and **every loss** |

**Three rules:**

- The destination session is always **new**; the source is never touched.
- The order is the harness's **native order**, never timestamps.
- **No loss is silent:** a dropped orphan result, a reasoning block left behind, a tool call written as text, an abandoned branch of a rewound session all appear in the manifest.

A branched session (a Claude Code rewind, Pi's `/tree`) travels as the branch its harness would resume.

- **Claude Code** histories are rebuilt from the selected branch and its dependencies, even when compaction, split responses or parallel tool results were flushed out of file order. Records without a UUID are recovered when their response, call or compaction boundary identifies them unambiguously. Missing parents, cycles, conflicting identifiers and records that cannot be placed safely are refused, not guessed. Automatic compaction and rewind on real sources are unverified ([next.md](next.md), item 10).
- **Codex** code-mode `exec` scripts are read as the calls they wrap (L10): 725 of 981 real scripts ([census](docs/verification/2026-11-11-codex-code-mode.md)); the other 256 (variables, `Promise.allSettled`, `ALL_TOOLS`) stay one `exec` call in text form. `apply_patch` becomes a write (one added file), an edit (one hunk) or a multi-edit toward Pi (several hunks); several files, a delete or a move travel as text ([conversion tables](docs/tool-conversion-tables.md)). A subagent thread records its parent (`meta.parent`) but travels as its own session.

## Compaction and reasoning between hops

| `--compaction` | the destination gets | library |
|---|---|---|
| `none` (default) | the whole selected history | `Compaction::None` |
| `portable` | the handoff line and the source's own summary, then the tail from the boundary its compaction recorded | `Compaction::Portable` |
| `summary` | a handoff summary the source's own model writes now, with a prompt of mukade's (`--intent <name>`), in a run that saves nothing in the source's store | `Compaction::Portable` with `SummarySource::SourceModel` |
| `native` | the whole history plus the destination's own deterministic compaction record (spec §4.4); no model called; needs `--context-window <n>` | `Compaction::Native(NativeBudget)` |

`dump` exports a `summary` and `write` imports it later; `native` works on `hop` and `document`, not `dump`. Details: [usage §5](docs/usage.md#5-choose-what-travels).

**Reasoning** is a payload only its provider protocol can replay. Each block is carried natively (the destination's protocol matches and the block keeps its signature or encrypted payload), written as labelled text, or dropped; the manifest says which. `--reasoning drop_foreign` (default) | `as_text` | `drop`.

## Sessions TUI

`mukade-tui` is a separate terminal explorer for the same stores: the four harnesses and their sessions in one window, with a transcript, resume (`R`), hop (`h`), export (`e`), import (`i`). Built only on request, so the CLI stays one binary with its own dependencies.

```text
cargo run -p mukade-tui
cargo run -p mukade-tui -- --turns 20    # last 20 user turns per transcript
cargo run -p mukade-tui -- --check       # harnesses and session counts, then exit
```

Keys, the hop dialog, examples: [mukade-tui/README.md](mukade-tui/README.md).

## One executable, nothing to install

`build.bat` (Windows) or `sh build.sh` (Linux) builds `target/release/mukade[.exe]` and `mukade-tui[.exe]`. Copy it anywhere, a USB stick too. The Windows executable carries the Musashi icon and the C runtime, so it needs no redistributable. No installer, no configuration file, no mode to switch on.

By default mukade writes only beside its executable ([usage §10](docs/usage.md#10-where-its-files-go)):

| path | what |
|---|---|
| `intent-prompts/` | the editable summary prompts, copied on first start, never replaced |
| `mukade-data/cache/` | versions and project ids learned from harness binaries, the Claude Code listing cache |
| `mukade-data/tmp/` | OpenCode import documents, `--compaction summary` run folders; a killed run's leftovers are swept an hour later |
| `mukade-data/exports/` | the TUI's `e` |

Nothing goes to the user profile, the application-data folders, the registry or the system's temporary folder; delete the executable's folder and nothing is left. A host sets `MUKADE_DATA_DIR` (and `MUKADE_INTENT_DIR`) to an absolute writable folder; a relative or empty override is refused. Where the folder cannot be made, the cache is not kept, and an OpenCode import or a summary says why it cannot run. A test runs the real binary with all those places pointed at an empty sandbox and asserts it stays empty.

## Speed

| source session | hop | before the speed work |
|---|---|---|
| 1 MB | 30–35 ms | 83 ms |
| 10 MB | 185–240 ms | 556 ms |
| 30 MB | 440–630 ms | 1657 ms |

The stores are JSON Lines and SQLite, used as they are; the cost was copies, hashing and asking a harness for its version. `--timing` shows the time phase by phase on stderr ([Speed](docs/usage.md#9-speed)).

## Tests and fixtures

`cargo test` runs against real sessions of the four harnesses kept under `fixtures/` (recorded runs, synthetic content: no user's conversation is here) and writes only to scratch directories. `cargo test --workspace --no-fail-fast` also tests the TUI.

## Documentation

- [Usage guide](docs/usage.md) · [mukade-tui](mukade-tui/README.md) · [next.md](next.md) (done, to verify, known limits).
- Harness pages (store, compaction, tools, plugins and hooks, how mukade reads and writes it): [Claude Code](docs/harnesses/claude-code.md), [Codex CLI](docs/harnesses/codex.md), [OpenCode](docs/harnesses/opencode.md), [Pi](docs/harnesses/pi.md). No support yet: [Hermes Agent](docs/harnesses/hermes-agent.md), [DeepSeek Harness](docs/harnesses/deepseek-harness.md), planned read-first in [ADR-0002](docs/adr/ADR-0002-hermes-and-deepseek-harness-read-first.md).
- Reference: [interchange specification](docs/session-interchange-spec.md), [tool conversion tables](docs/tool-conversion-tables.md), [session persistence reference](docs/reference/harness-session-persistence-reference.md) (24 harnesses, secondary research), [Pi verification](docs/verification/pi-session-and-reasoning-verification.md) (Pi 1.2.0 and 1.4.0).
- Everything else (verification records, decisions, archived designs): the [documentation index](docs/README.md).

## License

MIT — see [LICENSE](LICENSE).
