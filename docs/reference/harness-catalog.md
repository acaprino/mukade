# Harness catalog

`catalog/harnesses.json` (schema `mukade.harness-catalog/1`) is the one place that states, for each
harness mukade supports: how its binary is named and overridden, where that binary is installed by
default on Windows, macOS and Linux, which variable and which default name its store, the layout
its store makes, the lineage it records, and the resume command mukade returns.

It is embedded in the executable with `include_str!`, so a copied `mukade.exe` still needs no file
beside it. `mukade harnesses` prints the catalog's answers together with what it finds on the
machine it runs on.

## What an entry holds

| key | meaning |
|---|---|
| `id` | the command-line name: `claude_code`, `codex`, `opencode`, `pi` |
| `display` | the name a person reads |
| `binary.name` | the command looked up on `PATH` |
| `binary.flag` / `binary.env` | `--<harness>-exe` and `MUKADE_*_EXE`, the two overrides |
| `binary.locations` | the installers' own locations, per platform (a fallback after `PATH`) |
| `binary.beside_store` | where a self-contained installation keeps its executable, relative to the store's folder or one of its three nearest ancestors, per platform; `mukade scan` and `--dest-root` read it (a portable JanuraCode: `resources/opencode-cli/opencode.exe`) |
| `store.env` | the variable that points mukade at the store |
| `store.default` | the harness's own default store, expanded per platform |
| `store.root_is_a_file` | `true` for OpenCode: the store is the SQLite database itself |
| `store.layout` | the files that make a store recognizable, as patterns |
| `lineage.recorded` / `fields` / `note` | what the format records about forks, and what it does not |
| `resume` | the native resume argv; `{exe}`, `{id}` and `{file}` are filled in |

## Platform locations

A location is a string with tokens, expanded when it is used:

| token | value |
|---|---|
| `{home}` | `USERPROFILE` on Windows, `HOME` elsewhere |
| `{app_data}` | `APPDATA` (Windows only) |
| `{local_app_data}` | `LOCALAPPDATA` (Windows only) |
| `{xdg_data_home}` | `XDG_DATA_HOME`, else `{home}/.local/share` |

Each harness has three lists under `binary.locations`: `windows`, `macos` and `linux`. A list may be
empty when that platform has no fixed installer location and the binary is found on `PATH`. A
location whose token the platform does not provide is **skipped**: it is never turned into a path
with a literal `{token}` inside.

The canonical native-install locations currently catalogued are:

| harness | Windows | macOS / Linux |
|---|---|---|
| Claude Code | `{home}/.local/bin/claude.exe` | `{home}/.local/bin/claude` |
| Codex CLI | `{local_app_data}/Programs/OpenAI/Codex/bin/codex.exe` | `{home}/.local/bin/codex` |
| OpenCode | `{app_data}/npm/node_modules/@opencode/cli/bin/opencode.exe`, `{app_data}/npm/opencode.cmd`, `{home}/.opencode/bin/opencode.exe` | `{home}/.opencode/bin/opencode` |
| Pi | `{home}/.pi/agent/bin/pi.cmd`, then `{app_data}/npm/pi.cmd` or `pi` | `{home}/.pi/agent/bin/pi` |

Codex's Unix launcher links into `$CODEX_HOME/packages/standalone/current/`; Pi's managed launcher
selects a release under `<PI_CODING_AGENT_DIR>/install/releases/`. Package-manager installs (npm,
Homebrew, apt, dnf, apk, WinGet) and manual archive locations are discovered through `PATH` and the
package manager, not guessed by the catalog.

These locations are fallbacks. The resolution order is the one in the harness pages: the flag, then
`MUKADE_*_EXE`, then `PATH`, then the catalog location, then the bare name.

## The four entries

| harness | binary | store variable | default store | layout | lineage |
|---|---|---|---|---|---|
| Claude Code | `claude` | `CLAUDE_CONFIG_DIR` | `{home}/.claude` | `projects/<project>/<session-id>.jsonl` | not recorded (`--fork-session` writes none) |
| Codex CLI | `codex` | `CODEX_HOME` | `{home}/.codex` | `sessions/<yyyy>/<mm>/<dd>/rollout-<stamp>-<id>.jsonl`, `session_index.jsonl` | not recorded (subagent threads unlinked) |
| OpenCode | `opencode` | `OPENCODE_DB` | `{xdg_data_home}/opencode/opencode.db` | `opencode.db` (`session_v2`, `session_message`) | `parent_id`, `fork_session_id`, `fork_boundary` |
| Pi | `pi` | `PI_CODING_AGENT_DIR` | `{home}/.pi/agent` | `sessions/--<project>--/<stamp>_<id>.jsonl`, `models.json` | `parentSession` in the session header |

The default store is what the **harness** uses by itself. mukade's own commands still require the
explicit roots described in the [usage guide](../usage.md#3-use-cases); only `mukade harnesses`, the
read-only inventory, falls back to these defaults. An OpenCode build on a nonstandard channel may
use `opencode-<channel>.db`: select that file with `OPENCODE_DB`, since the catalogued default is
`opencode.db`.

## Finding existing installations

Start with the inventory:

```powershell
mukade harnesses
```

It reports **one resolved executable per harness**, its version, and the selected store: this is not
a full-disk scan or a list of every installed copy. Store reads are read-only; a discovered binary
may be asked for `--version`. To enumerate commands visible on `PATH`, including duplicates:

```powershell
# Windows PowerShell
Get-Command claude,codex,opencode,pi -All -ErrorAction SilentlyContinue |
  Select-Object Name,Source
```

```bash
# macOS / Linux (bash, zsh)
for h in claude codex opencode pi; do command -v "$h" || true; done
# In bash, `type -a claude` also lists every match for one name.
```

Check the selected binaries with `claude --version`, `codex --version`, `opencode --version` and
`pi --version`. Package-manager locations come from the manager itself:

```bash
npm prefix -g     # launchers are <prefix>/bin/<name> on macOS/Linux
npm root -g       # the global package directory
brew --prefix     # Homebrew launchers are normally <prefix>/bin/<name>
```

On Windows, npm launchers are normally `%APPDATA%\npm\<name>.cmd`; use the same `npm prefix -g` and
`npm root -g` commands for a customized prefix. OpenCode's active database can also be printed with
`opencode debug paths db`. WSL installations live in the WSL environment, not the native Windows one.

For a custom installation, mukade resolves an executable in this order: `--<harness>-exe`, the
matching `MUKADE_*_EXE`, `PATH`, the catalog locations, then the bare command name. Select its store
with `CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `OPENCODE_DB` or `PI_CODING_AGENT_DIR`; `--root` selects one
store per command where supported. `mukade detect <path>` reports which store layout a directory (or
OpenCode database file) holds, with evidence.

## How it is used

- `exe::resolve` — the well-known binary locations of `binary.locations` for the running platform.
- `mukade harnesses` — the store variable, default store, layout and lineage of every entry, with the
  session and fork counts read from the store (OpenCode's counts come from its database in one
  read-only query; Pi's forks from the session headers).
- The resume command in a receipt — `resume` with `{exe}` and `{id}`/`{file}` filled in.

## Changing it

Edit the JSON. Tests hold it to the code: every `HarnessId` must have exactly one entry, every entry
must have all three platform lists, every `store.env` must match the variable the CLI uses, and a
token the platform cannot expand must make the location unknown rather than a literal path.
