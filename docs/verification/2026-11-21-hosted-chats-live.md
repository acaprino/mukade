# Hosted chats, work on disk and handoff on Windows — 2026-11-21

The command line of the released build `Mukade-0.2.0-windows-x64.zip` (v0.2.0), run against the real
Codex home of the owner's machine. Machine: Windows 11, Codex desktop 26.1117.41037,
codex-cli 0.167.0-alpha.3.2, Claude Code 2.1.330, git for Windows. The session is the hosted chat
"Capire lo stack di Daodan", `6b00c623-2d08-83eb-a3cc-df6dadef8474`, in the ChatGPT project Daodan,
whose folder holds three clones.

This record covers the command line only. The desktop app and the start of a harness were not run:
see "Not checked".

## What was run

```text
mukade list codex --root %USERPROFILE%\.codex
mukade work codex 6b00c623-2d08-83eb-a3cc-df6dadef8474 --root %USERPROFILE%\.codex
mukade dump codex 6b00c623-2d08-83eb-a3cc-df6dadef8474 --root %USERPROFILE%\.codex
mukade handoff codex 6b00c623-2d08-83eb-a3cc-df6dadef8474 claude_code --root %USERPROFILE%\.codex
mukade handoff codex 6b00c623-2d08-83eb-a3cc-df6dadef8474 pi --root %USERPROFILE%\.codex
mukade handoff codex 6b00c623-2d08-83eb-a3cc-df6dadef8474 codx --root %USERPROFILE%\.codex
```

## What was seen

| command | result |
|---|---|
| `list codex` | 9 rows. The chat and its branch "Ramo · Capire lo stack di Daodan" are listed with `cwd` `C:\Users\<you>\Documents\ChatGPT\Daodan`, `location` the full path of `sqlite\codex.db`, and `unreadable` `hosted by ChatGPT: the conversation is not on this machine; the work it left is in its folder` |
| `work` | three lines and the last-fetch line, in 2.1 s: `daodan: branch develop, upstream origin/develop, in sync, 4 uncommitted paths`, `imago: branch develop, upstream origin/develop, clean`, `stack-analyser: branch feature/stack-detection, upstream fork/feature/stack-detection, 4 commits ahead` |
| `dump` | exit 2, `not found: Codex thread 6b00c623-… is hosted by ChatGPT: the conversation is not on this machine; the work it left is in its folder` |
| `handoff … claude_code` | exit 0. The note is printed, with the three lines of `work` as a list. Then `note: <folder of mukade.exe>\mukade-data\handoffs\codex-6b00c623-2d08-83eb-a3cc-df6dadef8474.md`, then the command as one JSON array: the full path of `claude.EXE` and the prompt `Read <that path> and continue from it.` The note file exists. `handoff-prompts\continue.md` was written beside the executable |
| `handoff … pi` | exit 2, `unsupported: pi cannot be started with a first prompt yet (verified: claude_code, codex)` |
| `handoff … codx` | exit 1, `unknown harness "codx" (claude_code, codex, opencode, pi, hermes, deepseek_harness)` |

The numbers of `work` were compared with git run by hand in the same folders (`GIT_OPTIONAL_LOCKS=0`):
`git status --porcelain` gives 4 paths in `daodan` and 0 in `stack-analyser`,
`git rev-list --count fork/feature/stack-detection..feature/stack-detection` gives 4, and
`git rev-list --count refs/heads/feature/stack-detection --not --remotes` gives 4. They agree.

Without `--root` and without `CODEX_HOME` the commands answer
`not found: CODEX_HOME is not set: no Codex store to list`, as `list` always did.

## Not checked

- `mukade handoff … --run`: no harness was started.
- The desktop app `mukade-ui.exe`: the chip `hosted`, "Work on disk", and `Start in…`.
- The one-line prompt reaching the harness through Windows Terminal (`wt new-tab -- <exe> <prompt>`),
  with a data folder whose path has a space in it.
- The Codex CLI destination.

ADR-0005 stays Proposed until these are seen.
