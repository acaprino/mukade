# Mukade

<p align="center">
  <img src="docs/assets/branding/mukade-logo-oni.png" alt="Mukade from Oni beside the Mukade wordmark" width="800">
</p>

Mukade moves a coding-agent session from one harness to another, and the destination continues it
**as one of its own sessions**: no proxy in the middle, no re-prompting the history, no third-party
converter. Claude Code, Codex CLI, OpenCode and Pi are the four harnesses it reads and writes.

It is a library first (`cargo add mukade`) and a command line second (`mukade`), because a task
runner that hosts these harnesses wants the pipeline in-process, not behind a subprocess.

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
orphan result, a reasoning block left behind, a tool call written as text all appear in the manifest.

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
session, `import(dest, &session, &ImportOpts)` writes one you already hold, and `Receipt` carries the
destination id, its location, the native resume command, the manifest and — when a resume prompt was
given — the destination's reply.

Two helpers cover the harnesses themselves: `exe::resolve(harness, override)` finds the binary to
spawn (explicit override → `MUKADE_*_EXE` → `PATH` → well-known install locations), and
`detect::detect_store(dir)` reports which harness stores a directory holds, with the evidence.
`WriteDetails::exe` carries a destination-side override, which the resume command also names.

## Command line

For complete examples for each harness, including custom and portable installations, see the
[command-line usage guide](docs/usage.md).

```text
mukade list  <harness>
mukade dump  <harness> <session-id>          # the interchange document on stdout
mukade write <harness> <document.json>       # checked write: id, location, resume and manifest
mukade hop   <src-harness> <session-id> <dst-harness> [options]
mukade detect <dir>                          # which harness stores the directory holds
```

A `hop` reads the source and writes the destination in one process, so set **both** harnesses' store
variables (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `OPENCODE_DB` with its `XDG_*`, `PI_CODING_AGENT_DIR`):
Claude Code, Codex and Pi require an explicit root. OpenCode uses its configured store and is always
invoked with `--standalone`, because without it its CLI hands the work to a background service started
with the user's own environment; an explicit `OPENCODE_DB` is passed to its subprocesses. Temporary
OpenCode import documents are removed on success and failure.

A store can also be named per command: `--root <dir>` (non-empty) points one harness's root at that
directory — for a hop, the source's; for OpenCode a directory means its `opencode.db`. For `list`,
`dump` and a `hop` source the harness itself can be omitted: it is inferred from the directory's own
layout when exactly one is recognized, and refused when none or several are. `mukade detect <dir>`
prints the same answer with its evidence. Detection needs concrete files — session records
(`projects/*/*.jsonl`, dated `rollout-*.jsonl`, `sessions/--*--/*.jsonl`), Pi's `models.json`,
OpenCode's `opencode.db`, whose file can also be passed directly; an empty directory tree infers
nothing, and another harness's SQLite state (Codex's `state_*.sqlite`) is not OpenCode.

Binaries are resolved as `--claude-exe`/`--codex-exe`/`--opencode-exe`/`--pi-exe` → the matching
`MUKADE_*_EXE` variable → `PATH` → the well-known install locations, so a portable install with its
own config can be selected explicitly. For a hop, set the destination's store variable separately;
`--root` selects the source store and does not configure a spawned resume process. Set the relevant
environment variables for that process too. The executable override also names the binary in the
receipt's resume command; an empty flag falls back to the default resolution. For OpenCode as a
source, select its custom executable with `MUKADE_OPENCODE_EXE`.

When a destination write succeeds but its resume fails, `HopError::ResumeFailed` contains the written
session's receipt and manifest, so callers can recover without blindly creating another session. The
command line prints that receipt on stdout and saves `--manifest` before it exits with code 5; once a
session is written, its receipt is printed even when the manifest file cannot be saved. The resume
runs from the session's working directory, and a timeout stops the harness, including one started
through a Windows `.cmd` launcher.

## Compaction and reasoning between hops

A hop can carry the source's own compaction instead of the whole history: `Compaction::Portable`
writes the handoff line and the source's summary, then the tail from the boundary the source's
compaction recorded. Reasoning is treated as what it is — a payload only its own provider protocol
can replay: it is carried natively when the destination's protocol matches and the block still has
its signature or encrypted payload, written as labelled text, or dropped. Which of the three
happened is in the manifest.

## Tests and fixtures

`cargo test` runs against the real sessions of the four harnesses kept under `fixtures/`, and writes
only to scratch directories. The fixtures are recorded runs with synthetic content: no user's
conversation is in this repository.

The documentation ships here too: the specification
([`docs/session-interchange-spec.md`](docs/session-interchange-spec.md)), the tool translation
tables ([`docs/tool-conversion-tables.md`](docs/tool-conversion-tables.md)), the reference notes on
the four harnesses' stores
([`docs/reference/harness-session-persistence-reference.md`](docs/reference/harness-session-persistence-reference.md))
and the conformance bench report
([`docs/verification/2026-10-27-H27.md`](docs/verification/2026-10-27-H27.md)). The bench itself —
the scripts that run the real harnesses against a mock model — belongs to Shinatama and stays there.

## License

MIT — see [LICENSE](LICENSE).
