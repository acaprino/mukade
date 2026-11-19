# mukade-tui — harness sessions manager (terminal UI)

One terminal window over the sessions of **Claude Code, Codex, OpenCode, Pi, Hermes Agent and DeepSeek Harness** and any installation a scan finds: browse, read, resume, hop, export, import.

A separate crate: the CLI's dependencies do not change. It goes only through the library (`Reader::list_all`, `export`, `hop_into`, `import_document`), never parsing a store itself.

> **In 20 seconds**
>
> 1. `mukade-tui` (or `cargo run -p mukade-tui`)
> 2. `↑`/`↓` pick a harness · `tab` → pick a folder · `tab` → pick a session · `enter` read it
> 3. **`h`** hop · **`e`** export · **`i`** import · **`R`** resume · **`q`** quit

## What it can do, per harness

| | Claude Code | Codex | OpenCode | Pi | Hermes | DeepSeek |
|---|:-:|:-:|:-:|:-:|:-:|:-:|
| list sessions, filter by folder | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| read the transcript | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| resume (`R`) | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ ³ |
| hop to / from (`h`) | ✅ | ✅ | ✅ | ✅ | ✅ ⁴ | ✅ |
| export (`e`) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| import into (`i`) | ✅ | ✅ | ✅ ¹ | ✅ | ✅ ⁴ | ✅ |
| `updated` time | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `live` (running now) | ✅ | — ² | — ² | — ² | — ² | — ² |

¹ OpenCode needs a project id for the folder. The TUI asks the destination OpenCode for a session already filed under that folder; with none, the first write there needs the CLI's `--project-id`.
² The store does not record running sessions.
³ DeepSeek Harness has no terminal UI: `R` shows the headless command that continues the session (`dsh --profile headless --session-id <id> "<task>"`), and a hop's result lists `interactive  none: …`.
⁴ A write into Hermes goes through `hermes sessions import` and takes about a minute, in the background with the busy indicator. Tool calls arrive as text.

## Run

```text
cargo run -p mukade-tui                  # or target\release\mukade-tui.exe
cargo run -p mukade-tui -- --turns 20    # last 20 user turns per transcript (default 50; 0 = all)
cargo run -p mukade-tui -- --scan D:\Apps    # also list every installation below a folder
```

`--turns=N` and `--scan=<dir>` work too; `--scan` may repeat.

**Where it reads from:** each harness's own variable (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `OPENCODE_DB`, `PI_CODING_AGENT_DIR`, `HERMES_HOME`, `DSH_HOME`; [usage §7](../docs/usage.md#7-stores-and-executables)); unset, the harness's default store (`catalog/harnesses.json`). Browsing only reads; resume, hop and import write.

Headless (scripts, CI):

| command | prints |
|---|---|
| `mukade-tui --check` | each row's session count (or its error), a scanned row's store and engine; then exits |
| `mukade-tui --check --scan D:\Apps` | the same, with scanned installations |
| `mukade-tui --snapshot` | one 120×40 frame |
| `mukade-tui --snapshot-hop` | the same frame with the hop dialog open |

## Screen

Columns: **harness/installation → folder → sessions → details**. From 120 columns all four show; narrower, the three lists with the transcript opening over them; below 70, only the focused panel.

| mark | means |
|---|---|
| `↳ agent <type>` | a subagent session, indented under its parent (`<type>` is the agent type, when the source records it) |
| `⑂ fork` | a fork of its parent, indented under it |
| `↳` | linked to a parent by a relation the source did not name |
| `[live]` | running now (Claude Code only) |
| `[archived]` | the harness marked it archived |
| `[unreadable]` | listed, not readable: open, hop, export and resume refuse it and say why |
| time, or `—` | `updated`, or none recorded |

**Children and forks nest.** The list is a tree: each subagent session and fork sits under its parent, two cells in per level, in the order the store lists them. A child whose parent is not in the list (another folder filter, or a parent the store no longer holds) stays at the top with its mark, so every session stays reachable. Within a folder filter a child nests under its parent only when both are in that filter. There is no collapsing yet.

A harness row marked `!` could not be listed; its details show the error. **Details:** `harness`, `id`, `parent` (with `(not listed)` when the parent is not in the list), `kind` (`subagent · <type>`, with `started from the parent's conversation` where it did; `fork`, with `at <entry>` where the source records it), `children` (`3 (2 subagents, 1 fork)`), `cwd`, `updated`, `archived`, `live`, `unreadable`, `path`; a scanned installation adds `store` and `engine`.

## Keys

**Move**

| key | does |
|---|---|
| `↑`/`↓` or `j`/`k` | move in the focused list, in the order shown (a parent, then its children); scroll, in the transcript |
| `tab` / `→` · `shift+tab` / `←` | next / previous column |
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
| `g` / `G` · page up/down | transcript top / bottom · scroll 10 lines |
| `o` | scan a folder for more installations (type it, `enter`) |
| `r` | reload every store (resets the folder filter; scanned rows stay) |
| `q` | quit (not while a hop or import runs) |

The footer shows every key but `g`/`G` and the page keys. A result box scrolls with those and `j`/`k`; `esc`, `enter` or `q` close it.

## Hop (`h`)

1. Press `h`.
2. **Destination:** `↑`/`↓`, `enter`. Every other row: another harness, or another installation of the same one.
3. **Compaction:** `↑`/`↓`, `enter`:
   - `none`: the whole history;
   - `portable`: the source's last compaction summary plus what follows (the source must have compacted);
   - `summary`: the source's own model writes a handoff summary, no turn kept verbatim (may take minutes, costs a model call).
4. **Subagents:** shown only when the source has subagent sessions in the list. `↑`/`↓` or `space` toggle `leave behind` (the default) and `carry`. Carrying is the CLI's `hop --children` ([usage](../docs/usage.md#carry-the-subagent-sessions)): each readable subagent session, at any depth, is written as a session of its own after the session, linked to its parent where the destination has a link. Forks are not subagents and are never carried.
5. **Branches:** shown only when the library reads every branch of the source's harness (Claude Code, Codex, Pi today; the TUI asks `mukade::hop::reads_all_branches`). `↑`/`↓` or `space` toggle `the resumed one` (the default) and `every branch`. Every branch is the CLI's `hop --all-branches` ([usage](../docs/usage.md#a-session-that-was-rewound-or-branched)): abandoned branches travel too, kept where the destination can keep them and counted where it cannot. Not with `summary`, which stands for the resumed branch (the hop is refused).
6. **Prompt:** an optional resume prompt (empty = write only), then `enter` to run.
7. Read the result (`j`/`k`, page keys), then `esc`.

`tab` / `shift+tab` move between the fields, `esc` cancels. The session keeps its (shown) working directory.
The result shows the **new id**, **location**, **headless** and **interactive** resume commands,
**manifest** path (`mukade-data/manifests/<harness>-<id>.json`), **every loss**, how many branches were kept and left out (when the source carried any), one line per carried subagent session (`<source id> -> <new id>`, and whether the destination linked it to its parent), and the destination's reply to a prompt. If a subagent session cannot be written, the result says so and shows the receipt of the session itself, which is written.

**Model and provider** come from the destination installation's own configuration
(`src/dest.rs` `configured_model_in()`):

| destination | read from |
|---|---|
| OpenCode at `<root>\share\opencode\opencode.db` (a portable JanuraCode) | `<root>\config\opencode\opencode.json` (JanuraCode: `data\config\opencode\opencode.json`) |
| any other OpenCode | `OPENCODE_CONFIG`, else `XDG_CONFIG_HOME` |
| Codex home | `config.toml` (the provider) |
| Claude Code, Pi | none read |

OpenCode refuses a write lacking a model, provider or project id; model and provider fall back to the session's recorded model.

## Export (`e`)

1. Select a session, press `e`.
2. The result shows `mukade-data/exports/<harness>-<id>.json` and its message count.

- The file is the **document `mukade dump` prints**: import it with `i`, or anywhere with `mukade write <harness> <file>`.
- Exporting a session again overwrites the file. `mukade-data` sits beside the executable, or at an absolute `MUKADE_DATA_DIR`.

## Import (`i`)

1. Press `i`: three fields; `tab` / `shift+tab` move, `enter` advances.
2. **document**: a `.json` path; starts with your last export.
3. **into**: the installation to write into (`↑`/`↓`); starts on the selected harness.
4. **prompt**: optional; the destination answers it once the session is written.
5. `enter` on the prompt runs it; the result is shown like a hop's, manifest included.

It always makes a **new** session, with the destination's own executable, version, model and (OpenCode)
project, as a hop does, in the document's working directory. `esc` cancels.

## Resume (`R`)

`R` leaves the window and runs the harness's interactive resume in the same terminal (the session's store, executable and working directory), and returns when you quit it. The commands are
`interactive_resume` in `catalog/harnesses.json` ([the six entries](../docs/reference/harness-catalog.md#the-six-entries); OpenCode: `mini --standalone`; DeepSeek Harness has none: no terminal UI). A scanned installation runs its own executable, not the bare name.

## Folders (directory filter)

- Folders are each session's recorded working directory, with counts.
- **All directories** shows every session; **Unknown directory** those with no recorded folder.
- The folder need not exist on disk. A long path wraps; identical project names show a parent path. Resume, hop and export act on the filtered selection.

## Installations found by a scan (`o`, `--scan`)

- Each installation becomes a row `<harness> · <name>` (`OpenCode · JanuraCode`) with its own store and the executable kept beside it; `<name>` is the folder holding both (a portable app's own), else the store's.
- The scan is the library's (`src/detect/scan.rs` `scan_installations()`, as `mukade scan`); a listed store is not added twice.
- It reads at most 8 levels and 100,000 folders; the status line says when it stopped early. Scan the folder that holds the installations, not a user profile.

## Limits

**Kept in the CLI** (`mukade hop` / `mukade write`, [usage §3](../docs/usage.md#3-commands)):

| kind | flags |
|---|---|
| content | `--reasoning`, `--without-tools`, `--interrupted drop` (the TUI always keeps interrupted work), `--all-branches` |
| compaction | `--compaction native`, `--keep-turns`, `--intent`, `--summary-model`, `--summary-timeout` |
| destination | `--cwd`, `--dest-shell`, `--dest-id` / `--dest-path`, `--model` / `--model-provider` / `--project-id`, `--dest-agent`, `--metadata-file`, `--tool-map` |
| safety | `--expect-sha256` |

A TUI hop takes the destination's shell and offered set from its own configuration, as a CLI hop
without `--dest-shell` does ([tables §4.1](../docs/tool-conversion-tables.md#41-shell)).

- **Names** come from each store (OpenCode `session_v2.title`, Claude Code the rows its `/resume`
  picker reads, Codex `session_index.jsonl`, Pi its display name or first prompt); an unnamed session shows its id.
- **`updated`:** Codex's comes from its state database, else the rollout file's last write (copying a rollout without its file time changes it); Pi's, its newest record.
- **Transcript:** user, assistant, reasoning, tool calls, full results and compactions in native
  order; a tool call's input is cut at 240 characters; bookkeeping (`Meta`) records are hidden.
- **Background work:** scans, reads, exports, imports and hops run on worker threads; a hop or import cannot be cancelled.
