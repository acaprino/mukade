# Live tests: mukade against real harnesses

One command installs pinned copies of the six harnesses, each with its own store, and checks that every
harness opens what mukade writes into it. It prints a matrix of pass, fail and not checked per pair of
harnesses. Nothing touches your real stores, sessions, configuration or credentials, and no model is
called unless you ask for it ([design](superpowers/specs/2026-11-28-live-tests-and-harness-parity-design.md)).

```bash
node tests/live/run.mjs
```

The fixture suite (`cargo test`, run by CI) checks mukade's readers and writers on recorded sessions. This
runner checks what that suite cannot: the harness versions out today, as installed programs.

## What you need

| what | why |
|---|---|
| Node.js 22.13 or later (24 recommended) | the runner, the mock model, and OpenCode's project id (`node:sqlite`) |
| npm | installs Claude Code, Codex, OpenCode, Pi and DeepSeek Harness |
| git and [uv](https://docs.astral.sh/uv/) | install Hermes Agent from its tag (PyPI carries an older Hermes, and its build refuses a wheel) |
| `gh`, logged in | fetches the `mukade` the latest green Verify run of `develop` built; not needed with `--mukade` |
| about 4 GB of disk | the six harnesses (OpenCode and DeepSeek Harness over 1 GB each) and Hermes's Python environment, all in `.live/` |

Windows and Linux. On macOS pass `--mukade <path>`: CI builds no macOS binary.

## What it does

1. **Installs** each harness at the version pinned in [`tests/live/harnesses.json`](../tests/live/harnesses.json) into
   `.live/harnesses/<harness>/<version>/`: the npm harnesses with `npm install --prefix`, Hermes with
   `git clone --branch <tag>` and `uv sync --frozen` into a virtual environment of its own. An install that is
   there already is used again.
2. **Fetches mukade**: the `mukade-Windows` or `mukade-Linux` artifact of the latest green Verify run of
   `develop` (`gh run download`), kept in `.live/mukade/run-<id>/`.
3. **Makes a run folder** `.live/runs/<time>/`: a home for every process (`HOME`, `USERPROFILE`, the `XDG_*`
   folders), a `work` folder (a git repository of its own) where every session is filed, and one home per
   harness and role (`src/`, `dst/`, `chat/`). Each harness gets the variables mukade itself sets for a sandbox
   (the adapter's `sandbox_install`), plus its store variable. The shell's store variables, API keys and the
   harnesses' own settings variables are removed.
4. **Seeds every source store** from [`fixtures/`](../fixtures): Claude Code, Codex, Pi and DeepSeek Harness by
   copying the recorded file into the store's layout; OpenCode through its own `session import`; Hermes by
   letting Hermes create its `state.db` and adding the fixture's rows. mukade must list the seeded session.
5. **Hops every source into every destination** (36 pairs): `mukade hop <src> <id> <dst> --root <src store>
   --dest-root <dst store> --cwd work`.
6. **Reads each result back with mukade**: `mukade list` names it and `mukade dump` reads its messages.
7. **Has the destination open it** with a command of its own:

   | destination | how it is confirmed | model |
   |---|---|---|
   | Claude Code | `claude -p --resume <id>` | the local mock |
   | Codex | `codex exec resume <id>` | the local mock (a `mock` provider in the store's `config.toml`) |
   | OpenCode | `opencode session list` names it in the work folder's project; `opencode session export <id>` | none |
   | Pi | `pi --export <file>` reads it back: same id, its messages | none |
   | Hermes | `hermes sessions export --session-id <id>` | none |
   | DeepSeek Harness | `dsh --profile headless --session-id <id>` | the local mock (a patch file) |

   A resume against the mock passes when the harness exits 0, prints the mock's answer
   (`MOCK-REPLY <n> messages`), and the request it sent carries the session's first prompt: the harness sent
   the history mukade wrote.
8. **Chat**: one instance per harness, launched with `mukade chat launch <harness>` into a room of its own, runs
   one turn against the mock. It passes when it joined the room and the line a host member wrote before it
   started reached the model. The chat data folder is a new folder in the system's temporary folder (chat
   refuses one inside a checkout), removed afterwards.
9. **Stops** what is left running (Hermes's gateway, by `hermes gateway stop`, then any process under `.live/`),
   and **reports**.

The **mock model** ([`tests/live/mock-model.mjs`](../tests/live/mock-model.mjs)) listens on `127.0.0.1` only and
answers the Anthropic Messages, OpenAI Responses and OpenAI Chat APIs with one short text. Every request is
logged in the run folder (`mock-requests.jsonl`).

## Reading the result

```text
source \ dest     claude_code     codex           opencode        pi              hermes          deepseek_harness
claude_code       pass            pass            pass            pass            pass            pass
...
chat:
  claude_code        pass         joined the room at start, and the host's message reached the model with the prompt
  hermes             not checked  chat launch refused: unsupported: hermes has no chat adapter yet; ...
```

- **pass**: the hop, the read-back and the destination's own open all passed.
- **fail**: one of them failed; the lines under the matrix say which, and why.
- **not checked**: a step could not run: the source could not be seeded, OpenCode gave the work folder no
  project, a chat adapter does not exist, or a check needs a real model and `--models` did not get its key. A
  check that did not run is never a pass.
- **SETUP FAIL**: what kept the run from testing what it was asked to: a source that could not be seeded, no
  OpenCode project, or (`--latest`) a newest version that could not be looked up, so the pin was tested instead.
  Each one fails the run, even when every pair that ran passed.

The same is saved as `.live/reports/<time>.md` and `.json`, and in the run folder as `report.md`. Each pair keeps
its receipt, manifest and the commands' output in `.live/runs/<time>/out/<src>--<dst>/`.

The exit code is 0 when nothing failed, 1 when a pair, a chat scenario or the setup failed, 2 when the runner
itself could not run (an install, the mukade download, a Node older than 22.13), 130 when it was interrupted.

## Options

| option | what |
|---|---|
| `--mukade <path>` | test this binary (a local build, a downloaded one) |
| `--release <tag>` | test the `mukade.exe` of a GitHub Release (Windows; `latest` too) |
| `--run-id <id>` | take the mukade of this Verify run instead of the latest green one |
| `--latest` | install the newest version of each harness instead of the pinned one; the report lists both |
| `--models` | also resume with real models, with the keys set in your shell (`real_models` in the table) |
| `--from <ids>`, `--to <ids>` | only these sources, only these destinations (comma-separated harness ids) |
| `--no-chat` | leave out the chat scenarios |
| `--install-only` | install and fetch, run nothing |
| `--clean` | stop anything the runner left running and delete `.live/`; refused while a run is going |

```bash
node tests/live/run.mjs --latest
```

```bash
node tests/live/run.mjs --from claude_code --to opencode,pi
```

```bash
node tests/live/run.mjs --clean
```

### After a harness update

Run with `--latest`. A pair that passes with the pin and fails with the newest version is the update breaking
mukade. When the newest versions pass, raise the pins in `tests/live/harnesses.json` in the same change that
records the run.

### Real models

`--models` resumes against real providers, with the key each `real_models` entry names (`ANTHROPIC_API_KEY` for
Claude Code, `OPENAI_API_KEY` for Codex, `OPENROUTER_API_KEY` for the others): Claude Code, Codex and DeepSeek
Harness resume with it instead of the mock, and OpenCode, Pi and Hermes add a resume to their own model-free
check. Only those key variables are passed through. The chat scenarios do not run with `--models`. Each resume
is one short prompt and costs that.

## Isolation, and what it does not cover

- Every store, configuration file and session the harnesses write is under `.live/runs/<time>/`. Every harness
  process starts with `HOME` there, without the shell's store variables, settings variables or keys.
- OpenCode runs with `--standalone` everywhere: never your background service.
- One run at a time: a second runner, and `--clean`, refuse while the first holds `.live/run.lock`.
- Ctrl-C stops the run and what it started (exit 130). At the end of every run the runner stops what is left:
  Hermes's gateway (`hermes gateway stop`), then every process whose program lies under `.live/harnesses` or
  `.live/mukade`, or (Linux) whose `HOME` is in a run folder. A process that only names a path there, an editor
  or the shell that started the run, is left alone.
- The installs keep their caches in `.live/cache/` (npm's cache, uv's cache, and the Python uv installs for
  Hermes), so they write nothing in your profile either. The chat scenarios' data folder is the one thing
  outside `.live/`, in the system's temporary folder, removed when they end.
- The harnesses may still reach the network on their own (an update notice, a model catalog): the runner turns off
  what it knows how to (`DISABLE_AUTOUPDATER`, `PI_OFFLINE`, `HERMES_DISABLE_LAZY_INSTALLS`, …), it does not cut the
  network.
- `.live/` is in `.gitignore`. `--clean` removes all of it; runs accumulate until then.

## Keeping it right

- `tests/live/harnesses.json` must say what the adapters say: `tests/live_table.rs` (run by CI) compares its
  `env`, `store` and package with each adapter's `sandbox_install`, and `store.env` with
  `catalog/harnesses.json`. Change an adapter's sandbox and the table together.
- The runner is not run in CI: it installs about 4 GB of harnesses and may need keys.
- Records of runs: [2026-11-28](verification/2026-11-28-live-runner.md). The first run on Windows is step 19 of the
  [Windows checklist](verification/windows-checklist.md#19-the-live-runner-on-windows-20-min-most-of-it-the-installs-).
