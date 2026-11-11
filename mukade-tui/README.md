# mukade-tui — harness sessions manager (terminal UI)

A small native terminal window over the sessions of **Claude Code, Codex, OpenCode and Pi**
(and any portable installation you point it at). Browse them, read a transcript, resume a
session, hop it to another harness, export it to a file, import a file.

> **In 20 seconds**
>
> 1. `mukade-tui` (or `cargo run -p mukade-tui`)
> 2. `↑`/`↓` pick a harness · `tab` → pick a folder · `tab` → pick a session · `enter` read it
> 3. **`h`** hop · **`e`** export · **`i`** import · **`R`** resume · **`q`** quit

It is a separate crate: the `mukade` CLI and its dependency list do not change. The TUI reads and
writes through the library (`reader::list_all`, `export`, `hop_into`, `import_document`) and never
parses a harness store itself.

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

¹ OpenCode needs a project id for the folder: the first write into a database with no session for
that folder must be done with the CLI's `--project-id`.
² The store does not record running sessions.

## Run

```text
cargo run -p mukade-tui                  # or target\release\mukade-tui.exe
cargo run -p mukade-tui -- --turns 20    # transcripts open on the last 20 user turns (default 50)
cargo run -p mukade-tui -- --turns 0     # the full history
cargo run -p mukade-tui -- --scan D:\Apps    # also list every installation below a folder
```

**Where it reads from:** the harnesses' own variables (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`,
`OPENCODE_DB`, `PI_CODING_AGENT_DIR`). A missing variable falls back to the harness's default store,
which is only read until you resume, hop or import.

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
| `mukade-tui --check` | harnesses and session counts, then exits |
| `mukade-tui --check --scan D:\Apps` | the same, scanned installations included |
| `mukade-tui --snapshot` | one 120×40 frame |
| `mukade-tui --snapshot-hop` | the same frame with the hop dialog open |

## Keys

| key | does |
|---|---|
| `↑`/`↓` or `j`/`k` | move in the focused list |
| `tab` / `→` · `shift+tab` / `←` | next / previous column (harness → folder → session) |
| `enter` | next column, or open the selected session's transcript |
| `esc` | close the transcript, or go back one column |
| **`h`** | **hop** the session to another harness or installation |
| **`e`** | **export** the session to a JSON document |
| **`i`** | **import** a JSON document as a new session |
| **`R`** | **resume** the session in its own harness |
| `t` | last-50-turns ↔ full history |
| `g` / `G` · page up/down | top / bottom · scroll the transcript |
| `o` | scan a folder for more installations |
| `r` | reload every store (resets the folder filter) |
| `q` | quit (not while a hop or import is running) |

The footer always shows these keys.

## Hop (`h`)

1. Press `h`.
2. Choose the destination with `↑`/`↓`, then `enter`. Every other row is offered: another harness,
   or another installation of the same one (an OpenCode session into a portable JanuraCode, say).
3. Choose compaction, then `enter`:
   - `none` — the whole history;
   - `portable` — the source's own last compaction summary plus what follows (needs a source that compacted);
   - `summary` — the source's own model writes a handoff summary (may take minutes, costs a model call).
4. Type an optional resume prompt (empty = write only), then `enter` to run.
5. Read the result (`j`/`k`, page keys), then `esc`.

The result shows the **new id**, its **location**, the **headless** and **interactive** resume
commands, the **manifest** path, **every loss**, and the destination's reply when you gave a prompt.
Manifests are saved under `mukade-data/manifests/`.

The destination's model and provider come from that installation's own configuration
(`mukade::dest::configured_model_in`): a portable JanuraCode's `data\config\opencode\opencode.json`,
a Codex home's `config.toml`; a standard OpenCode falls back to `OPENCODE_CONFIG` /
`XDG_CONFIG_HOME`. OpenCode refuses a write without them, and without a project id.

## Export (`e`)

1. Select a session.
2. Press `e`.
3. The result shows the file: `mukade-data/exports/<harness>-<id>.json`, and how many messages it holds.

The file is the **same document `mukade dump` prints**. You can import it here with `i`, or anywhere
with `mukade write <harness> <file>`, on this machine or another.

## Import (`i`)

1. Press `i`. The dialog has three fields; `tab` moves between them, `enter` goes to the next one.
2. **document** — the path of a `.json` document. It starts with the last file you exported.
3. **into** — the installation to write into (`↑`/`↓`). It starts on the harness you have selected.
4. **prompt** — optional; when given, the destination answers it once the session is written.
5. `enter` on the prompt runs the import. The result is shown like a hop's.

The import always makes a **new** session; nothing existing is changed. It uses the destination's
own executable, version, model and (for OpenCode) project, exactly like a hop.

## Resume (`R`)

`R` leaves the window, runs the harness's own interactive resume in the same terminal (with the
session's store and working directory), and comes back when you quit the harness. The commands come
from `catalog/harnesses.json` (`interactive_resume`):

| harness | command |
|---|---|
| Claude Code | `claude --resume <id>` |
| Codex | `codex resume <id>` |
| OpenCode | `opencode mini --standalone --session <id>` |
| Pi | `pi --session <file>` |

## Folders (directory filter)

The columns read **harness/installation → folder → sessions → details**.

- Folders come from each session's recorded working directory, with their session counts.
- **All directories** shows every session; **Unknown directory** those with no recorded folder.
- The filter never needs the folder to still exist on disk.
- A long path wraps; identical project names show a distinguishing parent path.
- Resume, hop and export act on the selected (filtered) session.

Layout: 120 columns or wider shows all four panels; narrower shows the three lists and opens the
transcript over them; below 70 columns only the focused panel is shown.

## Installations found by a scan (`o`, `--scan`)

Each installation found becomes one more row, `<harness> · <name>` (for example
`OpenCode · JanuraCode`), with its own store and the executable kept beside it. The scan is the
library's (`mukade::detect::scan_installations`, the same as `mukade scan`). A store already listed
is not added twice. A scan reads at most 8 levels and 100,000 folders: scan the folder that holds
the installations, not a whole user profile.

## Limits

- **Options kept in the CLI:** reasoning policy, `--without-tools`, `--interrupted drop`
  (the TUI always keeps interrupted work), a cwd override, `--all-branches`, destination ids,
  `--compaction native`. Use `mukade hop` / `mukade write` for those.
- **Names:** each row is named by its own store (OpenCode `session_v2.title`, Claude Code the rows
  its `/resume` picker reads, Codex `session_index.jsonl`, Pi its display name or first prompt). A
  session its store does not name is listed under its id.
- **`updated`:** Codex's comes from its state database, else from the rollout file's last write;
  copying a rollout without keeping its file time changes it.
- **Transcript:** user, assistant, reasoning, tool calls, full results and compactions in native
  order; bookkeeping (`Meta`) records are hidden.
- **Background work:** scans, reads, exports, imports and hops run on worker threads; a hop or an
  import cannot be cancelled once it starts.
