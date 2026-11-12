# Destination shell — implementation plan

Spec: `docs/superpowers/specs/2026-11-12-destination-shell-design.md`. Branch `dest-shell`,
worktree `D:\Projects\mukade-grep`, base `1a6c302`. TDD for every task: the test fails first.
Global: no other behaviour change; public signatures unchanged (additions only); no new crates;
`cargo test -j 4 --workspace` and `cargo clippy -j 4 --workspace --all-targets` (no new warnings)
before each commit; commit trailer as the repository uses.

### Task 1: The trait methods, the shared name helper, Codex and Pi
**Files:** `src/harnesses/mod.rs`, `src/harnesses/{codex,pi}.rs`, `src/tools.rs` (or `src/dest.rs`),
tests in `tests/harnesses.rs` (or a new `tests/dest_shell.rs`).
- [ ] `Destination::default_shell(&self, roots, cwd) -> Option<Shell>` (default `None`) and
      `Destination::offered_in(&self, roots, cwd) -> ToolSet` (default `self.offered()`).
- [ ] `pub fn dest::shell_in(harness, roots, cwd) -> Option<Shell>`; `pub fn tools::offered_in(harness, roots, cwd) -> ToolSet`; non-destination → `None` / empty.
- [ ] `shell_named(name: &str) -> Option<Shell>` per the spec table (path, `.exe`, case). Tests cover every listed name, a Windows path, `cmd`, `fish`, empty.
- [ ] Codex: `cfg!(windows)` → `PowerShell`, else `Posix`.
- [ ] Pi: `shellPath` from `<cwd>/.pi/settings.json` then `<agent dir>/settings.json`; unset → `Posix`. Tests with temp dirs (project wins; pwsh path → PowerShell; unknown → None).
- [ ] Commit `feat(harnesses): a destination names its default shell and offered set`.

### Task 2: OpenCode and Claude Code
**Files:** `src/harnesses/{opencode,claude_code}.rs`, tests.
- [ ] OpenCode `default_shell`: config `shell` (sources and order per spec; `.jsonc` tolerated — reuse or add a small comment/trailing-comma stripper, no new crate) → `$SHELL` → Windows `PowerShell` else `None`. Env reads go through a seam so tests pass values in.
- [ ] Claude `offered_in`: the pinned list plus `PowerShell` when enabled (order per spec: env, settings `env` nearest first, newest transcript snapshot, else off). A second static list for the extended set. Tests: each step decides, and a later step is not consulted once one answers; snapshot parsing from real-shaped lines (`"shell":"PowerShell (primary); Bash tool also available …"` → on; `"shell":"bash"` → off); tail-only read bound.
- [ ] Commit `feat(harnesses): OpenCode's shell and Claude's PowerShell from their configuration`.

### Task 3: The edges and `--dest-shell none`
**Files:** `src/cli.rs` (`write_opts`, usage text), `mukade-tui/src/main.rs` (`write_details_for`), `tests/cli.rs`.
- [ ] `write_opts`: `--dest-shell none` → `None`; a name → parsed (error text lists `none` too); absent → `dest::shell_in(harness, roots, cwd)`. `offered: Some(tools::offered_in(harness, roots, cwd))`. The `cwd` is the one the write files the session under.
- [ ] TUI `write_details_for`: `dest_shell` and `offered` the same way.
- [ ] CLI tests: a Codex→Codex hop of a PowerShell-declared session without `--dest-shell` writes a native `exec_command` on Windows (`#[cfg(windows)]`; a POSIX-declared one on other platforms), and `--dest-shell none` keeps it text form; Pi destination with a `shellPath` in its settings.
- [ ] Commit `feat(cli): a hop takes the destination's shell unless --dest-shell says otherwise`.

### Task 4: Docs
**Files:** `docs/usage.md` (flag row, §8 shell text), `docs/tool-conversion-tables.md` (§4.1 shell, §6 Today/Planned, changelog, Last updated), `docs/harnesses/{claude-code,codex,opencode,pi}.md` (how each runs its shell tool, what mukade reads; fix `claude-code.md` "elsewhere opt-in" for Windows with Git Bash; settle `pi.md` [U] on the shell), `docs/session-interchange-spec.md` (L11 row, L10 dialect row), `docs/architecture.md` (group table), `next.md` (item done; new open items from the spec's Out of scope). Facts and sources: `D:\Projects\horness-launcher\sidecar\runs\musashi-terminal-4444-2379968-0-134389045251963263-0\tmp\claude\c56a38692edb5f5e\40865358-8ab0-4903-8cee-f73bbc95729b\scratchpad\dest-shell-research.md`. Style: clear, complete, not wordy; tables; one home per fact.
- [ ] Commit `docs: the destination's shell comes from its configuration`.
