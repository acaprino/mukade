# Using mukade — command-line guide

Use this page to run `mukade` from a shell. Mukade moves a coding-agent session from one harness to
another — Claude Code, Codex CLI, OpenCode, Pi — and the destination continues it **as one of its
own sessions**. Find your task below; every command has its own section with steps and copy-paste
examples.

- The sessions explorer window is a separate crate: [mukade-tui](../mukade-tui/README.md) (its
  commands, keys, resume and hop examples).
- Paths, session IDs, project IDs, model and provider names in the examples are **illustrative**:
  use the values of your installation.
- Examples are Windows PowerShell. On macOS or Linux use `/` paths and `export NAME=value` instead
  of `$env:NAME = 'value'`; a POSIX block is shown where more than that differs.

## Pick your task

**Find**

| I want to… | Run | Section |
|---|---|---|
| see which harnesses are installed, and their stores | `mukade harnesses` | [harnesses](#mukade-harnesses) |
| find every installation below a folder, portable ones too | `mukade scan <dir>` | [scan](#mukade-scan) |
| know which harness a folder belongs to | `mukade detect <dir>` | [detect](#mukade-detect) |
| find a session ID | `mukade list <harness>` | [list](#mukade-list) |

**Move**

| I want to… | Run | Section |
|---|---|---|
| continue a session in another harness | `mukade hop <from> <id> <to>` | [Quick start](#2-quick-start), [hop](#mukade-hop) |
| try a hop without touching the real store | `hop` into a scratch store | [Rehearse first](#rehearse-first) |
| move a session to another machine or person | `dump` there, `write` here | [Move a session to another machine](#move-a-session-to-another-machine) |
| get OpenCode's import document, writing nothing | `mukade document opencode …` | [document](#mukade-document) |
| run mukade on a machine I do not own | copy the executable | [Get it](#1-get-it) |

**Shape and check**

| I want to… | Use | Section |
|---|---|---|
| carry a long session in less space | `--compaction`, `--intent` | [Make a long session smaller](#make-a-long-session-smaller) |
| leave out half-done turns | `--interrupted drop` | [Interrupted turns](#a-session-with-interrupted-turns) |
| carry a rewound Codex thread whole | `--all-branches` | [Rewound or branched](#a-session-that-was-rewound-or-branched) |
| know exactly what a hop lost | the receipt, `--manifest` | [Receipt and manifest](#4-reading-the-receipt-and-the-manifest) |
| understand an error | exit code, message | [Exit codes](#11-exit-codes), [Troubleshooting](#12-troubleshooting) |

**Contents:** 1. [Get it](#1-get-it) · 2. [Quick start](#2-quick-start) · 3. [Commands](#3-commands) ·
4. [Receipt and manifest](#4-reading-the-receipt-and-the-manifest) ·
5. [Choose what travels](#5-choose-what-travels) · 6. [Host and automation options](#6-host-and-automation-options) ·
7. [Stores and executables](#7-stores-and-executables) · 8. [Per-harness notes](#8-per-harness-notes) ·
9. [Speed](#9-speed) · 10. [Where its files go](#10-where-its-files-go) · 11. [Exit codes](#11-exit-codes) ·
12. [Troubleshooting](#12-troubleshooting)

## 1. Get it

Mukade is one command-line executable: no installer, no configuration file.

1. Build it and copy it anywhere (a folder, a USB stick):

   ```powershell
   cargo build --release                            # target\release\mukade.exe
   Copy-Item target\release\mukade.exe D:\tools\
   D:\tools\mukade.exe --help
   ```

   ```bash
   cargo build --release                            # target/release/mukade
   cp target/release/mukade ~/tools/
   ~/tools/mukade --help
   ```

2. Run it from a shell, by its path or from a folder on `PATH`. In the examples `mukade` stands for
   whichever you use (`D:\tools\mukade.exe`, say).
3. Install and configure the harnesses you use separately: mukade moves sessions; it does not
   install harnesses or transfer their credentials.

What it does on its own:

- The Windows executable has the C runtime built in: it runs with no Visual C++ redistributable.
- On first start it copies the editable intent prompts to `intent-prompts` beside the executable
  (an existing file is never replaced).
- Its other files go to `mukade-data` beside the executable, made only when something is first
  written. A host may set an absolute `MUKADE_DATA_DIR` instead. See [Where its files go](#10-where-its-files-go).
- `mukade help`, `-h` or `--help` prints the usage text. `mukade --identity` prints
  `{"name","version","commit","target"}` as JSON.

**On a machine you do not own, or cannot install on:** copy `mukade.exe` to the stick and run it
from there. It generates what it needs next to itself, so deleting the folder leaves nothing on the
machine.

## 2. Quick start

Continue a Claude Code session in Pi.

1. See what is installed and where the stores are: `mukade harnesses`.
2. Know the **existing project directory** the session belongs to. `--cwd` names the destination's
   project directory; without it mukade uses the source's recorded one, when present.
3. Point both harnesses at their stores:

   ```powershell
   $env:CLAUDE_CONFIG_DIR   = Join-Path $HOME '.claude'
   $env:PI_CODING_AGENT_DIR = Join-Path $HOME '.pi\agent'
   ```

   ```bash
   export CLAUDE_CONFIG_DIR="$HOME/.claude"
   export PI_CODING_AGENT_DIR="$HOME/.pi/agent"
   ```

4. Find the session ID, then hop:

   ```powershell
   mukade list claude_code
   mukade hop claude_code YOUR_CLAUDE_SESSION_ID pi --cwd 'C:\work\my-project'
   ```

5. Read the one JSON receipt it prints: `id` (the new session), `resume` (how to open it),
   `manifest.losses` (what did not travel). See [section 4](#4-reading-the-receipt-and-the-manifest).
6. Open the new session with the receipt's `resume` command, from the project folder:

   ```powershell
   Set-Location 'C:\work\my-project'
   pi -p --session 'C:\Users\you\.pi\agent\sessions\--C--work-my-project--\2026-10-28T16-00-00-000Z_a7b29d83-….jsonl' 'where were we?'
   ```

   ```bash
   cd ~/work/my-project
   pi -p --session '<the location from the receipt>' 'where were we?'
   ```

Always true:

- The destination session is **new**: the source is never touched.
- The order is the harness's **native order**, never timestamps.
- **No loss is silent**: whatever was left behind or changed is a line in the receipt's manifest.
- Without `--resume-prompt` mukade only writes the session and gives you the command. A
  `--resume-prompt` *runs* the destination harness and may make a model/API call.

Prefer a window? `mukade-tui`: arrows to choose, `h` hop, `e` export, `i` import, `R` resume.

## 3. Commands

| command | does | prints |
|---|---|---|
| `mukade harnesses` | inventory of the supported harnesses on this machine | text |
| `mukade scan <dir> [--depth <n>]` | every installation below a folder | JSON |
| `mukade detect <dir>` | which harness stores a folder holds | JSON |
| `mukade list <harness> [options]` | the sessions, one per line | JSON lines |
| `mukade intents` | the editable summary prompts | JSON |
| `mukade dump <harness> <session-id> [options]` | export a session as one harness-neutral document | JSON document |
| `mukade write <harness> <document.json> [options]` | import a document as a new session | receipt |
| `mukade hop <src-harness> <session-id> <dst-harness> [options]` | read the source and write the destination in one process | receipt |
| `mukade document opencode <src-harness> <session-id> [options]` | OpenCode's import document, nothing written | JSON |

- Harness names: `claude_code`, `codex`, `opencode`, `pi`.
- With `--root` the harness may be left out of `list`, `dump` and a hop's source
  ([section 7](#7-stores-and-executables)). `write` always needs it.
- Options go anywhere after the command. An option a command does not take is refused (exit 1).
- `--timing` works with every command: see [Speed](#9-speed).

**Which options each command takes**

| option | commands | what |
|---|---|---|
| `--root <dir>` | list, dump, write, hop (source) | the store to use, and to infer the harness from |
| `--dest-root <dir>` | hop | the destination's store or installation folder |
| `--cwd <dir>` | write, hop, document | the project the destination files the session under (a relative path is resolved against where the command runs) |
| `--from-file <path>` | dump, hop, document | read the source from a file instead of its store |
| `--archived <true\|false>` | list | include (default) or leave out archived sessions |
| `--depth <n>` | scan | folder levels to descend (default 8) |
| `--output <path>` | dump | write the document to this file (UTF-8) instead of stdout |
| `--manifest <path>` | write, hop | also save the manifest there |

**What travels** ([section 5](#5-choose-what-travels))

| option | commands | what |
|---|---|---|
| `--compaction <mode>` | dump, hop (`native`: document, hop) | `none` (default), `portable`, `summary`, `native` |
| `--intent <name>` | dump, hop | summary prompt `intent-prompts/<name>.md`; implies `--compaction summary` |
| `--keep-turns <n>`, `--summary-model <id>`, `--summary-timeout <s>` | dump, hop | with `summary` only: last n user turns verbatim (default 0); the model to ask (default: the session's own); deadline (default 300) |
| `--context-window <n>`, `--max-output-tokens <n>`, `--text-token-factor <f>` | document, hop | with `native` only |
| `--interrupted keep\|drop` | write, hop, document | whether the work of unfinished turns travels |
| `--all-branches` | dump, hop, document | every branch a Codex thread keeps |
| `--without-tools` | write, hop, document | omit historical tool calls and results |
| `--reasoning <mode>` | write, hop, document | `drop_foreign` (default), `as_text`, `drop` |
| `--dest-protocol <p>` | write, hop, document | `anthropic-messages`, `openai-responses`, `openai-chat` |
| `--dest-shell <s>` | write, hop, document | `bash`, `sh`, `zsh`, `powershell`, `pwsh` |
| `--tool-map <path>` | write, hop, document | explicit source-to-destination tool bindings |

**The destination** ([section 6](#6-host-and-automation-options), [section 7](#7-stores-and-executables))

| option | commands | what |
|---|---|---|
| `--dest-version <v>` | write, hop, document | the destination's own version stamp |
| `--model-provider <id>`, `--model <id>` | write, hop, document | the destination's provider and model |
| `--project-id <id>` | write, hop, document | the destination's project (OpenCode requires one) |
| `--dest-id <id>`, `--dest-path <path>` | write, hop, document | a reserved session ID; a reserved file (file-based destinations) |
| `--dest-agent <name>`, `--metadata-file <path>` | write, hop, document | OpenCode only: primary agent; JSON session metadata |
| `--expect-sha256 <hash>` | hop | refuse when the source's fingerprint changed |
| `--resume-prompt <text>` | write, hop | resume in the destination and keep its reply in the receipt |
| `--claude-exe`, `--codex-exe`, `--opencode-exe`, `--pi-exe <path>` | write, hop, document, dump (summary) | the binary to spawn |

### `mukade harnesses`

Lists every supported harness with the executable and store mukade finds on this machine, its
version, session and fork counts, and the lineage each format records.

1. Optional: set store variables or `MUKADE_*_EXE` for non-default locations. It honors
   `CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `OPENCODE_DB`, `PI_CODING_AGENT_DIR` and `MUKADE_*_EXE`.
2. Run `mukade harnesses`.
3. Read one block per harness:

| line | says |
|---|---|
| `<harness> (<name>) - <status>` | the executable is `configured` (`MUKADE_*_EXE`), `installed` (`PATH` or the catalog location) or `not found` |
| `binary:` | the **one** resolved executable, not every copy on disk, and where it came from |
| `version:` | what the executable answers to `--version`; `unknown` otherwise |
| `store:` | the store path, its variable, and `environment` or `default` |
| `status:` | the store's state (below) |
| `sessions:`, `forks:`, `lineage:`, `layout:` | counts, lineage support and file layout; unknown is shown as unknown, never guessed |

The `status:` line, so a store that is there but unreadable is never mistaken for an empty one:

| state | meaning |
| --- | --- |
| `recognized` | the store holds files in that harness's own layout and mukade listed them |
| `recognized, read error: <reason>` | the layout is right but counting failed; the reason follows |
| `present, layout not recognized` | the path exists but no file matches that harness's layout. An explicit store variable skips this check and is read directly (failures then show as a read error). `mukade detect <dir>` prints the evidence it looked for |
| `read error: <reason>` | the harness reader could not be opened at all |
| `not found` | no store at the default or the configured path |
| `non-persistent (:memory:)` | OpenCode's `OPENCODE_DB` points at an in-memory database |

Good to know:

- It reads store files without modifying them and asks a discovered executable for its version.
- It is the only command that falls back to default store locations: the explicit-root behavior of
  `list`, `dump`, `write` and `hop` does not change.
- It takes no options and prints text, not JSON: it is meant for a person. A program that needs the
  same facts uses the library or `mukade list`, whose rows are JSON.
- The locations it consults, per harness and platform, are the [harness catalog](reference/harness-catalog.md)
  (`catalog/harnesses.json`), which ships inside the executable.

To enumerate `PATH` matches yourself:

```powershell
Get-Command claude,codex,opencode,pi -All -ErrorAction SilentlyContinue |
  Select-Object Name,Source
```

```bash
for h in claude codex opencode pi; do command -v "$h" || true; done
```

For package-managed installs use `npm prefix -g`, `npm root -g` or `brew --prefix`, then run each
binary's `--version`. Custom executables and stores: [section 7](#7-stores-and-executables). The full
per-OS procedure and the canonical native locations:
[Finding existing installations](reference/harness-catalog.md#finding-existing-installations).

### `mukade scan`

Finds **every** installation below a folder, portable and custom ones included.

1. Scan: `mukade scan D:\` or `mukade scan D:\Apps --depth 4`.
2. Read the JSON: `path`, `depth`, `visited`, `truncated`, and `installations`, one entry each with
   `harness`, `store` (as the harness's variable names it), `exe` (the executable kept beside the
   store; `null` when there is none, and the standard resolution then applies) and `evidence`.
3. Pass a found folder as `--root` (source) or `--dest-root` (destination):

```powershell
mukade scan 'D:\Apps'
mukade list --root 'D:\Apps\JanuraCode'
mukade hop claude_code YOUR_CLAUDE_SESSION_ID opencode --dest-root 'D:\Apps\JanuraCode' --cwd 'C:\work\my-project'
```

Example: a portable JanuraCode copy is an OpenCode installation whose store is
`<app>\data\share\opencode\opencode.db` and whose executable is `<app>\resources\opencode-cli\opencode.exe`.

How it walks:

- 8 folder levels by default (`--depth <n>` changes it); it takes no other option.
- It never follows links or junctions; it skips `node_modules`, `.git` and `target`, and does not
  enter a store's own session folders.
- It reads at most 100,000 folders. `"truncated": true` says the depth or that budget stopped it
  before the whole tree was read.
- An installation is recognized by its store: a copy that was never started has no sessions yet and
  is not reported.

### `mukade detect`

Prints which harness stores one folder holds, with the evidence.

1. Run `mukade detect 'D:\portable\pi\agent'`.
2. Read the JSON: `path` and `candidates`, each with `harness` and `evidence` (session records, Pi's
   `models.json`, or OpenCode's `opencode.db`).
3. Pass the harness explicitly to later commands when the answer is empty or ambiguous.

- It can return zero or several candidates. An empty folder tree infers nothing: a newly created
  store has no reliable session layout.
- `opencode.db` can be passed directly. Other SQLite files (Codex's `state_*.sqlite`, say) are not
  OpenCode. For a nonstandard OpenCode database filename, pass `opencode` and the file path
  explicitly rather than relying on detection.

### `mukade list`

Prints one JSON line per session.

1. Set the harness's store variable, or pass `--root`. Claude Code, Codex and Pi need an explicit
   root. `list opencode` needs `OPENCODE_DB` or `--root` (it never falls back to OpenCode's
   configured store) and reads the database read-only, without starting OpenCode.
2. Run `mukade list <harness>`.
3. Take the `id` of the session you want.

```powershell
$env:CLAUDE_CONFIG_DIR   = Join-Path $HOME '.claude';     mukade list claude_code
$env:CODEX_HOME          = Join-Path $HOME '.codex';      mukade list codex
$env:OPENCODE_DB         = 'C:\path\to\opencode.db';      mukade list opencode
$env:PI_CODING_AGENT_DIR = Join-Path $HOME '.pi\agent';   mukade list pi
mukade list --root 'D:\portable\pi\agent'                 # harness inferred from the folder
mukade list codex --archived false                        # without the archived threads
```

```bash
CLAUDE_CONFIG_DIR="$HOME/.claude" mukade list claude_code
CODEX_HOME="$HOME/.codex" mukade list codex
OPENCODE_DB="$HOME/.local/share/opencode/opencode.db" mukade list opencode
PI_CODING_AGENT_DIR="$HOME/.pi/agent" mukade list pi
```

#### The list output

```json
{"id":"01a1a817-…","title":"Alpha work","cwd":"D:\\work\\alpha","timestamp":"2026-11-08T14:03:20.000+00:00","location":"C:\\Users\\you\\.codex\\sessions\\2026\\11\\08\\rollout-….jsonl","archived":false,"unreadable":null,"live":null}
```

| key | meaning |
|---|---|
| `id`, `title`, `cwd` | what the store records; any may be `null` |
| `timestamp` | the session's **last activity**: Claude Code and OpenCode from the store, Codex from its state database or else the rollout file's last write, Pi from the newest record time; `null` when nothing says |
| `location` | the file (or database) the session is recorded in |
| `archived` | the harness marked it archived |
| `unreadable` | `null` when nothing is known against reading it; otherwise **why `dump` and `hop` refuse it** (exit 2, same words) |
| `live` | `true` when a process of the harness runs the session now; `false` when the store records running sessions and none runs it; `null` when the store does not say |

A harness forgets transcripts long before it forgets sessions, so `list` reports both. The
`unreadable` rows are how a caller sees every project:

| harness | an `unreadable` row is |
|---|---|
| Codex | a thread with a row in the home's newest `state_<n>.sqlite` whose rollout file is gone. `location` is the path the row records. Subagent and guardian threads are not listed |
| Claude Code | a session `history.jsonl` still has prompts for, whose session file is gone (`location` is `history.jsonl`; `title` is its first prompt); or a running session that has not written its transcript yet |
| OpenCode | a session in another `opencode*.db` beside the selected one (read it by selecting that file), or one only in the legacy `session` table, which OpenCode cannot export |

A project is the distinct `cwd` of the rows. To count only what can be hopped, drop the rows with an
`unreadable` reason; to count every project a harness remembers, keep them all:

```powershell
mukade list codex | ConvertFrom-Json | Where-Object { -not $_.unreadable } | Select-Object -ExpandProperty cwd -Unique
mukade list claude_code | ConvertFrom-Json | Where-Object { $_.live } | Select-Object id, cwd
```

```bash
mukade list codex | jq -r 'select(.unreadable == null) | .cwd' | sort -u
mukade list claude_code | jq -c 'select(.live == true) | {id, cwd}'
```

**`live`** is Claude Code's: it keeps `sessions/<pid>.json` while a process runs a session. The check
looks at the process and its start time, so a reused process ID is not the session: on Windows the
creation time, on Linux `/proc/<pid>/stat` (verified with Claude Code 2.1.322 on 2026-11-11). On
macOS the start time is not compared, so a reused ID reads as live. Codex, OpenCode and Pi keep no
such record, so their `live` is `null`: **a cleaner must read `null` as "not known", not as "not
running".**

#### Archived sessions

`--archived <true|false>` works the same for every harness:

| value | rows printed |
|---|---|
| `true`, or the flag left out (the default) | every session, the archived ones included |
| `false` | only the rows whose `archived` is `false` |

Any other value, or `--archived` on a command other than `list`, is a command-line mistake (exit 1,
nothing printed). It combines with `--root`.

| harness | `archived` is `true` when |
|---|---|
| Codex | the rollout is in `<CODEX_HOME>/archived_sessions/` (where `codex archive` moves it), or its row in `state_<n>.sqlite` says `archived`. Found with or without that database; `dump` and `hop` read it |
| OpenCode | the session has a `time_archived` (top-level sessions) |
| Claude Code, Pi | never: their stores record no archived flag, so `--archived false` leaves nothing out |

```powershell
# the threads that are not archived
mukade list codex --archived false

# the same for an OpenCode database named directly
mukade list opencode --root 'C:\path\to\opencode.db' --archived false

# only the archived ones, with where they are kept
mukade list codex | ConvertFrom-Json | Where-Object { $_.archived } | Select-Object id, title, location

# the projects that still have work not archived
mukade list codex --archived false | ConvertFrom-Json | Select-Object -ExpandProperty cwd -Unique
```

```bash
mukade list codex | jq -c 'select(.archived) | {id, title, location}'
mukade list codex --archived false | jq -r '.cwd' | sort -u
```

### `mukade intents`

Lists the editable summary prompts as JSON, without reading a session or starting a harness. It
takes no options.

1. Run `mukade intents`.
2. Read `directory`, `default`, and every prompt with `name`, `path`, `builtin` (the name ships with
   mukade) and `custom` (a file in the folder supplies it).
3. Use a name with `--intent` ([Choose what the summary emphasizes](#choose-what-the-summary-emphasizes)).

A trimmed answer (paths are illustrative):

```json
{
  "directory": "D:\\tools\\intent-prompts",
  "default": "generic",
  "prompts": [
    {"name": "generic", "path": "D:\\tools\\intent-prompts\\generic.md", "builtin": true, "custom": true},
    {"name": "review", "path": "D:\\tools\\intent-prompts\\review.md", "builtin": true, "custom": true},
    {"name": "security", "path": "D:\\tools\\intent-prompts\\security.md", "builtin": false, "custom": true}
  ]
}
```

### `mukade dump`

Exports a session as one harness-neutral JSON document (**export**; the TUI's `e`).

1. Set the source's store variable, or pass `--root`.
2. Run `mukade dump <harness> <session-id> --output <file>`. Use `--output`, not a shell pipe:
   PowerShell re-encodes piped text with the console code page and garbles non-ASCII text.
   `--output` writes UTF-8.
3. Protect the file like the original session: it holds conversation data.

```powershell
mukade dump claude_code YOUR_CLAUDE_SESSION_ID --output C:\work\session.json
mukade dump codex YOUR_CODEX_SESSION_ID --output C:\work\session.json
mukade dump opencode YOUR_OPENCODE_SESSION_ID --output C:\work\session.json
mukade dump pi YOUR_PI_SESSION_ID --root 'D:\portable\pi\agent' --output 'C:\work\session.json'
mukade dump codex --from-file 'C:\path\rollout.jsonl' --output C:\work\session.json   # no ID with --from-file
mukade dump codex YOUR_CODEX_SESSION_ID --intent analyze --output 'C:\work\summary.json'
```

- **`dump` converts nothing.** Every tool call keeps its name, its arguments and the canonical form
  mukade read it as; reasoning keeps its text and opaque part. Conversion happens when the document
  is **written**, so one document written into any of the four harnesses gives the result a hop
  would.
- It exports the session as it is, interrupted turns included; what is left out is decided when it
  is written.
- It takes `--compaction portable|summary` and `--intent`; `native` is refused, because the document
  is harness-neutral. It does not take `--interrupted` or `--without-tools`.

### `mukade write`

Imports a document as a **new** session of any harness (**import**; the TUI's `i`).

1. Set the **destination's** store variable. The destination needs its own configuration and
   credentials: mukade does not transfer them.
2. Run `mukade write <harness> <document.json> --cwd <project>`. The harness is always required: a
   destination store can legitimately be empty.
3. Read the receipt ([section 4](#4-reading-the-receipt-and-the-manifest)).

```powershell
$env:CLAUDE_CONFIG_DIR = 'D:\portable\claude\config'
mukade write claude_code 'C:\work\session.json' --cwd 'C:\work\my-project' --claude-exe 'D:\portable\claude\bin\claude.exe'

$env:CODEX_HOME = 'D:\codex\home'
mukade write codex C:\work\session.json --cwd 'D:\work\my-project'

$env:OPENCODE_DB = 'C:\path\to\opencode.db'
mukade write opencode 'C:\work\summary.json' --cwd 'C:\work\my-project'

$env:PI_CODING_AGENT_DIR = Join-Path $HOME '.pi\agent'
mukade write pi 'C:\work\session.json' --cwd 'C:\work\my-project' --interrupted drop
```

- `--root` names the destination store; the executable kept beside that store imports into it.
- It takes `--without-tools`, `--interrupted drop` and the destination options like `hop`.
- Compaction is chosen on the source, by `dump`: `write` refuses `--compaction` and `--intent`, and
  imports the summary a document already holds.
- A document saved with a UTF-8 byte-order mark is accepted.

#### Move a session to another machine

1. Machine A: `mukade dump claude_code YOUR_CLAUDE_SESSION_ID --output C:\work\session.json`.
2. Move the file.
3. Machine B: set the destination's store variable, then `write`:

```powershell
$env:CODEX_HOME = 'D:\codex\home'
mukade write codex C:\work\session.json --cwd 'D:\work\my-project'
```

```bash
export CODEX_HOME="$HOME/.codex"
mukade write codex ~/work/session.json --cwd ~/work/my-project
```

### `mukade hop`

Reads the source and writes the destination in one process: `dump` and `write` without the file.

1. Set **both** harnesses' store variables (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `OPENCODE_DB` with
   its `XDG_*`, `PI_CODING_AGENT_DIR`). Claude Code, Codex and Pi need an explicit root. With
   `OPENCODE_DB` unset, the OpenCode export, import and resume run against OpenCode's own
   configured store.
2. Find the ID: `mukade list <src-harness>`.
3. Run `mukade hop <src-harness> <session-id> <dst-harness> --cwd <project>`.
4. Read the receipt; open the session with its `resume` command, with the destination's store
   variable still set.

Any pair works:

```powershell
mukade hop claude_code YOUR_CLAUDE_SESSION_ID pi          --cwd 'C:\work\my-project'
mukade hop claude_code YOUR_CLAUDE_SESSION_ID codex       --cwd 'C:\work\my-project'
mukade hop codex       YOUR_CODEX_SESSION_ID  claude_code --cwd 'C:\work\my-project'
mukade hop opencode    YOUR_OPENCODE_SESSION_ID pi        --cwd 'C:\work\my-project'
mukade hop pi          YOUR_PI_SESSION_ID     opencode    --cwd 'C:\work\my-project' --project-id YOUR_PROJECT --model-provider YOUR_PROVIDER --model YOUR_MODEL
```

- Tool calls are translated to what the destination really offers. A call it cannot express travels
  as text, with its arguments, and is counted (`tools_text_form`).
- `--resume-prompt 'continue'` also runs the destination once and keeps its reply in the receipt
  (`resume_output`).
- `--root` selects the **source**, even when both ends are the same harness. The destination is its
  environment variable, or `--dest-root` ([section 7](#7-stores-and-executables)).
- With `--from-file` the source is that file; the `<session-id>` position is still written.
- A hop is repeatable: each run writes another new session and changes nothing else.

#### Rehearse first

See what a hop would do before it touches your real store: point the **destination's** store
variable at an empty scratch folder.

1. Set the source's real store and a scratch destination:

   ```powershell
   $env:CLAUDE_CONFIG_DIR   = Join-Path $HOME '.claude'      # the real source
   $env:PI_CODING_AGENT_DIR = 'C:\work\rehearsal\pi'         # an empty scratch destination
   mukade hop claude_code YOUR_CLAUDE_SESSION_ID pi --cwd 'C:\work\my-project' --manifest C:\work\rehearsal\manifest.json
   ```

2. Read the manifest ([section 4](#4-reading-the-receipt-and-the-manifest)).
3. When it is what you want, set the variable to the real store and run it again.

### `mukade document`

Builds the OpenCode import document for a session and prints it with a report. **Nothing is
written.** OpenCode is the only destination it supports.

1. Set the source's store variable (or use `--from-file`).
2. Run `mukade document opencode <src-harness> <session-id>` with the OpenCode facts
   (`--project-id`, `--model-provider`, `--model`) and `--cwd` when the source records none.
3. Read the JSON: `document`, `native_compaction` (or `null`), `turns`, `interrupted_last_turn`,
   `counts` (`messages_in`, `messages_out`, `tools_translated`, `tools_text_form`, `tools_omitted`)
   and `losses`.

```powershell
mukade document opencode claude_code YOUR_CLAUDE_SESSION_ID --project-id YOUR_OPENCODE_PROJECT_ID --interrupted drop
mukade document opencode codex YOUR_CODEX_THREAD_ID --from-file 'C:\path\rollout.jsonl' --cwd 'C:\work\my-project' --project-id YOUR_PROJECT --model-provider YOUR_PROVIDER --model YOUR_MODEL --compaction native --context-window 1000000 --max-output-tokens 64000 --text-token-factor 1.3333
```

- It needs a working directory and at least one complete turn (otherwise `noCompleteTurn`).
- The document's session ID is `ses_preview` unless `--dest-id` gives one.
- The plan is held to the provider rules first: a document that would violate them is refused.
- `native_compaction` reports `keep_id`, `summary_chars`, `estimated_tokens`, `projected_tokens`,
  `summarized_messages`, `summarized_user_messages`, `budget`, `context_window` and `fits`.

## 4. Reading the receipt and the manifest

`hop` and `write` print one JSON object. This one is a real run: the Claude Code fixture hopped into
Pi with `--compaction portable` (long paths shortened):

```json
{
  "id": "a7b29d83-3e83-4c9a-87fd-b136eba01dfa",
  "location": "<PI_CODING_AGENT_DIR>/sessions/--…--/2026-10-28T16-00-00-000Z_<id>.jsonl",
  "resume": ["pi", "-p", "--session", "<that file>"],
  "manifest": {
    "source": { "harness": "claude_code", "version": "2.1.311",
                "session_id": "c45db7bc-…", "content_sha256": "df8f8f84d7fd3d21…" },
    "dest":   { "harness": "pi", "version": "1.2.0", "session_id": "a7b29d83-…", "location": "…" },
    "options": { "compaction": "portable", "summary": null, "reasoning": "drop_foreign", "resume_prompt": false,
                 "without_tools": false, "interrupted": "keep" },
    "counts": { "messages_in": 5, "messages_out": 5, "tool_calls": 0, "tools_text_form": 0,
                "reasoning_dropped": 0, "reasoning_as_text": 0,
                "dropped_orphan_results": 0, "dropped_duplicate_results": 0,
                "rewritten_call_ids": 0, "dropped_off_branch": 0, "dropped_unfinished_lines": 0,
                "aborted_steps_as_replies": 0, "dropped_interrupted_steps": 0,
                "unanswered_calls": 0, "dropped_empty_steps": 0,
                "dropped_attachments": 0, "dropped_result_blocks": 0,
                "dropped_unreadable_summaries": 0 },
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

| field | what it tells you |
|---|---|
| `id`, `location`, `resume` | the new session's ID, where it was written, and the destination's own command to continue it. Keep the destination's store variable set when you run it by hand |
| `resume_output` | the destination's reply, when you passed `--resume-prompt` |
| `counts` | every kind of loss as a number; every number above zero also has a line in `losses` |
| `strict` | the plan was held against the rules a provider API enforces (every tool call answered once, IDs in the allowed pattern, …) **before** anything was written; a plan that would be refused is not written (exit 4) |
| `source.content_sha256` | the SHA-256 of the **source file's own bytes** |

**Audit what a conversion lost.**

1. Save the manifest with `--manifest <file>` (the receipt prints it anyway).
2. Read `counts` and `losses`: what was dropped, rewritten, or written as text.
3. Check which exact file was carried: compare `source.content_sha256` with
   `Get-FileHash -Algorithm SHA256 <file>` (or `sha256sum <file>`).

What the `losses` lines mean:

| line | what happened |
|---|---|
| `interrupted_last_turn` | the source ended on a turn it never finished (the harness was killed, or the turn was interrupted). With no reply at all it travels as a user message alone; what the agent had done before the interruption travels too, unless `--interrupted drop`. Normal. |
| `steps of interrupted turns omitted by request, with their tool results (n)` | `--interrupted drop`: the work of the turns the source never finished was left out; their prompts travelled. |
| `tool calls and their results omitted by request (n)` | `--without-tools`. |
| `reasoning dropped: <reason> (n)` | thinking can only be replayed to a provider that speaks the same protocol and whose format can hold its signature or encrypted payload; otherwise it is dropped, one line per reason: *the destination's provider protocol is not known; name it with --dest-protocol* (OpenCode, Pi), *made by another provider protocol*, *no signature or encrypted payload to replay it with*, *the destination's format has no place for its payload*, *not carried, as --reasoning drop asks*, *encrypted only, no visible text to write as text*. `--reasoning as_text` keeps it as labelled text instead. |
| `reasoning written as labelled text, no longer replayable by a provider (n)` | `--reasoning as_text`: the destination's model can read it, its provider cannot replay it. |
| `tool calls written as text, not translated (n, Tool, …)` | the destination has no equivalent tool, or an argument had no counterpart; the call and its result travel as text. Nothing is invented. |
| `unanswered tool calls carried as text (n)` | the source never recorded a result for a call (a killed tool); a provider refuses a call without its result, so it travels as text. |
| `orphan tool results dropped (n)` · `duplicate tool results dropped, the first answer kept (n)` | a result with no call, or a second answer to one call; a provider would refuse either. |
| `tool call ids rewritten to the destination's id pattern (n)` | Anthropic accepts only `[a-zA-Z0-9_-]` IDs; the call and its result were rewritten together. |
| `records of an abandoned branch … left behind (n)` | a rewind, an edited prompt, or the earlier file of a Codex paginated thread: only the resumed branch travelled. `--all-branches` carries a Codex thread whole. |
| `compaction summaries with no readable text (encrypted by the source's provider), not carried (n)` | Codex 0.165 keeps its compaction summary encrypted: there is no text to write, so it is left out rather than written as an empty message. The history before and after it travels. |
| `an unfinished last line of the source left unread (n)` | a hard kill cut the last line mid-write; the rest of the session was read. |
| `steps the source aborted or that failed, written as ordinary replies (n)` | the destination cannot mark a step as aborted (Pi can, and does). To leave the interrupted ones out, use `--interrupted drop`. |
| `empty steps dropped (n)` · `user attachments dropped (n)` · `structured tool-result blocks flattened to text (n)` | content the destination cannot hold (a PDF, an image toward a text-only result) is counted, never silently dropped. Images are rewritten into each harness's own shape. |
| `the source's compaction summary travels as one user message` | `--compaction portable`. |
| `the resume prompt did not complete: …` | the session was written but `--resume-prompt` failed (exit 5): see [troubleshooting](#12-troubleshooting). |

## 5. Choose what travels

### Make a long session smaller

| `--compaction` | the destination receives | model call | commands |
|---|---|---|---|
| `none` (default) | the whole history | none | all |
| `portable` | one handoff message holding the **source's own compaction summary**, then the tail that compaction kept | none | dump, hop |
| `summary` | a summary the source's model writes now; `--keep-turns n` adds the last n user turns verbatim | one, on the source's account | dump, hop |
| `native` | the whole history, plus the destination's own compaction record with a deterministic summary | none | document, hop |

#### `portable`: reuse the source's compaction

*The history is huge but only the recent turns matter.*

```powershell
mukade hop claude_code YOUR_CLAUDE_SESSION_ID pi --cwd 'C:\work\my-project' --compaction portable
```

- The source must have compacted at least once (a Claude `/compact`, a Pi compaction, …). Otherwise
  the hop is refused with a reason and nothing is written (exit 2).
- A Codex 0.165 summary is encrypted, so `portable` is refused for it (exit 2).
- The manifest keeps the line `the source's compaction summary travels as one user message` and how
  many entries were kept.

#### `native`: keep everything, add a native boundary

*The imported history is larger than the destination model's useful budget, but nothing should be
removed.* Mukade keeps the whole generated session and appends the destination's own compaction
record with a deterministic, free summary of the earlier user requests. The destination then sends
that summary plus the recent tail; the older rows stay in the session and remain readable.

```powershell
mukade document opencode codex YOUR_CODEX_THREAD_ID --from-file 'C:\path\rollout.jsonl' --cwd 'C:\work\my-project' --project-id YOUR_PROJECT --model-provider YOUR_PROVIDER --model YOUR_MODEL --compaction native --context-window 1000000 --max-output-tokens 64000 --text-token-factor 1.3333
mukade hop codex YOUR_CODEX_THREAD_ID claude_code --cwd 'C:\work\my-project' --compaction native --context-window 200000
```

| destination | the record written |
|---|---|
| OpenCode | `compaction` with `metadata.keepID` |
| Pi | `compaction` with `firstKeptEntryId` |
| Codex | `compacted` with `replacement_history` |
| Claude Code | `compact_boundary` followed by an `isCompactSummary` row |

- `--context-window` is required (above zero). `--max-output-tokens` defaults to 16000.
  `--text-token-factor` is optional (default: chars/3; a positive number).
- The budget is the smaller of half the window and OpenCode's verified automatic-compaction ceiling
  minus room for the next turn (`src/native_compaction.rs` `import_budget()`).
- `document` reports the boundary under `native_compaction`; `hop` puts `"compaction": "native"` in
  the manifest.
- No source or destination model is called. `dump` refuses this mode: its document is
  harness-neutral.

#### `summary`: the source's own model writes one

*The session is long, it never compacted, or its summary is encrypted (Codex).* The source harness
summarizes the session with its own model, login and configuration, in a run that saves nothing in
its store; the destination starts from that summary.

```powershell
mukade hop claude_code YOUR_CLAUDE_SESSION_ID pi --cwd 'C:\work\my-project' --compaction summary

# export the summary and import it later, or elsewhere
mukade dump codex YOUR_CODEX_SESSION_ID --compaction summary --output 'C:\work\summary.json'
mukade write opencode 'C:\work\summary.json' --cwd 'C:\work\my-project'
```

| option | effect |
|---|---|
| `--keep-turns 2` | also keep the last two user turns verbatim after the summary |
| `--summary-model <id>` | a cheaper model (default: the session's own) |
| `--summary-timeout <s>` | bounds the run, in seconds above zero (default 300); a value too large for the clock sets no bound |

- Cost: one model call on the source's account. The manifest's `options.summary` says which model
  wrote it, which intent prompt was used, how long it is and, for Claude Code, what it cost.
- If the summary cannot be written (an error, an empty reply, the deadline) nothing is written
  (exit 4).
- Refused (exit 2): with `--all-branches` (the summary stands for the branch the source resumes);
  with `--from-file` unless the source is Pi (only Pi forks a session from a file).
- When OpenCode's fork could not be deleted, `dump` prints on stderr
  `the summarizing run left a fork in the source store: run <command>`.

#### Choose what the summary emphasizes

`--intent <name>` selects a prompt for the source model. It implies `--compaction summary`, so the
destination receives the summary alone unless `--keep-turns` adds turns.

1. See the prompts: `mukade intents`.
2. Pick one:

   ```powershell
   mukade hop claude_code YOUR_CLAUDE_SESSION_ID pi --cwd 'C:\work\my-project' --intent review
   mukade hop codex YOUR_CODEX_THREAD_ID claude_code --intent execute --keep-turns 2
   mukade dump codex YOUR_CODEX_SESSION_ID --intent analyze --output 'C:\work\summary.json'
   ```

3. To add your own, save `<name>.md` in `intent-prompts`: `security.md` is `--intent security`.

- Shipped: `generic`, `continue`, `execute`, `review`, `analyze`, `brainstorm`, `plan`. `generic.md`
  is used when `--intent` is omitted.
- The prompts are ordinary `.md` files in `intent-prompts` beside the executable, copied there on
  first start. Edited or added files are picked up by the next run; mukade never overwrites one. A
  host may set an absolute `MUKADE_INTENT_DIR` to keep them elsewhere.
- A prompt is read as plain text and its whitespace is folded to one line before it is passed to the
  harness.
- A Windows `.cmd` launcher can receive only ASCII letters, digits, spaces and `.,;:()`; a prompt
  outside that subset is refused before the harness starts. Name the harness's native executable to
  use accents or richer punctuation.
- The intent affects only the summary prompt: it does not start the destination, change its
  permissions, or add a second model call. `dump` and `hop` take it, and only with
  `--compaction summary` or none; `write` imports the summary a document already holds.

### A session with interrupted turns

*You stopped the agent in the middle of an answer, or the harness was killed, and the session holds
half-done work.* Nothing asks for a finished turn: a session whose turns were **all** interrupted is
exported like any other, with every prompt and whatever the agent had written or run before each
interruption.

| `--interrupted` | what is written |
|---|---|
| `keep` (default) | every turn as the source holds it |
| `drop` | finished turns whole; of an interrupted turn, the prompt alone (it is yours, and complete) |

```powershell
mukade hop claude_code YOUR_CLAUDE_SESSION_ID pi --cwd 'C:\work\my-project'                     # keep
mukade hop claude_code YOUR_CLAUDE_SESSION_ID pi --cwd 'C:\work\my-project' --interrupted drop   # drop
```

What each kind of turn becomes:

| a turn in the source | `keep` | `drop` |
|---|---|---|
| finished | prompt, replies, tool calls and results | the same |
| interrupted while the model was answering | prompt and the cut-off reply. Pi marks it `aborted`; another destination writes an ordinary reply, counted in `aborted_steps_as_replies` | the prompt |
| interrupted after tools had run | prompt, calls and results | the prompt |
| interrupted before anything was written | the prompt | the prompt |

`hop`, `write` and `document` take the option. Any other value than `keep` or `drop`, or any other
command, is refused before a store is read (exit 1).

**How a turn is known to be interrupted.** Mukade does not guess: it reads the sign the source
itself wrote.

| source | the sign |
|---|---|
| Claude Code | the user row `[Request interrupted by user]`, or `[Request interrupted by user for tool use]` beside a tool result |
| Codex | the `<turn_aborted>` message and the `turn_aborted` event |
| OpenCode | an assistant row with no `finish`, or `finish: "aborted"` |
| Pi | an assistant message with `stopReason: "aborted"` |

Those markers are the harness's words, not yours: they are never written into the destination as a
prompt, in either mode.

**Check what was left out.** The manifest states the mode, counts the steps and adds a loss line
(a run on a hand-made Codex rollout of two turns, both interrupted):

```powershell
$env:PI_CODING_AGENT_DIR = 'C:\work\rehearsal\pi'         # an empty scratch destination
mukade hop codex YOUR_CODEX_THREAD_ID pi --cwd 'C:\work\my-project' --interrupted drop --manifest C:\work\rehearsal\drop.json
(Get-Content C:\work\rehearsal\drop.json | ConvertFrom-Json).counts.dropped_interrupted_steps    # 2
```

```bash
jq '.counts.dropped_interrupted_steps' ~/work/rehearsal/drop.json                                # 2
```

```json
"options": { "compaction": "none", "summary": null, "reasoning": "drop_foreign",
             "resume_prompt": false, "without_tools": false, "interrupted": "drop" },
"counts":  { "messages_in": 5, "messages_out": 2, "dropped_interrupted_steps": 2, … },
"losses":  [ "steps of interrupted turns omitted by request, with their tool results (2)",
             "interrupted_last_turn" ]
```

`messages_in` counts what the source holds, `messages_out` what was written: here the two prompts.
`interrupted_last_turn` is still stated, because the source did end on a turn it never finished.

**More examples.**

```powershell
# start the last request again in the destination, without the half-written answer
mukade hop codex YOUR_CODEX_THREAD_ID claude_code --cwd 'C:\work\my-project' --interrupted drop --resume-prompt 'Start again from my last request.'

# text only: no historical tool calls, and no interrupted work
mukade hop pi YOUR_PI_SESSION_ID codex --cwd 'C:\work\my-project' --without-tools --interrupted drop

# through a document, on another machine
mukade dump pi YOUR_PI_SESSION_ID --output C:\work\session.json
mukade write claude_code C:\work\session.json --cwd 'C:\work\my-project' --interrupted drop

# the OpenCode import document, without writing anything
mukade document opencode claude_code YOUR_CLAUDE_SESSION_ID --project-id YOUR_OPENCODE_PROJECT_ID --interrupted drop
```

What it does not do:

- **A process killed while the model was answering leaves no sign** in a Claude Code or Codex file:
  that reply reads as an ordinary one, and `drop` keeps it. (A turn killed before anything was
  written is a prompt alone, and travels as one.)
- **A document made by `dump` keeps each reply's end but not the markers.** `write --interrupted
  drop` therefore sees the turns whose last reply was cut, not the ones interrupted right after a
  tool answered. `hop` reads the source itself and sees both: prefer it.
- **A summary is not rewritten.** With `--compaction portable` or `summary`, the turns folded into
  the summary are the summary's; the option acts on the turns that travel as they are.
- It is not a `dump` or `list` option, and the sessions explorer (`mukade-tui`) always keeps.

### A session that was rewound or branched

*You rewound in Claude Code, edited a prompt, or navigated back in Pi's `/tree`.* Mukade reads the
selected branch: the abandoned turns do not become history in the destination. They stay in the
source untouched, and the manifest counts them (`dropped_off_branch`). Pi's `branch_summary`,
`bashExecution`, `custom_message` and `context_edit` entries reach the destination the way Pi shows
them to its model.

**Claude Code: how the branch is chosen.**

- `last-prompt.leafUuid` selects a checkpoint when present. A later continuation descending from it
  can supersede it. A new user branch can supersede it when the checkpoint's leaf was already
  recorded before the checkpoint. A checkpoint flushed before its own leaf is not treated as a rewind
  ordering barrier. Text beside tool results can start a new user turn too.
- Without a checkpoint, the last non-sidechain user or assistant row selects the branch.
- Parent links and compaction metadata reconstruct its order; split responses and parallel results
  are grouped by message and call IDs, even if written late. A UUID-less block, result or compaction
  summary is recovered only when those identities establish its position. An explicit parent on an
  abandoned conversation branch prevents a tool result from being recovered into the selected
  branch, even if `sourceToolAssistantUUID` names an active response.
- Missing parents, cycles, duplicate UUIDs, contradictory compaction metadata and genuinely
  ambiguous records are refused, not silently assigned to a branch.
- Real automatic compaction and rewind remain to be verified against Claude Code itself.

**Codex Desktop 0.165: a thread over several files.** Codex keeps such a thread over two files: the
earlier one, and a page that continues from part of it, which is what Codex resumes (redone again, a
thread gets a further page that continues from the previous one). Mukade reads the latest version,
the way Codex resumes it: each earlier file up to the point the next one continues from, then the
latest page. What the earlier files hold past those points is left behind and counted.

To carry **everything**, the abandoned turns included (in file order, before the active ones), add
`--all-branches` to `dump`, `hop` or `document`:

```powershell
mukade hop codex YOUR_CODEX_THREAD_ID claude_code --cwd 'D:\Projects\my-project' --all-branches
```

`--all-branches` is read for Codex only; for another harness it is refused (exit 2).

### Leave out tool calls

`--without-tools` on `hop`, `write` or `document` omits the historical calls and their results. The
manifest records `options.without_tools`, the `tools_omitted` count and the loss. It is not a `dump`
or `list` option.

### Tool calls and reasoning: what is converted, and when

**Tool calls** are translated to the destination's own tool when a session is written (`write`,
`hop`, `document`), by the rules of [`tool-conversion-tables.md`](tool-conversion-tables.md). One with
no equivalent travels as text, with its arguments, and is counted (`tools_text_form`).

**Shell calls** (`src/tools.rs` `shell()`):

- Toward Codex, OpenCode and Pi a shell call is translated only when `--dest-shell` names a shell of
  its dialect: `bash`, `sh` or `zsh` (also `posix`) for a POSIX call, `powershell` or `pwsh` for a
  PowerShell call. Without it, it travels as text.
- Toward Claude Code a POSIX call becomes `Bash`. A PowerShell call stays text: Claude's
  `PowerShell` tool is not in its offered set.

**Reasoning (chain of thought) is not converted**, because it cannot be. A block has visible text
and an opaque part — a signature over the text, or a provider-encrypted payload — that only the
provider protocol that produced it can read back, and that mukade never edits or makes up.
`--reasoning` decides what happens to each block:

| `--reasoning` | what happens |
|---|---|
| `drop_foreign` (default) | kept **natively** when the destination speaks the same protocol and its format has a place for the opaque part; otherwise dropped and counted (`reasoning_dropped`), with the reason in `losses` |
| `as_text` | written as visible text before the answer it led to, labelled `[earlier reasoning, not replayable]`: the destination's model can read it, but it is no longer reasoning to its provider; counted (`reasoning_as_text`). A block with no visible text (encrypted only) is dropped |
| `drop` | never carried |

Where the opaque part fits (`src/policy.rs` `payload_fits()`):

| destination | takes |
|---|---|
| Claude Code | a signature (`thinking`) or an encrypted payload (`redacted_thinking`) |
| Codex | an encrypted payload only |
| Pi | a signature, or an Anthropic encrypted payload, written as a `redacted: true` thinking block |
| OpenCode | either |

So Claude → Claude and Codex → Codex keep reasoning natively; Claude → Codex and Codex → Claude drop
it, or carry it as text with `as_text`.

**OpenCode and Pi:** they speak whatever provider they are configured with, so mukade does not know
their protocol, and **with the default every reasoning block toward them is dropped**. Say which
protocol the destination will use, and blocks from the same protocol are kept:

```powershell
mukade hop claude_code YOUR_CLAUDE_SESSION_ID pi --cwd 'C:\work\my-project' --dest-protocol anthropic-messages
```

- `--dest-protocol` is `anthropic-messages`, `openai-responses` or `openai-chat` (also accepted:
  `anthropic`, `claude`; `openai`, `responses`, `codex`; `chat`, `chat-completions`,
  `openai-completions`). Without it, Claude Code is `anthropic-messages` and Codex
  `openai-responses`.
- Name the protocol the destination's configured provider really uses. Mukade takes it on trust: the
  strict check refuses a block of one protocol carried into another (exit 4), but it compares against
  this value, so it cannot tell that the value itself is wrong.

## 6. Host and automation options

**Use it from a program.** A host that already runs these harnesses can have the pipeline
in-process: mukade is a library first. `mukade::hop(&HopRequest { … })` does everything the command
does, with typed errors (`HopError::ResumeFailed` carries the written session's receipt, so a caller
never has to guess whether the destination exists). See the [README](../README.md#library).

| option | rule |
|---|---|
| `--expect-sha256 <hash>` | `hop` only; 64 hex digits. Checks the captured source fingerprint before writing or starting a source-model summary; a mismatch is refused (exit 2). It checks the selected source in its entirety: an appended file changes the fingerprint rather than freezing the previous prefix |
| `--dest-id <id>` | reserves the destination session ID: `ses_<id>` for OpenCode, a UUID for Claude Code, Codex and Pi |
| `--dest-path <path>` | reserves the destination file itself, for the file-based destinations (Claude Code, Codex, Pi). Absolute, inside the destination root, free of `..`. OpenCode refuses it (exit 2): its native import allocates the file |
| `--dest-agent <name>` | OpenCode only: the primary agent |
| `--metadata-file <path>` | OpenCode only: a JSON object stored as session metadata; at most 1 MiB; protect it like the session |
| `--tool-map <path>` | explicit bindings instead of the destination catalog's defaults (below) |
| `--dest-shell <s>` | the shell the destination is configured to use: decides whether shell calls are translated ([section 5](#tool-calls-and-reasoning-what-is-converted-and-when)), and is recorded where the destination records one |

**OpenCode IDs and metadata.** When an ID or metadata is supplied, `OPENCODE_DB` must name the
destination explicitly: mukade checks the resulting ID and metadata there after the native import.
If the process fails or the check cannot confirm the write, exit 5 reports an uncertain result and
its ID; inspect the destination before retrying.

**Retrying safely.** A host that retries a hop states both `--dest-id`/`--dest-path` and
`--expect-sha256`, so the second run lands on the same destination, and only while the source is
unchanged.

**Tool map file.** A JSON array of `[source, destination, original, mapped]` tuples: the two names are
tool names (1 to 256 characters), the two values are the JSON objects the conversation holds and the
destination should receive. At most 4096 entries and 1 MiB. `hop`, `write` and `document` take it; a
malformed map is refused before anything is written.

## 7. Stores and executables

| Harness | CLI name | Store/config variable | Executable flag | Executable environment variable |
|---|---|---|---|---|
| Claude Code | `claude_code` | `CLAUDE_CONFIG_DIR` | `--claude-exe` | `MUKADE_CLAUDE_EXE` |
| Codex CLI | `codex` | `CODEX_HOME` | `--codex-exe` | `MUKADE_CODEX_EXE` |
| OpenCode | `opencode` | `OPENCODE_DB` (database file), plus its applicable `XDG_*` settings | `--opencode-exe` | `MUKADE_OPENCODE_EXE` |
| Pi | `pi` | `PI_CODING_AGENT_DIR` | `--pi-exe` | `MUKADE_PI_EXE` |

**Stores.**

- Claude Code, Codex and Pi require an explicit root, even for standard installations (an empty
  variable is no root).
- OpenCode: `list opencode` needs an explicit `OPENCODE_DB` or `--root`; without one it refuses with `OPENCODE_DB is not set: no OpenCode store
  to list`. The OpenCode processes mukade starts (`session export`, the import, a resume) get
  `OPENCODE_DB` when it is set, and otherwise run with the variable removed, against OpenCode's own
  configured store. Supply the database and environment explicitly when isolation matters.
- `--root` changes the selected harness's store for `list`, `dump` or `write`. For `hop` it applies to
  the **source**, including when both ends use the same harness; the destination is its environment
  variable, or `--dest-root`.
- For OpenCode, an existing directory passed as `--root` means its `opencode.db`; a file path is used
  as-is.
- `--root` and `--dest-root` take a store, or a folder that holds one installation. A path that is
  not a store itself is scanned as `mukade scan` would, and must hold exactly one installation (of
  the harness named, when one is). Several are refused with the list of their stores, so one of them
  can be passed instead. A path that holds nothing is still used as the store of a named harness: a
  destination store can legitimately be empty.
- Without a harness name, `--root` infers it from the directory's own layout when exactly one
  harness is recognized, or from the only installation below it.

**Executables.**

1. An override points to the **launcher or executable**, not its installation directory.
2. Resolution order: the `--<harness>-exe` flag, then `MUKADE_*_EXE`, then `PATH` (`.cmd` launchers
   included), then the platform locations in [`catalog/harnesses.json`](../catalog/harnesses.json),
   then the bare command name. `mukade harnesses` shows which one it resolves.
3. An explicitly configured executable is also put into the returned resume command.
4. OpenCode as the **source**: its export reader uses `MUKADE_OPENCODE_EXE` or default resolution,
   not the `--opencode-exe` flag. The flag applies to OpenCode as a destination.

**`--dest-root`: a hop into a named installation.** Its executable, the one `scan` reports beside the
store, writes the session and appears in the resume command. An explicit `--<harness>-exe` still wins
over it; it wins over `MUKADE_*_EXE` and `PATH`. Into a portable JanuraCode:

```powershell
mukade hop claude_code YOUR_CLAUDE_SESSION_ID opencode --dest-root 'D:\Apps\JanuraCode' --cwd 'C:\work\my-project'
```

- The destination's model and provider come from that installation's own configuration when it keeps
  one beside its store: an OpenCode database in an XDG data layout (`<root>\share\opencode\opencode.db`,
  as in a portable JanuraCode) is configured by `<root>\config\opencode\opencode.json`, a Codex home
  by its `config.toml`. When that file exists it alone answers; otherwise `OPENCODE_CONFIG` and
  `XDG_CONFIG_HOME` apply as before. `--model-provider` and `--model` still win over both.
- A destination whose database holds no session for the source's folder yet cannot name its project:
  pass `--project-id` for that first hop.
- Only `OPENCODE_DB` is set for the import itself, as for any OpenCode destination: the JanuraCode
  engine runs with the `XDG_*` configuration of the shell that runs mukade.

**Resume.**

- A `--resume-prompt` runs the destination with its store variable set to the root the session was
  written to (from the environment or from `--root`), and from the destination's working directory
  (`--cwd`, or the source's recorded directory, which must exist on this machine).
- A resume command you run later by hand does not inherit that: set the destination's variable in
  that shell first. For OpenCode, also configure the appropriate `XDG_*` and `OPENCODE_CONFIG`
  settings where needed; a database path alone does not guarantee complete isolation.
- Refused before anything is written: a resume prompt that starts with `-` (every harness CLI would
  read it as an option), and one with line breaks when the destination's executable is a `.cmd`
  launcher, which cannot take them.

**Deadlines.** Every harness process mukade starts, except the interactive resume of `mukade-tui`,
runs with a deadline. One that outlives it is stopped **together with everything it started** (an MCP
server, a shell): on Windows with `taskkill /T`, on Linux and macOS by walking the process tree
(`src/exe.rs` `kill_tree()`). A process that detached itself from the tree (a daemon) is not found. A
timeout too large for the clock sets no deadline: mukade waits for the process to exit.

## 8. Per-harness notes

Most examples use the harnesses' standard locations. Where a harness is kept in a folder of its own
(`D:\portable\…`, a USB stick, say) the example shows its store variable and executable flag; that is
about the **harness**, not about mukade, which needs nothing beyond its own executable. Each harness
also has a reference page (install, launch, store, compaction, tools, plugins, hooks):
[`docs/harnesses/`](harnesses/).

### Claude Code

Read Claude Code, write Pi (take the source ID from `list`):

```powershell
$env:CLAUDE_CONFIG_DIR = Join-Path $HOME '.claude'
$env:PI_CODING_AGENT_DIR = Join-Path $HOME '.pi\agent'
mukade list claude_code
mukade hop claude_code YOUR_CLAUDE_SESSION_ID pi --cwd 'C:\work\my-project'
```

```bash
export CLAUDE_CONFIG_DIR="$HOME/.claude"
export PI_CODING_AGENT_DIR="$HOME/.pi/agent"
mukade list claude_code
mukade hop claude_code YOUR_CLAUDE_SESSION_ID pi --cwd ~/work/my-project
```

A custom Claude Code destination, with Pi as the source:

```powershell
$env:PI_CODING_AGENT_DIR = 'D:\portable\pi\agent'
$env:CLAUDE_CONFIG_DIR = 'D:\portable\claude\config'
mukade list pi
mukade hop pi YOUR_PI_SESSION_ID claude_code --cwd 'C:\work\my-project' --claude-exe 'D:\portable\claude\bin\claude.exe'
```

- Keep `CLAUDE_CONFIG_DIR` set when running the returned `resume` arguments.
- Mukade does not install Claude or create its authentication configuration.
- Claude Code names a project's folder with its own rule (every character outside `[a-zA-Z0-9]`
  becomes `-`; a name past 200 characters is cut and suffixed with a hash), and mukade follows it.

### Codex CLI

```powershell
$env:CODEX_HOME = Join-Path $HOME '.codex'
$env:PI_CODING_AGENT_DIR = Join-Path $HOME '.pi\agent'
mukade list codex
mukade list codex --archived false                       # only the threads that are not archived
mukade hop codex YOUR_CODEX_SESSION_ID pi --cwd 'C:\work\my-project'
```

```bash
export CODEX_HOME="$HOME/.codex"
export PI_CODING_AGENT_DIR="$HOME/.pi/agent"
mukade list codex
mukade hop codex YOUR_CODEX_SESSION_ID pi --cwd ~/work/my-project
```

A custom Codex destination:

```powershell
$env:CLAUDE_CONFIG_DIR = 'D:\portable\claude\config'
$env:CODEX_HOME = 'D:\portable\codex\home'
mukade hop claude_code YOUR_CLAUDE_SESSION_ID codex --cwd 'C:\work\my-project' --codex-exe 'D:\portable\codex\bin\codex.exe'
```

- `list codex` includes the threads archived with `codex archive` (their rollouts are in
  `$env:CODEX_HOME\archived_sessions`) and marks them `"archived": true`; `dump` and `hop` read them
  like any other.
- Codex requires a destination CLI version in its session metadata. Mukade asks the chosen
  executable's `--version`; if that cannot be done or parsed, supply it: `--dest-version 0.164.0`
  (your version).
- Mukade reads `model_provider` from `$env:CODEX_HOME\config.toml` when available; `--model-provider`
  states it explicitly. Only the top-level `model_provider` key counts, not a profile's or a provider
  table's.
- Keep `CODEX_HOME` set for a resume command run later.

**Threads of the Codex Desktop app (0.165).** The ID in a `codex://threads/<id>` link is the thread
ID mukade takes:

```powershell
$env:CODEX_HOME = Join-Path $HOME '.codex'
$env:CLAUDE_CONFIG_DIR = Join-Path $HOME '.claude'
mukade hop codex 01a181e2-4cd9-7c63-82b8-f75e3d6bb172 claude_code --cwd 'D:\Projects\daodan'
```

What to expect from such a thread:

- **Several files.** A thread kept over two files (a page that redoes its later turns) is read as
  Codex resumes it; `--all-branches` carries it whole (see
  [rewound or branched](#a-session-that-was-rewound-or-branched)).
- **Encrypted compaction.** Its compaction summaries are encrypted, so `--compaction portable` is
  refused (exit 2) and a whole-history hop leaves them out, counted. A long thread is therefore
  carried whole: a 17.8 MB thread became a 4.1 MB Claude Code session of several hundred thousand
  tokens, which only a model with a very large context can resume.
- **Code mode.** Most of its tool calls are one `exec` script each. A script that is only a sequence
  of `text(await tools.<name>(<literal>));` statements, with an output that matches, is read as the
  calls it made, each translated like any other (`src/reader/codex/exec.rs` `split_code_mode()`;
  `apply_patch` becomes a write or edit, `src/tools/patch.rs` `view()`). A script with variables,
  `Promise.allSettled` or any other shape stays one call in text form, with its script. On 34 real
  rollouts (Codex 0.167), 725 of 981 scripts were split
  ([verification](verification/2026-11-11-codex-code-mode.md)).
- **PowerShell shell calls.** Such rollouts declare PowerShell, so their shell calls translate only
  with `--dest-shell powershell` (toward Codex, OpenCode, Pi); toward Claude Code they stay text.
- **Reasoning** is OpenAI's and is dropped toward any other provider, counted.
- **Subagents.** Messages its subagents sent to the main agent are left out on purpose: their content
  is encrypted, readable only by OpenAI. The messages the main agent sent to its subagents travel as
  text, with that encrypted content replaced by `<encrypted by the source's provider, N chars>`.
- **Model warning in Claude Code.** Resuming such a session prints `Session model gpt-… could not be
  restored … using claude-…`: the rows name the model that really wrote each reply, which Claude Code
  does not know, so it carries on with its own. Expected; nothing to fix.
- **Mind the size before writing in it.** Each message sent in a resumed session reads its whole
  context first: a session of several hundred thousand tokens costs that much on the first message,
  or makes Claude Code compact it before answering.

### OpenCode

Read OpenCode, write Pi. `list opencode` needs the database via `OPENCODE_DB` or `--root`, reads it
without starting OpenCode, and includes top-level archived sessions (`--archived false` leaves them
out).

```powershell
$env:OPENCODE_DB = 'C:\path\to\opencode.db'
$env:PI_CODING_AGENT_DIR = Join-Path $HOME '.pi\agent'
Set-Location 'C:\work\my-project'
mukade list opencode
mukade list opencode --archived false                    # without the archived sessions
mukade hop opencode YOUR_OPENCODE_SESSION_ID pi --cwd 'C:\work\my-project'
```

```bash
export OPENCODE_DB="$HOME/.local/share/opencode/opencode.db"
export PI_CODING_AGENT_DIR="$HOME/.pi/agent"
cd ~/work/my-project
mukade list opencode
mukade hop opencode YOUR_OPENCODE_SESSION_ID pi --cwd ~/work/my-project
```

Mukade calls `opencode session export … --standalone` to read the source. Set the `XDG_*` variables
your OpenCode installation uses in this shell as well, if applicable.

**A custom OpenCode destination.** The database, configuration, XDG directories and executable must
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

- OpenCode's import needs a project ID and a complete provider/model reference. Mukade can discover
  the project from OpenCode's existing sessions (asked of the store the session will be written to),
  and the provider and model from its configuration. Explicit values avoid relying on discovery and
  are faster ([Speed](#9-speed)). OpenCode imports with `--standalone`.
- The receipt's `location` names the database the session was imported into (`OPENCODE_DB`), or
  says it went to OpenCode's configured store. Use the returned ID with that store, and keep the
  OpenCode environment configured for the resume command.
- The import document is a temporary file of mukade's own, removed after the import, on success and
  on failure.

**A custom OpenCode source:** set its executable through the environment variable before exporting.

```powershell
$env:MUKADE_OPENCODE_EXE = 'D:\portable\opencode\opencode.exe'
$env:PI_CODING_AGENT_DIR = 'D:\portable\pi\agent'
mukade hop opencode YOUR_OPENCODE_SESSION_ID pi --cwd 'C:\work\my-project'
```

### Pi

Read Pi, write Claude Code:

```powershell
$env:PI_CODING_AGENT_DIR = Join-Path $HOME '.pi\agent'
$env:CLAUDE_CONFIG_DIR = Join-Path $HOME '.claude'
mukade list pi
mukade hop pi YOUR_PI_SESSION_ID claude_code --cwd 'C:\work\my-project'
```

```bash
export PI_CODING_AGENT_DIR="$HOME/.pi/agent"
export CLAUDE_CONFIG_DIR="$HOME/.claude"
mukade list pi
mukade hop pi YOUR_PI_SESSION_ID claude_code --cwd ~/work/my-project
```

A Pi destination kept in its own folder:

```powershell
$env:CLAUDE_CONFIG_DIR = 'D:\portable\claude\config'
$env:PI_CODING_AGENT_DIR = 'D:\portable\pi\agent'
mukade hop claude_code YOUR_CLAUDE_SESSION_ID pi --cwd 'C:\work\my-project' --pi-exe 'D:\portable\pi\bin\pi.cmd'
```

- Mukade writes the new Pi session below `PI_CODING_AGENT_DIR\sessions`, in the folder Pi itself
  would use for the project. The receipt's `resume` holds the chosen launcher and the session file
  path.
- Keep `PI_CODING_AGENT_DIR` set when running it; separately verify that Pi installation's provider
  credentials and configuration. A separate agent directory does not by itself prove that every
  resource Pi reads is isolated.
- Sessions written this way were run against Pi 1.2.0 and 1.4.0
  (`docs/verification/pi-session-and-reasoning-verification.md`).

## 9. Speed

A handover is meant to feel instant. Measured on this repository's release build, hopping a Claude
Code session of that size (mostly tool output, like real ones) into Pi, Claude or Codex:

| source session | time (Pi, Claude or Codex as destination) | peak memory |
|---|---|---|
| 1 MB | 30–35 ms (`dump` alone: 24 ms) | not measured |
| 10 MB | 185–240 ms (`dump` alone: 150 ms) | about 100 MB |
| 30 MB | 440–630 ms (`dump` alone: 420 ms) | about 300 MB |

Before the work on speed the same hops took 83, 556 and 1657 ms. The time is proportional to the size
of the session, because every record is read and decoded once.

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

A real run on the 5-message Claude Code fixture, `hop claude_code … pi --compaction portable`. A
phase is printed when it ends, so one that runs inside another comes first: `serialize + write file`
is inside `write files`, and `resumed branch`, `entries`, `finish` are inside `build entries`.
`fingerprint (parallel)` runs on a second thread while the file is parsed.

What makes it fast, and what you can do:

- **The file formats are not the cost.** Claude Code, Codex and Pi store JSON Lines and OpenCode
  SQLite; those are the harnesses' own and mukade reads and writes them as they are. A binary
  intermediate would not help — a hop has none. Decoding is spread over up to 8 threads for large
  files; the source's fingerprint is hashed on another thread while the rest is read.
- **Starting the destination to ask its version is the real cost for small sessions.** Pi's
  `--version` takes about half a second (it starts Node). Mukade therefore reads the version from the
  npm package the launcher starts, without starting anything. For a native binary it asks once and
  remembers the answer by the binary's path, size and modification time, in `mukade-data/cache`
  beside the executable ([section 10](#10-where-its-files-go)).
- **To skip the question entirely, state the facts:** `--dest-version`, and for OpenCode
  `--project-id`, `--model-provider`, `--model`.
- A `--resume-prompt` starts the destination harness and a model: that is its time, not mukade's.

## 10. Where its files go

Mukade has no installer or configuration file. On first start it copies the editable intent prompts
to `intent-prompts` beside the executable; an existing file is never replaced. By default its other
files go to `mukade-data` beside the executable. `MUKADE_DATA_DIR` can select a different
**absolute** data folder for a host; an invalid override is refused (exit 1) rather than redirected.
The data folder is made the first time something is written to it, never at start-up.

| file | what for |
|---|---|
| `intent-prompts/*.md` | the editable summary prompts: `generic.md` is the default, every other file stem is an `--intent` name; copied on first start and never overwritten |
| `mukade-data/cache/probes.json` | the versions and OpenCode project IDs learned from harness binaries, keyed by the binary's path, size and modification time, so the next hop does not start the harness to ask again |
| `mukade-data/cache/claude-list-<hash>.json` | what `list claude_code` read from each session file (ID, title, cwd, time), keyed by the file's path, size and modification time, so only the files that changed are read next time; one file per Claude Code `projects/` folder, rewritten whole; a file modified in the last 2 seconds is never kept in it |
| `mukade-data/tmp/mukade-import-*.json` | the document `opencode session import` needs as a file; private, and removed when the import ends, even when it fails |
| `mukade-data/tmp/summary-*/` | one folder per `--compaction summary` run: the file Codex writes its summary to, the session Pi forks; removed when the run ends, even when it fails |
| `.mukade-*.tmp` | beside the session being written, in the destination's store: the session as it is written, renamed into place when complete |

- Nothing is written to the user profile, the application-data folders, the registry or the system's
  temporary folder.
- A temporary file or summary folder a **killed** run left behind is removed by the next run once it
  is an hour old.
- What is yours stays yours: the sessions it writes into the harnesses' stores, and what you ask for
  with `--output` and `--manifest`.
- To remove mukade, delete the folder it is in.

**When the data folder cannot be written.** The folder cannot be made, and mukade does not look for
another place: the cache is not kept (every hop asks the harness, which costs time but works), and a
hop to OpenCode stops with an error that names the folder, before anything is written. Set
`MUKADE_DATA_DIR` to an absolute writable folder, or run the executable from a writable location
without the override.

**What it looks like.** A real run: the executable copied alone into an empty folder, then two hops to
Claude Code whose launcher answers `--version` (`--timing` shows the cost of asking):

```text
before the first hop          after it
tools\                          tools\
  mukade.exe                        mukade.exe
                                    intent-prompts\
                                    mukade-data\cache\probes.json

1st hop   probe destination version   70.8 ms   total  78.6 ms
2nd hop   probe destination version    0.3 ms   total   9.4 ms     (answered from the cache)

probes.json: {"version|<dir>/claude.cmd|20|1793205746861450700":"2.9.9"}
```

- A hop that has nothing to ask adds nothing under `mukade-data`: a hop to Pi, with no `pi` launcher
  to start, left `mukade-data` absent.
- An automated test runs the real binary alone in a folder, with the temporary folder, the home folder
  and the application-data folders pointed at an empty sandbox, and asserts the sandbox stays empty
  and the folder holds the executable, `intent-prompts` and `mukade-data`.
- **Harnesses are separate.** A harness installed in its own folder is reached with its store
  variable and `--<harness>-exe`: see [section 7](#7-stores-and-executables) and
  [section 8](#8-per-harness-notes).

## 11. Exit codes

| code | meaning |
|---|---|
| 0 | done |
| 1 | a command-line mistake (unknown option, a flag without its value, an unexpected argument, an option the command does not take, a bad value), or another error outside the codes below (a document that cannot be read or parsed, a `MUKADE_DATA_DIR` that is not absolute) |
| 2 | refused with a reason; nothing was written: the session is not there, the store is not configured, the ID is ambiguous, the source changed (`--expect-sha256`), or the request is not supported (a resume prompt starting with `-`, `--compaction portable` on a source that never compacted, `--all-branches` on a harness other than Codex, …) |
| 4 | failed and nothing changed: the plan would be refused by a provider, the source's model could not write the summary, or the write failed before starting the destination import |
| 5 | recovery is needed: the source could not be read, an external import has an uncertain outcome, or the session was written but the resume failed. A failed resume still prints its receipt and saves `--manifest`; an uncertain import names its destination ID |

## 12. Troubleshooting

**Finding the session or the harness**

- **No store or session found** (exit 2): check the variable, `--root`, source ID and whether the
  intended project/store holds that session. `mukade detect <path>` prints diagnostic evidence.
- **Ambiguous or empty detection:** name the harness explicitly; a newly created empty store has no
  reliable session layout to infer.
- **Cannot run the executable:** verify the launcher path, its runtime and permissions. A custom
  OpenCode source requires `MUKADE_OPENCODE_EXE`; a destination uses its flag or env var.
- **Codex version or OpenCode project/model missing:** pass the destination's actual version,
  project ID, provider or model (`--dest-version`, `--project-id`, `--model-provider`, `--model`).

**Writing**

- **"mukade could not create its temporary directory"** (exit 4, from a hop to OpenCode): the
  selected data folder cannot be written. Nothing was imported; set `MUKADE_DATA_DIR` to an absolute
  writable folder or use the default beside a writable executable.
- **External OpenCode import is uncertain** (exit 5): the error names the attempted session ID. Check
  that ID and its provenance in the selected database before retrying; a zero exit from the
  underlying command alone does not prove that it imported the session.
- **Manifest could not be saved:** the session was still written, and its receipt is on stdout; the
  error names its location. Save the `manifest` field of that receipt instead of running again.
- **A path or file name problem on Windows:** Pi files a session under a folder named after the whole
  project path, so a very deep project path can pass the 260-character limit; use a shorter `--cwd`.

**Summary**

- **"summary failed: …"** (exit 4, `--compaction summary`): the source harness could not write the
  summary, and nothing was written. The message names the command (the prompt shown as
  `<summary prompt v1>`), its exit status and what it printed: a login or quota problem of the source
  harness, a model it cannot reach (name another with `--summary-model`), a run longer than
  `--summary-timeout`, or a model that called a tool instead of summarizing. When OpenCode's fork
  could not be deleted, the message ends with the command that removes it.
- **"the summary prompt cannot be passed through the batch launcher …"** (exit 2): the selected
  `intent-prompts/<name>.md` holds a line break or a character an install's `.cmd` shim cannot carry.
  Name the harness's native executable, or keep the prompt to ASCII letters, digits, spaces and
  `.,;:()`.

**Resume**

- **Resume fails after a successful write** (exit 5): the session already exists. Mukade still prints
  its receipt on stdout and saves `--manifest`: use that `id` and `location` instead of retrying, which
  would create another session. A resume that outlives its timeout is stopped, including a harness
  behind a `.cmd` launcher. Check configuration and credentials in the process environment; provider
  availability must be verified in your own environment.

**Interrupted turns**

- **The destination carries on from a half-written answer:** the source turn was interrupted and its
  work travelled, as it does by default. Hop again with `--interrupted drop`: the prompt is written
  without the cut-off reply (see [A session with interrupted turns](#a-session-with-interrupted-turns)).
  The first hop's session stays where it is; a hop never changes or removes one.
- **`--interrupted drop` kept a reply that was cut:** the source wrote no sign of the interruption. A
  harness killed while its model was answering leaves none in a Claude Code or Codex file, and a
  document made by `dump` does not hold the markers: hop from the source itself, and check
  `dropped_interrupted_steps` in the manifest.
- **"unknown --interrupted …"** (exit 1): the mode is `keep` or `drop`. **"--interrupted is supported
  only by hop, write and document"**: `dump` and `list` do not take it.

**Speed and memory**

- **Slower than the table in [Speed](#9-speed):** run with `--timing`. A large `probe …` line is a
  harness being started to ask its version or project: pass `--dest-version` (and `--project-id`), so
  the question is not asked at all. (The answer is remembered after the first time, in
  `mukade-data/cache`, when the executable's folder can be written to.)
- **"The paging file is too small" / "memory allocation failed":** the machine ran out of memory (a
  30 MB session needs about 300 MB). Close other programs and retry; nothing is half-written, because
  a session is written to a temporary file and renamed into place.
