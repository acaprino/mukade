# The live runner, first runs — 2026-11-28

The first runs of [`tests/live/run.mjs`](../../tests/live/run.mjs) ([guide](../live-tests.md),
[design](../superpowers/specs/2026-11-28-live-tests-and-harness-parity-design.md)): the six harnesses
installed at their pins, each with its own store, every source hopped into every destination, and every
destination opening what was written. No model was called: Claude Code, Codex and DeepSeek Harness resumed
against the runner's local mock.

## Setup

| | |
|---|---|
| machine | Linux 6.18 (a cloud container), Node 22.23.5, npm, git, uv 0.13 |
| mukade | the `mukade-Linux` artifact of Verify run [46140561892](https://github.com/acaprino/mukade/actions/runs/46140561892) (`develop` at `af76b8c`), passed with `--mukade` (this container's `gh` is limited, so it was downloaded through the GitHub API) |
| Claude Code | 2.1.332 (npm `@anthropic-ai/claude-code`) |
| Codex | 0.172.0 (npm `@openai/codex`) |
| OpenCode | 2.4.2 (npm `@opencode/cli`) |
| Pi | 1.7.0 (npm `@earendil-works/pi-coding-agent`, `--ignore-scripts`) |
| Hermes Agent | 0.23.5 (tag `v0.23.5`, `uv sync --frozen --no-dev` on Python 3.14) |
| DeepSeek Harness | 0.2.2-rc.5 (npm `@deepseek-ai/dsh`) |
| command | `node tests/live/run.mjs --mukade .live/mukade/run-46140561892/mukade` |

## Result

All 36 pairs passed (hop, read-back and the destination's own open), in 84 s with the installs already in place.

| source \ dest | claude_code | codex | opencode | pi | hermes | deepseek_harness |
|---|---|---|---|---|---|---|
| claude_code | pass | pass | pass | pass | pass | pass |
| codex | pass | pass | pass | pass | pass | pass |
| opencode | pass | pass | pass | pass | pass | pass |
| pi | pass | pass | pass | pass | pass | pass |
| hermes | pass | pass | pass | pass | pass | pass |
| deepseek_harness | pass | pass | pass | pass | pass | pass |

How each destination opened it:

| destination | its own open | what it said, over the six sources |
|---|---|---|
| Claude Code | `claude -p --resume <id>` against the mock | `MOCK-REPLY 13 messages` (11 from dsh); every request carried the session's first prompt |
| Codex | `codex exec resume <id>` against the mock | `MOCK-REPLY 16` to `18 messages` (13 from dsh), first prompt carried |
| OpenCode | `opencode session list` names it in the work folder's project; `opencode session export <id>` | 6 to 10 messages exported |
| Pi | `pi --export <file>` | the same id, 8 to 12 messages |
| Hermes | `hermes sessions export --session-id <id>` | 8 to 10 messages; the first import (from Claude Code) took 10.7 s, the gateway starting, the next ones under 1.5 s |
| DeepSeek Harness | `dsh --profile headless --session-id <id>` against the mock | `MOCK-REPLY 10` to `14 messages`, first prompt carried |

Chat, one launched instance per harness against the mock:

| harness | result |
|---|---|
| Claude Code, Codex, OpenCode, Pi | pass: each joined its room at start, and the line a host member wrote before the launch reached the model with the prompt |
| Hermes, DeepSeek Harness | not checked: `chat launch` refuses them (`has no chat adapter yet`), the gap the design closes next |

**Negative control.** A written Claude Code session cut to its last two rows, under a new id, was resumed the
same way: Claude Code answered `No conversation found with session ID`, exit 1, and the mock received no request
carrying the fixture's first prompt. The runner reports such a pair as fail.

## The open facts the design left to the runner

- **Which harnesses open a session without a model:** OpenCode (`session export`, `session list`), Pi
  (`--export`) and Hermes (`sessions export`). Claude Code, Codex and DeepSeek Harness have no such command; a
  resume against the mock confirms them. (Codex's `debug prompt-input` renders a new session's input, not a
  saved one.)
- **Whether Hermes's hooks can start a chat member:** not settled. mukade has no Hermes chat adapter yet, so
  there is nothing to launch; the runner reports it as not checked until the adapter exists.

## Found on the way

- **Hermes refuses a wheel build** (`Building wheels or sdists for hermes-agent is not supported`), and its
  `pyproject.toml` pins its dependencies for Python 3.14 only. The runner clones the tag and installs its
  `uv.lock` (`uv sync --frozen --no-dev --python 3.14`) into a virtual environment of its own.
- **Hermes installs a runtime into each new `HERMES_HOME`** in the background (a Python and a `uv sync` under
  `<home>/installs/`), and its import leaves a gateway running (`gateway run --idle-exit`).
  `HERMES_DISABLE_LAZY_INSTALLS=1` stops the first; the runner runs `hermes gateway stop` for every Hermes home
  and then stops any process left under `.live/`.
- **OpenCode without `--standalone`** starts the background service on its fixed port, whatever the store: a
  first try left one running. The runner passes `--standalone` to every OpenCode command, as mukade does.
- **OpenCode's project id** for a folder is the one OpenCode records in its `project` table; the runner makes the
  work folder a git repository of its own, lets OpenCode see it once (`session list`), and reads the id there for
  `--project-id`.
- **Disk:** the six installs take about 4 GB (OpenCode 1.1 GB, DeepSeek Harness 1.3 GB).

## Second run, after the review fixes

The runner after a review of its first version: the mock answers JSON unless a request asks for a stream; the
first-prompt check looks for the prompt's first 40 characters as one run of text in the request; the sweep stops
only processes whose program lies under `.live/harnesses` or `.live/mukade`, or whose `HOME` is in a run folder;
Ctrl-C stops what the run started; a seed or project failure fails the run; the installs keep npm's and uv's
caches, and uv's Python, in `.live/cache/`; git runs without the shell's `GIT_*`.

| check | result |
|---|---|
| every install from nothing, mukade of Verify run [46144343376](https://github.com/acaprino/mukade/actions/runs/46144343376) (`e4c117e`) by `--run-id` | 36 of 36 pairs, 4 of 4 chat launches, exit 0 |
| run with `GIT_DIR` and `GIT_WORK_TREE` pointing at the mukade checkout | the work folder got its own commit; the checkout's history and index unchanged |
| Hermes's Python | `.live/cache/uv-python/cpython-3.14.6`; nothing new under `~/.local/share/uv` |
| what the harnesses asked the mock | 23 requests, every one with `stream: true` (Messages 14, Responses 7, Chat 2) |
| `tail -f <checkout>/.live/reports/decoy.md` left running in another shell | alive after the run, after `--clean` and after Ctrl-C |
| `--clean` while a run holds the lock | refused, exit 2 |
| Ctrl-C during the hops into Hermes | `live: SIGINT: stopping what this run started`, exit 130: the hop, Hermes's import and its detached gateway stopped, the lock removed, nothing left running |

A first try of the `GIT_DIR` run found that the installs' `git clone` inherited it too (`fatal: working tree …
already exists`); the installs and every harness process now start without the shell's `GIT_*`.

## Not covered by this run

- Windows, where the owner runs it: the `.cmd` launchers, `taskkill` and the PowerShell process sweep are written
  for it and not yet run there.
- The default path that fetches mukade with `gh run download`, and `--release`: this container's `gh` is limited
  to searches.
- `--latest` and `--models` (no keys here).
