# Harness catalog

`catalog/harnesses.json` (schema `mukade.harness-catalog/1`, read by `src/catalog.rs`) is the one
place that states, for each harness mukade supports:

- how its binary is named and overridden, and where installers put it on Windows, macOS and Linux;
- which variable and which default name its store, and the layout the store makes;
- the lineage it records;
- the resume commands mukade returns.

Use it to find where a harness lives ([Finding existing installations](#finding-existing-installations))
and edit it when an installer or a store moves ([Changing it](#changing-it)). It is embedded in the
executable with `include_str!`, so a copied `mukade.exe` still needs no file beside it.
`mukade harnesses` prints the catalog's answers together with what it finds on the machine it runs on.

## What an entry holds

Top level: `schema`, `description`, `expansions` (the tokens below, for a person reading the file;
the code expands them itself in `src/catalog.rs` `expand()`) and `harnesses`, one entry per harness.

| key | meaning |
|---|---|
| `id` | the command-line name: `claude_code`, `codex`, `opencode`, `pi` |
| `display` | the name a person reads |
| `binary.name` | the command looked up on `PATH` |
| `binary.flag` / `binary.env` | `--<harness>-exe` and `MUKADE_*_EXE`, the two overrides |
| `binary.locations` | the installers' own locations, per platform (a fallback after `PATH`) |
| `binary.beside_store` | where a self-contained installation keeps its executable, relative to the store's folder or one of its three nearest ancestors, per platform; `mukade scan` and `--dest-root` read it (a portable JanuraCode: `resources/opencode-cli/opencode.exe`) |
| `store.env` | the variable that points mukade at the store |
| `store.default` | the harness's own default store, one string expanded with the tokens |
| `store.root_is_a_file` | `true` for OpenCode: the store is the SQLite database itself |
| `store.layout` | the files that make a store recognizable, as patterns |
| `lineage.recorded` / `fields` / `note` | what the format records about forks, and what it does not |
| `resume` | the native resume argv; `{exe}`, `{id}` and `{file}` are filled in (`{file}` is the session file, or the id when there is none) |
| `interactive_resume` | the argv that opens the session in the harness's own terminal UI (`mukade-tui`); when empty, `resume` is used |

## Platform locations

A location is a string with tokens, expanded when it is used:

| token | value |
|---|---|
| `{home}` | `USERPROFILE` on Windows, `HOME` elsewhere (the other one when that is unset) |
| `{app_data}` | `APPDATA` (Windows only) |
| `{local_app_data}` | `LOCALAPPDATA` (Windows only) |
| `{xdg_data_home}` | `XDG_DATA_HOME`, else `{home}/.local/share` |

- Each harness has three lists under `binary.locations`: `windows`, `macos` and `linux`.
- A list may be empty when that platform has no fixed installer location; the binary is then found
  on `PATH`.
- A location whose token the platform does not provide (or whose variable is empty) is
  **skipped**: it is never turned into a path with a literal `{token}` inside.
- `/` becomes the platform's separator.

The canonical native-install locations currently catalogued, in the order they are tried:

| harness | Windows | macOS / Linux |
|---|---|---|
| Claude Code | `{home}/.local/bin/claude.exe`, `{home}/.local/bin/claude` | `{home}/.local/bin/claude` |
| Codex CLI | `{local_app_data}/Programs/OpenAI/Codex/bin/codex.exe` | `{home}/.local/bin/codex` |
| OpenCode | `{app_data}/npm/node_modules/@opencode/cli/bin/opencode.exe`, `{app_data}/npm/opencode.cmd`, `{home}/.opencode/bin/opencode.exe` | `{home}/.opencode/bin/opencode` |
| Pi | `{home}/.pi/agent/bin/pi.cmd`, `{app_data}/npm/pi.cmd`, `{app_data}/npm/pi` | `{home}/.pi/agent/bin/pi` |

- Codex's Unix launcher links into `$CODEX_HOME/packages/standalone/current/`.
- Pi's managed launcher selects a release under `<PI_CODING_AGENT_DIR>/install/releases/`.
- Package-manager installs (npm, Homebrew, apt, dnf, apk, WinGet) and manual archive locations are
  discovered through `PATH` and the package manager, not guessed by the catalog.

These locations are fallbacks. The resolution order is the one in the harness pages: the flag,
then `MUKADE_*_EXE`, then `PATH`, then the catalog location, then the bare name.

## The four entries

| harness | binary | store variable | default store | layout | lineage |
|---|---|---|---|---|---|
| Claude Code | `claude` | `CLAUDE_CONFIG_DIR` | `{home}/.claude` | `projects/<project>/<session-id>.jsonl` | not recorded (`claude --fork-session` writes no lineage field, so a fork is a session with no parent) |
| Codex CLI | `codex` | `CODEX_HOME` | `{home}/.codex` | `sessions/<yyyy>/<mm>/<dd>/rollout-<stamp>-<thread-id>.jsonl`, `session_index.jsonl` | not recorded (no cross-session parent; subagent threads not linked) |
| OpenCode | `opencode` | `OPENCODE_DB` | `{xdg_data_home}/opencode/opencode.db` | `opencode.db (SQLite; session_v2, session_message)` | `parent_id`, `fork_session_id`, `fork_boundary` |
| Pi | `pi` | `PI_CODING_AGENT_DIR` | `{home}/.pi/agent` | `sessions/--<project>--/<stamp>_<session-id>.jsonl`, `models.json` | `parentSession` in the session header |

Codex lineage: the catalog's note predates the reader. A Codex subagent's rollout names the thread
that spawned it in `session_meta.session_id`, and the reader now keeps it as `meta.parent`
(`src/reader/codex.rs` `Builder::finish()`). Forks are still not recorded.

**Resume commands** (`{exe}` is the resolved executable):

| harness | `resume` (in a receipt) | `interactive_resume` (`mukade-tui`) |
|---|---|---|
| Claude Code | `{exe} -p --resume {id}` | `{exe} --resume {id}` |
| Codex CLI | `{exe} exec resume {id}` | `{exe} resume {id}` |
| OpenCode | `{exe} run --standalone --session {id}` | `{exe} mini --standalone --session {id}` |
| Pi | `{exe} -p --session {file}` | `{exe} --session {file}` |

**Default stores.** The default store is what the **harness** uses by itself.

- mukade's other commands use the explicit roots described in the
  [usage guide](../usage.md#7-stores-and-executables).
- Only two fall back to the defaults when the store variable is unset or empty: `mukade harnesses` (the
  read-only inventory) and `mukade-tui` (its standard locations).
- An OpenCode build on a nonstandard channel may use `opencode-<channel>.db`: select that file with
  `OPENCODE_DB`, since the catalogued default is `opencode.db`.
- In the inventory, a relative `OPENCODE_DB` is resolved beneath OpenCode's data directory
  (`{xdg_data_home}/opencode/`), as OpenCode does; `:memory:` is reported as non-persistent.

## Finding existing installations

1. **Run the inventory.**

   ```powershell
   mukade harnesses
   ```

   It reports **one resolved executable per harness**, its version and the selected store: not a
   full-disk scan or a list of every installed copy. Store reads are read-only. The version comes
   from an npm launcher's `package.json` or a cache when possible, else the binary is asked for
   `--version` (`src/exe.rs` `version_of()`). Output per harness:

   | line | shows |
   |---|---|
   | header | `<id> (<display>) - configured` / `installed` / `not found` |
   | `binary` | the path, and where it came from: the flag, the `MUKADE_*_EXE` variable, `PATH`, `catalog location`, or `not on PATH, no catalog location` |
   | `version` | the version, or `unknown` |
   | `store`, `status` | the path, its variable, `environment` or `default`; then `recognized`, `not found`, `present, layout not recognized`, `non-persistent (:memory:)` or a read error |
   | `sessions`, `forks` | counts read from the store; Claude Code and Codex forks: `unknown (not recorded)` |
   | `lineage`, `layout` | `recorded: <fields>` or `not recorded (<note>)`; the layout patterns |

2. **List every command visible on `PATH`**, duplicates included:

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

3. **Check the selected binaries:** `claude --version`, `codex --version`, `opencode --version`,
   `pi --version`.
4. **Ask the package manager** for its locations:

   ```bash
   npm prefix -g     # launchers are <prefix>/bin/<name> on macOS/Linux
   npm root -g       # the global package directory
   brew --prefix     # Homebrew launchers are normally <prefix>/bin/<name>
   ```

   - On Windows, npm launchers are normally `%APPDATA%\npm\<name>.cmd`; use the same
     `npm prefix -g` and `npm root -g` for a customized prefix.
   - OpenCode's active database: `opencode debug paths db`.
   - WSL installations live in the WSL environment, not the native Windows one.
5. **Point mukade at a custom installation.**
   - Executable, in this order: `--<harness>-exe`, the matching `MUKADE_*_EXE`, `PATH`, the
     catalog locations, then the bare command name.
   - Store: `CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `OPENCODE_DB` or `PI_CODING_AGENT_DIR`; `--root`
     selects one store per command where supported.
   - `mukade detect <path>` reports which store layout a directory (or OpenCode database file)
     holds, with evidence.

## How it is used

| reader of the catalog | keys |
|---|---|
| `src/exe.rs` `resolve()` | `binary.name` (the `PATH` lookup), `binary.locations` for the running platform (`src/catalog.rs` `binary_candidates()`) |
| `src/detect/engine.rs` `engine_beside_store()` (`mukade scan`, `--dest-root`) | `binary.beside_store` |
| `mukade harnesses` (`src/cli.rs` `print_harnesses()`) | `display`, `binary.flag` and `binary.env` (as labels), `store.env`, `store.default`, `store.layout`, `lineage` |
| the writers' receipts (`src/catalog.rs` `resume()`) | `resume` |
| `mukade-tui` | `display`, `store.default`, `store.root_is_a_file`, `interactive_resume` |

OpenCode's counts come from its database in one read-only query; Pi's forks from the session
headers.

## Changing it

1. Edit `catalog/harnesses.json`.
2. Know what the JSON does not change: the CLI parses the `--<harness>-exe` flags in `src/cli.rs`
   and the `MUKADE_*_EXE` names come from `src/exe.rs` `env_var_name()`; the store variables the
   commands read are named in `src/roots.rs` `StoreRoots::from_env()`. `binary.flag` and
   `store.env` only label and feed the inventory.
3. Run the tests. A malformed file still builds (it is embedded as text) but fails them, and
   panics on first use. They hold:
   - one entry per `HarnessId`, the schema `mukade.harness-catalog/1`, a non-empty `display` and
     `store.layout`;
   - `binary.env` equal to `src/exe.rs` `env_var_name()`;
   - no empty Windows location, at most 3 macOS and 3 Linux locations (a missing list reads as
     empty);
   - a token the platform cannot expand makes the location unknown, never a literal path;
   - OpenCode's default store falls back to `{home}/.local/share/opencode/opencode.db`;
   - the exact `resume` argv of Claude Code, OpenCode and Pi (`src/catalog.rs` tests), and the
     exact `interactive_resume` argv of all four (`mukade-tui/src/main.rs`
     `every_harness_has_the_exact_interactive_resume_argv`).
