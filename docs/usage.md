# Using Mukade

Mukade moves a coding-agent session from one harness to another — Claude Code, Codex CLI,
OpenCode, Pi — and the destination continues it **as one of its own sessions**. This guide is the
command-line manual: how to get it, the use cases it is made for, how to read what it prints, and
every harness's details.

Paths, session IDs, project IDs, model names and provider names in the examples are
**illustrative**: replace them with values from your installation. Commands are shown for Windows
PowerShell; on macOS or Linux use the corresponding paths and `export NAME=value` instead of
`$env:NAME = 'value'`.

## Contents

1. [Get it](#1-get-it) · 2. [Quick start](#2-quick-start) · 3. [Use cases](#3-use-cases) ·
4. [Reading the receipt and the manifest](#4-reading-the-receipt-and-the-manifest) ·
5. [Speed](#5-speed) · 6. [Where its files go](#6-where-its-files-go) · 7. [Exit codes](#7-exit-codes) ·
8. [Stores and executables](#8-stores-and-executables) · 9. [Per-harness examples](#9-per-harness-examples) ·
10. [Discover, export, import](#10-discover-export-import) · 11. [Troubleshooting](#11-troubleshooting)

## 1. Get it

mukade is one command-line executable. There is nothing to install and nothing to configure:

```powershell
cargo build --release                       # target\release\mukade.exe
Copy-Item target\release\mukade.exe D:\tools\    # anywhere: a folder, a USB stick
D:\tools\mukade.exe --help
```

The Windows executable has the C runtime built in, so it runs on a machine with no Visual C++
redistributable. Run it from a shell, by its path or from a folder on `PATH`. The files it needs it
generates in the folder where the executable is, and nowhere else: see
[Where its files go](#6-where-its-files-go).

Install and configure the harnesses you intend to use separately: mukade moves sessions, it does
not install harnesses or transfer their credentials.

## 2. Quick start

Continue a Claude Code session in Pi:

```powershell
$env:CLAUDE_CONFIG_DIR   = Join-Path $HOME '.claude'
$env:PI_CODING_AGENT_DIR = Join-Path $HOME '.pi\agent'

mukade list claude_code                                  # find the session id
mukade hop claude_code YOUR_CLAUDE_SESSION_ID pi --cwd 'C:\work\my-project'
```

The last command prints one JSON receipt. The new Pi session exists; open it with the `resume`
command the receipt gives, from the project folder:

```powershell
Set-Location 'C:\work\my-project'
pi -p --session 'C:\Users\you\.pi\agent\sessions\--C--work-my-project--\2026-10-28T16-00-00-000Z_a7b29d83-….jsonl' 'where were we?'
```

Three things always hold: the destination session is **new** (the source is never touched), the
order is the harness's **native order** (never timestamps), and **no loss is silent**: whatever was
left behind or changed is a line in the receipt's manifest.

Before you start, know the **existing project directory** the source belongs to; `--cwd` names the
destination's project directory (if omitted, mukade uses the source's recorded one, when present).
A `--resume-prompt` *runs* the destination harness and may make a model/API call; without it mukade
only writes the session and gives you the command.

## 3. Use cases

### Switch harness in the middle of a task

*You started in Claude Code and want to continue in Pi, Codex or OpenCode.* This is the quick start
above. Any pair works; the four harnesses are `claude_code`, `codex`, `opencode`, `pi`:

```powershell
mukade hop codex YOUR_CODEX_SESSION_ID claude_code --cwd 'C:\work\my-project'
mukade hop pi    YOUR_PI_SESSION_ID    opencode    --cwd 'C:\work\my-project' --project-id YOUR_PROJECT --model-provider YOUR_PROVIDER --model YOUR_MODEL
```

Tool calls are translated to what the destination really offers; a call it cannot express travels
as text, with its arguments, and is counted (`tools_text_form`). Add `--resume-prompt 'continue'`
to also run the destination once and keep its reply in the receipt (`resume_output`).

### Carry a very long session cheaply

*The history is huge but only the recent turns matter.* `--compaction portable` writes one handoff
message holding the **source's own compaction summary**, then the tail the source's compaction
kept, instead of the whole history:

```powershell
mukade hop claude_code YOUR_CLAUDE_SESSION_ID pi --cwd 'C:\work\my-project' --compaction portable
```

The source must have compacted at least once (a Claude `/compact`, a Pi compaction, …); otherwise
the hop is refused with a reason and nothing is written. The manifest keeps the lines
`the source's compaction summary travels as one user message` and how many entries were kept.

### Rehearse before touching the real store

*You want to see what a hop would do first.* Point the **destination's** store variable at an empty
scratch folder: the session is written there, and your real store is not touched.

```powershell
$env:CLAUDE_CONFIG_DIR   = Join-Path $HOME '.claude'      # the real source
$env:PI_CODING_AGENT_DIR = 'C:\work\rehearsal\pi'         # an empty scratch destination
mukade hop claude_code YOUR_CLAUDE_SESSION_ID pi --cwd 'C:\work\my-project' --manifest C:\work\rehearsal\manifest.json
```

Read the manifest ([section 4](#4-reading-the-receipt-and-the-manifest)); when it is what you want,
set the variable to the real store and run it again. A hop is repeatable: each run writes another
new session and changes nothing else.

### Hand a session to someone else, or another machine

*Export a harness-neutral document, move the file, write it on the other side.* Use `--output`, not
a shell pipe (PowerShell re-encodes piped text with the console code page and garbles non-ASCII
text):

```powershell
# machine A
mukade dump claude_code YOUR_CLAUDE_SESSION_ID --output C:\work\session.json

# machine B (any destination harness; its store variable set)
$env:CODEX_HOME = 'D:\codex\home'
mukade write codex C:\work\session.json --cwd 'D:\work\my-project'
```

A document holds conversation data: protect it like the original session. `write` needs the
destination harness named explicitly, and the destination must have its own configuration and
credentials — mukade does not transfer them.

### Audit what a conversion lost

*You need to know exactly what changed.* `--manifest <file>` also saves the manifest, and the
receipt prints it anyway. Its `counts` and `losses` say what was dropped, rewritten, or written as
text; its `source.content_sha256` is the SHA-256 of the **source file's own bytes**, so you can
check it with `Get-FileHash -Algorithm SHA256 <file>` (or `sha256sum`) and know which exact file
was carried. See [section 4](#4-reading-the-receipt-and-the-manifest).

### A session that was rewound or branched

*You rewound in Claude Code, edited a prompt, or navigated back in Pi's `/tree`.* The harness
resumes only the branch of its latest entry, and so does mukade: the abandoned turns do not become
history in the destination. They stay in the source untouched, and the manifest counts them
(`dropped_off_branch`). Pi's `branch_summary`, `bashExecution`, `custom_message` and `context_edit`
entries reach the destination the way Pi shows them to its model.

### Use it from a task runner or another program

*A host that already runs these harnesses wants the pipeline in-process.* mukade is a library first:
`mukade::hop(&HopRequest { … })` does everything the command does, with typed errors
(`HopError::ResumeFailed` carries the written session's receipt, so a caller never has to guess
whether the destination exists). See the [README](../README.md#library).

### Carry mukade itself on a stick

*A machine you do not own, or one you cannot install on.* Copy `mukade.exe` to the stick and run
it from there: it generates what it needs next to itself ([section 6](#6-where-its-files-go)), so
deleting the folder leaves nothing on the machine.

## 4. Reading the receipt and the manifest

`hop` and `write` print one JSON object (this one is a real run: the Claude Code fixture hopped
into Pi with `--compaction portable`; long paths shortened):

```json
{
  "id": "a7b29d83-3e83-4c9a-87fd-b136eba01dfa",
  "location": "<PI_CODING_AGENT_DIR>/sessions/--…--/2026-10-28T16-00-00-000Z_<id>.jsonl",
  "resume": ["pi", "-p", "--session", "<that file>"],
  "manifest": {
    "source": { "harness": "claude_code", "version": "2.1.311",
                "session_id": "c45db7bc-…", "content_sha256": "df8f8f84d7fd3d21…" },
    "dest":   { "harness": "pi", "version": "1.2.0", "session_id": "a7b29d83-…", "location": "…" },
    "options": { "compaction": "portable", "reasoning": "drop_foreign", "resume_prompt": false },
    "counts": { "messages_in": 5, "messages_out": 5, "tool_calls": 0, "tools_text_form": 0,
                "reasoning_dropped": 0, "dropped_orphan_results": 0, "dropped_duplicate_results": 0,
                "rewritten_call_ids": 0, "dropped_off_branch": 0, "dropped_unfinished_lines": 0,
                "aborted_steps_as_replies": 0, "unanswered_calls": 0, "dropped_empty_steps": 0,
                "dropped_attachments": 0, "dropped_result_blocks": 0 },
    "losses": [
      "interrupted_last_turn",
      "the source's compaction summary travels as one user message",
      "the source's own compaction is in the session (kept 49 entries after the boundary, from 5237bfb4-…)"
    ],
    "strict": { "checked": true, "violations": [] }
  },
  "resume_output": null
}
```

- `id`, `location`, `resume`: the new session's id, where it was written, and the destination's own
  command to continue it. Keep the destination's store variable set when you run it by hand.
- `resume_output`: the destination's reply, when you passed `--resume-prompt`.
- `counts`: every kind of loss as a number; every number above zero also has a line in `losses`.
- `strict`: the plan was held against the rules a provider API enforces (every tool call answered
  once, ids in the allowed pattern, …) **before** anything was written; a plan that would be
  refused is not written (exit code 4).

What the `losses` lines mean, and what to do:

| line | what happened |
|---|---|
| `interrupted_last_turn` | the source ended on a turn with no answer (the harness was killed, or the step was aborted); it travels as a user message with no reply. Normal. |
| `reasoning dropped: foreign provider or no opaque payload (n)` | thinking can only be replayed to a provider that speaks the same protocol and whose format can hold its signature or encrypted payload; otherwise it is dropped. `--reasoning as_text` keeps it as labelled text instead. |
| `tool calls written as text, not translated (n, Tool, …)` | the destination has no equivalent tool, or an argument had no counterpart; the call and its result travel as text. Nothing is invented. |
| `unanswered tool calls carried as text (n)` | the source never recorded a result for a call (a killed tool); a provider refuses a call without its result, so it travels as text. |
| `orphan tool results dropped (n)` · `duplicate tool results dropped, the first answer kept (n)` | a result with no call, or a second answer to one call; a provider would refuse either. |
| `tool call ids rewritten to the destination's id pattern (n)` | Anthropic accepts only `[a-zA-Z0-9_-]` ids; the call and its result were rewritten together. |
| `records of an abandoned branch … left behind (n)` | a rewind or an edited prompt: only the resumed branch travelled. |
| `an unfinished last line of the source left unread (n)` | a hard kill cut the last line mid-write; the rest of the session was read. |
| `steps the source aborted or that failed, written as ordinary replies (n)` | the destination cannot mark a step as aborted (Pi can, and does). |
| `empty steps dropped (n)` · `user attachments dropped (n)` · `structured tool-result blocks flattened to text (n)` | content the destination cannot hold (a PDF, an image toward a text-only result) is counted, never silently dropped. Images are rewritten into each harness's own shape. |
| `the source's compaction summary travels as one user message` | `--compaction portable`. |
| `the resume prompt did not complete: …` | the session was written but `--resume-prompt` failed (exit 5): see [troubleshooting](#11-troubleshooting). |

## 5. Speed

A handover is meant to feel instant. Measured on this repository's release build, hopping a Claude
Code session of that size (mostly tool output, like real ones) into Pi, Claude or Codex:

| source session | time (Pi, Claude or Codex as destination) | peak memory |
|---|---|---|
| 1 MB | 30–35 ms (`dump` alone: 24 ms) | not measured |
| 10 MB | 185–240 ms (`dump` alone: 150 ms) | about 100 MB |
| 30 MB | 440–630 ms (`dump` alone: 420 ms) | about 300 MB |

Before the work on speed the same hops took 83, 556 and 1657 ms. The time is proportional to the
size of the session, because every record is read and decoded once.

`--timing` prints where the time went, phase by phase, on stderr:

```text
read file                          0.5 ms
parse json lines                   2.5 ms
resumed branch                     0.2 ms
entries                            0.4 ms
finish                             0.0 ms
fingerprint (parallel)             3.4 ms
build entries                      1.8 ms
probe destination version          0.6 ms
select                             0.8 ms
plan                               0.0 ms
strict check                       0.0 ms
serialize + write file             5.2 ms
write files                        5.8 ms
total                             16.2 ms
```

(A real run on the 5-message Claude Code fixture, `hop claude_code … pi --compaction portable`.
A phase is printed when it ends, so one that runs inside another comes first: `serialize + write
file` is inside `write files`, and `resumed branch`, `entries`, `finish` are inside `build entries`.
`fingerprint (parallel)` runs on a second thread while the file is parsed.)

What makes it fast, and what you can do:

- **The file formats are not the cost.** Claude Code, Codex and Pi store JSON Lines and OpenCode
  SQLite; those are the harnesses' own and mukade reads and writes them as they are. A binary
  intermediate would not help — a hop has none. Decoding is spread over up to 8 threads for large
  files; the source's fingerprint is hashed on another thread while the rest is read.
- **Starting the destination to ask its version is the real cost for small sessions.** Pi's
  `--version` takes about half a second (it starts Node). mukade therefore reads the version from the
  npm package the launcher starts, without starting anything; for a native binary it asks once and
  remembers the answer by the binary's path, size and modification time, in `mukade-data/cache`
  beside the executable ([section 6](#6-where-its-files-go)). To skip the question entirely,
  state the facts: `--dest-version`, and for OpenCode `--project-id`, `--model-provider`, `--model`.
- A `--resume-prompt` starts the destination harness and a model: that is its time, not mukade's.

## 6. Where its files go

mukade is portable by construction: there is no installer, no configuration file, no mode and no
setting. It generates what it needs in **the folder where the executable is**, and nowhere else.
Everything it keeps for itself is in one folder, `mukade-data`, made the first time something is
written to it (never at start-up):

| file | what for |
|---|---|
| `mukade-data/cache/probes.json` | the versions and OpenCode project ids learned from harness binaries, keyed by the binary's path, size and modification time, so the next hop does not start the harness to ask again |
| `mukade-data/tmp/mukade-import-*.json` | the document `opencode session import` needs as a file; private, and removed when the import ends, even when it fails |
| `.mukade-*.tmp` | beside the session being written, in the destination's store: the session as it is written, renamed into place when complete |

Nothing is written to the user profile, the application-data folders, the registry or the system's
temporary folder. A temporary file a **killed** run left behind is removed by the next run once it
is an hour old. The files that *are* yours stay yours: the sessions it writes into the harnesses'
stores, and what you ask for with `--output` and `--manifest`. To remove mukade, delete the folder
it is in.

If the executable is in a place it cannot write to (a read-only install), the folder cannot be made.
mukade does not look for another place: the cache is not kept (every hop asks the harness, which
costs time but works), and a hop to OpenCode stops with an error that names the folder, before
anything is written. Copy `mukade.exe` to a folder you can write to.

This is checked by an automated test that runs the real binary alone in a folder, with the
temporary folder, the home folder and the application-data folders pointed at an empty sandbox, and
asserts the sandbox stays empty and the folder holds only the executable and `mukade-data`.

**Harnesses are separate.** A harness installed in its own folder is reached with its store
variable and `--<harness>-exe`: see [section 8](#8-stores-and-executables) and the examples in
[section 9](#9-per-harness-examples).

## 7. Exit codes

| code | meaning |
|---|---|
| 0 | done |
| 1 | a command-line mistake (unknown option, a flag without its value, an unexpected argument) |
| 2 | refused with a reason; nothing was written: the session is not there, the store is not configured, the id is ambiguous, or the request is not supported (a resume prompt starting with `-`, `--compaction portable` on a source that never compacted, …) |
| 4 | failed and nothing changed: the destination refused the write, or the plan would be refused by a provider |
| 5 | failed and recovery is needed: the source exists but could not be read, **or the session was written but the resume failed** — then the receipt is still printed and `--manifest` saved |

## 8. Stores and executables

| Harness | CLI name | Store/config variable | Executable flag | Executable environment variable |
|---|---|---|---|---|
| Claude Code | `claude_code` | `CLAUDE_CONFIG_DIR` | `--claude-exe` | `MUKADE_CLAUDE_EXE` |
| Codex CLI | `codex` | `CODEX_HOME` | `--codex-exe` | `MUKADE_CODEX_EXE` |
| OpenCode | `opencode` | `OPENCODE_DB` (database file), plus its applicable `XDG_*` settings | `--opencode-exe` | `MUKADE_OPENCODE_EXE` |
| Pi | `pi` | `PI_CODING_AGENT_DIR` | `--pi-exe` | `MUKADE_PI_EXE` |

Claude Code, Codex and Pi require an explicit root, even for standard installations (an empty
variable is no root). OpenCode may use its configured store, but supply its database and
environment explicitly when isolation matters.

An executable override points to the **launcher or executable**, not its installation directory.
The CLI resolves the destination executable using its flag, then its `MUKADE_*_EXE` variable,
then `PATH` (`.cmd` launchers included), then known installation locations, and finally the bare
command name. An explicitly configured executable is also put into the returned resume command.
OpenCode is special: when it is the **source**, its export reader uses `MUKADE_OPENCODE_EXE` or
default resolution, not the `--opencode-exe` flag. The flag applies to OpenCode as a destination.

`--root` changes the selected harness's store for `list`, `dump` or `write`. For `hop`, it applies
to the **source**, not a separate destination root; if source and destination are the same harness,
they share that overridden root. Set the destination variable in the environment. For OpenCode,
an existing directory passed as `--root` means its `opencode.db`; a file path is used as-is.

A `--resume-prompt` runs the destination with its store variable set to the root the session was
written to, whether it came from the environment or from `--root`, and from the destination's
working directory (`--cwd`, or the source's recorded directory, which must exist on this machine).
A resume command you run later by hand does not inherit that: set the destination's variable in
that shell first. For OpenCode, also configure the appropriate `XDG_*` and `OPENCODE_CONFIG`
settings where needed; a database path alone is not a guarantee of complete isolation.

A resume prompt that starts with `-` is refused, because every harness CLI would read it as an
option; so is one with line breaks when the destination's executable is a `.cmd` launcher, which
cannot take them. Both are refused before anything is written. Every harness process mukade
starts runs with a deadline; one that outlives it is stopped together with everything it started.

## 9. Per-harness examples

### Claude Code

Standard location, read Claude and write Pi (select the source ID from the `list` output):

```powershell
$env:CLAUDE_CONFIG_DIR = Join-Path $HOME '.claude'
$env:PI_CODING_AGENT_DIR = Join-Path $HOME '.pi\agent'
mukade list claude_code
mukade hop claude_code YOUR_CLAUDE_SESSION_ID pi --cwd 'C:\work\my-project'
```

A custom Claude destination, with Pi as the source:

```powershell
$env:PI_CODING_AGENT_DIR = 'D:\portable\pi\agent'
$env:CLAUDE_CONFIG_DIR = 'D:\portable\claude\config'
mukade list pi
mukade hop pi YOUR_PI_SESSION_ID claude_code --cwd 'C:\work\my-project' --claude-exe 'D:\portable\claude\bin\claude.exe'
```

Keep `CLAUDE_CONFIG_DIR` set when executing the returned `resume` arguments. Mukade does not install
Claude or create its authentication configuration. Claude Code writes the folder name of a project
with its own rule (every character outside `[a-zA-Z0-9]` becomes `-`, a name past 200 characters
is cut and suffixed with a hash), and mukade follows it.

### Codex CLI

```powershell
$env:CODEX_HOME = Join-Path $HOME '.codex'
$env:PI_CODING_AGENT_DIR = Join-Path $HOME '.pi\agent'
mukade list codex
mukade hop codex YOUR_CODEX_SESSION_ID pi --cwd 'C:\work\my-project'
```

A custom Codex destination:

```powershell
$env:CLAUDE_CONFIG_DIR = 'D:\portable\claude\config'
$env:CODEX_HOME = 'D:\portable\codex\home'
mukade hop claude_code YOUR_CLAUDE_SESSION_ID codex --cwd 'C:\work\my-project' --codex-exe 'D:\portable\codex\bin\codex.exe'
```

Codex requires a destination CLI version in its session metadata. Mukade asks the chosen
executable's `--version`; if that cannot be done or parsed, supply it with `--dest-version 0.164.0`
(your version). Mukade reads `model_provider` from `$env:CODEX_HOME\config.toml` when available;
`--model-provider` states it explicitly: only the top-level `model_provider` key counts, not a
profile's or a provider table's. Keep `CODEX_HOME` set for a resume command run later.

### OpenCode

Read OpenCode and write Pi. Use OpenCode's own listing to obtain the ID, from the relevant project
directory; Mukade's `list opencode` returns an unsupported-listing error.

```powershell
$env:OPENCODE_DB = 'C:\path\to\opencode.db'
$env:PI_CODING_AGENT_DIR = Join-Path $HOME '.pi\agent'
Set-Location 'C:\work\my-project'
opencode session list --standalone
mukade hop opencode YOUR_OPENCODE_SESSION_ID pi --cwd 'C:\work\my-project'
```

Mukade calls `opencode session export … --standalone` to read the source. Set the `XDG_*`
variables used by your OpenCode installation in this shell as well, if applicable.

A custom OpenCode destination. The database, configuration, XDG directories and executable must
belong to the intended installation; the project ID, provider and model must already be valid for it.

```powershell
$env:CLAUDE_CONFIG_DIR = 'D:\portable\claude\config'
$env:OPENCODE_DB = 'D:\portable\opencode\data\opencode.db'
$env:XDG_DATA_HOME = 'D:\portable\opencode\data'
$env:XDG_CONFIG_HOME = 'D:\portable\opencode\config'
$env:XDG_STATE_HOME = 'D:\portable\opencode\state'
$env:XDG_CACHE_HOME = 'D:\portable\opencode\cache'
$env:OPENCODE_CONFIG = 'D:\portable\opencode\config\opencode.json'
mukade hop claude_code YOUR_CLAUDE_SESSION_ID opencode --cwd 'C:\work\my-project' --opencode-exe 'D:\portable\opencode\opencode.exe' --project-id YOUR_OPENCODE_PROJECT_ID --model-provider YOUR_PROVIDER --model YOUR_MODEL
```

OpenCode's import needs a project ID and a complete provider/model reference. Mukade can discover
the project from OpenCode's existing sessions (asked of the store the session will be written to),
and the provider and model from its configuration; explicit values avoid relying on discovery, and
are faster (see [Speed](#5-speed)). OpenCode imports with `--standalone`.

For a **custom OpenCode source**, set its executable through the environment variable before
exporting:

```powershell
$env:MUKADE_OPENCODE_EXE = 'D:\portable\opencode\opencode.exe'
$env:PI_CODING_AGENT_DIR = 'D:\portable\pi\agent'
mukade hop opencode YOUR_OPENCODE_SESSION_ID pi --cwd 'C:\work\my-project'
```

For an OpenCode destination, the receipt's `location` names the database the session was imported
into (`OPENCODE_DB`), or says it went to OpenCode's configured store; the import document is a
temporary file of mukade's own and is removed after the import, on success and on failure. Use the
returned ID with that store, and keep the OpenCode environment configured when using the resume command.

### Pi

Read Pi and write Claude Code:

```powershell
$env:PI_CODING_AGENT_DIR = Join-Path $HOME '.pi\agent'
$env:CLAUDE_CONFIG_DIR = Join-Path $HOME '.claude'
mukade list pi
mukade hop pi YOUR_PI_SESSION_ID claude_code --cwd 'C:\work\my-project'
```

A portable Pi destination:

```powershell
$env:CLAUDE_CONFIG_DIR = 'D:\portable\claude\config'
$env:PI_CODING_AGENT_DIR = 'D:\portable\pi\agent'
mukade hop claude_code YOUR_CLAUDE_SESSION_ID pi --cwd 'C:\work\my-project' --pi-exe 'D:\portable\pi\bin\pi.cmd'
```

Mukade writes the new Pi session below `PI_CODING_AGENT_DIR\sessions`, in the folder Pi itself
would use for the project. The receipt's `resume` contains the chosen launcher and the session
file path. Keep `PI_CODING_AGENT_DIR` set when running it; separately verify the portable Pi
installation's provider credentials and configuration. A separate agent directory does not by itself
prove that every resource Pi reads is isolated. Sessions written this way were run against Pi 1.2.0
and 1.4.0 (see `docs/verification/2026-10-28-pi-live.md`).

Each harness has its own reference page, with how it is installed and launched, its store,
compaction, tools, plugins and hooks: [`docs/harnesses/`](harnesses/).

## 10. Discover, export, import

`detect` inspects store evidence; it does **not** identify an installation directory or configure an
executable. It can return zero or multiple candidates. An empty or ambiguous root cannot be inferred;
pass the harness explicitly when you know it. For a nonstandard OpenCode database filename, pass
`opencode` and its file path explicitly rather than relying on detection.

```powershell
mukade detect 'D:\portable\pi\agent'
mukade list --root 'D:\portable\pi\agent'
mukade dump pi YOUR_PI_SESSION_ID --root 'D:\portable\pi\agent' --output 'C:\work\session.json'
$env:CLAUDE_CONFIG_DIR = 'D:\portable\claude\config'
mukade write claude_code 'C:\work\session.json' --cwd 'C:\work\my-project' --claude-exe 'D:\portable\claude\bin\claude.exe'
```

With `--root` the harness may be left out of `list`, `dump` and a hop's source: it is inferred from
the directory's own layout when exactly one harness is recognized. `write` always needs it.

## 11. Troubleshooting

- **No store or session found** (exit 2): check the variable, `--root`, source ID and whether the
  intended project/store contains that session. Use `mukade detect <path>` for diagnostic evidence.
- **Ambiguous or empty detection:** specify the harness explicitly; a newly created empty store has
  no reliable session layout to infer.
- **Cannot run the executable:** verify the launcher path, its runtime and permissions. A custom
  OpenCode source requires `MUKADE_OPENCODE_EXE`; a destination uses its corresponding flag or env var.
- **Codex version or OpenCode project/model missing:** provide the actual destination version,
  project ID, provider or model using the flags described above.
- **Resume fails after a successful write** (exit 5): the session already exists. Mukade still
  prints its receipt on stdout and saves `--manifest`: use that `id` and `location` instead of
  retrying, which would create another session. A resume that outlives its timeout is stopped,
  including a harness behind a `.cmd` launcher. Check configuration and credentials in the process
  environment; provider availability must be verified in your own environment.
- **Manifest could not be saved:** the session was still written, and its receipt is on stdout; the
  error names its location. Save the `manifest` field of that receipt instead of running again.
- **It is slower than the table in [Speed](#5-speed):** run with `--timing`. A large `probe …` line
  is a harness being started to ask its version or project: pass `--dest-version` (and
  `--project-id`), so the question is not asked at all. (The answer is remembered after the first
  time, in `mukade-data/cache`, when the executable's folder can be written to.)
- **"The paging file is too small" / "memory allocation failed":** the machine ran out of memory
  (a 30 MB session needs about 300 MB). Close other programs and retry; nothing is half-written,
  because a session is written to a temporary file and renamed into place.
- **A path or file name problem on Windows:** Pi files a session under a folder named after the
  whole project path, so a very deep project path can pass the 260-character limit; use a shorter
  `--cwd`.
