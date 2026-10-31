# mukade-tui

A small native terminal browser for Mukade sessions: the four harnesses on the left, their
sessions in the middle, the selected session's transcript on the right. It can also resume a
session in its own harness and hop it to another harness.

It is a separate crate: the `mukade` CLI and its dependency list do not change. The TUI reads and
writes through the library (`reader::list_all`, `export`, `hop`) and never parses a harness store
itself.

## Run

```text
cargo run -p mukade-tui
```

The transcript opens on the last **50 user turns**; `--turns N` changes it (`--turns 0` loads all).
The flag also accepts `--turns=N`; a missing or invalid value falls back to 50.

Store roots come from the harnesses' own variables (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`,
`OPENCODE_DB`, `PI_CODING_AGENT_DIR`). A missing variable falls back to the catalog's default
store for that harness, read-only until an explicit resume or hop.

## Commands

| command | what it does |
|---|---|
| `cargo run -p mukade-tui` | open the explorer over the stores the environment names |
| `cargo run -p mukade-tui -- --turns 20` | open transcripts on the last 20 user turns |
| `cargo run -p mukade-tui -- --turns 0` | open transcripts on the full history |
| `cargo run -p mukade-tui -- --check` | print harnesses and session counts, then exit |
| `cargo run -p mukade-tui -- --scan D:\Apps` | also list every installation below a folder |
| `cargo run -p mukade-tui -- --snapshot` | render one 120×40 frame on stdout |
| `cargo run -p mukade-tui -- --snapshot-hop` | render the same frame with the hop dialog open |
| `target\release\mukade-tui.exe` | the release binary on Windows |

## Examples

Start on the real stores:

```powershell
cargo run -p mukade-tui
```

Limit the transcript to 20 turns, or show everything:

```powershell
cargo run -p mukade-tui -- --turns 20
cargo run -p mukade-tui -- --turns 0
```

Point the explorer at explicit stores (PowerShell; a missing variable uses the catalog default):

```powershell
$env:CLAUDE_CONFIG_DIR = "$env:USERPROFILE\.claude"
$env:CODEX_HOME = "$env:USERPROFILE\.codex"
$env:PI_CODING_AGENT_DIR = "$env:USERPROFILE\.pi\agent"
$env:OPENCODE_DB = "$env:USERPROFILE\.local\share\opencode\opencode.db"
.\target\release\mukade-tui.exe
```

Add the installations below a folder (portable and custom ones, a JanuraCode copy, say), at start
or from inside the explorer with `o`:

```powershell
.\target\release\mukade-tui.exe --scan D:\Apps
.\target\release\mukade-tui.exe --scan D:\Apps --scan E:\
```

Each installation becomes one more row, `<harness> · <name>` (for example
`OpenCode · JanuraCode`), with its own store and the executable kept beside it. The scan is the
library's (`mukade::detect::scan_installations`, the same as `mukade scan`): the explorer recognizes
nothing itself. A store already listed is not added twice.

Headless checks (useful in a script or CI):

```powershell
.\target\release\mukade-tui.exe --check
.\target\release\mukade-tui.exe --check --scan D:\Apps
.\target\release\mukade-tui.exe --snapshot
.\target\release\mukade-tui.exe --snapshot-hop
```

Resume the selected session:

1. select a session with `j`/`k`;
2. press `R`;
3. the harness opens in the same terminal with the session's store and cwd;
4. quit the harness to return to the explorer.

Hop the selected session:

1. press `h`;
2. choose the destination (`↑`/`↓`), then `enter`;
3. choose compaction (`none`, `portable`, `summary`), then `enter`;
4. type an optional resume prompt, then `enter` to run;
5. read the scrollable result (`j`/`k`, page keys), then `esc`.

`t` switches between the configured history limit and the full history without re-reading the
session.

## Keys

| key | action |
|---|---|
| `j` / `k`, arrows | move in the focused list |
| `tab` | switch focus between harnesses and sessions |
| `enter` | open the selected session's transcript |
| `t` | toggle last-50-turns / full history |
| `h` | hop the selected session to another harness or installation |
| `R` | resume the selected session in its own installation |
| `o` | scan a folder for more installations and add them as rows |
| `g` / `G` | top / bottom of the transcript |
| page up/down | scroll the transcript |
| `esc` | close the transcript, then return to the harness list |
| `r` | rescan every store |
| `q` | quit |

## Resume

`R` leaves the TUI, runs the harness's own interactive resume command in the same terminal with
the session's store variable and working directory, then returns to the TUI when the harness exits.
The commands come from `catalog/harnesses.json` (`interactive_resume`), not from a second table:

- Claude Code: `claude --resume <id>`
- Codex: `codex resume <id>`
- OpenCode: `opencode mini --standalone --session <id>`
- Pi: `pi --session <file>`

## Hop

`h` opens a small dialog: destination (every other row: another harness, or another installation
of the same harness, such as an OpenCode session into a portable JanuraCode), compaction (`none`, `portable`, `summary`) and an
optional resume prompt. `portable` needs a source that compacted at least once; `summary` asks the
source's own model and may take minutes. `enter` on the prompt runs the hop in the background.
The result is scrollable (`j/k`, page keys) and shows the new id, its location, the headless and
interactive resume commands, the manifest path, every loss, and the destination's reply when a
prompt was given. The manifest is saved under `mukade-data/manifests/`.

The source is read from its row's store and the destination written to its own
(`mukade::hop_into`), with the destination installation's executable, which also appears in the
resume commands. A destination's model and provider are read from the configuration of the shell
that started the explorer (`OPENCODE_CONFIG` / `XDG_CONFIG_HOME` for OpenCode,
`CODEX_HOME/config.toml` for Codex), not from a scanned installation's own; OpenCode refuses a hop
without them, and without a project id, which a destination whose database has no session for the
source's folder yet cannot supply: use the CLI's `--model-provider`, `--model` and `--project-id`
for that first hop.

## Limits

- No export/import dialog yet.
- Hop options are limited to destination, compaction and resume prompt; reasoning policy,
  historical tools, cwd override, `all-branches` and advanced destination ids stay in the CLI.
- Codex and Pi list only ids and paths from the current `Reader::list_all`; titles and lineage
  need the library changes described in
  `../docs/superpowers/specs/2026-10-30-mukade-ui-design.md`.
- The transcript shows user, assistant, reasoning, tool calls, full results and compactions in
  native order; `Meta` records are hidden.
- Scans, session reads and hops run on worker threads; a hop cannot be cancelled once it starts.
- A folder scan reads at most 8 levels and 100,000 folders; a whole user profile can take tens of
  seconds and stop early, so scan the folder that holds the installations.
