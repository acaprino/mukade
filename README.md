# Mukade

<p align="center">
  <img src="docs/assets/branding/mukade-logo-oni.png" alt="Mukade from Oni beside the Mukade wordmark" width="800">
</p>

Mukade moves a coding-agent session from one harness to another, and the destination continues it
**as one of its own sessions**: no proxy in the middle, no re-prompting the history, no third-party
converter. Claude Code, Codex CLI, OpenCode and Pi are the four harnesses it reads and writes.

It is a library first (`cargo add mukade --git https://github.com/acaprino/mukade`; it is not on
crates.io yet) and a command line second (`mukade`), because a task runner that hosts these
harnesses wants the pipeline in-process, not behind a subprocess.

## What it does

| step | what happens |
|---|---|
| read | the source's own store is parsed into one model, keeping every original record verbatim (`Entry::raw`) |
| plan | tool calls are translated to what the destination actually offers, and what cannot be translated travels as text — never dropped |
| check | the plan is held against the provider rules that make a real API refuse a request; nothing is written if it would be refused |
| write | the destination's native files are written: Claude's `projects/<dir>/<uuid>.jsonl`, Codex's `rollout-*.jsonl`, OpenCode's document plus `session import`, Pi's `sessions/--<dir>--/<stamp>_<uuid>.jsonl` |
| report | a manifest states the source's fingerprint, the destination, the counts and **every loss** |

Three rules hold everywhere: the destination session is always **new** (the source is never touched),
the order is the harness's **native order** (never timestamps), and **no loss is silent** — a dropped
orphan result, a reasoning block left behind, a tool call written as text, an abandoned branch of a
rewound session all appear in the manifest. A session that branched (a Claude Code rewind, Pi's
`/tree`) travels as the branch the source harness itself would resume.

## Library

```rust
use mukade::hop;
use mukade::hop::{HopRequest, SessionRef, WriteDetails};
use mukade::model::HarnessId;
use mukade::roots::StoreRoots;

let receipt = hop(&HopRequest {
    source: SessionRef::id(HarnessId::ClaudeCode, "c45db7bc-542c-4201-89de-b590c6fdb797"),
    dest: HarnessId::Pi,
    cwd: None,
    compaction: Default::default(),
    resume: None,
    reasoning: Default::default(),
    roots: StoreRoots::from_env(),      // configure the intended stores in the environment
    write: WriteDetails::default(),
})?;

println!("{}", serde_json::to_string_pretty(&receipt.manifest)?);
```

`hop` is the whole pipeline. Its halves are public on their own: `export(&SessionRef, &roots)` reads a
session (`export_path` reads one from a file), `import(dest, &session, &ImportOpts)` writes one you
already hold, and `Receipt` carries the destination id, its location, the native resume command, the
manifest and — when a resume prompt was given — the destination's reply. `compact(&session,
&Compaction, &roots)` is the step between them: the session a compaction policy hands to the writer,
with the summary the source's model wrote when one was asked for (`summary::summarize` runs it).

Two helpers cover the harnesses themselves: `exe::resolve(harness, override)` finds the binary to
spawn (explicit override → `MUKADE_*_EXE` → `PATH` → well-known install locations), and
`detect::detect_store(dir)` reports which harness stores a directory holds, with the evidence. `WriteDetails::exe` carries a destination-side override, which
the resume command also names. Every harness process mukade starts runs with a deadline, and a
timeout stops it with everything it started (`exe::run_within`).

Each harness has its own page — what it is, how it is installed and launched, its store, compaction,
tools, plugins and hooks, and how mukade reads and writes it: [Claude Code](docs/harnesses/claude-code.md),
[Codex CLI](docs/harnesses/codex.md), [OpenCode](docs/harnesses/opencode.md), [Pi](docs/harnesses/pi.md).
The [tool conversion tables](docs/tool-conversion-tables.md) are the one place that says how a tool
call of one harness becomes a call of another.

## Command line

For complete examples for each harness, including ones kept in a folder of their own, see the
[command-line usage guide](docs/usage.md).

```text
mukade harnesses                           # local inventory: installed harnesses, versions, stores and session/fork counts (text output, no options)
mukade list  <harness>
mukade dump  <harness> <session-id>          # the interchange document on stdout, or --output <file>
mukade write <harness> <document.json>       # checked write: id, location, resume and manifest
mukade hop   <src-harness> <session-id> <dst-harness> [options]   # --all-branches: a Codex thread whole
mukade detect <dir>                          # which harness stores the directory holds
```

The binaries' own install locations, the default store of each harness on Windows, macOS and Linux,
their store layouts and the lineage each records are one data file,
[`catalog/harnesses.json`](catalog/harnesses.json), documented in the
[harness catalog](docs/reference/harness-catalog.md). It is embedded in the executable, and
`mukade harnesses` reads it.

A `hop` reads the source and writes the destination in one process, so set **both** harnesses' store
variables (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `OPENCODE_DB` with its `XDG_*`, `PI_CODING_AGENT_DIR`):
Claude Code, Codex and Pi require an explicit root (an empty variable is no root). OpenCode's
session listing requires an explicit `OPENCODE_DB` or `--root` and opens it read-only. Other
OpenCode operations can use its configured store. Every OpenCode command that touches the store
(`session export`, `import`, `run`, or a project probe) carries `--standalone`, because without it its CLI hands the work to a background
service started with the user's own environment; an explicit `OPENCODE_DB` is passed to those
commands. Temporary OpenCode import documents are removed on success and failure.

A store can also be named per command: `--root <dir>` (non-empty) points one harness's root at that
directory — for a hop, the source's; for OpenCode a directory means its `opencode.db`. For `list`,
`dump` and a `hop` source the harness itself can be omitted: it is inferred from the directory's own
layout when exactly one is recognized, and refused when none or several are. `mukade detect <dir>`
prints the same answer with its evidence. Detection needs concrete files — session records
(`projects/*/*.jsonl`, `sessions/**/rollout-*.jsonl`, `sessions/--*--/*.jsonl`), Codex's
`session_index.jsonl`, Pi's `models.json`, OpenCode's `opencode.db`, whose file can also be passed
directly; an empty directory tree infers nothing, and another harness's SQLite state (Codex's
`state_*.sqlite`) is not OpenCode.

Binaries are resolved as `--claude-exe`/`--codex-exe`/`--opencode-exe`/`--pi-exe` → the matching
`MUKADE_*_EXE` variable → `PATH` → the well-known install locations, so an install kept in its own folder with its
own config can be selected explicitly. For a hop, set the destination's store variable in the
environment; `--root` selects the source store. A `--resume-prompt` runs the destination with its
store variable set to the root the session was written to; a resume command run later by hand needs
that variable set too. The executable override also names the binary in the receipt's resume command;
without one the command names the bare binary, which mukade itself looks up on `PATH` (`.cmd`
launchers included) when it runs it. An empty flag falls back to the default resolution. For
OpenCode as a source, select its custom executable with `MUKADE_OPENCODE_EXE`.

`--without-tools` on `hop` or `write` omits recorded tool calls and results; the manifest counts
the omissions. An OpenCode destination may also receive `--dest-id`, `--dest-agent` and
`--metadata-file`: a reserved ID or supplied metadata is checked in the explicitly selected
database after the import. An external import whose outcome cannot be confirmed returns code 5 and names
the reserved ID; reconcile it before retrying.

For a `hop`, `--expect-sha256 <hash>` refuses a source whose captured fingerprint differs from
the caller's. It compares the complete source selected by that read; it does not freeze an
earlier prefix of a file that has since grown.

Claude Code histories are reconstructed from the selected branch and its dependencies, even when
compaction, split responses or parallel tool results were flushed out of file order. Records without
a UUID are recovered when their response, call or compaction boundary identifies them unambiguously.
Missing parents, cycles, conflicting identifiers and records that cannot be placed safely are
refused rather than guessed. Automatic compaction and rewind on real sources remain unverified.

When a destination write succeeds but its resume fails, `HopError::ResumeFailed` contains the written
session's receipt and manifest, so callers can recover without blindly creating another session. The
command line prints that receipt on stdout and saves `--manifest` before it exits with code 5; once a
session is written, its receipt is printed even when the manifest file cannot be saved. The resume
runs from the session's working directory, and a timeout stops the harness, including one started
through a Windows `.cmd` launcher. A resume prompt that starts with `-`, or one with line breaks for a
`.cmd` launcher, is refused before anything is written. Exit codes: 0 done, 1 a command-line error,
2 refused with a reason
(nothing written: the session is not there, the request is not supported), 4 failed and nothing
changed (for example the plan would be refused by a provider), 5 failed and recovery is needed
(the source could not be read, an external import has an uncertain outcome, or the resume failed
after the write).

## Sessions TUI

`mukade-tui` is a separate native terminal explorer and manager for the same stores: the four
harnesses and their sessions in one window, a transcript, interactive resume and hop. It is built
only on request, so the `mukade` CLI stays a single binary with its own dependency list.

```text
cargo run -p mukade-tui
cargo run -p mukade-tui -- --turns 20
cargo run -p mukade-tui -- --check
```

The full command list, keys, hop dialog and examples are in
[mukade-tui/README.md](mukade-tui/README.md).

## One executable, nothing to install, and speed

mukade is a single command-line executable: run `build.bat` on Windows or `sh build.sh` on Linux,
then copy `target/release/mukade.exe` or `target/release/mukade` anywhere (a USB stick included)
and run it. The Windows executable includes the Musashi icon and links the C runtime in, so it
needs no redistributable. There is no installer, no configuration file and no mode to switch on.

By default it generates the files it needs in the folder where the executable is: a
`mukade-data` folder, made the first time something is written to it, holding a small cache of the
versions and OpenCode project ids it learned from harness binaries (`cache/`) and its temporary
files (`tmp/`): the document an OpenCode import needs, and the folder of a `--compaction summary`
run (Codex's last message, Pi's fork), each removed when its work ends; a killed run's leftovers
are swept an hour later. By default nothing goes to the user profile, the application-data folders, the
registry or the system's temporary folder; where the folder cannot be made (an executable in a
read-only place) the cache is simply not kept, and an OpenCode import or a summary says why it
cannot run. A host can set `MUKADE_DATA_DIR` to an absolute writable folder instead; a relative or
empty override is refused, never silently redirected. Delete the default
executable's folder and nothing is left. An automated test runs the real binary with all of
those places pointed at an empty sandbox and asserts it stays empty.

A hop is quick: 30–35 ms for a 1 MB session, 185–240 ms for 10 MB, 440–630 ms for 30 MB (it was
83, 556 and 1657 ms). The harnesses'
own stores are JSON Lines and SQLite and are read and written as they are, so the cost was never the
format; it was copies, hashing and asking a harness for its version, which is what `--timing` shows
phase by phase on stderr. See the [usage guide](docs/usage.md#5-speed).

## Compaction and reasoning between hops

A hop can carry the source's own compaction instead of the whole history: `Compaction::Portable`
writes the handoff line and the source's summary, then the tail from the boundary the source's
compaction recorded. `--compaction summary` asks the source harness's own model for a
handoff summary instead, with a prompt of mukade's own and a run that saves nothing in the source's
store; `dump` exports that summary and `write` imports it later. Reasoning is treated as what it is — a payload only its own provider protocol
can replay: it is carried natively when the destination's protocol matches and the block still has
its signature or encrypted payload, written as labelled text, or dropped. Which of the three
happened is in the manifest.

## Tests and fixtures

`cargo test` runs against the real sessions of the four harnesses kept under `fixtures/`, and writes
only to scratch directories. The fixtures are recorded runs with synthetic content: no user's
conversation is in this repository.

The documentation ships here too: the specification
([`docs/session-interchange-spec.md`](docs/session-interchange-spec.md)), the tool translation
tables ([`docs/tool-conversion-tables.md`](docs/tool-conversion-tables.md)), one page per harness
([`docs/harnesses/`](docs/harnesses/)), the reference notes on the four harnesses' stores
([`docs/reference/harness-session-persistence-reference.md`](docs/reference/harness-session-persistence-reference.md))
and the verification of mukade-written sessions against Pi 1.2.0 and 1.4.0
([`docs/verification/pi-session-and-reasoning-verification.md`](docs/verification/pi-session-and-reasoning-verification.md)).

See the [documentation index](docs/README.md) for guides, reference material, verification records
and archived design documents.

## License

MIT — see [LICENSE](LICENSE).
