# Harness catalog

`catalog/harnesses.json` (schema `mukade.harness-catalog/1`, read by `src/catalog.rs`) is the one
place that states, per harness: binary name, override and installer locations; the store's variable,
default and layout; the configuration folder; the lineage it records; the resume commands mukade returns.

Find a harness: [Finding existing installations](#finding-existing-installations). Edit when an
installer or store moves: [Changing it](#changing-it). It is embedded with `include_str!`, so a
copied `mukade.exe` needs no file beside it.

## What an entry holds

Top level: `schema`, `description`, `expansions` (the tokens below, for readers; code in
`src/catalog.rs` `expand()`) and `harnesses`, one entry each.

| key | meaning |
|---|---|
| `id` | command-line name: `claude_code`, `codex`, `opencode`, `pi`, `hermes`, `deepseek_harness` |
| `display` | the name people read |
| `binary.name` | command looked up on `PATH` |
| `binary.flag` / `binary.env` | `--<harness>-exe` and `MUKADE_*_EXE`, the two overrides |
| `binary.locations` | installers' own locations per platform (fallback after `PATH`) |
| `binary.beside_store` | where a self-contained installation keeps its executable, relative to the store's folder or one of its three nearest ancestors, per platform; read by `mukade scan` and `--dest-root` (portable JanuraCode: `resources/opencode-cli/opencode.exe`) |
| `store.env` | variable that points mukade at the store |
| `store.default` | the harness's own default store, a string with tokens |
| `store.default_on` | a default for a platform whose convention differs (`windows`, `macos`, `linux`): Hermes on Windows |
| `store.root_is_a_file` | `true` for OpenCode: the store is the SQLite database |
| `store.layout` | patterns of the files that make a store recognizable |
| `config.env` / `config.default` / `config.default_on` | the folder the harness keeps its own settings, plugins and MCP servers in: the variable that moves it (absent when only a token's variable does), its default, a platform's own default. Absent for DeepSeek Harness, not established; `src/catalog.rs` `config_root()` reads it |
| `lineage.recorded` / `fields` / `note` | what the format records about forks, and what not |
| `resume` | native resume argv; `{exe}`, `{id}`, `{file}` filled in (`{file}` is the session file, or the id when none) |
| `interactive_resume` | argv that opens the session in the harness's terminal UI (the desktop app's resume); when empty, `resume` is used |
| `interactive_start` | argv that starts a new interactive session with a first prompt, with `{exe}` and `{prompt}` filled in (`src/catalog.rs` `interactive_start()`). Only Claude Code and Codex CLI have one, `["{exe}", "{prompt}"]`, from their own help text (`claude [prompt]` 2.1.330, `codex [PROMPT]` 0.167). Empty, or absent: the form has not been seen to work, and the harness is not offered as a handoff destination |

## Platform locations

A location is a string with tokens, expanded when it is used:

| token | value |
|---|---|
| `{home}` | `USERPROFILE` on Windows, `HOME` elsewhere (the other when unset) |
| `{app_data}` | `APPDATA` (Windows only) |
| `{local_app_data}` | `LOCALAPPDATA` (Windows only) |
| `{xdg_data_home}` | `XDG_DATA_HOME`, else `{home}/.local/share` |
| `{xdg_config_home}` | `XDG_CONFIG_HOME`, else `{home}/.config` |

- `binary.locations` has lists `windows`, `macos`, `linux`; an empty one means `PATH` only.
- A location whose token the platform lacks (or whose variable is empty) is **skipped**, never a
  path with a literal `{token}`. `/` becomes the platform's separator.

Native-install locations catalogued, in the order tried:

| harness | Windows | macOS / Linux |
|---|---|---|
| Claude Code | `{home}/.local/bin/claude.exe`, `{home}/.local/bin/claude` | `{home}/.local/bin/claude` |
| Codex CLI | `{local_app_data}/Programs/OpenAI/Codex/bin/codex.exe` | `{home}/.local/bin/codex` |
| OpenCode | `{app_data}/npm/node_modules/@opencode/cli/bin/opencode.exe`, `{app_data}/npm/opencode.cmd`, `{home}/.opencode/bin/opencode.exe` | `{home}/.opencode/bin/opencode` |
| Pi | `{home}/.pi/agent/bin/pi.cmd`, `{app_data}/npm/pi.cmd`, `{app_data}/npm/pi` | `{home}/.pi/agent/bin/pi` |
| Hermes Agent | `{local_app_data}/hermes/bin/hermes.exe`, `{local_app_data}/hermes/hermes-agent/.hermes/bin/hermes.exe` | `{home}/.local/bin/hermes` |
| DeepSeek Harness | `{app_data}/npm/dsh.cmd` | `PATH` only (an npm launcher) |

- Codex's Unix launcher links into `$CODEX_HOME/packages/standalone/current/`; Pi's managed
  launcher selects a release under `<PI_CODING_AGENT_DIR>/install/releases/` (details:
  [codex](../harnesses/codex.md), [pi](../harnesses/pi.md)).
- Package-manager installs (npm, Homebrew, apt, dnf, apk, WinGet) and manual archives are found
  through `PATH` and the package manager, not guessed.

These are fallbacks after `PATH`; order: [usage §7](../usage.md#7-stores-and-executables).

## The six entries

| harness | binary | store variable | default store | configuration folder | layout | lineage |
|---|---|---|---|---|---|---|
| Claude Code | `claude` | `CLAUDE_CONFIG_DIR` | `{home}/.claude` | the store's | `projects/<project>/<session-id>.jsonl` | `forkedFrom` on the rows a fork copies; a subagent's transcript names its parent in `sessionId` and meta.json |
| Codex CLI | `codex` | `CODEX_HOME` | `{home}/.codex` | the store's | `sessions/<yyyy>/<mm>/<dd>/rollout-<stamp>-<thread-id>.jsonl`, `session_index.jsonl` | `session_meta.forked_from_id` (a fork); a subagent thread names the thread that spawned it, `session_meta.parent_thread_id`) |
| OpenCode | `opencode` | `OPENCODE_DB` | `{xdg_data_home}/opencode/opencode.db` | `{xdg_config_home}/opencode` | `opencode.db (SQLite; session_v2, session_message)` | `parent_id`, `fork_session_id`, `fork_boundary` |
| Pi | `pi` | `PI_CODING_AGENT_DIR` | `{home}/.pi/agent` | the store's | `sessions/--<project>--/<stamp>_<session-id>.jsonl`, `models.json` | `parentSession` in the session header |
| Hermes Agent | `hermes` | `HERMES_HOME` | `{home}/.hermes`; Windows `{local_app_data}/hermes` | the store's | `state.db (SQLite; sessions, messages)` | `parent_session_id`, `model_config._delegate_from` (subagent), `model_config._branched_from` (`/branch`) |
| DeepSeek Harness | `dsh` | `DSH_HOME` | `{home}/.dsh` | not established | `sessions/--<project>--/<session-id>/session.v4.jsonl.zstd`, `sessions/_no-cwd/<session-id>/session.v4.jsonl.zstd` | `parentSession`, `origin`, `isSeeded` in the log's header |

Lineage, per reader: a Codex subagent's rollout names its spawning thread in `session_meta.parent_thread_id`
(else `session_id`), kept as `meta.parent` with `Spawn`; a fork names its origin in `forked_from_id`,
kept as `meta.parent` with `Fork { at }` (`src/reader/codex.rs`). A Claude Code fork names its origin in
`forkedFrom` on every row it copies. A Pi fork's `parentSession` is a file path; `parent` is the id in
that file's header.

**Resume commands** (`{exe}` is the resolved executable):

| harness | `resume` (in a receipt) | `interactive_resume` (desktop app) |
|---|---|---|
| Claude Code | `{exe} -p --resume {id}` | `{exe} --resume {id}` |
| Codex CLI | `{exe} exec resume {id}` | `{exe} resume {id}` |
| OpenCode | `{exe} run --standalone --session {id}` | `{exe} mini --standalone --session {id}` |
| Pi | `{exe} -p --session {file}` | `{exe} --session {file}` |
| Hermes Agent | `{exe} chat -Q --resume {id} -q` | `{exe} --resume {id}` |
| DeepSeek Harness | `{exe} --profile headless --session-id {id}` (a task follows) | none: dsh has no terminal UI, so the desktop app refuses (`Harness::resume_refusal`) |

**Default stores** are what the **harness** itself uses. Only `mukade harnesses` and the desktop app
fall back to them when the store variable is unset or empty; other commands use the explicit roots of
[usage §7](../usage.md#7-stores-and-executables).

- A nonstandard-channel OpenCode build may use `opencode-<channel>.db`: select it with `OPENCODE_DB`
  (default `opencode.db`).
- In the inventory a relative `OPENCODE_DB` resolves beneath `{xdg_data_home}/opencode/`, as in
  OpenCode; `:memory:` is non-persistent.

## Finding existing installations

1. **Run `mukade harnesses`** (lines and `status:` states: [usage](../usage.md#mukade-harnesses)):
   one resolved executable per harness, its version and store, read-only; no full-disk scan.
   - `binary` names its source: the flag, `MUKADE_*_EXE`, `PATH`, `catalog location`, or
     `not on PATH, no catalog location`.
   - The version comes from an npm launcher's `package.json` or a cache, else from `--version`
     (`src/exe.rs` `version_of()`).
   - `forks` is a count of the sessions that name a session they were forked from (a subagent is not a fork), `unknown` when the store cannot be read; `lineage` is
     `recorded: <fields>` or `not recorded (<note>)`.
2. **List every command on `PATH`** and run each `--version`: one-liners in
   [usage](../usage.md#mukade-harnesses); in bash `type -a claude` lists every match.
3. **Ask the package manager:**

   ```bash
   npm prefix -g     # launchers are <prefix>/bin/<name> on macOS/Linux
   npm root -g       # the global package directory
   brew --prefix     # Homebrew launchers are normally <prefix>/bin/<name>
   ```

   - Windows npm launchers: `%APPDATA%\npm\<name>.cmd` (`dsh.cmd`). Hermes's own launcher: `%LOCALAPPDATA%\hermes\bin\hermes.exe`. OpenCode's active database:
     `opencode debug paths db`. WSL installations live in the WSL environment, not the Windows one.
4. **Point mukade at a custom installation** with the flags and variables of
   [usage §7](../usage.md#7-stores-and-executables); `mukade detect <path>` reports which store
   layout a directory or OpenCode database holds, with evidence.

## How it is used

| reader of the catalog | keys |
|---|---|
| `src/exe.rs` `resolve()` | `binary.name`, `binary.locations` for the running platform (`src/catalog.rs` `binary_candidates()`) |
| `src/detect/engine.rs` `engine_beside_store()` (`mukade scan`, `--dest-root`) | `binary.beside_store` |
| `mukade harnesses` (`src/cli.rs` `print_harnesses()`) | `display`, `binary.flag`/`binary.env` (labels), `store.env`, `store.default`, `store.layout`, `config`, `lineage` |
| OpenCode's MCP target and global configuration file (`src/harnesses/opencode.rs` `chat_mcp_target()`, `global_config_files()`) | `config.default` |
| writers' receipts (`src/catalog.rs` `resume()`) | `resume` |
| `mukade handoff`, the desktop app's Start in… (`src/handoff.rs` `destinations()`, `src/catalog.rs` `interactive_start()`) | `interactive_start`: a harness is a handoff destination when it has one |
| the desktop app (`apps/desktop/src-tauri`) | `display`, `store.default`, `store.root_is_a_file`, `resume`, `interactive_resume` |

OpenCode's counts: one read-only query; Pi's forks: the session headers.

## Changing it

1. Edit `catalog/harnesses.json`.
2. The JSON does not change the code: flags `--<harness>-exe` are parsed in `src/cli.rs`,
   `MUKADE_*_EXE` names come from `src/exe.rs` `env_var_name()`, store variables from
   `src/roots.rs` `StoreRoots::from_env()`; `binary.flag` and `store.env` only label the inventory.
   The exception is `config`: `src/catalog.rs` `config_root()` reads the variable `config.env`
   names, so renaming it changes where the configuration folder is looked for.
3. Run the tests: a malformed file builds (embedded as text) but fails them and panics on first
   use. They check:
   - one entry per `HarnessId`, schema `mukade.harness-catalog/1`, non-empty `display` and
     `store.layout`; `binary.env` equals `env_var_name()`;
   - no empty Windows location, at most 3 macOS and 3 Linux (a missing list reads as empty); an
     unexpandable token makes the location unknown, never a literal path;
   - OpenCode's default store falls back to `{home}/.local/share/opencode/opencode.db`;
   - the configuration folder is the store's for Claude Code, Codex and Pi, Hermes's per platform,
     OpenCode's under `{xdg_config_home}`, none for DeepSeek Harness (`src/catalog.rs`
     `every_harness_states_its_configuration_folder`);
   - the exact `resume` argv of Claude Code, OpenCode and Pi, and Hermes's store default per
     platform (`src/catalog.rs`); `interactive_resume` of all six (`src/catalog.rs`
     `every_harness_has_the_exact_interactive_resume_argv`).
   - `interactive_start` is `["{exe}", "{prompt}"]` for Claude Code and Codex CLI and empty for the rest
     (`src/catalog.rs` `claude_code_and_codex_start_with_a_prompt_and_no_other_harness_does`).
