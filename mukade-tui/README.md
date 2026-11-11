# mukade-tui — harness sessions manager (terminal UI)

One terminal window over the sessions of **Claude Code, Codex, OpenCode and Pi**, and of any
installation a scan finds. Browse them, read a transcript, resume a session, hop it to another
harness, export it to a file, import a file.

A separate crate: the `mukade` CLI and its dependency list do not change. The TUI reads and writes
only through the library (`Reader::list_all`, `export`, `hop_into`, `import_document`) and never parses a harness store itself.

> **In 20 seconds**
>
> 1. `mukade-tui` (or `cargo run -p mukade-tui`)
> 2. `↑`/`↓` pick a harness · `tab` → pick a folder · `tab` → pick a session · `enter` read it
> 3. **`h`** hop · **`e`** export · **`i`** import · **`R`** resume · **`q`** quit

## What it can do, per harness

| | Claude Code | Codex | OpenCode | Pi |
|---|:-:|:-:|:-:|:-:|
| list sessions, filter by folder | ✅ | ✅ | ✅ | ✅ |
| read the transcript | ✅ | ✅ | ✅ | ✅ |
| resume (`R`) | ✅ | ✅ | ✅ | ✅ |
| hop to / from (`h`) | ✅ | ✅ | ✅ | ✅ |
| export (`e`) | ✅ | ✅ | ✅ | ✅ |
| import into (`i`) | ✅ | ✅ | ✅ ¹ | ✅ |
| `updated` time | ✅ | ✅ | ✅ | ✅ |
| `live` (running now) | ✅ | — ² | — ² | — ² |

¹ OpenCode needs a project id for the folder. The TUI asks the destination OpenCode for a session
it already files under that folder; when there is none, the first write there must be done with the
CLI's `--project-id`.
² The store does not record running sessions.

## Run

```text
cargo run -p mukade-tui                  # or target\release\mukade-tui.exe
cargo run -p mukade-tui -- --turns 20    # transcripts open on the last 20 user turns (default 50)
cargo run -p mukade-tui -- --turns 0     # the full history
cargo run -p mukade-tui -- --scan D:\Apps    # also list every installation below a folder
```

`--turns=N` and `--scan=<dir>` work too; `--scan` may be given more than once.

**Where it reads from:** each harness's own variable (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`,
`OPENCODE_DB`, `PI_CODING_AGENT_DIR`). An unset one falls back to the harness's default store
(`catalog/harnesses.json`). Browsing only reads; resume, hop and import write.

Point it at explicit stores (PowerShell):

```powershell
$env:CLAUDE_CONFIG_DIR = "$env:USERPROFILE\.claude"
$env:CODEX_HOME = "$env:USERPROFILE\.codex"
$env:PI_CODING_AGENT_DIR = "$env:USERPROFILE\.pi\agent"
$env:OPENCODE_DB = "$env:USERPROFILE\.local\share\opencode\opencode.db"
.\target\release\mukade-tui.exe
```

Headless checks (scripts, CI):

| command | prints |
|---|---|
| `mukade-tui --check` | each row's session count (or its error), a scanned row's store and engine; then exits |
| `mukade-tui --check --scan D:\Apps` | the same, scanned installations included |
| `mukade-tui --snapshot` | one 120×40 frame |
| `mukade-tui --snapshot-hop` | the same frame with the hop dialog open |

## Screen

The columns read **harness/installation → folder → sessions → details**.

**Layout:** 120 columns or wider shows all four panels; narrower shows the three lists and opens the
transcript over them; below 70 columns only the focused panel is shown.

**Marks in the session list:**

| mark | means |
|---|---|
| `↳` | the session has a parent (a fork or a subagent); details show it as `parent` |
| `[live]` | running now (Claude Code only) |
| `[archived]` | the harness marked it archived |
| `[unreadable]` | listed, not readable: open, hop, export and resume refuse it and say why |
| time, or `—` | `updated`, or none recorded |

A harness row marked `!` could not be listed; its details show the error.

**Details:** `harness`, `id`, `parent`, `cwd`, `updated`, `archived`, `live`, `unreadable`, `path`;
for a scanned installation also `store` and `engine`.

## Keys

**Move**

| key | does |
|---|---|
| `↑`/`↓` or `j`/`k` | move in the focused list (scroll, in the transcript) |
| `tab` / `→` · `shift+tab` / `←` | next / previous column (harness → folder → session) |
| `enter` | next column, or open the selected session's transcript |
| `esc` | close the transcript, or go back one column |

**Act on the selected session**

| key | does |
|---|---|
| **`h`** | **hop** it to another harness or installation |
| **`e`** | **export** it to a JSON document |
| **`i`** | **import** a JSON document as a new session |
| **`R`** | **resume** it in its own harness |

**View, stores, quit**

| key | does |
|---|---|
| `t` | last N turns (`--turns`, default 50) ↔ full history |
| `g` / `G` · page up/down | transcript top / bottom · scroll it 10 lines |
| `o` | scan a folder for more installations (type it, `enter`) |
| `r` | reload every store (resets the folder filter; scanned rows stay) |
| `q` | quit (not while a hop or import is running) |

The footer shows every key except `g`/`G` and the page keys. In a result box, `j`/`k`, the page keys
and `g`/`G` scroll it; `esc`, `enter` or `q` close it.

## Hop (`h`)

1. Press `h`.
2. **Destination:** `↑`/`↓`, then `enter`. Every other row is offered: another harness, or another
   installation of the same one (an OpenCode session into a portable JanuraCode, say).
3. **Compaction:** `↑`/`↓`, then `enter`:
   - `none` — the whole history;
   - `portable` — the source's own last compaction summary plus what follows (needs a source that compacted);
   - `summary` — the source's own model writes a handoff summary, no turn kept verbatim (may take
     minutes, costs a model call).
4. **Prompt:** an optional resume prompt (empty = write only), then `enter` to run.
5. Read the result (`j`/`k`, page keys), then `esc`.

In the dialog `tab` moves between the three fields and `esc` cancels. The session keeps its own
working directory (the dialog shows it).

**The result shows:** the **new id**, its **location**, the **headless** and **interactive** resume
commands, the **manifest** path, **every loss**, and the destination's reply when you gave a prompt.
Manifests are saved as `mukade-data/manifests/<harness>-<id>.json`.

**Model and provider** come from the destination installation's own configuration
(`src/dest.rs` `configured_model_in()`):

| destination | read from |
|---|---|
| OpenCode at `<root>\share\opencode\opencode.db` (a portable JanuraCode) | `<root>\config\opencode\opencode.json` (JanuraCode: `data\config\opencode\opencode.json`) |
| any other OpenCode | `OPENCODE_CONFIG`, else `XDG_CONFIG_HOME` |
| Codex home | `config.toml` (the provider) |
| Claude Code, Pi | none read |

OpenCode refuses a write that lacks a model, a provider or a project id. Model and provider fall
back to the session's recorded model.

## Export (`e`)

1. Select a session.
2. Press `e`.
3. The result shows the file, `mukade-data/exports/<harness>-<id>.json`, and how many messages it holds.

- The file is the **same document `mukade dump` prints**. Import it here with `i`, or anywhere with
  `mukade write <harness> <file>`, on this machine or another.
- Exporting the same session again overwrites the file.
- `mukade-data` is the folder beside the executable, or an absolute `MUKADE_DATA_DIR`.

## Import (`i`)

1. Press `i`. The dialog has three fields; `tab` / `shift+tab` move between them, `enter` goes to the next one.
2. **document** — the path of a `.json` document. It starts with the last file you exported.
3. **into** — the installation to write into (`↑`/`↓`). It starts on the harness you have selected.
4. **prompt** — optional; when given, the destination answers it once the session is written.
5. `enter` on the prompt runs the import. The result is shown like a hop's, manifest included.

The import always makes a **new** session; nothing existing is changed. It uses the destination's
own executable, version, model and (for OpenCode) project, exactly like a hop, and the document's
working directory. `esc` cancels.

## Resume (`R`)

`R` leaves the window, runs the harness's own interactive resume in the same terminal (with the
session's store, executable and working directory), and comes back when you quit the harness. The
commands come from `catalog/harnesses.json` (`interactive_resume`):

| harness | command |
|---|---|
| Claude Code | `claude --resume <id>` |
| Codex | `codex resume <id>` |
| OpenCode | `opencode mini --standalone --session <id>` |
| Pi | `pi --session <file>` |

A scanned installation runs its own executable in place of the bare name.

## Folders (directory filter)

- Folders come from each session's recorded working directory, with their session counts.
- **All directories** shows every session; **Unknown directory** those with no recorded folder.
- The filter never needs the folder to still exist on disk.
- A long path wraps; identical project names show a distinguishing parent path.
- Resume, hop and export act on the selected (filtered) session.

## Installations found by a scan (`o`, `--scan`)

- Each installation found becomes one more row, `<harness> · <name>` (for example
  `OpenCode · JanuraCode`), with its own store and the executable kept beside it.
- `<name>` is the folder that holds both the store and the executable (a portable app's own
  folder), else the store's folder.
- The scan is the library's (`src/detect/scan.rs` `scan_installations()`, the same as `mukade scan`).
  A store already listed is not added twice.
- A scan reads at most 8 levels and 100,000 folders, and the status line says when it stopped early:
  scan the folder that holds the installations, not a whole user profile.

## Limits

**Options kept in the CLI** (`mukade hop` / `mukade write`):

| kind | flags |
|---|---|
| content | `--reasoning`, `--without-tools`, `--interrupted drop` (the TUI always keeps interrupted work), `--all-branches` |
| compaction | `--compaction native`, `--keep-turns`, `--intent`, `--summary-model`, `--summary-timeout` |
| destination | `--cwd`, `--dest-shell`, `--dest-id` / `--dest-path`, `--model` / `--model-provider` / `--project-id` overrides, `--dest-agent`, `--metadata-file`, `--tool-map` |
| safety | `--expect-sha256` |

A TUI hop names no destination shell, so its shell calls travel as text form (`next.md`, destination shell).

- **Names:** each row is named by its own store (OpenCode `session_v2.title`, Claude Code the rows
  its `/resume` picker reads, Codex `session_index.jsonl`, Pi its display name or first prompt). A
  session its store does not name is listed under its id.
- **`updated`:** Codex's comes from its state database, else from the rollout file's last write;
  copying a rollout without keeping its file time changes it. Pi's is its newest record's time.
- **Transcript:** user, assistant, reasoning, tool calls, full results and compactions in native
  order; a tool call's input is cut at 240 characters; bookkeeping (`Meta`) records are hidden.
- **Background work:** scans, reads, exports, imports and hops run on worker threads; a hop or an
  import cannot be cancelled once it starts.
