# Using mukade — command-line guide

Use this page to run `mukade` from a shell. Mukade moves a coding-agent session from one harness to
another — Claude Code, Codex CLI, OpenCode, Pi, Hermes Agent, DeepSeek Harness — and the destination
continues it **as one of its own sessions**.

- The sessions explorer window is a separate crate: [mukade-tui](../mukade-tui/README.md) (its
  commands, keys, resume and hop examples).
- Paths, session IDs, project IDs, model and provider names in the examples are **illustrative**:
  use the values of your installation.
- Examples are Windows PowerShell. On macOS or Linux use `/` paths and `export NAME=value` instead
  of `$env:NAME = 'value'`; a POSIX block is shown where more than that differs.

## Pick your task

**Find**

| I want to… | Run |
|---|---|
| see which harnesses are installed, and their stores | [`mukade harnesses`](#mukade-harnesses) |
| find every installation below a folder, portable ones too | [`mukade scan <dir>`](#mukade-scan) |
| know which harness a folder belongs to | [`mukade detect <dir>`](#mukade-detect) |
| find a session ID | [`mukade list <harness>`](#mukade-list) |

**Move**

| I want to… | Run |
|---|---|
| continue a session in another harness | [`mukade hop <from> <id> <to>`](#mukade-hop) ([Quick start](#2-quick-start)) |
| try a hop without touching the real store | [`hop` into a scratch store](#rehearse-first) |
| move a session to another machine or person | [`dump` there, `write` here](#move-a-session-to-another-machine) |
| get OpenCode's import document, writing nothing | [`mukade document opencode …`](#mukade-document) |
| run mukade on a machine I do not own | [copy the executable](#1-get-it) |

**Shape and check**

| I want to… | Use |
|---|---|
| carry a long session in less space | [`--compaction`, `--intent`](#make-a-long-session-smaller) |
| leave out half-done turns | [`--interrupted drop`](#a-session-with-interrupted-turns) |
| carry a rewound Codex thread whole | [`--all-branches`](#a-session-that-was-rewound-or-branched) |
| carry the subagent sessions with their parent | [`--children`](#carry-the-subagent-sessions) |
| know exactly what a hop lost | [the receipt, `--manifest`](#4-reading-the-receipt-and-the-manifest) |
| understand an error | [exit codes](#11-exit-codes), [troubleshooting](#12-troubleshooting) |

## 1. Get it

Mukade is one command-line executable: no installer, no configuration file.

1. Build it and copy it anywhere (a folder, a USB stick):

   ```powershell
   cargo build --release                            # target\release\mukade.exe (target/release/mukade on macOS/Linux)
   Copy-Item target\release\mukade.exe D:\tools\
   D:\tools\mukade.exe --help
   ```

2. Run it from a shell, by its path or from a folder on `PATH`. In the examples `mukade` stands for
   whichever you use.
3. Install and configure the harnesses separately: mukade moves sessions; it installs no harness
   and transfers no credentials.

- The Windows executable has the C runtime built in: no Visual C++ redistributable.
- Its own files (editable intent prompts, a cache) go beside it: [section 10](#10-where-its-files-go).
- `mukade help`, `-h` or `--help` prints the usage text; `mukade --identity` prints
  `{"name","version","commit","target"}` as JSON.

**A machine you do not own:** run `mukade.exe` from the stick; deleting its folder leaves nothing on
the machine ([section 10](#10-where-its-files-go)).

## 2. Quick start

Continue a Claude Code session in Pi.

1. See what is installed and where the stores are: `mukade harnesses`.
2. Know the **existing project directory** the session belongs to: `--cwd` names the destination's;
   without it mukade uses the source's recorded one, when present.
3. Point both harnesses at their stores:

   ```powershell
   $env:CLAUDE_CONFIG_DIR   = Join-Path $HOME '.claude'
   $env:PI_CODING_AGENT_DIR = Join-Path $HOME '.pi\agent'
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

Always true:

- The destination session is **new**; the source is never touched.
- The order is the harness's **native order**, never timestamps.
- **No loss is silent**: whatever was left behind or changed is a line in the receipt's manifest.
- Without `--resume-prompt` mukade only writes the session and gives you the command; with it, mukade
  *runs* the destination harness, which may make a model/API call.

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

- Harness names: `claude_code`, `codex`, `opencode`, `pi`, `hermes`, `deepseek_harness`. Store variables per harness:
  [section 7](#7-stores-and-executables).
- With `--root` the harness may be left out of `list`, `dump` and a hop's source. `write` always
  needs it.
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
| `--keep-turns <n>`, `--summary-model <id>`, `--summary-timeout <s>` | dump, hop | with `summary` only ([summary](#summary-the-sources-own-model-writes-one)) |
| `--context-window <n>`, `--max-output-tokens <n>`, `--text-token-factor <f>` | document, hop | with `native` only |
| `--interrupted keep\|drop` | write, hop, document | whether the work of unfinished turns travels |
| `--all-branches` | dump, hop, document | every branch a Codex thread keeps |
| `--without-tools` | write, hop, document | omit historical tool calls and results |
| `--reasoning <mode>` | write, hop, document | `drop_foreign` (default), `as_text`, `drop` |
| `--dest-protocol <p>` | write, hop, document | `anthropic-messages`, `openai-responses`, `openai-chat` |
| `--dest-shell <s>` | write, hop, document | `bash`, `sh`, `zsh`, `dash`, `ksh`, `posix`, `powershell`, `pwsh` (any case, a path to one too), `none`; default: the shell the destination runs |
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
| `--claude-exe`, `--codex-exe`, `--opencode-exe`, `--pi-exe`, `--hermes-exe`, `--dsh-exe <path>` | write, hop, document, dump (summary) | the binary to spawn |

### `mukade harnesses`

Lists every supported harness with the executable and store mukade finds, its version, session and
fork counts, and the lineage each format records. It takes no options and prints text for a person;
a program uses the library or `mukade list` (JSON rows).

1. Optional: set the store variables or `MUKADE_*_EXE` ([section 7](#7-stores-and-executables)) for
   non-default locations.
2. Run `mukade harnesses`.
3. Read one block per harness:

| line | says |
|---|---|
| `<harness> (<name>) - <status>` | the executable is `configured` (`MUKADE_*_EXE`), `installed` (`PATH` or the catalog location) or `not found` |
| `binary:` | the **one** resolved executable, not every copy on disk, and where it came from |
| `version:` | its answer to `--version`; `unknown` otherwise |
| `store:` | the store path, its variable, and `environment` or `default` |
| `status:` | the store's state (below) |
| `sessions:`, `forks:`, `lineage:`, `layout:` | counts, lineage support and file layout; unknown is shown as unknown, never guessed |

`status:` says why a store that is there but unreadable is not an empty one:

| state | meaning |
|---|---|
| `recognized` | the store holds files in that harness's own layout and mukade listed them |
| `recognized, read error: <reason>` | the layout is right but counting failed |
| `present, layout not recognized` | the path exists but no file matches that harness's layout. An explicit store variable skips this check and is read directly (failures show as a read error). `mukade detect <dir>` prints the evidence |
| `read error: <reason>` | the harness reader could not be opened at all |
| `not found` | no store at the default or the configured path |
| `non-persistent (:memory:)` | OpenCode's `OPENCODE_DB` points at an in-memory database |

- It reads store files without modifying them and asks a discovered executable for its version.
- It is the only command that falls back to default store locations: `list`, `dump`, `write` and
  `hop` keep their explicit-root behavior.
- The locations it consults are the [harness catalog](reference/harness-catalog.md)
  (`catalog/harnesses.json`), which ships inside the executable.

To enumerate `PATH` matches yourself: `Get-Command claude,codex,opencode,pi,hermes,dsh -All -ErrorAction
SilentlyContinue | Select-Object Name,Source` (POSIX: `command -v <name>`). For package-managed
installs use `npm prefix -g`, `npm root -g` or `brew --prefix`, then run each binary's `--version`.
Custom stores and executables: [section 7](#7-stores-and-executables). Per-OS procedure and native
locations: [Finding existing installations](reference/harness-catalog.md#finding-existing-installations).

### `mukade scan`

Finds **every** installation below a folder, portable and custom ones included. It takes only
`--depth`.

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

A portable JanuraCode copy is an OpenCode installation: store `<app>\data\share\opencode\opencode.db`,
executable `<app>\resources\opencode-cli\opencode.exe`.

How it walks:

- 8 folder levels by default.
- It never follows links or junctions; it skips `node_modules`, `.git` and `target`, and does not
  enter a store's own session folders.
- It reads at most 100,000 folders; `"truncated": true` says the depth or that budget stopped it.
- An installation is recognized by its store: a copy never started has no sessions and is not reported.

### `mukade detect`

Prints which harness stores one folder holds, with the evidence.

1. Run `mukade detect 'D:\portable\pi\agent'`.
2. Read the JSON: `path` and `candidates`, each with `harness` and `evidence` (session records, Pi's
   `models.json`, or OpenCode's `opencode.db`).
3. Pass the harness explicitly to later commands when the answer is empty or ambiguous.

- It can return zero or several candidates. An empty folder tree infers nothing: a newly created
  store has no reliable session layout.
- `opencode.db` can be passed directly; other SQLite files (Codex's `state_*.sqlite`) are not OpenCode.
  For a nonstandard database filename, pass `opencode` and the file path explicitly.

### `mukade list`

Prints one JSON line per session.

1. Set the harness's store variable ([section 7](#7-stores-and-executables)) or pass `--root`: Claude
   Code, Codex, Pi, Hermes and DeepSeek Harness need an explicit root, and `list opencode` needs `OPENCODE_DB` or `--root` (it
   never falls back to OpenCode's configured store). It reads the database read-only, without starting
   OpenCode.
2. Run `mukade list <harness>`.
3. Take the `id` of the session you want.

```powershell
$env:CODEX_HOME = Join-Path $HOME '.codex';   mukade list codex
mukade list --root 'D:\portable\pi\agent'                 # harness inferred from the folder
mukade list codex --archived false                        # without the archived threads
mukade list claude_code --children                        # with the subagent sessions
```

#### The list output

```json
{"id":"01a1a817-…","title":"Alpha work","cwd":"D:\\work\\alpha","timestamp":"2026-11-08T14:03:20.000+00:00","location":"C:\\Users\\you\\.codex\\sessions\\2026\\11\\08\\rollout-….jsonl","archived":false,"unreadable":null,"live":null}
```

| key | meaning |
|---|---|
| `id`, `title`, `cwd` | what the store records; any may be `null` |
| `timestamp` | the session's **last activity**: Claude Code and OpenCode from the store, Codex from its state database or else the rollout file's last write, Pi from the newest record time, Hermes from `last_activity_at` (else `ended_at`, `started_at`), DeepSeek Harness from the log file's last write; `null` when nothing says |
| `location` | the file (or database) the session is recorded in |
| `archived` | the harness marked it archived |
| `unreadable` | `null` when nothing is known against reading it; otherwise **why `dump` and `hop` refuse it** (exit 2, same words) |
| `live` | `true` when a process of the harness runs the session now; `false` when the store records running sessions and none runs it; `null` when the store does not say |
| `parent`, `kind` | only on a row with a parent: the session it came from, and `spawn` (a subagent) or `fork`. Without `--children` only a fork can have them (Pi, OpenCode, Hermes, DeepSeek Harness) |

A harness forgets transcripts long before it forgets sessions, so `list` reports both. The
`unreadable` rows are how a caller sees every project:

| harness | an `unreadable` row is |
|---|---|
| Codex | a thread with a row in the home's newest `state_<n>.sqlite` whose rollout file is gone. `location` is the path the row records. Subagent and guardian threads are listed with `--children` only |
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
also compares the process start time, so a reused process ID is not the session: Windows creation
time, Linux `/proc/<pid>/stat` (verified with Claude Code 2.1.322 on 2026-11-11). macOS does not
compare it, so a reused ID reads as live. Codex, OpenCode and Pi keep no such record, so their `live`
is `null`: **a cleaner must read `null` as "not known", not as "not running".**

#### Subagent sessions

`--children` lists every session the store holds, subagent sessions included (Hermes and DeepSeek Harness too). `dump` and `hop` take
it too ([Carry the subagent sessions](#carry-the-subagent-sessions)); any other command given it is a
command-line mistake (exit 1). `dump` and `hop` read a child by its `id` as listed.

| harness | `--children` adds | child `id` | `parent` |
|---|---|---|---|
| Claude Code | each `subagents/agent-<agentId>.jsonl` (a meta.json without a transcript is not listed); `title` is its meta.json `description` | `<parent-id>.<agentId>`, matched exactly (no prefix) | the spawning session, or the spawning subagent for a child of a child |
| Codex | the subagent and guardian threads of `state_<n>.sqlite` | the thread id | the thread that spawned it (`thread_spawn_edges`); none for a guardian review |
| OpenCode | the child sessions (`parent_id`) | the session id | the spawning session |
| Pi | nothing: no subagents | | |
| Hermes | the `delegate_task` sessions (`model_config._delegate_from`, `source = subagent`); a `/branch` copy is listed anyway, as a fork | the session id | the spawning session |
| DeepSeek Harness | the logs whose header has `origin: "subagent"` | the log's id | its header's `parentSession` |

What `parent_kind` holds per harness: [spec §3.1](session-interchange-spec.md#31-the-librarys-own-model-session).
What a child reported to its parent travels in the parent as a user turn
([Choose what travels](#subagents-and-what-they-reported)); the children themselves travel only
with `--children`.

#### Archived sessions

`--archived <true|false>` works the same for every harness:

| value | rows printed |
|---|---|
| `true`, or the flag left out (the default) | every session, the archived ones included |
| `false` | only the rows whose `archived` is `false` |

Any other value, or `--archived` on another command, is a command-line mistake (exit 1). It combines
with `--root`.

| harness | `archived` is `true` when |
|---|---|
| Codex | the rollout is in `<CODEX_HOME>/archived_sessions/` (where `codex archive` moves it), or its row in `state_<n>.sqlite` says `archived`. Found with or without that database; `dump` and `hop` read it |
| OpenCode | the session has a `time_archived` (top-level sessions) |
| Claude Code, Pi | never: their stores record no archived flag, so `--archived false` leaves nothing out |

```powershell
mukade list opencode --root 'C:\path\to\opencode.db' --archived false      # an OpenCode database named directly
mukade list codex | ConvertFrom-Json | Where-Object { $_.archived } | Select-Object id, title, location
mukade list codex --archived false | ConvertFrom-Json | Select-Object -ExpandProperty cwd -Unique   # projects with work not archived
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

```json
{
  "directory": "D:\\tools\\intent-prompts",
  "default": "generic",
  "prompts": [
    {"name": "generic", "path": "D:\\tools\\intent-prompts\\generic.md", "builtin": true, "custom": true},
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
3. Protect the file like the original session: it holds the conversation.

```powershell
mukade dump claude_code YOUR_CLAUDE_SESSION_ID --output C:\work\session.json
mukade dump pi YOUR_PI_SESSION_ID --root 'D:\portable\pi\agent' --output 'C:\work\session.json'
mukade dump codex --from-file 'C:\path\rollout.jsonl' --output C:\work\session.json   # no ID with --from-file
```

- **`dump` converts nothing.** Every tool call keeps its name, its arguments and the canonical form
  mukade read it as; reasoning keeps its text and opaque part. Conversion happens when the document
  is **written**, so one document written into any of the six harnesses gives the result a hop
  would.
- It exports the session as it is, interrupted turns included; what is left out is decided at write.
- It takes `--compaction portable|summary` and `--intent`; `native` is refused, because the document
  is harness-neutral. It does not take `--interrupted` or `--without-tools`.
- `--children` adds the session's subagent sessions to the document, in `children`
  ([Carry the subagent sessions](#carry-the-subagent-sessions)).

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

```

- `--root` names the destination store; the executable kept beside it imports into it.
- It takes `--without-tools`, `--interrupted drop` and the destination options like `hop`; it refuses
  `--compaction` and `--intent` (chosen by `dump`) and imports the summary a document already holds.
- A document saved with a UTF-8 byte-order mark is accepted.
- It imports the session alone: a `children` list in the document is left unwritten, and `write` says
  so on stderr. Use `hop --children` to write the subagent sessions.

#### Move a session to another machine

1. Machine A: `mukade dump claude_code YOUR_CLAUDE_SESSION_ID --output C:\work\session.json`.
2. Move the file.
3. Machine B: set the destination's store variable (`$env:CODEX_HOME = 'D:\codex\home'`), then
   `mukade write codex C:\work\session.json --cwd 'D:\work\my-project'`.

### `mukade hop`

Reads the source and writes the destination in one process: `dump` and `write` without the file.

1. Set **both** harnesses' store variables ([section 7](#7-stores-and-executables)); Claude Code,
   Codex and Pi need an explicit root.
2. Find the ID: `mukade list <src-harness>`.
3. Run `mukade hop <src-harness> <session-id> <dst-harness> --cwd <project>`.
4. Read the receipt; open the session with its `resume` command, with the destination's store
   variable still set.

Any pair works:

```powershell
mukade hop claude_code YOUR_CLAUDE_SESSION_ID codex --cwd 'C:\work\my-project'
mukade hop pi YOUR_PI_SESSION_ID opencode --cwd 'C:\work\my-project' --project-id YOUR_PROJECT --model-provider YOUR_PROVIDER --model YOUR_MODEL
```

- Tool calls are translated to what the destination offers; one it cannot express travels as text and
  is counted (`tools_text_form`; [section 5](#tool-calls-and-reasoning-what-is-converted-and-when)).
- `--resume-prompt 'continue'` also runs the destination once and keeps its reply in the receipt
  (`resume_output`).
- `--root` selects the **source**, even when both ends are the same harness; the destination takes its
  variable or `--dest-root` ([section 7](#7-stores-and-executables)).
- With `--from-file` the source is that file; the `<session-id>` position is still written.
- `--children` also writes the session's subagent sessions
  ([Carry the subagent sessions](#carry-the-subagent-sessions)).
- A hop is repeatable: each run writes another new session and changes nothing else.

#### Rehearse first

Point the **destination's** store variable at an empty scratch folder to see what a hop would do.

1. Set the source's real store and a scratch destination:

   ```powershell
   $env:CLAUDE_CONFIG_DIR   = Join-Path $HOME '.claude'      # the real source
   $env:PI_CODING_AGENT_DIR = 'C:\work\rehearsal\pi'         # an empty scratch destination
   mukade hop claude_code YOUR_CLAUDE_SESSION_ID pi --cwd 'C:\work\my-project' --manifest C:\work\rehearsal\manifest.json
   ```

2. Read the manifest ([section 4](#4-reading-the-receipt-and-the-manifest)); when it is right, set the
   variable to the real store and run it again.

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
```

- It needs a working directory and one complete turn at least (else `noCompleteTurn`); the plan is
  held to the provider rules first, and a violating document is refused.
- The session ID is `ses_preview` unless `--dest-id` gives one.
- `native_compaction` reports `keep_id`, `summary_chars`, `estimated_tokens`, `projected_tokens`,
  `summarized_messages`, `summarized_user_messages`, `budget`, `context_window` and `fits`; an
  example with `--compaction native` is in [section 5](#native-keep-everything-add-a-native-boundary).

## Local chat between harness instances

`mukade chat` runs one local MCP server that harness instances use to talk to each other.
Design: [chat specification](superpowers/specs/2026-11-11-mcp-chat-design.md), [ADR-0003](adr/ADR-0003-local-mcp-chat.md) (Proposed).

| command | what it does |
|---|---|
| `mukade chat start [--port <n>]` | start the background service; prints its endpoint as JSON |
| `mukade chat status` | `running`, `stopped` or `unreachable`; writes no file |
| `mukade chat stop` | stop it; rooms and messages stay |
| `mukade chat tools` | every operation with its argument schema |
| `mukade chat <operation> [--<argument> <value>]…` | call one operation on the running service |

Roles of a room member:

| role | may |
|---|---|
| `observer` | read the room; follow another member's bound session in a reduced view (prompts, replies, which tool ran on what; no file contents, no command output) |
| `peer` | read and send messages |
| `lead` | read and send; address instructions to workers |
| `worker` | read; answer an instruction only with `done` or `blocked` |

Operations: `open close reopen delete rooms join rejoin leave members candidates bind unbind send
signal read peek wait ack observe ledger export`. An argument is the field name with dashes
(`room_id` is `--room-id`); a boolean is a bare flag; `--text-file <file|->` reads a message.

A first exchange:

1. `mukade chat start`
2. `mukade chat open --name design` prints a `room_id`.
3. `mukade chat join --room-id <id> --harness claude --role peer` prints a `member_handle`; do the same for the other instance.
4. `mukade chat send --room-id <id> --member-handle <handle> --text "what do you think? @codex-1"`
5. The other instance: `mukade chat wait --room-id <id> --member-handle <its handle>`.

The quickest way: start each instance already in the room.

```text
mukade chat launch claude_code --room design --listen 300
mukade chat launch claude_code --room design --role worker --listen 600
```

| option | what it does |
|---|---|
| `--room <name>` | the room to join; opened when there is none. The service is started when it is not running |
| `--role <role>` | `peer` (default), `lead`, `worker` or `observer` |
| `--listen <seconds>` | after each turn the instance waits, spending no tokens, up to that long for a message or signal addressed to it, and goes on when one comes (at most 3600) |
| `--max-wakes <n>` | how many times listening may restart it in one session (default 10), so two listening agents cannot answer each other for ever |
| `-- <arguments>` | passed to the harness as they are, e.g. `-- -p --model haiku "…"` |

What the launched instance gets, without any setting of yours changing:

1. A plugin for that session only (`claude --plugin-dir`), in the service's own folder, removed when the instance exits.
2. At session start it joins the room; its name, role, `member_handle` and the rules of its role are given to the model.
3. At each prompt, what the room wrote since is given to the model. A batch is acknowledged only at the next hook call, so a batch the session never took comes again.
4. With `--listen`, at the end of a turn the Stop hook waits; a message addressed to the instance restarts it with the messages. One written to the whole room does not restart it: it comes with the next addressed one, or at the next prompt.
5. The instance writes with the MCP tools (`chat_send`, `chat_signal`, `chat_observe`), never `chat_read` or `chat_wait`: its hooks are its reader.

Only Claude Code has an adapter so far; `launch` refuses the other harnesses (exit 2), which join with `connect` and the tools. A Claude Code launched this way is seen live answering a question while listening ([record](verification/2026-11-12-chat-live.md)).

Connect a harness as an MCP client, with the service running:

| command | what it does |
|---|---|
| `mukade chat connect <harness> --dry-run` | show the file and the entry, credential hidden; write nothing |
| `mukade chat connect <harness>` | add the entry `mukade` to the harness's user-scope MCP servers |
| `mukade chat disconnect <harness>` | take it out again |

| harness | file written |
|---|---|
| `claude_code` | `~/.claude.json` (`$CLAUDE_CONFIG_DIR/.claude.json` when set), `mcpServers` |
| `codex` | `~/.codex/config.toml` (`$CODEX_HOME`), `[mcp_servers.mukade]` |
| `opencode` | `~/.config/opencode/opencode.json` (`$XDG_CONFIG_HOME`), `mcp.servers` |
| `pi` | `~/.pi/agent/mcp.json` (`$PI_CODING_AGENT_DIR`), `mcpServers` |

- Everything else in the file stays; the earlier file is kept in `<data folder>/bus/backups/` (five per harness).
- Refused, with the file untouched: an entry `mukade` that mukade did not write or that was changed since; a JSON file with comments; `--project` (the credential would sit in a file that can be committed).
- Restart the harness session to load the entry. By hand instead: an HTTP server at the `endpoint`, with the headers `Authorization: Bearer <content of <data folder>/bus/credential>` and `X-Mukade-Service-Id: <service_id>`.
- Seen live: a Claude Code and a Codex instance exchanging a tagged question and answer through these entries ([record](verification/2026-11-12-chat-live.md)). The OpenCode and Pi entries follow each harness's documented format and are not yet tried live **[U]**.
- A harness without an MCP entry can run the same operations as shell commands.

| limit | detail |
|---|---|
| idle instance | an instance launched with `--listen` waits after each turn for that long; any other instance gets a message only when it calls `read` or `wait`, or (launched) at its next prompt. Nothing wakes one that sits at its prompt after that |
| security | one user account: any process of that user can read the credential and the harnesses' own session files. Messages and observed text are checked for secrets, which reduces the risk and does not remove it |
| data | in `<data folder>/bus/`, never in a project or a harness store; nothing expires |

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
                "reasoning_dropped": 0, "dropped_off_branch": 0, "dropped_interrupted_steps": 0, … },
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
| `id`, `location`, `resume` | the new session's ID, where it was written, and the destination's command to continue it (keep its store variable set when you run it by hand) |
| `resume_output` | the destination's reply, when you passed `--resume-prompt` |
| `counts` | every kind of loss as a number (all of them: [spec §6.3](session-interchange-spec.md#63-manifest-written-with-every-hop)); every number above zero also has a line in `losses` |
| `strict` | the plan was held against the rules a provider API enforces (every tool call answered once, IDs in the allowed pattern, …) **before** writing; a plan that would be refused is not written (exit 4) |
| `source.content_sha256` | the SHA-256 of the **source file's own bytes** |
| `children` | only with `hop --children`: one entry per subagent session written ([Carry the subagent sessions](#carry-the-subagent-sessions)) |

**Audit the loss.**

1. Save the manifest with `--manifest <file>` (the receipt prints it anyway).
2. Read `counts` and `losses`: what was dropped, rewritten, or written as text.
3. Compare `source.content_sha256` with `Get-FileHash -Algorithm SHA256 <file>` (or `sha256sum <file>`)
   to know which exact file was carried.

What the `losses` lines mean:

| line | what happened |
|---|---|
| `interrupted_last_turn` | the source ended on a turn it never finished (the harness was killed, or the turn was interrupted). With no reply at all it travels as a user message alone; what the agent had done before the interruption travels too, unless `--interrupted drop`. Normal |
| `steps of interrupted turns omitted by request, with their tool results (n)` | `--interrupted drop`: the work of the turns the source never finished was left out; their prompts travelled |
| `tool calls and their results omitted by request (n)` | `--without-tools` |
| `reasoning dropped: <reason> (n)` | thinking is replayed only to a provider of the same protocol whose format can hold its signature or encrypted payload; otherwise one line per reason: *the destination's provider protocol is not known; name it with --dest-protocol* (OpenCode, Pi), *made by another provider protocol*, *no signature or encrypted payload to replay it with*, *the destination's format has no place for its payload*, *not carried, as --reasoning drop asks*, *encrypted only, no visible text to write as text*. `--reasoning as_text` keeps it as labelled text instead |
| `reasoning written as labelled text, no longer replayable by a provider (n)` | `--reasoning as_text`: the destination's model can read it, its provider cannot replay it |
| `tool calls written as text, not translated (n, Tool, …)` | the destination has no equivalent tool, or an argument had no counterpart; the call and its result travel as text. Nothing is invented |
| `unanswered tool calls carried as text (n)` | the source never recorded a result for a call (a killed tool); a provider refuses a call without its result, so it travels as text |
| `orphan tool results dropped (n)` · `duplicate tool results dropped, the first answer kept (n)` | a result with no call, or a second answer to one call; a provider would refuse either |
| `tool call ids rewritten to the destination's id pattern (n)` | Anthropic accepts only `[a-zA-Z0-9_-]` IDs; the call and its result were rewritten together |
| `records of an abandoned branch … left behind (n)` | a rewind, an edited prompt, or the earlier file of a Codex paginated thread: only the resumed branch travelled. `--all-branches` carries a Codex thread whole |
| `messages between a subagent and its parent with no readable text (encrypted by the source), not carried (n)` | Codex keeps most `agent_message` rows encrypted: no text to write. The readable ones travel as user turns |
| `compaction summaries with no readable text (encrypted by the source's provider), not carried (n)` | Codex 0.165 keeps its compaction summary encrypted: no text to write, so it is left out rather than written as an empty message. The history before and after it travels |
| `an unfinished last line of the source left unread (n)` | a hard kill cut the last line mid-write; the rest of the session was read |
| `steps the source aborted or that failed, written as ordinary replies (n)` | the destination cannot mark a step as aborted (Pi can, and does). To leave them out, use `--interrupted drop` |
| `empty steps dropped (n)` · `user attachments dropped (n)` · `structured tool-result blocks flattened to text (n)` | content the destination cannot hold (a PDF, an image toward a text-only result) is counted, never silently dropped. Images are rewritten into each harness's own shape |
| `the source's compaction summary travels as one user message` | `--compaction portable` |
| `the resume prompt did not complete: …` | the session was written but `--resume-prompt` failed (exit 5): see [troubleshooting](#12-troubleshooting) |
| `child <id>: <reason>` | `hop --children`: a subagent session was not carried (`<id>` is the source's), or carried with a loss of its own (the child's own line, prefixed). See [Carry the subagent sessions](#carry-the-subagent-sessions) |

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

- The source must have compacted at least once (a Claude `/compact`, a Pi compaction, a Hermes
  summary row, a dsh `compaction/summary`, …); otherwise
  the hop is refused with a reason, nothing written (exit 2). A Codex 0.165 summary is encrypted, so
  it is refused too.
- The manifest keeps the line `the source's compaction summary travels as one user message` and how
  many entries were kept.

#### `native`: keep everything, add a native boundary

*The imported history is larger than the destination model's useful budget, but nothing should be
removed.* Mukade keeps the whole session and appends the destination's own compaction record with a
deterministic, free summary of the earlier user requests; the destination sends that summary plus the
recent tail, and the older rows stay readable.

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
| Hermes, DeepSeek Harness | none: `--compaction native` toward them is refused (exit 2); use `portable` or no compaction |

- `--context-window` is required (above zero); `--max-output-tokens` defaults to 16000;
  `--text-token-factor` is optional (a positive number; default chars/3).
- The budget is the smaller of half the window and OpenCode's automatic-compaction ceiling minus room
  for the next turn (`src/native_compaction.rs` `import_budget()`).
- `document` reports the boundary under `native_compaction`; `hop` puts `"compaction": "native"` in
  the manifest. No model is called; `dump` refuses this mode.

#### `summary`: the source's own model writes one

*The session is long, it never compacted, or its summary is encrypted (Codex).* The source harness
summarizes it with its own model, login and configuration, in a run that saves nothing in its store;
the destination starts from that summary.

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
  wrote it, which intent prompt, how long it is and, for Claude Code, what it cost.
- A summary that cannot be written (error, empty reply, deadline) writes nothing (exit 4).
- Refused (exit 2): with `--all-branches` (the summary stands for the branch the source resumes), and
  with `--from-file` unless the source is Pi (only Pi forks a session from a file). A Hermes or
  DeepSeek Harness **source** is refused with its reason: neither has a run that leaves the session
  as it was (`hermes chat --resume` and `dsh --session-id` append the question to it). Use `portable`
  or `native` for those two, or any other source's summary.
- If OpenCode's fork could not be deleted, `dump` prints on stderr
  `the summarizing run left a fork in the source store: run <command>`.

#### Choose what the summary emphasizes

`--intent <name>` selects a prompt for the source model. It implies `--compaction summary`, so the
destination receives the summary alone unless `--keep-turns` adds turns. It changes only the summary
prompt. `dump` and `hop` take it.

1. See the prompts: `mukade intents`.
2. Pick one:

   ```powershell
   mukade hop claude_code YOUR_CLAUDE_SESSION_ID pi --cwd 'C:\work\my-project' --intent review
   mukade dump codex YOUR_CODEX_SESSION_ID --intent analyze --output 'C:\work\summary.json'
   ```

3. To add your own, save `<name>.md` in `intent-prompts`: `security.md` is `--intent security`.

- Shipped: `generic`, `continue`, `execute`, `review`, `analyze`, `brainstorm`, `plan`. `generic.md`
  is used when `--intent` is omitted.
- The prompts are plain `.md` files in `intent-prompts` beside the executable ([section 10](#10-where-its-files-go));
  edited or added files are picked up by the next run. A host may set an absolute `MUKADE_INTENT_DIR`
  to keep them elsewhere.
- A prompt is plain text, its whitespace folded to one line.
- A Windows `.cmd` launcher can receive only ASCII letters, digits, spaces and `.,;:()`; any other
  prompt is refused before the harness starts. Name the harness's native executable to use accents
  or richer punctuation.

### A session with interrupted turns

*You stopped the agent mid-answer, or the harness was killed, and the session holds half-done work.*
A session whose turns were **all** interrupted is exported like any other, with every prompt and
whatever the agent had written or run before each interruption.

| `--interrupted` | what is written |
|---|---|
| `keep` (default) | every turn as the source holds it |
| `drop` | finished turns whole; of an interrupted turn, the prompt alone (it is yours, and complete) |

`hop`, `write` and `document` take it; another value, or another command, is refused before a store
is read (exit 1).

```powershell
mukade hop claude_code YOUR_CLAUDE_SESSION_ID pi --cwd 'C:\work\my-project' --interrupted drop
# start the last request again in the destination, without the half-written answer
mukade hop codex YOUR_CODEX_THREAD_ID claude_code --cwd 'C:\work\my-project' --interrupted drop --resume-prompt 'Start again from my last request.'
```

| a turn in the source | `keep` | `drop` |
|---|---|---|
| finished | prompt, replies, tool calls and results | the same |
| interrupted while the model was answering | prompt and the cut-off reply. Pi marks it `aborted`; another destination writes an ordinary reply, counted in `aborted_steps_as_replies` | the prompt |
| interrupted after tools had run | prompt, calls and results | the prompt |
| interrupted before anything was written | the prompt | the prompt |

**How a turn is known to be interrupted.** Mukade does not guess: it reads the sign the source wrote.

| source | the sign |
|---|---|
| Claude Code | the user row `[Request interrupted by user]`, or `[Request interrupted by user for tool use]` beside a tool result |
| Codex | the `<turn_aborted>` message and the `turn_aborted` event |
| OpenCode | an assistant row with no `finish`, or `finish: "aborted"` |
| Pi | an assistant message with `stopReason: "aborted"` |

These markers are the harness's words: never written into the destination as a prompt, in either mode.

**Check what was left out.** The manifest states the mode, counts the steps and adds a loss line (a
hand-made Codex rollout of two interrupted turns, hopped with `--interrupted drop --manifest <file>`):

```json
"options": { "compaction": "none", "summary": null, "reasoning": "drop_foreign",
             "resume_prompt": false, "without_tools": false, "interrupted": "drop" },
"counts":  { "messages_in": 5, "messages_out": 2, "dropped_interrupted_steps": 2, … },
"losses":  [ "steps of interrupted turns omitted by request, with their tool results (2)",
             "interrupted_last_turn" ]
```

`messages_out` is the two prompts. `interrupted_last_turn` is still stated: the source did end on a
turn it never finished.

What it does not do:

- **A process killed while the model was answering leaves no sign** in a Claude Code or Codex file:
  that reply reads as an ordinary one, and `drop` keeps it. (A turn killed before anything was
  written is a prompt alone.)
- **A document made by `dump` keeps each reply's end but not the markers**, so `write --interrupted
  drop` sees the turns whose last reply was cut, not those interrupted right after a tool answered.
  `hop` reads the source itself and sees both: prefer it.
- **A summary is not rewritten**: turns folded into the summary (`--compaction portable` or
  `summary`) are the summary's; the option acts on the turns that travel as they are.
- It is not a `dump` or `list` option, and `mukade-tui` always keeps.

### A session that was rewound or branched

*You rewound in Claude Code, edited a prompt, or navigated back in Pi's `/tree`.* Mukade reads the
selected branch: abandoned turns do not become history in the destination, stay in the source
untouched, and are counted (`dropped_off_branch`). Pi's `branch_summary`, `bashExecution`,
`custom_message` and `context_edit` entries reach the destination the way Pi shows them to its model.

**Claude Code: how the branch is chosen.** By the `last-prompt.leafUuid` checkpoint when present
(a later continuation or a new user branch can supersede it), else the last non-sidechain user or
assistant row; parent links and compaction metadata give the order. Missing parents, cycles,
duplicate UUIDs, contradictory compaction metadata and ambiguous records are refused, not assigned
to a branch. Rules and refusals: [spec §2.1](session-interchange-spec.md#21-claude-code-21311) and
[the Claude Code page](harnesses/claude-code.md#how-mukade-reads-it). Real automatic compaction and
rewind are still to be verified against Claude Code itself ([next.md](../next.md)).

**Codex Desktop 0.165: a thread over several files.** Such a thread is an earlier file plus a page
that continues from part of it (redone again, a further page continues from the previous one). Mukade
reads it as Codex resumes it: each earlier file up to the point the next continues from, then the
latest page; what the earlier files hold past those points is left behind and counted.

To carry **everything**, abandoned turns included (in file order, before the active ones), add
`--all-branches` to `dump`, `hop` or `document`; Codex only, refused (exit 2) for another harness:

```powershell
mukade hop codex YOUR_CODEX_THREAD_ID claude_code --cwd 'D:\Projects\my-project' --all-branches
```

### Subagents and what they reported

*The agent started subagents (Claude Code `Agent`, Codex `spawn_agent`, OpenCode `subagent`).* A hop
carries the session itself: its spawn calls travel as text, and each report the parent's model
received travels where it was received — a call's result stays its result, a report that arrived
on its own is a user turn:

| harness | the report in the parent |
|---|---|
| Claude Code | the `Agent` call's result (a foreground subagent); the `<task-notification>` that announces a background one, a plain row or an `attachment` row |
| Codex | an `agent_message` with readable text; an encrypted one is counted (`dropped_subagent_messages`) |
| OpenCode | the `synthetic` message with `metadata.source: "subagent"` |

A report on an abandoned branch (a rewind) is left behind with that branch and counted in
`dropped_off_branch`. A subagent's own transcript is a session of its own: `list --children` finds
it, `dump` and `hop` take it by its id ([Subagent sessions](#subagent-sessions)). The TUI does not
resume a Claude Code subagent (`R`): Claude Code resumes sessions, not their subagents; it names the
parent to resume instead.

#### Carry the subagent sessions

`--children` on `dump` and `hop` carries every readable subagent session of the session, at any
depth. Without it nothing changes.

| | |
|---|---|
| `dump --children` | the document gets `children`: one full document per child, nested as the children hang (a child's own children sit in its `children`). A child that could not be read is named in `children_lost`, one `<id>: <reason>` each. `--from-file` cannot go with it: children are found in the store |
| `hop --children` | the session is written first, then each child as a session of its own, parents before children, in the order their spawn calls came in the parent; a spawn call unknown comes after those known |

What `hop --children` writes, per destination:

| destination | each child is | link to the parent |
|---|---|---|
| OpenCode | an imported session | `info.parentID`: the parent is imported first |
| Claude Code | `subagents/agent-<agentId>.jsonl` and its `.meta.json` in the folder of the parent's id; layout and fields: [Claude Code page, "Writing subagent sessions"](harnesses/claude-code.md) | the folder; its id is `<parent-id>.<agentId>`, the id `list --children` shows |
| Codex | a rollout of its own | none: Codex records no link a writer can make; the manifest is the only record |
| Pi | a session file of its own | none: Pi's `parentSession` means fork, not spawner |
| Hermes | an import of its own (about a minute each) | none: the import has no field for a spawner |
| DeepSeek Harness | a log of its own | none |

- **Same pipeline, same options.** A child is read, planned and written like the session:
  `--reasoning`, `--without-tools`, `--tool-map`, the destination facts. `--dest-id`, `--dest-path`,
  `--metadata-file`, `--resume-prompt` and `--expect-sha256` are the session's alone; a child gets a
  new id every run, so a repeated hop writes its children again.
- **Compaction.** `--compaction native` applies to each child (it is deterministic); `portable` and
  `summary` are the session's alone, and a child travels whole (`compaction: "none"` in its entry).
- **The session names its children.** A spawn call written as text gets one more line, `→ <id>`:
  the destination session of the child it started. A call written as a tool call of the destination
  (`--tool-map`) names it through Claude Code's `toolUseId` instead.
- **A child that cannot be read** (a Claude meta.json with no transcript, a Codex thread whose rollout
  is gone, a Claude child that continues a sibling's transcript) is a `child <id>: <reason>` line in
  `losses`; the hop goes on.
- **A child that cannot be written** stops the children there: the session stays written, the later
  children are lost lines (`not written, an earlier child failed`), the receipt is printed, `--manifest`
  is saved and the exit code is 5.
- **A child that inherits context** (a Claude `fork` agent, a Codex thread forked from its parent)
  is written with what its own file holds; its entry has `inherits_context: true`.

`manifest.children`, one entry per child written:

| key | holds |
|---|---|
| `source_id` | the child's id in the source |
| `dest_id`, `dest_path` | its id and file (or database) in the destination |
| `link` | `native` (OpenCode, Claude Code) or `none` (Codex, Pi, Hermes, DeepSeek Harness) |
| `depth` | 1 for a child of the session, 2 for a child of a child, … |
| `compaction` | how it was written: `none` or `native` |
| `inherits_context` | `true` when the child starts from its parent's conversation, which its file does not hold |

The receipt on stdout is unchanged; one line per child goes to stderr:
`child <source id> -> <destination id> (linked to its parent)`.

```powershell
mukade hop claude_code YOUR_CLAUDE_SESSION_ID opencode --children --cwd 'C:\work\my-project' --project-id YOUR_PROJECT --model-provider YOUR_PROVIDER
mukade dump codex YOUR_CODEX_THREAD_ID --children --output C:\work\session.json
```

### Leave out tool calls

`--without-tools` on `hop`, `write` or `document` omits the historical calls and their results; the
manifest records `options.without_tools`, `tools_omitted` and the loss. `dump` and `list` do not take it.

### Tool calls and reasoning: what is converted, and when

**Tool calls** are translated to the destination's own tool when a session is written (`write`,
`hop`, `document`), by the rules of [`tool-conversion-tables.md`](tool-conversion-tables.md). One with
no equivalent travels as text, with its arguments, and is counted (`tools_text_form`).

**Shell calls** keep their dialect: the command is never rewritten (`src/tools.rs` `other_dialect()`).

- Toward Codex, OpenCode, Pi and DeepSeek Harness a shell call is translated only when the destination runs a shell of
  its dialect: the one `--dest-shell` names (`bash`, `sh`, `zsh`, `dash`, `ksh` or `posix` for
  POSIX, `powershell` or `pwsh` for PowerShell), else the one the destination's own configuration or
  platform names for the project the session is filed under
  ([per destination](tool-conversion-tables.md#41-shell)). `--dest-shell none` names no dialect:
  every such call travels as text. DeepSeek Harness runs `pwsh` on Windows and `bash` elsewhere.
- Toward Hermes no call is translated: its import keeps no tool call, so every call travels as text with
  its arguments and its result.
- Toward Claude Code a POSIX call becomes `Bash`, a PowerShell call `PowerShell` where the
  installation runs that tool ([how mukade tells](harnesses/claude-code.md#the-powershell-tool)),
  else text. That is the offered set, not a dialect: `--dest-shell` does not change it, `none`
  included.

**Reasoning (chain of thought) is not converted**, because it cannot be: a block has visible text
and an opaque part (a signature over the text, or a provider-encrypted payload) that only the
protocol that produced it can read back, and mukade never edits or makes up. `--reasoning` decides
what happens to each block:

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
| Hermes, DeepSeek Harness | nothing: reasoning text is carried (dsh) or dropped (Hermes's import has no place for it); a signature or encrypted payload never |

**OpenCode, Pi, Hermes and DeepSeek Harness** speak whatever provider they are configured with, so their protocol is unknown and
**by default every reasoning block toward them is dropped**. Name the protocol to keep blocks of the
same protocol:

```powershell
mukade hop claude_code YOUR_CLAUDE_SESSION_ID pi --cwd 'C:\work\my-project' --dest-protocol anthropic-messages
```

- `--dest-protocol` is `anthropic-messages`, `openai-responses` or `openai-chat` (also accepted:
  `anthropic`, `claude`; `openai`, `responses`, `codex`; `chat`, `chat-completions`,
  `openai-completions`). Without it, Claude Code is `anthropic-messages` and Codex `openai-responses`.
- Name the protocol the destination's provider really uses: mukade takes it on trust. The strict
  check refuses a block of one protocol carried into another (exit 4) but compares against this value,
  so it cannot tell that the value itself is wrong.

## 6. Host and automation options

**Use it from a program.** Mukade is a library first: `mukade::hop(&HopRequest { … })` does what the
command does, with typed errors (`HopError::ResumeFailed` carries the written session's receipt).
See the [README](../README.md#rust-library).

| option | rule |
|---|---|
| `--expect-sha256 <hash>` | `hop` only; 64 hex digits. Checks the captured source fingerprint before writing or starting a source-model summary; a mismatch is refused (exit 2). It covers the selected source in its entirety: an appended file changes it |
| `--dest-id <id>` | reserves the destination session ID: `ses_<id>` for OpenCode, a UUID for Claude Code, Codex and Pi, `session-<uuid>` (or a UUID) for DeepSeek Harness. Hermes refuses it: Hermes allocates the id |
| `--dest-path <path>` | reserves the destination file itself, for the file-based destinations (Claude Code, Codex, Pi). Hermes (a database) and DeepSeek Harness (found by id and project folder) refuse it (exit 2). Absolute, inside the destination root, free of `..`. OpenCode refuses it (exit 2): its native import allocates the file. `hop` checks it before reading the source, so a refused path runs nothing, a summarizing model included |
| `--dest-agent <name>` | OpenCode only: the primary agent (every other destination refuses it) |
| `--metadata-file <path>` | OpenCode only: a JSON object stored as session metadata; at most 1 MiB; protect it like the session |
| `--tool-map <path>` | explicit bindings instead of the destination catalog's defaults (below) |
| `--dest-shell <s>` | the shell the destination runs, over the one mukade reads from its configuration; `none`: no dialect (shell calls into Codex, OpenCode and Pi as text; Claude Code's offered set unchanged). Decides whether shell calls are translated ([section 5](#tool-calls-and-reasoning-what-is-converted-and-when)), and is recorded where the destination records one |

- **OpenCode IDs and metadata.** When supplied, `OPENCODE_DB` must name the destination explicitly:
  mukade checks the resulting ID and metadata there after the native import. If the process fails or
  the check cannot confirm the write, exit 5 reports an uncertain result and its ID; inspect the
  destination before retrying.
- **Retrying safely.** State both `--dest-id`/`--dest-path` and `--expect-sha256`, so the second run
  lands on the same destination, and only while the source is unchanged.
- **Tool map file.** A JSON array of `[source, destination, original, mapped]` tuples: two tool names
  (1 to 256 characters) and the two JSON objects the conversation holds and the destination should
  receive. At most 4096 entries and 1 MiB; a malformed map is refused before anything is written.

## 7. Stores and executables

| Harness | CLI name | Store/config variable | Executable flag | Executable environment variable |
|---|---|---|---|---|
| Claude Code | `claude_code` | `CLAUDE_CONFIG_DIR` | `--claude-exe` | `MUKADE_CLAUDE_EXE` |
| Codex CLI | `codex` | `CODEX_HOME` | `--codex-exe` | `MUKADE_CODEX_EXE` |
| OpenCode | `opencode` | `OPENCODE_DB` (database file), plus its applicable `XDG_*` settings | `--opencode-exe` | `MUKADE_OPENCODE_EXE` |
| Pi | `pi` | `PI_CODING_AGENT_DIR` | `--pi-exe` | `MUKADE_PI_EXE` |
| Hermes Agent | `hermes` | `HERMES_HOME` (the folder holding `state.db`) | `--hermes-exe` | `MUKADE_HERMES_EXE` |
| DeepSeek Harness | `deepseek_harness` | `DSH_HOME` (the folder holding `sessions/`) | `--dsh-exe` | `MUKADE_DSH_EXE` |

**Stores.**

- Claude Code, Codex, Pi, Hermes and DeepSeek Harness require an explicit root, even for standard
  installations (an empty variable is no root). Hermes's own home is `%LOCALAPPDATA%\hermes` (Windows) or `~/.hermes`; dsh's is `~/.dsh`.
- OpenCode: `list opencode` needs an explicit `OPENCODE_DB` or `--root`; without one it refuses with
  `OPENCODE_DB is not set: no OpenCode store to list`. The OpenCode processes mukade starts
  (`session export`, the import, a resume) get `OPENCODE_DB` when set, and otherwise run with the
  variable removed, against OpenCode's own configured store: supply the database and environment
  when isolation matters.
- `--root` changes the selected harness's store for `list`, `dump` or `write`. For `hop` it applies to
  the **source**, including when both ends use the same harness; the destination is its variable, or
  `--dest-root`.
- For OpenCode, an existing directory passed as `--root` means its `opencode.db`; a file path is used
  as-is.
- `--root` and `--dest-root` take a store, or a folder that holds one installation: a path that is
  not a store is scanned as `mukade scan` would and must hold exactly one installation (of the
  harness named, when one is); several are refused with the list of their stores. A path that holds
  nothing is still used as the store of a named harness: a destination store can be empty.
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
store, writes the session and appears in the resume command; an explicit `--<harness>-exe` still wins
over it (and over `MUKADE_*_EXE` and `PATH`). Into a portable JanuraCode:

```powershell
mukade hop claude_code YOUR_CLAUDE_SESSION_ID opencode --dest-root 'D:\Apps\JanuraCode' --cwd 'C:\work\my-project'
```

- The destination's model and provider come from the installation's own configuration when it keeps
  one beside its store: an OpenCode database in an XDG data layout (`<root>\share\opencode\opencode.db`)
  is configured by `<root>\config\opencode\opencode.json`, a Codex home by its `config.toml`. When
  that file exists it alone answers; otherwise `OPENCODE_CONFIG` and `XDG_CONFIG_HOME` apply.
  `--model-provider` and `--model` win over both.
- A database with no session for the source's folder yet cannot name its project: pass `--project-id`
  for that first hop.
- Only `OPENCODE_DB` is set for the import itself, as for any OpenCode destination: the engine runs
  with the `XDG_*` configuration of the shell that runs mukade.

**Resume.**

- A `--resume-prompt` runs the destination with its store variable set to the root the session was
  written to, from the destination's working directory (`--cwd`, or the source's recorded one, which
  must exist on this machine).
- A resume command you run later by hand does not inherit that: set the destination's variable in
  that shell first (for OpenCode also `XDG_*` and `OPENCODE_CONFIG` where needed: a database path
  alone does not isolate completely).
- Refused before anything is written: a resume prompt that starts with `-` (every harness CLI would
  read it as an option), and one with line breaks when the destination's executable is a `.cmd`
  launcher, which cannot take them.

**Deadlines.** Every harness process mukade starts, except `mukade-tui`'s interactive resume, runs with
a deadline; one that outlives it is stopped **with everything it started** (an MCP server, a shell):
Windows `taskkill /T`, elsewhere a walk of the process tree (`src/exe.rs` `kill_tree()`). A detached
daemon is not found. A timeout too large for the clock sets no deadline.

## 8. Per-harness notes

A harness kept in a folder of its own (`D:\portable\…`, a USB stick) needs its store variable and
`--<harness>-exe` (that concerns the **harness**; mukade needs only its executable). Each harness
has a reference page (install, store, compaction, tools, hooks): [`docs/harnesses/`](harnesses/). The
examples above work for any pair; what changes per harness:

| harness | store variable, resume | destination flags | also |
|---|---|---|---|
| Claude Code | `CLAUDE_CONFIG_DIR`, kept set for `resume` | `--claude-exe` | mukade sets up no install or authentication |
| Codex CLI | `CODEX_HOME`, kept set for `resume` | `--codex-exe`, `--dest-version` | `list` includes archived threads |
| OpenCode | `OPENCODE_DB` plus `XDG_*`, kept set for `resume` | `--opencode-exe`, `--project-id`, `--model-provider`, `--model` | reads a source with `opencode session export … --standalone`; imports with `--standalone` |
| Pi | `PI_CODING_AGENT_DIR`, kept set for `resume` | `--pi-exe` (a `.cmd` launcher works) | a separate agent directory does not prove every resource Pi reads is isolated |
| Hermes Agent | `HERMES_HOME`, set for the import and for `resume` | `--hermes-exe` | a destination only through `hermes sessions import`: tools travel as text, the id is Hermes's, the import takes about a minute |
| DeepSeek Harness | `DSH_HOME` | `--dsh-exe` (a `.cmd` launcher works) | the log is written by mukade; resume from the session's own folder |

**Every harness is a destination.** How each is written:

| destination | written by | id | tools | native compaction | resume in the receipt |
|---|---|---|---|---|---|
| Claude Code | mukade, a JSONL file | UUID | translated | ✅ | `claude -p --resume <id>` |
| Codex CLI | mukade, a rollout file | UUID | translated (`exec_command` only) | ✅ | `codex exec resume <id>` |
| OpenCode | `opencode import` of an interchange document | `ses_…` | translated | ✅ | `opencode run --standalone --session <id>` |
| Pi | mukade, a JSONL file | UUID | translated | ✅ | `pi -p --session <file>` |
| Hermes Agent | `hermes sessions import` of a Claude-shaped file | Hermes's own | text form only | ❌ refused | `hermes chat -Q --resume <id> -q` |
| DeepSeek Harness | mukade, a zstd log | `session-<uuid>` | translated (files, search, fetch, shell) | ❌ refused | `dsh --profile headless --session-id <id>` (add a task) |

A harness in its own folder, as destination:

```powershell
$env:CLAUDE_CONFIG_DIR = 'D:\portable\claude\config'
$env:PI_CODING_AGENT_DIR = 'D:\portable\pi\agent'
mukade hop pi YOUR_PI_SESSION_ID claude_code --cwd 'C:\work\my-project' --claude-exe 'D:\portable\claude\bin\claude.exe'
```

### Claude Code

Claude Code names a project's folder by its own rule (every character outside `[a-zA-Z0-9]` becomes
`-`; a name past 200 characters is cut and suffixed with a hash); mukade follows it
([Claude Code page](harnesses/claude-code.md)).

### Codex CLI

- `list codex` marks the threads archived with `codex archive` (rollouts in
  `$env:CODEX_HOME\archived_sessions`) `"archived": true` ([Archived sessions](#archived-sessions));
  `dump` and `hop` read them like any other.
- Codex requires a destination CLI version in its session metadata. Mukade asks the chosen
  executable's `--version`; if that cannot be done or parsed, supply it: `--dest-version 0.164.0`
  (your version).
- Mukade reads `model_provider` from `$env:CODEX_HOME\config.toml` when available; `--model-provider`
  states it explicitly. Only the top-level `model_provider` key counts, not a profile's or a provider
  table's.

**Threads of the Codex Desktop app (0.165).** The ID in a `codex://threads/<id>` link is the thread ID
mukade takes: `mukade hop codex 01a181e2-4cd9-7c63-82b8-f75e3d6bb172 claude_code --cwd 'D:\Projects\daodan'`.
What to expect:

- **Several files**: read as Codex resumes it; `--all-branches` carries it whole
  ([rewound or branched](#a-session-that-was-rewound-or-branched)).
- **Encrypted compaction**: `--compaction portable` is refused (exit 2) and a whole-history hop leaves
  the summaries out, counted. A long thread is therefore carried whole: a 17.8 MB thread became a
  4.1 MB Claude Code session of several hundred thousand tokens, which only a model with a very large
  context can resume. Every message in a resumed session reads the whole context first: the first one
  costs that much, or makes Claude Code compact before answering.
- **Code mode**: most tool calls are one `exec` script each. A script of only literal
  `text(await tools.<name>(<literal>));` statements is read as the calls it made, each translated like
  any other (`split_code_mode()`; `apply_patch` becomes a write or edit); any other script stays one
  call in text form. On 34 real rollouts (Codex 0.167), 725 of 981 scripts were split
  ([verification](verification/2026-11-11-codex-code-mode.md), [rule](tool-conversion-tables.md#5-reading-codex-desktop-code-mode-exec)).
- **PowerShell shell calls**: such rollouts declare PowerShell, so their shell calls translate only
  toward a destination that runs PowerShell, Codex on Windows included
  ([section 5](#tool-calls-and-reasoning-what-is-converted-and-when)).
- **Reasoning** is OpenAI's: dropped toward any other provider, counted.
- **Subagents**: their messages to the main agent are left out on purpose (encrypted, readable only by
  OpenAI); the main agent's messages to them travel as text with that content replaced by
  `<encrypted by the source's provider, N chars>` ([Codex page](harnesses/codex.md#how-mukade-reads-it)).
- **Model warning in Claude Code**: resuming prints `Session model gpt-… could not be restored …
  using claude-…`: the rows name the model that wrote each reply, which Claude Code does not know.
  Expected; nothing to fix.

### OpenCode

`list opencode` needs the database via `OPENCODE_DB` or `--root`, reads it without starting OpenCode,
and includes top-level archived sessions (`--archived false` leaves them out). Set the `XDG_*`
variables your installation uses in the shell as well, if applicable.

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
  are faster ([Speed](#9-speed)).
- The receipt's `location` names the database the session was imported into (`OPENCODE_DB`), or
  says it went to OpenCode's configured store: use the returned ID with that store.
- **A custom OpenCode source:** set `MUKADE_OPENCODE_EXE` ([section 7](#7-stores-and-executables)).

### Pi

- The new session goes below `PI_CODING_AGENT_DIR\sessions`, in the folder Pi itself would use for the
  project; the receipt's `resume` holds the chosen launcher and the session file path.
- Verify that Pi installation's provider credentials and configuration separately.
- Sessions written this way were run against Pi 1.2.0 and 1.4.0
  (`docs/verification/pi-session-and-reasoning-verification.md`).

### Hermes Agent

- **Source.** `list hermes` reads `<HERMES_HOME>/state.db` read-only, without starting Hermes; it
  lists what Hermes's own `sessions list` lists: the sessions of the sources `oneshot`, `tool` and `kanban`
  (finite runs, integrations, workers) are left out, and `list --children` and the TUI add them; any
  is readable by id. A store older than schema 32 is refused, naming it.
- **Destination.** mukade runs `hermes sessions import --from claude` on a file it writes, then deletes
  the file. That keeps user and assistant text only: every tool call and result is text inside the
  turns, no model or reasoning is recorded, and an image is `[image]`. Refused: `--dest-id`,
  `--dest-agent` and `--metadata-file` (exit 1, a command-line mistake: the same as for any destination
  that does not take them); `--dest-path` and `--compaction native` (exit 2, unsupported).
- **Resume.** The receipt's command is `hermes chat -Q --resume <id> -q`; interactively `hermes --resume <id>`.
  `--resume-prompt` runs it with `HERMES_HOME` set to the store written to.
- **Slow.** The import starts Hermes's gateway: about a minute per session, and per child with `--children`.
  An import that failed or did not confirm may still have written: exit 5 names `state.db`; look with
  `hermes sessions list` before retrying.

```powershell
$env:HERMES_HOME = "$env:LOCALAPPDATA\hermes"
mukade list hermes
mukade dump hermes YOUR_HERMES_SESSION_ID --output C:\work\session.json
mukade hop hermes YOUR_HERMES_SESSION_ID claude_code --cwd 'C:\work\my-project'      # out of Hermes
mukade hop claude_code YOUR_CLAUDE_SESSION_ID hermes --cwd 'C:\work\my-project'      # into Hermes
```

Details and verification: [Hermes Agent page](harnesses/hermes-agent.md).

### DeepSeek Harness

- **Source.** `list deepseek_harness` walks `<DSH_HOME>/sessions/**/session.v4.jsonl[.zstd]`, decoding
  the zstd frames itself (no `dsh` needed). Only format 4 is read; a torn tail is dropped and noted.
- **Destination.** mukade writes a v4 log in the folder `dsh` derives from the id and the cwd. The id
  is `session-<uuid>` (`--dest-id` takes that or a UUID); `--cwd` must be an absolute folder. Refused:
  `--dest-agent` and `--metadata-file` (exit 1), `--dest-path` and `--compaction native` (exit 2).
- **Resume.** `dsh` has no terminal UI, so the receipt's command is headless and needs a task:
  `dsh --profile headless --session-id <id> "<task>"`, run from the session's cwd. The TUI says so
  instead of opening it.

```powershell
$env:DSH_HOME = "$HOME\.dsh"
mukade list deepseek_harness
mukade dump deepseek_harness YOUR_DSH_SESSION_ID --output C:\work\session.json
mukade hop deepseek_harness YOUR_DSH_SESSION_ID claude_code --cwd 'C:\work\my-project'   # out of dsh
mukade hop claude_code YOUR_CLAUDE_SESSION_ID deepseek_harness --cwd 'C:\work\my-project' # into dsh
cd 'C:\work\my-project'; dsh --profile headless --session-id NEW_SESSION_ID "continue"
```

Details and verification: [DeepSeek Harness page](harnesses/deepseek-harness.md).

## 9. Speed

Measured on this repository's release build, hopping a Claude Code session of that size (mostly tool
output, like real ones) into Pi, Claude or Codex:

| source session | time (Pi, Claude or Codex as destination) | peak memory |
|---|---|---|
| 1 MB | 30–35 ms (`dump` alone: 24 ms) | not measured |
| 10 MB | 185–240 ms (`dump` alone: 150 ms) | about 100 MB |
| 30 MB | 440–630 ms (`dump` alone: 420 ms) | about 300 MB |

Before the work on speed the same hops took 83, 556 and 1657 ms. The time is proportional to the size
of the session, because every record is read and decoded once.

`--timing` prints the phases on stderr (a real run, `hop claude_code … pi --compaction portable` on
the 5-message fixture). A phase prints when it ends, so a nested one comes first: `serialize + write
file` is inside `write files`; `resumed branch`, `entries`, `finish` inside `build entries`;
`fingerprint (parallel)` runs on a second thread while the file is parsed:

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

What makes it fast, and what you can do:

- **The file formats are not the cost.** Claude Code, Codex and Pi store JSON Lines and OpenCode
  SQLite; mukade reads and writes them as they are. Decoding is spread over up to 8 threads for large
  files; the fingerprint is hashed on another thread.
- **Asking the destination its version is the real cost for small sessions.** Pi's `--version` takes
  about half a second (it starts Node), so mukade reads the version from the npm package the launcher
  starts. A native binary is asked once; the answer is remembered by path, size and modification time
  in `mukade-data/cache` ([section 10](#10-where-its-files-go)).
- **To skip the question, state the facts:** `--dest-version`, and for OpenCode `--project-id`,
  `--model-provider`, `--model`. A `--resume-prompt` starts the destination and a model: that time is
  not mukade's.

## 10. Where its files go

On first start mukade copies the editable intent prompts to `intent-prompts` beside the executable (an
existing file is never replaced). Its other files go to `mukade-data` beside the executable, made the
first time something is written to it. `MUKADE_DATA_DIR` can select a different **absolute** data
folder for a host; an invalid override is refused (exit 1) rather than redirected.

| file | what for |
|---|---|
| `intent-prompts/*.md` | the editable summary prompts: `generic.md` is the default, every other file stem is an `--intent` name; copied on first start and never overwritten |
| `mukade-data/cache/probes.json` | versions and OpenCode project IDs learned from harness binaries, keyed by the binary's path, size and modification time, so the next hop does not ask again |
| `mukade-data/cache/claude-list-<hash>.json` | what `list claude_code` read from each session file (ID, title, cwd, time), keyed by path, size and modification time, so only changed files are read next time; one per Claude Code `projects/` folder, rewritten whole; a file modified in the last 2 seconds is never kept in it |
| `mukade-data/tmp/mukade-import-*.json` | the document `opencode session import` needs as a file; private, and removed when the import ends, even when it fails |
| `mukade-data/tmp/summary-*/` | one folder per `--compaction summary` run: the file Codex writes its summary to, the session Pi forks; removed when the run ends, even when it fails |
| `.mukade-*.tmp` | beside the session being written, in the destination's store: the session as it is written, renamed into place when complete |

- Nothing is written to the user profile, the application-data folders, the registry or the system's
  temporary folder.
- A temporary file or summary folder a **killed** run left behind is removed by the next run once it
  is an hour old.
- Yours stay yours: the sessions written into the harnesses' stores, and what `--output` and
  `--manifest` ask for.
- To remove mukade, delete its folder. Harnesses are separate ([section 7](#7-stores-and-executables)).

**When the data folder cannot be written**, mukade looks for no other place: the cache is not kept
(every hop asks the harness: slower, but works), and a hop to OpenCode stops with an error that names
the folder, before anything is written. Set `MUKADE_DATA_DIR` to an absolute writable folder, or run
the executable from a writable location.

**What it looks like.** The executable copied alone into an empty folder, then two hops to Claude Code
whose launcher answers `--version`; afterwards the folder holds `mukade.exe`, `intent-prompts\` and
`mukade-data\cache\probes.json`:

```text
1st hop   probe destination version   70.8 ms   total  78.6 ms
2nd hop   probe destination version    0.3 ms   total   9.4 ms     (answered from the cache)
probes.json: {"version|<dir>/claude.cmd|20|1793205746861450700":"2.9.9"}
```

A hop that has nothing to ask adds nothing under `mukade-data`: a hop to Pi, with no `pi` launcher to
start, left it absent. A test asserts the sandbox around the real binary stays empty
([README](../README.md)).

## 11. Exit codes

| code | meaning |
|---|---|
| 0 | done |
| 1 | a command-line mistake (unknown option, a flag without its value, an unexpected argument, an option the command does not take, a bad value), or another error outside the codes below (a document that cannot be read or parsed, a `MUKADE_DATA_DIR` that is not absolute) |
| 2 | refused with a reason; nothing was written: the session is not there, the store is not configured, the ID is ambiguous, the source changed (`--expect-sha256`), or the request is not supported (a resume prompt starting with `-`, `--compaction portable` on a source that never compacted, `--all-branches` on a harness other than Codex, …) |
| 4 | failed and nothing changed: the plan would be refused by a provider, the source's model could not write the summary, or the write failed before starting the destination import |
| 5 | recovery is needed: the source could not be read, an external import has an uncertain outcome, the session was written but the resume failed, or `--children` wrote the session but not all its subagent sessions. A failed resume or child still prints its receipt and saves `--manifest`; an uncertain import names its destination ID |

## 12. Troubleshooting

**Finding the session or the harness**

| symptom | cause and fix |
|---|---|
| No store or session found (exit 2) | check the variable, `--root`, source ID and whether the project/store holds that session; `mukade detect <path>` prints evidence |
| Detection ambiguous or empty | name the harness explicitly; a newly created empty store has no layout to infer |
| Cannot run the executable | verify the launcher path, runtime and permissions. A custom OpenCode source needs `MUKADE_OPENCODE_EXE`; a destination uses its flag or variable |
| Codex version or OpenCode project/model missing | pass `--dest-version`, `--project-id`, `--model-provider`, `--model` |

**Writing**

| symptom | cause and fix |
|---|---|
| "mukade could not create its temporary directory" (exit 4, hop to OpenCode) | the data folder cannot be written; nothing was imported. Set `MUKADE_DATA_DIR` to an absolute writable folder |
| External OpenCode import uncertain (exit 5) | the error names the attempted session ID: check it and its provenance in the selected database before retrying; a zero exit from the underlying command does not prove the import |
| A child write failed (`hop --children`, exit 5) | the session and the children before the failed one are written; the receipt on stdout and `--manifest` list them, and each later child is a `not written, an earlier child failed` loss line. Fix the cause the error names, then hop again with a fresh `--dest-id`: the id of the session already written is refused, so reusing it never overwrites. Without `--dest-id` a repeated hop writes the session and its children again under new ids |
| Manifest could not be saved | the session was still written, its receipt is on stdout: save the receipt's `manifest` field, do not run again |
| Path problem on Windows | Pi files a session under a folder named after the whole project path, which can pass 260 characters: use a shorter `--cwd` |

**Summary**

| symptom | cause and fix |
|---|---|
| "summary failed: …" (exit 4) | nothing was written. The message names the command (prompt shown as `<summary prompt v1>`), its exit status and output: a login or quota problem of the source harness, an unreachable model (`--summary-model`), a run past `--summary-timeout`, or a model that called a tool. If OpenCode's fork could not be deleted, it ends with the command that removes it |
| "the summary prompt cannot be passed through the batch launcher …" (exit 2) | `intent-prompts/<name>.md` holds a line break or a character a `.cmd` shim cannot carry ([charset](#choose-what-the-summary-emphasizes)): name the harness's native executable |

**Resume and interrupted turns**

| symptom | cause and fix |
|---|---|
| Resume fails after a successful write (exit 5) | the session exists; its receipt is on stdout and `--manifest` is saved: use its `id` and `location`, since retrying creates another session. Check configuration and credentials in the process environment |
| The destination carries on from a half-written answer | the interrupted turn's work travelled (the default): hop again with `--interrupted drop`; the first session stays ([interrupted turns](#a-session-with-interrupted-turns)) |
| `--interrupted drop` kept a reply that was cut | no sign of the interruption in the source, or the document came from `dump`: hop from the source itself, check `dropped_interrupted_steps` |
| "unknown --interrupted …" (exit 1) · "--interrupted is supported only by hop, write and document" | the mode is `keep` or `drop`; `dump` and `list` do not take it |

**Speed and memory**

| symptom | cause and fix |
|---|---|
| Slower than the [Speed](#9-speed) table | run with `--timing`: a large `probe …` line is a harness started to ask its version or project; pass `--dest-version` (and `--project-id`) |
| "The paging file is too small" / "memory allocation failed" | out of memory (a 30 MB session needs about 300 MB): close other programs and retry; nothing is half-written, because a session is written to a temporary file and renamed into place |
