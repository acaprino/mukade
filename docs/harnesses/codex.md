# Codex CLI

| | |
|---|---|
| **Documentation updated** | 2026-10-28 |
| **Version described** | 0.164.0 (the version mukade's readers and writers were verified against) |
| **Vendor** | OpenAI [1] |
| **Website** | https://developers.openai.com/codex [2] |
| **License / source** | Apache-2.0, https://github.com/openai/codex [1][17]; Codex Desktop app licence **[U]** |
| **mukade name** | `codex` |

Evidence labels: `[n]` cites §9. **[U]** = not verified. Repository paths are relative to the repository root.

## 1. Introduction

Codex CLI is OpenAI's coding agent that runs locally in a terminal: it inspects a repository, edits
files, runs commands and reviews changes [1][2]. It is meant for developers who work from a shell
or an editor, or who automate repeatable work (`codex exec`) [2][3]. The IDE extension is an
app-server client [7], the Desktop app writes rollouts in the same format [21], and all three
share `config.toml` [6][8].
mukade was verified against CLI 0.164.0 and has also read Codex Desktop rollouts up to
0.164.0-alpha.11.1 [21]. npm already lists 0.165.0 (published 2026-10-27), which mukade has not
been verified against [17].

## 2. Description

| aspect | 0.164.0 |
|---|---|
| agent loop | the model reads and edits files, runs locally installed tools, reviews changes, delegates to subagents and calls MCP servers [2]; it is reached over the Responses API (`wire_api` accepts only `responses`) [4][6] |
| sandbox | `-s/--sandbox` or `sandbox_mode`: `read-only`, `workspace-write`, `danger-full-access`; Seatbelt on macOS, bubblewrap (`bwrap`) on Linux and WSL2, the native Windows sandbox in PowerShell [4][5][18]; `codex exec` defaults to `read-only` [3] |
| approvals | `-a/--ask-for-approval on-request \| never` [18]; `untrusted` retired, `on-failure` deprecated [4][5]; `--approve-for-me` routes eligible requests to automatic review (`approvals_reviewer = "auto_review"`) [5][18]; `--dangerously-bypass-approvals-and-sandbox` removes both [18] |
| models | recommended `gpt-6-astra`, `gpt-6.1-sol`, `gpt-6.1-terra`, `gpt-6-luna`, chosen with `/model`, `-m/--model` or `model = "…"` [6]; `--oss --local-provider lmstudio\|ollama` for a local provider [18]; custom providers in §5 |

## 3. Entry points

| entry | command | notes |
|---|---|---|
| interactive TUI | `codex [PROMPT]` | `--search` enables the native `web_search` tool; `--no-alt-screen` runs inline [18] |
| non-interactive | `codex exec "<prompt>"` (alias `codex e`) | `--json` prints JSONL events (`thread.started`, `turn.*`, `item.*`, `error`). `-` or piped stdin is accepted. `--ephemeral` writes no rollout [3][18] |
| resume (TUI) | `codex resume [SESSION_ID] [PROMPT]` | picker by default; `--last`, `--all`, `--include-non-interactive` [18] |
| resume (exec) | `codex exec resume <SESSION_ID> "<prompt>"` / `codex exec resume --last "<prompt>"` | id or thread name; appends to the same rollout [3][18][19] |
| fork | `codex fork [SESSION_ID] [PROMPT]`, `codex exec fork <SESSION_ID> [PROMPT]` | new session from an old one [18] |
| app server | `codex app-server` (experimental) | JSON-RPC 2.0 over stdio by default; `--listen ws://IP:PORT` and `unix://` are also accepted. Methods include `thread/start`, `thread/resume`, `thread/fork`, `thread/compact/start`, `turn/start`, `turn/interrupt` [7][18] |
| remote TUI | `codex --remote ws://host:port` | attaches the TUI to an app server [7][18] |
| Desktop app | `codex app [PATH]` | opens the installer if the app is missing [1][18] |
| IDE | VS Code, Cursor, Windsurf extension | an app-server client (`clientInfo.name` `"codex_vscode"`) [1][7] |
| other | `codex review`, `codex agents`, `codex queue --thread <THREAD> --message <TEXT>`, `codex cloud`, `codex archive/unarchive/delete`, `codex migrate-rollouts` | from `codex --help` 0.164.0 [18] |

## 4. Harness type

### Vendor

OpenAI. The CLI is developed in the open in `openai/codex`; the Rust workspace is `codex-rs`
[1][7].

### Website and documentation

Documentation: https://developers.openai.com/codex [2] (as of 2026-10-28 its pages redirect, 308, to
`learn.chatgpt.com/docs/…`; §9 cites the targets). Source and releases: https://github.com/openai/codex [1][16].

### Installation

| OS | method | command |
|---|---|---|
| macOS, Linux | install script | `curl -fsSL https://chatgpt.com/codex/install.sh \| sh` [1][2] |
| Windows | install script (PowerShell) | `powershell -ExecutionPolicy ByPass -c "irm https://chatgpt.com/codex/install.ps1 \| iex"` [1] |
| any (Node.js) | npm | `npm install -g @openai/codex` [1] |
| macOS | Homebrew | `brew install --cask codex` [1] |
| macOS, Linux | GitHub Releases archive | `codex-aarch64-apple-darwin.tar.gz`, `codex-x86_64-apple-darwin.tar.gz`, `codex-x86_64-unknown-linux-musl.tar.gz`, `codex-aarch64-unknown-linux-musl.tar.gz`; rename the binary to `codex` [1]. Windows archive name **[U]** |

- By default the scripts download from `https://releases.openai.com/codex`. To force GitHub
  Releases, set `CODEX_INSTALLER_USE_RELEASES_OPENAI_COM=false` (Windows:
  `$env:CODEX_INSTALLER_USE_RELEASES_OPENAI_COM='false'; irm https://chatgpt.com/codex/install.ps1 | iex`) [1].
- Windows install location: `%LOCALAPPDATA%\Programs\OpenAI\Codex\bin\codex.exe`. The binary used
  for this page is there, and `src/exe.rs:149-160` records it as the installer's location [18][24].
- To update, re-run the install script or use `codex update` [2][18].
- To sign in, run `codex` and choose **Sign in with ChatGPT**. An API key also works:
  `codex login --with-api-key`, or `CODEX_API_KEY=<key>` for `codex exec`. `--device-auth` is also
  offered [1][2][3][18].

### Licence

The CLI repository is Apache-2.0 [1], and the npm package `@openai/codex` declares Apache-2.0 [17].
The Desktop app's licence was not checked **[U]**.

## 5. Features

### Session store

Root: `CODEX_HOME`, `~/.codex` by default [9][20]. All facts below were seen on disk with 0.164.0
unless cited otherwise [19 §2.2].

| file | what it is |
|---|---|
| `sessions/YYYY/MM/DD/rollout-<local ts>-<uuid>.jsonl` | one thread; JSONL lines `{timestamp, ordinal, type, payload[, metadata]}`. A rollout alone resumes by id, even from the wrong date folder |
| `sessions/YYYY/MM/DD/rollout-<local ts>-<uuid>_<page uuid>.jsonl` | a later **page** of a paginated thread (0.165, below) |
| `state_5.sqlite` | thread index. **`threads.rollout_path` wins over the file on disk**; a row pointing at a missing file fails with `no rollout found for thread id … (code -32600)` |
| `thread_history_1.sqlite` | exists, but the model context comes from the rollout, not from here |
| `session_index.jsonl` | append-only name catalogue `{id, thread_name, updated_at}`; the newest name wins [20]. It was not written in the bench's scratch home [19] |
| `history.jsonl` | transcript history, controlled by `history.persistence = "save-all" \| "none"` and `history.max_bytes` [4] |

Record kinds in a rollout (`type`, then `payload.type`) [19][22]:

| `type` | payloads seen | role |
|---|---|---|
| `session_meta` | `id` (native rollouts also `session_id` [22]), `cwd`, `originator`, `source`, `cli_version`, `model_provider`, `git`, `base_instructions` | thread header; Codex refuses a rollout without `cli_version` [23] |
| `response_item` | `message` (`user`/`assistant`/`developer`), `reasoning` (`summary`, `encrypted_content`), `function_call`/`function_call_output`, `custom_tool_call`/`custom_tool_call_output`, `local_shell_call`, `web_search_call` | the conversation itself |
| `event_msg` | `task_started`, `task_complete`, `turn_aborted {reason:"interrupted"}`, `thread_settings_applied` | events about the conversation |
| `turn_context` | `turn_id`, `model`, `cwd` | starts a turn |
| `compacted` | see Compaction | replaces history |
| `world_state`, `token_usage_record` | — | events, not conversation |

- Parallel calls: consecutive `function_call` lines, then their outputs, paired by `call_id`. A
  graceful interrupt writes a user message `<turn_aborted>…</turn_aborted>` plus
  `event_msg turn_aborted`; a hard kill just ends the file. `thread/list` hides `exec` threads by default [19].
- 0.164.0 searches and restores **compressed rollouts** [16], and `codex migrate-rollouts`
  migrates legacy sessions to "paginated thread history" [18]. The on-disk form of compressed
  rollouts is **[U]**.
- **Paginated threads, seen on disk with Codex Desktop 0.165.0-alpha.1.2 (2026-10-28)** [26].
  `session_meta.history_mode` is `"paginated"`. When the later turns of a thread are redone, Codex
  starts a new page: a file `rollout-<ts>-<thread>_<page>.jsonl` whose first record is a
  `session_meta` with `history_base: {thread_id, end_ordinal_exclusive, end_byte_offset}`. The page
  continues the thread from the first `end_ordinal_exclusive` records of the earlier file (exactly
  `end_byte_offset` bytes), and its own ordinals carry on from there. `state_5.sqlite`
  `threads.rollout_path` names the page, so the page is what Codex resumes. The earlier file's
  records past the cut stay on disk: the branch the thread left. In the thread seen, the base file
  held 1064 records and the page continued from its first 511.
- Also seen in 0.165 Desktop rollouts [26]: `response_item` `agent_message` (a subagent's
  message to the root agent: a plain header, then the payload as `encrypted_content`),
  `inter_agent_communication_metadata`, `token_usage_record`, `world_state`, and
  `event_msg/item_completed`.

### Compaction

- Triggers: automatic at `model_auto_compact_token_limit` [4]; `/compact` in the Desktop app [15]
  (in the CLI TUI **[U]**); app-server `thread/compact/start` [7][19]. `codex exec` has no
  compaction command [19].
- Compaction runs **locally** by default: an ordinary `/v1/responses` call with
  `request_kind:"compaction"` and the prompt `You are performing a CONTEXT CHECKPOINT COMPACTION…`
  [19]. The keys `compact_prompt` and `experimental_compact_prompt_file` override that prompt.
  `model_providers.<id>.capabilities.remote_compaction = "v2"` sends the trigger through the
  Responses API instead [4].
- Record: `compacted.payload = {message:"Another language model started … <summary>",
  replacement_history:[…], replacement_history_metadata, retained_context, window_number, window ids,
  compaction_response_id, latest_token_usage_record, resume_metadata}`. A line reduced to
  `{message}` still resumes [19].
- On resume after a compaction, the model receives the base developer message, then
  `replacement_history`, later items and the prompt. No `encrypted_content` is sent [19].
- **0.165 Desktop keeps the summary encrypted** [26]: in every `compacted` record of the threads
  seen, `message` is empty and the summary is an item `{type:"compaction", id, encrypted_content}`
  (77 000 to 99 000 characters) at the end of `replacement_history`, after the retained user
  messages. Only OpenAI can read it: there is no summary text to carry to another harness.

### Built-in tools

Offered by 0.164.0 `exec` in the bench, with an unknown model and default settings [21 §2]:

- `exec_command`: runs a shell command (`cmd`, `workdir`, `justification`, `yield_time_ms`; shell is the user's, `pwsh` on the test machine) [21 §4.1].
- `write_stdin`: writes to a running `exec_command` session [21 §4.1].
- `request_user_input`: asks the user a question (canonical `AskUserQuestion`) [21 §3].
- `view_image`: image viewing; purpose taken from the name only **[U]**.
- `multi_agent_v1` (namespace): subagent tools; real stores show `spawn_agent`, `send_message`, `wait_agent`.
- `get_goal`, `create_goal`, `update_goal`: the thread's persistent goal (`/goal`) [15].
- `web_search`: built-in, server-side Responses tool (`--search`) [18][21 §3].
- `apply_patch`: patch-based file edits; **not offered** to the bench's unknown model, but used by Codex Desktop [21].
- `local_shell_call`: an older record type, read by mukade from its `action` [22].

**Codex Desktop "code mode".** Desktop rollouts (`originator: codex_work_desktop`) hold almost every
call as **one custom tool `exec`**. Its input is a script such as
`text(await tools.exec_command({cmd:S, yield_time_ms:N}));` or `text(await tools.apply_patch(S));`.
In the census, 4,844 of 4,908 Codex calls were of this form [21 §5]. The 0.164.0 app server also
has `--code-mode-host <URL>` [18].

Conversion rules and argument tables: [`../tool-conversion-tables.md`](../tool-conversion-tables.md).

### Branches, forks and subagents

- **Fork.** `codex fork` and `codex exec fork <SESSION_ID>` start a new session from a previous one
  [18]. App-server `thread/fork` copies stored history into a new thread id [7]. How a forked
  rollout records its parent **[U]**.
- **Subagents.** They are on by default (`agents.enabled = true`) and spawned on request, or when an
  AGENTS.md or skill asks for delegation. `/subagents` switches between agent threads in the CLI.
  Custom agents are TOML files, one per agent, in `~/.codex/agents/` or `.codex/agents/`, with
  `name`, `description` and `developer_instructions`. The built-in agents are `default`, `worker`
  and `explorer`. Concurrency is capped by `agents.max_concurrent_threads_per_session` (legacy alias
  `agents.max_threads`) [14]. How subagent threads are persisted on disk **[U]**.

### Models and providers

```toml
model = "gpt-6.1-sol"
model_provider = "openai"            # default; reserved ids: openai, ollama, lmstudio
model_reasoning_effort = "medium"    # low | medium | high | xhigh | max | ultra (model-dependent)
[model_providers.<id>]
name = "…"
base_url = "https://…/v1"
wire_api = "responses"               # the only supported value; "chat" is not supported
env_key = "<VAR holding the API key>"
```
[4][6]

- **Profiles in 0.164.0** are separate files. `-p/--profile <name>` layers
  `$CODEX_HOME/<name>.config.toml` on top of the base config, and a project-local
  `.codex/config.toml` ignores `profile`/`profiles` [4][18]. Whether older inline `[profiles.<name>]`
  tables are still read **[U]**.
- Any key can be overridden per run with `-c key=value` (TOML-parsed). `--strict-config` rejects
  unknown keys [18].

## 6. Plugins and extensions

| kind | how it is installed and configured |
|---|---|
| MCP servers | `codex mcp add <NAME> [--env K=V] -- <COMMAND>...` (stdio) or `codex mcp add <NAME> --url <URL> [--bearer-token-env-var VAR]` (streamable HTTP); `codex mcp list\|get\|remove\|login\|logout` [8][18]. Stored as `[mcp_servers.<name>]` (`command`, `args`, `env`, `env_vars`, `cwd` / `url`, `bearer_token_env_var`, `http_headers`; `startup_timeout_sec` 10, `tool_timeout_sec` 60, `enabled`, `required`, `enabled_tools`, `disabled_tools`) in `~/.codex/config.toml`, or `.codex/config.toml` of a trusted project; OAuth with `codex mcp login <name>` [8] |
| AGENTS.md | global `$CODEX_HOME/AGENTS.override.md`, else `AGENTS.md`; then one file per directory from the project root down to the cwd (`AGENTS.override.md`, `AGENTS.md`, `project_doc_fallback_filenames`), concatenated root first, up to `project_doc_max_bytes` (32 KiB); `/init` writes a scaffold [2][9] |
| skills | a folder with `SKILL.md` (frontmatter `name`, `description`), found in `.agents/skills` (cwd up to the repo root), `$HOME/.agents/skills`, `/etc/codex/skills` and the bundled system skills; used with `$<skill>`, installed with `$skill-installer <name>`, created with `$skill-creator`; disabled with `[[skills.config]]` `path` + `enabled = false` [10][11] |
| plugins | bundles of skills, MCP servers, browser extensions and hooks [11][12]; `/plugins` in the TUI [12]; `codex plugin add <PLUGIN[@MARKETPLACE]>`, `codex plugin list\|marketplace\|remove` [18]. mukade's bench sets `[features] plugins = false`: with plugins on, every Codex process contacts chatgpt.com and github.com [19 §2.2] |
| prompts | custom prompts appear as `/prompts:<name>` (Desktop app) [15]; 0.164.0 adds ten numbered `/presets` slots [16]; prompt file location (`~/.codex/prompts`) **[U]** |

## 7. Hooks

Codex has hooks, enabled by default. `[features] hooks = false` disables them; `codex_hooks` is a
deprecated alias [13].

| aspect | detail [13] |
|---|---|
| files | `~/.codex/hooks.json`, `~/.codex/config.toml` `[hooks]`, `<repo>/.codex/hooks.json`, `<repo>/.codex/config.toml`; plugin `hooks/hooks.json`; managed `requirements.toml` `[hooks]`. All matching sources load |
| events | `PreToolUse`, `PermissionRequest`, `PostToolUse`, `PreCompact`, `PostCompact`, `UserPromptSubmit`, `SubagentStop`, `Stop`, `Interrupt`, `SessionStart`, `SubagentStart`, `SessionEnd` |
| handlers | `command`, `mcp_tool` (`prompt`, `agent` are parsed but skipped); `timeout` in seconds (default 600; `SessionEnd`/`Interrupt` 1, max 3); `async: true` |
| matcher | regex on tool name, compaction trigger, start source (`startup\|resume\|clear\|compact\|fork`) or agent type; `UserPromptSubmit`, `Stop`, `Interrupt` ignore it |
| trust | non-managed hooks run only after review, recorded against their hash (`/hooks`); `--dangerously-bypass-hook-trust` skips the check for one run [18] |

- 0.164.0 added observation-only model-request hooks and hooks for inter-agent messages [16].
- **`notify`** is a separate key: an array of strings naming a command that receives a JSON
  payload. Project-local config ignores it [4]. Which events fire `notify` **[U]**.
- mukade neither reads nor writes hook configuration; it moves sessions only.

## 8. How mukade reads and writes it

| | |
|---|---|
| CLI name | `codex` |
| store variable | `CODEX_HOME`, **required** (no implicit `~/.codex`). Errors: `CODEX_HOME is not set: no Codex store to list` / `…to read` / `nowhere to write the Codex rollout` (`src/reader/codex.rs:546-576`, `src/writer/codex.rs:34-38`) |
| executable | `--codex-exe` → `MUKADE_CODEX_EXE` → `PATH` → `%LOCALAPPDATA%\Programs\OpenAI\Codex\bin\codex.exe` → `codex` (`src/exe.rs:26-50,149-160`) |
| destination options | `--dest-version`, `--model-provider`, `--model`, `--dest-shell bash\|sh\|zsh\|powershell\|pwsh` (`src/cli.rs:26-70`) |
| resume command in receipt | `[<exe or "codex">, "exec", "resume", <new id>]` (`src/writer/codex.rs:64-73`) |
| compaction (CLI) | `--compaction none` (default) or `portable`; `DestinationNative` is refused by the library (`src/hop.rs:353-358`) |

**Reader** (`src/reader/codex.rs`) [22]:

| source record | read as | lines |
|---|---|---|
| store | `list` walks `sessions/` for `*.jsonl`, one entry per thread at its latest file; `read <id>` matches an id **prefix** of the thread uuid in the file name (none → `NotFound`, several threads → `Ambiguous`; several files of **one** thread → its latest page) | `src/reader/codex.rs` `list`, `read_with`, `rollout_id` |
| paginated thread | a page with `history_base` is read with the base file's first `end_ordinal_exclusive` records (checked against `end_byte_offset`), then its own records; the base's conversation records past the cut become `Meta::OffBranch` and are counted (`dropped_off_branch`). `--all-branches` (`Branches::All`) reads the base file whole instead, then the page. A missing base, an offset that does not fall at the end of that record, or a base that is itself a page → error with the reason. The fingerprint covers the bytes read (the base's prefix, or all of it, then the page) | `read_file_with`, `base_prefix` |
| line order | file order, never timestamps; a cut-off last line → `Meta::Unfinished`; a bad line elsewhere fails with `file:line` | `src/reader/json.rs:11-40` |
| user `message` | `User`, unless its text, after leading whitespace, **starts with** `<environment_context>`/`<skills_instructions>` (→ `Meta::SystemPrompt`) or `<turn_aborted>` (→ `Meta::TurnAborted`); a prompt that only mentions a tag stays the user's words | `:87-104` |
| `developer`/`system` `message` | `Meta::SystemPrompt` | `:81-88` |
| assistant `message`, `reasoning`, calls | one `Step` per model response; `turn_context` or a new `internal_chat_message_metadata_passthrough.turn_id` starts the next; parallel calls stay together | `:307-330` |
| `reasoning` | `summary` text + `Opaque{openai-responses, model, encrypted_content}`; `content` is not read | `:353-365` |
| `function_call`, `custom_tool_call` | name + arguments (the JSON string parsed) | `:374-393,492-501` |
| `local_shell_call`, `web_search_call` | **read from `action`**: named `local_shell`/`web_search`, input = `action`; a web search's id is its `id` (it has no `call_id`) | `:374-393,476-478` |
| shell dialect | `exec_command`/`local_shell` → `Bash` or `PowerShell` from the call's `shell`, else the `<shell>` of `<environment_context>`, else `Bash` | `:503-531` |
| `function_call_output`, `custom_tool_call_output` | string → text; **array (images, MCP content) → structured blocks kept** (`input_text`/`output_text` → `{"type":"text"}`, other items as recorded); `is_error` always `false` | `:399-433,481-489` |
| `compacted` | `Compaction{summary: message, kept_from: next non-Meta entry, trigger: Unknown, tokens_before: latest_token_usage_record.usage.input_tokens}`; the step of the same `turn_id` that wrote the summary is folded in; `replacement_history` stays in `raw` only. An empty `message` (0.165's encrypted summary) is never written as an empty user message: the plan leaves it out and counts it (`dropped_unreadable_summaries`), and `--compaction portable` is refused, exit 2 | `:271-300,435-455`; `src/writer/plan.rs`, `src/hop.rs` `select` |
| `event_msg`, `turn_context`, `world_state`, `token_usage_record` | `Meta`, kept raw; models from `event_msg/thread_settings_applied` | `:69-80,195-207` |
| `session_meta` | id (`session_id`, else `id`), `cwd`, `cli_version` (harness version), `git.branch`; created/updated from the timestamps; **title and parent always empty** (`session_index.jsonl` not read, forks not linked) | `:149-181,455-470` |

**Writer** (`src/writer/codex.rs`) [23]:

| writes | detail | lines |
|---|---|---|
| file | new uuid; `<CODEX_HOME>/sessions/YYYY/MM/DD/rollout-YYYY-MM-DDTHH-MM-SS-<uuid>.jsonl`, folder and stamp from one UTC instant; lines `{timestamp, ordinal, type, payload}`, `ordinal` from 0 | `:44-58,77-110` |
| `session_meta` | `base_instructions: null`, `id`, `timestamp`, `cwd`, `originator: "codex_cli_rs"`, `source: "cli"`, `cli_version`, `model_provider` (when known), `git.branch`; **no `session_id`** | `:112-136` |
| environment | user message `<environment_context>` with `<cwd>` and, from `--dest-shell`, `<shell>bash\|powershell</shell>` | `:138-161` |
| each step | `turn_context {turn_id:"turn-N", model, cwd}`, native `reasoning`, assistant `message` (`output_text`), `function_call {name, arguments (JSON string), call_id}` | `:189-251` |
| each result | `function_call_output {call_id, output}` as text; a text-form call's result is a user message in the same place | `:252-273` |
| `portable` summary | one user message | `:180-188` |
| not written | `event_msg` records, a `session_index.jsonl` line; **`state_5.sqlite` is never opened**, so no stale `threads` row (L7) | `:10-13` |
| `cli_version` (**required**) | Codex refuses a rollout whose `session_meta` has no `cli_version`, so the write fails without one: ``Codex needs the destination's cli_version (`--dest-version`): it refuses a rollout whose session_meta has none``. The CLI takes `--dest-version`, else the first `x.y.z` token of `<exe> --version` (`codex-cli 0.164.0` → `0.164.0`); pass `--dest-version` when that probe fails [25] | `:48-54`; `src/cli.rs:462-468,528-546` |
| `model_provider` | `--model-provider`, else the top-level `model_provider = "…"` of `$CODEX_HOME/config.toml`: `codex_config_provider`/`top_level_string` read only keys before the first `[table]`, match the key exactly and drop a trailing `# comment`; `[profiles.x]` and dotted `model_providers.x.*` are ignored | `src/cli.rs:646-693` |
| reasoning | default `drop_foreign`: only `openai-responses` blocks that still carry `encrypted_content` are written natively; others are dropped and counted (or kept as text with `as_text`). Reasoning without `encrypted_content` against the real API is open (R3) **[U]** [19 §10] | `src/policy.rs:24-46` |
| results | structured blocks are flattened to their text; non-text blocks are dropped and counted in `dropped_result_blocks` | `src/writer/plan.rs:209-221,377-388` |
| tools | offered set `exec_command`, `write_stdin`, `request_user_input`, `view_image`, `get_goal`, `create_goal`, `update_goal`, `web_search`. Only shell calls translate, as `exec_command {cmd, workdir, justification}` (`timeout` dropped; background → text form; dialect must match `--dest-shell`). **Codex has no read or edit tool in the pinned bench**, so `Read`, `Write`, `Edit`, `Grep`, `Glob` travel as text [21 §3] | `src/tools.rs:71-81,150-165,368-450,659-665,813-835` |

**Known limits:**

- **Code mode is not parsed yet.** Requirement L10 (read a one-call `exec` script as that call)
  is specified [19 §8.1][21 §5] but is not in `src/`. An `exec` call reads as
  `CanonicalTool::Other("exec")` and travels as text form in every direction, Codex → Codex
  included (`src/model/tool.rs:97-107`, `src/tools.rs:178-183`). So most Codex Desktop calls arrive
  as text.
- **Codex `apply_patch` calls** read as canonical `Edit`, but their patch input has no path or
  `old_string`, so they become text form everywhere (`src/tools.rs:595-619`).
- **No native compaction record.** A `portable` hop writes the summary as a user message, not a
  `compacted` record, even though Codex accepts `compacted{message}` (P6). That form waits for a
  fixture [19 §4.4].
- **Not converted:** subagent threads, forks, names from `session_index.jsonl`, and
  compressed/migrated rollouts (only `*.jsonl` files are listed) **[U]** [19 §10].
- **Subagent messages are not carried.** `agent_message` records (a subagent writing to the root
  agent; the payload is encrypted) read as `Meta` and are not counted in the manifest. 223 of them
  were in the 0.165 thread seen [26].
- **A chain of pages** (a page whose base is itself a page) is refused with its reason; only one
  page over a base file has been seen [26].
- **Resume:** keep `CODEX_HOME` set for the resume process and run it from the session's cwd
  [19 §5][25].

## 9. Sources

[26] Two real Codex Desktop 0.165.0-alpha.1.2 threads of the Daodan project, read on
2026-10-28 with their `state_5.sqlite` and `thread_history_1.sqlite` rows (read only): thread
`01a181e2-…` (17.8 MB, 5 compactions, all with an empty `message`) and thread `01a181df-…` (a base
file plus one page, `history_base.end_ordinal_exclusive` 511 of 1064). Not in the repository: they
are the user's own conversations.

All accessed 2026-10-28.

1. OpenAI, *openai/codex* README, https://github.com/openai/codex
2. OpenAI, *Codex CLI*, https://learn.chatgpt.com/docs/codex/cli (redirect from https://developers.openai.com/codex/cli)
3. OpenAI, *Non-interactive mode*, https://learn.chatgpt.com/docs/non-interactive-mode
4. OpenAI, *Configuration reference*, https://learn.chatgpt.com/docs/config-file/config-reference (redirect from https://developers.openai.com/codex/config-file/config-reference)
5. OpenAI, *Sandboxing*, https://learn.chatgpt.com/docs/sandboxing (redirect from https://developers.openai.com/codex/sandboxing)
6. OpenAI, *Models*, https://learn.chatgpt.com/docs/models (redirect from https://developers.openai.com/codex/models)
7. OpenAI, *App server*, https://learn.chatgpt.com/docs/app-server (redirect from https://developers.openai.com/codex/app-server)
8. OpenAI, *MCP*, https://learn.chatgpt.com/docs/extend/mcp (redirect from https://developers.openai.com/codex/extend/mcp)
9. OpenAI, *AGENTS.md*, https://learn.chatgpt.com/docs/agent-configuration/agents-md (redirect from https://developers.openai.com/codex/agent-configuration/agents-md)
10. OpenAI, *Build skills*, https://learn.chatgpt.com/docs/build-skills
11. OpenAI, *Skills and plugins*, https://learn.chatgpt.com/docs/skills-and-plugins (redirect from https://developers.openai.com/codex/skills-and-plugins)
12. OpenAI, *Plugins*, https://learn.chatgpt.com/docs/plugins
13. OpenAI, *Hooks*, https://learn.chatgpt.com/docs/hooks (redirect from https://developers.openai.com/codex/hooks)
14. OpenAI, *Subagents*, https://learn.chatgpt.com/docs/agent-configuration/subagents (redirect from https://developers.openai.com/codex/subagents)
15. OpenAI, *Slash commands* (Desktop app), https://learn.chatgpt.com/docs/reference/slash-commands
16. OpenAI, *Release 0.164.0* (tag `rust-v0.164.0`, 23 Oct), https://github.com/openai/codex/releases/tag/rust-v0.164.0
17. npm registry metadata for `@openai/codex` (`npm view @openai/codex version license time`: latest 0.165.0, 0.164.0 published 2026-10-23, licence Apache-2.0), https://www.npmjs.com/package/@openai/codex
18. Installed binary `C:\Users\alfio\AppData\Local\Programs\OpenAI\Codex\bin\codex.exe`: `codex --version` (`codex-cli 0.164.0`); `--help` of `codex`, `exec`, `exec resume`, `exec fork`, `resume`, `fork`, `login`, `mcp`, `mcp add`, `plugin`, `plugin add`, `app-server`, `app`, `agents`, `queue`, `sandbox`, `features`, `debug`
19. Repository: [`docs/session-interchange-spec.md`](../session-interchange-spec.md) §1.3, §2.2, §3, §4, §5, §8, §10
20. Repository: [`docs/reference/harness-session-persistence-reference.md`](../reference/harness-session-persistence-reference.md), Codex dossier (lines 83–105)
21. Repository: [`docs/tool-conversion-tables.md`](../tool-conversion-tables.md) §2–§6
22. Repository: [`src/reader/codex.rs`](../../src/reader/codex.rs), `src/reader/json.rs`
23. Repository: [`src/writer/codex.rs`](../../src/writer/codex.rs), `src/writer/plan.rs`, `src/policy.rs`, `src/tools.rs`, `src/model/tool.rs`, `src/hop.rs`
24. Repository: [`src/exe.rs`](../../src/exe.rs), [`src/cli.rs`](../../src/cli.rs) (`codex_config_provider`, `top_level_string`, `destination_version`), `src/detect.rs`
25. Repository: [`README.md`](../../README.md), [`docs/usage.md`](../usage.md) §2
