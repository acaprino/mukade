# Destination shell from the destination's own configuration — design

Status: decided 2026-11-12 (the user delegated design calls). Open item `next.md` "Destination
shell (L11/L13)". Research record: facts below are [V] in each harness's code unless marked.

## Problem

A shell call keeps its source dialect (L13) and becomes a native call only when the destination
runs that dialect. Without `--dest-shell` mukade knows no destination dialect, so every shell call
toward Codex, OpenCode and Pi is text form (a Codex Desktop session toward Codex: 0 native calls
instead of 696). Claude's `PowerShell` tool is never in its offered set, so a PowerShell call
toward Claude is text form even where Claude runs one. The TUI passes no `--dest-shell` at all.

## Decision

1. **Precedence:** `--dest-shell <s>` > the destination's configuration > the destination's
   platform default. `--dest-shell none` keeps today's behaviour (no dialect: shell calls text form).
2. **Who resolves it:** the edges (CLI `write_opts`, TUI `write_details_for`), the same way they
   already resolve `configured_model_in`. The library's `plan()` stays a pure function of
   `WriteOpts`; library callers get two public functions to do the same.
3. **Where the answer lives:** two `Destination` methods, defaulted:
   - `fn default_shell(&self, roots: &StoreRoots, cwd: Option<&Path>) -> Option<Shell>` (default `None`);
   - `fn offered_in(&self, roots: &StoreRoots, cwd: Option<&Path>) -> ToolSet` (default `self.offered()`).
   Public wrappers: `dest::shell_in(harness, roots, cwd)` and `tools::offered_in(harness, roots, cwd)`
   (non-destination → `None` / empty set, like the other wrappers).

## Per destination

| destination | `default_shell` | `offered_in` |
|---|---|---|
| Claude Code | `None` (its `Bash` is always POSIX and `PowerShell` is a separate tool; the adapter picks the tool by the call's dialect) | the pinned list, plus `PowerShell` when enabled (below) |
| Codex | Windows → `PowerShell`; else `Posix` (Codex runs pwsh/powershell on Windows, the login shell zsh/bash/sh elsewhere; no config key) | pinned list |
| OpenCode | config `shell` → `$SHELL` → Windows `PowerShell`, else `None` | pinned list |
| Pi | settings `shellPath` mapped by name; unset → `Posix` | pinned list |

**Shell name → dialect** (one shared helper): base name, without `.exe`, lower case.
`bash`, `sh`, `zsh`, `dash`, `ksh` → `Posix`; `pwsh`, `powershell` → `PowerShell`; anything else
(`cmd`, `fish`, `nu`, unknown) → no answer from that source (OpenCode skips `fish`/`nu` in its config
and goes on to `$SHELL`; mukade does the same for both).

**Claude `PowerShell` enabled**, first answer wins:
1. process env `CLAUDE_CODE_USE_POWERSHELL_TOOL` = `1` → on, `0` → off (empty = unset);
2. the same key under `env` in settings, nearest first: `<cwd>/.claude/settings.local.json`,
   `<cwd>/.claude/settings.json`, `<config>/settings.json`;
3. evidence of this installation: the newest transcript under `<config>/projects/*/` (top level,
   by modification time) whose environment snapshot
   (`{"type":"attachment","attachment":{"type":"environment","snapshot":{"platform":…,"shell":…}}}`)
   names this machine's `platform` (`win32`, `darwin`, `linux`): on when that `shell` starts with
   `PowerShell`. Each file is read in its last 1 MiB and its first 1 MiB: the tail's last snapshot
   wins, else the head's last (a long session records its snapshot near its start). A file without
   one (a session mukade wrote, a snapshot of another platform) is no answer: the next newest is
   examined, at most 20 files and 40 MiB read in all, the 20 picked without sorting every
   transcript. Amended 2026-11-12 after review: the first rule (last 1 MiB, 5 files) missed the
   snapshot of a long session and stopped after a few hops into Claude Code;
4. no evidence → off (today's behaviour).
Not modelled [U]: a `Bash` deny rule turning PowerShell off; the cached account flag in
`.claude.json`. The transcript evidence reflects both.

**OpenCode config `shell`**, first file that sets it wins: `OPENCODE_CONFIG_CONTENT` (JSON in the
environment) → project files walking up from `cwd`: in each folder `.opencode/opencode.jsonc`,
`.opencode/opencode.json`, `opencode.jsonc`, `opencode.json` → `OPENCODE_CONFIG` → the global file
(`opencode_config_beside` the database, else `$XDG_CONFIG_HOME/opencode/`, else
`~/.config/opencode/`, `.jsonc` then `.json`). `.jsonc` comments and trailing commas are tolerated.
Not modelled [U]: managed config (`%ProgramData%\opencode`).

**Pi `shellPath`**: `<cwd>/.pi/settings.json` wins over `<agent dir>/settings.json`. Pi's
`powershell` tool (off unless `defaultTools` names it) is not translated to: open item.

## Visible changes

- A `hop`/`write` without `--dest-shell` now writes matching shell calls as native calls; the
  Codex writer's `<environment_context>` names the shell (as native rollouts do).
- A PowerShell call toward a Claude Code installation that runs the `PowerShell` tool becomes a
  native `PowerShell` call.
- `--dest-shell none` is new.

## Out of scope (recorded in `next.md`)

- Pi `powershell` tool as a destination tool.
- Codex reader: `<shell>cmd</shell>` is read as Bash (`src/reader/codex.rs` `shell_declared`); a
  call's own `shell` argument path is ignored.
- Codex Desktop with its agent in WSL [U].

## Tests

Pure helpers take their inputs (env value, parsed settings, file text, platform flag) so tests do
not depend on the machine; thin wrappers read the files. Each adapter: fixtures in a temp store.
CLI: `hop` Codex→Codex without `--dest-shell` on Windows writes a native `exec_command` for a
PowerShell call (cfg-gated per platform); `--dest-shell none` keeps it text form.
