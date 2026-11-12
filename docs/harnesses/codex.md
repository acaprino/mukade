# Codex CLI

OpenAI's coding agent for the terminal: inspects a repository, edits files, runs commands, reviews
changes [1][2]. The IDE extension (an app-server client [7]) and the Codex Desktop app (same rollout
format [21]) share its `config.toml` [6][8]. mukade (CLI name `codex`) lists, dumps, hops, summarizes
with Codex's own model and compacts natively.

## Status in mukade

| | |
|---|---|
| Read (`list`, `dump`) | ✅ CLI and Desktop rollouts, archived and paginated threads, threads that survive only as a `state_<n>.sqlite` row (listed `unreadable`); subagent and guardian threads with `list --children` |
| Write | ✅ a new rollout under `sessions/`; needs the destination's `cli_version` |
| Hop as source | ✅ Desktop code-mode `exec` scripts split into calls (725 of 981 [50]); left out: encrypted summaries, encrypted messages between agents (counted), subagent threads (each read on its own), forks, compressed rollouts |
| Hop as destination | ⚠️ only shell calls become `exec_command`, those of the dialect Codex runs (PowerShell on Windows, POSIX elsewhere); the rest travels as text; no patch form |
| Resume | ✅ `codex exec resume <id>` (receipt) or `codex resume <id>`; live-verified on 0.166.0 [48] |
| Summary by its own model | ⚠️ `--compaction summary` runs `codex exec fork … --ephemeral`; checked against a mock only **[U]** [27] |
| Native compaction | ✅ `--compaction native` appends a `compacted` record; live-verified 2026-11-04 [19 §4.4] |
| Versions | researched and readers verified on 0.164.0; writers live-tested on the installed 0.166.0 [48][49]; CLI 0.165.0 (published 2026-10-27) never exercised [17]; Desktop rollouts read up to 0.164.0-alpha.11.1 [21], 0.165.0-alpha.1.2 [26], 0.167.0 / 0.167.0-alpha.3.2 / 0.167.1 [50] |

`[n]` = an entry in [Sources](#sources). **[U]** = not verified. Repository paths are relative to
the repository root. Page updated 2026-11-12.

## Quick start

```powershell
# 1. Name the store (never assumed; an empty variable is no store).
$env:CODEX_HOME = Join-Path $HOME '.codex'        # bash: export CODEX_HOME="$HOME/.codex"

# 2. List threads (archived included; --archived false hides them).
mukade list codex

# 3. Dump one (id prefix is enough; so is the id of a codex://threads/<id> link).
mukade dump codex <thread-id> --output 'C:\work\thread.json'

# 4. Hop out of Codex.
$env:CLAUDE_CONFIG_DIR = Join-Path $HOME '.claude'
mukade hop codex <thread-id> claude_code --cwd 'C:\work\my-project'

# 5. Hop into Codex. Shell calls of the dialect Codex runs stay calls.
mukade hop claude_code <session-id> codex --cwd 'C:\work\my-project'

# 6. Resume, from the session's cwd, with CODEX_HOME still set.
codex exec resume <new-id> "<prompt>"             # interactive: codex resume <new-id>
```

More: [usage](../usage.md#3-commands), [Codex CLI](../usage.md#codex-cli).

| when | add |
|---|---|
| `codex --version` fails | `--dest-version 0.166.0` (your version) |
| redone later turns | `--all-branches` |
| long thread, encrypted summary | `--compaction summary` (`--intent <name>`, `--keep-turns <n>`) or `native --context-window <n>` |
| leave out what interrupted turns did | `--interrupted drop` |
| custom Codex | `--codex-exe <path>`, `--root <dir>` (source), `--dest-root <dir>` (destination) |

## Install and locate

**Install** [1][2]

| OS | method | command |
|---|---|---|
| macOS, Linux | install script | `curl -fsSL https://chatgpt.com/codex/install.sh \| sh` |
| Windows | install script (PowerShell) | `powershell -ExecutionPolicy ByPass -c "irm https://chatgpt.com/codex/install.ps1 \| iex"` |
| any (Node.js) | npm | `npm install -g @openai/codex` |
| macOS | Homebrew | `brew install --cask codex` |
| macOS, Linux | GitHub Releases archive | `codex-aarch64-apple-darwin.tar.gz`, `codex-x86_64-apple-darwin.tar.gz`, `codex-x86_64-unknown-linux-musl.tar.gz`, `codex-aarch64-unknown-linux-musl.tar.gz`; rename the binary to `codex`. Windows archive name **[U]** |

- The scripts download from `https://releases.openai.com/codex`. Force GitHub Releases with
  `CODEX_INSTALLER_USE_RELEASES_OPENAI_COM=false` (Windows:
  `$env:CODEX_INSTALLER_USE_RELEASES_OPENAI_COM='false'; irm https://chatgpt.com/codex/install.ps1 | iex`) [1].
- Update: re-run the script, or `codex update` [2][18].
- Sign in: run `codex`, choose **Sign in with ChatGPT**; or `codex login --with-api-key`, or
  `CODEX_API_KEY=<key>` for `codex exec`; `--device-auth` too [1][2][3][18].

**Where it lives**

| what | Windows | macOS / Linux |
|---|---|---|
| native-install launcher | `%LOCALAPPDATA%\Programs\OpenAI\Codex\bin\codex.exe`: the binary used for this page, and the installer's location in the catalog [18][24] | `~/.local/bin/codex` in the catalog; the script uses `${CODEX_INSTALL_DIR:-$HOME/.local/bin}/codex` (`CODEX_INSTALL_DIR` moves it), linked to selected release [28] |
| native-install releases | — | `$CODEX_HOME/packages/standalone/releases/`, selected through `current/` [28] |
| default store | `%USERPROFILE%\.codex` | `~/.codex` |
| variable that moves the store | `CODEX_HOME` [9][20] | `CODEX_HOME` |

Homebrew and npm installs are found through `PATH` and their manager (`brew --prefix`,
`npm prefix -g`), then `codex --version`; `PATH` order decides among several [28]. Per-OS procedure
and selecting a non-default installation:
[harness catalog](../reference/harness-catalog.md#finding-existing-installations).

**How mukade finds it** (resolution order and env vars: [usage §7](../usage.md#7-stores-and-executables))

| what | how |
|---|---|
| store | `CODEX_HOME`, required by `list`, `dump`, `write`, `hop` (`src/roots.rs` `StoreRoots`). `--root <dir>` names a source store and lets mukade infer the harness; `--dest-root <dir>` the destination store or installation |
| executable | `--codex-exe` → `MUKADE_CODEX_EXE` → `PATH` → the catalog's launchers ([`catalog/harnesses.json`](../../catalog/harnesses.json)) → `codex` (`src/exe.rs` `resolve()`) |
| `mukade harnesses` | the one executable it resolves, its version, the store (`CODEX_HOME`, else catalog `~/.codex`, read-only), session and fork counts, lineage |
| `mukade detect <dir>` | `sessions/` holding `rollout-*.jsonl` files (4 levels down), `session_index.jsonl`; a `state_*.sqlite` is not taken for OpenCode (`src/detect/codex.rs` `CodexLayout`) |
| `mukade scan <dir>` | an executable beside the store: `codex.exe` or `bin/codex.exe` (Windows), `codex` or `bin/codex` |

**Ways to run it** (`codex --help` 0.164.0 [18])

| entry | command | notes |
|---|---|---|
| interactive TUI | `codex [PROMPT]` | `--search` selects `live` `web_search`, already on in `cached` mode without it [29][35][47]; `--no-alt-screen` runs inline [18] |
| non-interactive | `codex exec "<prompt>"` (alias `codex e`) | repeatable, automated work [2][3]. `--json` prints JSONL events (`thread.started`, `turn.*`, `item.*`, `error`); `-` or piped stdin accepted; `--ephemeral` writes no rollout [3][18]. `--search` is not an `exec` flag: per run, `-c web_search="live"` [43][47] |
| resume | `codex resume [SESSION_ID] [PROMPT]`; `codex exec resume <SESSION_ID> "<prompt>"`, `codex exec resume --last "<prompt>"` | TUI: picker by default, `--last`, `--all`, `--include-non-interactive`. `exec`: id or thread name, appends to the same rollout [3][18][19] |
| fork | `codex fork [SESSION_ID] [PROMPT]`, `codex exec fork <SESSION_ID> [PROMPT]` | a new session from an old one [18] |
| app server | `codex app-server` (experimental) | JSON-RPC 2.0 over stdio; `--listen ws://IP:PORT` and `unix://` too. Methods include `thread/start`, `thread/resume`, `thread/fork`, `thread/compact/start`, `turn/start`, `turn/interrupt` [7][18] |
| remote TUI | `codex --remote ws://host:port` | the TUI attached to an app server [7][18] |
| Desktop app, IDE | `codex app [PATH]`; VS Code, Cursor, Windsurf extension | `app` opens the installer if the app is missing [1][18]; the extension is an app-server client, `clientInfo.name` `"codex_vscode"` [1][7] |
| other | `codex review`, `codex agents`, `codex queue --thread <THREAD> --message <TEXT>`, `codex cloud`, `codex archive/unarchive/delete`, `codex migrate-rollouts` | [18] |

**Vendor, site, licence**

| | |
|---|---|
| vendor | OpenAI; open development in `openai/codex`, Rust workspace `codex-rs` [1][7] |
| documentation | https://developers.openai.com/codex [2]; since 2026-10-28 its pages redirect (308) to `learn.chatgpt.com/docs/…`, which the Sources cite |
| source, releases | https://github.com/openai/codex [1][16] |
| licence | Apache-2.0: repository [1], npm package `@openai/codex` [17]; Codex Desktop app **[U]** |

## Session store

Root `CODEX_HOME`, `~/.codex` by default [9][20]. Seen on disk with 0.164.0 unless cited [19 §2.2].

**Layout**

| path | holds |
|---|---|
| `sessions/YYYY/MM/DD/rollout-<local ts>-<uuid>.jsonl` | one thread; lines `{timestamp, ordinal, type, payload[, metadata]}`. A rollout alone resumes by id, even from the wrong date folder |
| `sessions/YYYY/MM/DD/rollout-<local ts>-<uuid>_<page uuid>.jsonl` | a later **page** of a paginated thread (below) |
| `archived_sessions/` | the rollouts `codex archive` moved out of `sessions/` |
| `state_5.sqlite` | thread index. **`threads.rollout_path` wins over the file on disk**: a row pointing at a missing file fails with `no rollout found for thread id … (code -32600)`. The row outlives the rollout: on the machine of 2026-11-08 all 8598 rows pointed at deleted files [51] |
| `thread_history_1.sqlite` | exists; the model context comes from the rollout, not from here |
| `session_index.jsonl` | append-only name catalogue `{id, thread_name, updated_at}`; the newest name wins [20]. Not written in the bench's scratch home [19] |
| `history.jsonl` | transcript history: `history.persistence = "save-all" \| "none"`, `history.max_bytes` [4] |

**Record types** (`type`, then `payload.type`) [19][22]

| `type` | payloads seen | role |
|---|---|---|
| `session_meta` | `id` (the rollout's own thread), `session_id` in native rollouts [22] (the same id, except in a subagent's rollout, where it names the spawning thread [26]), `cwd`, `originator`, `source`, `cli_version`, `model_provider`, `git`, `base_instructions`; a subagent's (0.167 [53]): `parent_thread_id`, `thread_source` (`subagent`, `guardian_review`), `agent_role`, `agent_nickname`, `agent_path` | thread header; Codex refuses a rollout without `cli_version` [23] |
| `response_item` | `message` (`user`/`assistant`/`developer`), `reasoning` (`summary`, `encrypted_content`), `function_call`/`function_call_output`, `custom_tool_call`/`custom_tool_call_output`, `local_shell_call`, `web_search_call` | the conversation |
| `event_msg` | `task_started`, `task_complete`, `turn_aborted {reason:"interrupted"}`, `thread_settings_applied` | events about the conversation |
| `turn_context` | `turn_id`, `model`, `cwd` | starts a turn |
| `compacted` | see [Compaction](#compaction) | replaces history |
| `world_state`, `token_usage_record` | — | events, not conversation |

Also in 0.165 Desktop rollouts [26]: `response_item` `agent_message` (a message from one agent of
the spawn tree to another: `author`, `recipient`, `content` = an `input_text` header and usually an
`encrypted_content` block; plain text only in 55 of 320 seen [53]), `inter_agent_communication_metadata`,
`token_usage_record`, `world_state`, `event_msg/item_completed`.

**Ordering** [19]

- Parallel calls: consecutive `function_call` lines, then their outputs, paired by `call_id`.
- Graceful interrupt: a user message `<turn_aborted>…</turn_aborted>` plus `event_msg turn_aborted`;
  hard kill: the file just ends.
- `thread/list` hides `exec` threads by default.

**Paginated threads** (Codex Desktop 0.165.0-alpha.1.2, 2026-10-28 [26])

- `session_meta.history_mode` is `"paginated"`. When a thread's later turns are redone, Codex starts
  a page `rollout-<ts>-<thread>_<page>.jsonl`, whose first record is a `session_meta` with
  `history_base: {thread_id, end_ordinal_exclusive, end_byte_offset}`.
- The page continues from the first `end_ordinal_exclusive` records of the earlier file (exactly
  `end_byte_offset` bytes); its own ordinals carry on from there.
- `state_5.sqlite` `threads.rollout_path` names the page: what Codex resumes.
- The earlier file's records past the cut stay on disk: the branch the thread left. In the thread
  seen, the base held 1064 records and the page continued from its first 511.

**Compressed rollouts.** 0.164.0 searches and restores them [16]; `codex migrate-rollouts` migrates
legacy sessions to "paginated thread history" [18]. Their on-disk form **[U]**.

**Lineage**

- Subagent: its own rollout. 0.167 [53]: `session_meta.parent_thread_id` and
  `source.subagent.thread_spawn {parent_thread_id, depth, agent_path, agent_nickname, agent_role}`
  name the spawning thread (`session_id` too, at depth 1 [26]); a guardian review:
  `source: {"subagent": {"other": "guardian"}}`, `thread_source: "guardian_review"`. Codex's state
  database links spawns in `thread_spawn_edges` (not guardian reviews) and keeps `agent_role`.
- Fork: how a forked rollout records its parent **[U]**.
- `mukade harnesses` prints lineage `not recorded` for Codex from `catalog/harnesses.json`
  (`lineage.recorded: false`: forks), not from the reader, which does set a subagent's parent.

## Features

### Compaction

- **Triggers:** automatic at `model_auto_compact_token_limit` [4]; `/compact` in the Desktop app [15]
  (CLI TUI **[U]**); app-server `thread/compact/start` [7][19]. `codex exec` has no compaction
  command [19].
- **Headless:** `thread/compact/start` over stdio (`initialize`, `initialized`,
  `thread/resume {threadId,cwd}`, `thread/compact/start {threadId}`, then wait for item
  `contextCompaction` and `turn/completed`); the `compacted` record lands in the compacted thread
  [7][19].
- **Local** by default: an ordinary `/v1/responses` call with `request_kind:"compaction"` and the
  prompt `You are performing a CONTEXT CHECKPOINT COMPACTION…` [19]. `compact_prompt` and
  `experimental_compact_prompt_file` override the prompt;
  `model_providers.<id>.capabilities.remote_compaction = "v2"` sends the trigger through the
  Responses API [4].
- **Record:** `compacted.payload = {message:"Another language model started … <summary>",
  replacement_history:[…], replacement_history_metadata, retained_context, window_number, window ids,
  compaction_response_id, latest_token_usage_record, resume_metadata}`. A line reduced to
  `{message}` still resumes [19].
- **On resume** the model receives the base developer message, then `replacement_history`, later
  items and the prompt. No `encrypted_content` is sent [19].
- **0.165 Desktop keeps the summary encrypted** [26]: in every `compacted` record seen, `message` is
  empty and the summary is an item `{type:"compaction", id, encrypted_content}` (77 000 to 99 000
  characters) at the end of `replacement_history`, after the retained user messages. Only OpenAI can
  read it: no summary text to carry to another harness.

### Ephemeral runs

- **`--ephemeral`** (`exec`, `exec resume`, `exec fork`): "Run without persisting session files to
  disk" [18]. It stops a *new* rollout only: `exec resume <id> --ephemeral` still appends the turn to
  the rollout it resumes (11 records, against a mock) [27].
- **`codex exec fork <id> --ephemeral`** starts from the stored history and leaves `CODEX_HOME`
  unchanged: rollouts, `session_index.jsonl`, the SQLite files and their WAL [27] (mock). The request
  carries the whole source conversation, an unanswered last user message included [27].
- **Output:** `-o, --output-last-message <file>` writes the last agent message; `--json` prints the
  events [18]. `exec` reads stdin when left open (`Reading additional input from stdin...`) [27].
- **Deleting a session without a terminal** needs `codex delete --force <id>`; without `--force`:
  `cannot confirm session deletion without an interactive terminal` [27].

### Tools

**Offered by 0.164.0 `exec`** in the bench, unknown model, default settings [21 §2]:

| tool | purpose |
|---|---|
| `exec_command` | runs a shell command (`cmd`, `workdir`, `justification`, `yield_time_ms`) [21 §4.1]: on Windows always PowerShell (`pwsh`, then `powershell`, …, `cmd` last); elsewhere the login shell when it is zsh, bash or sh, else the first of those found; no configuration key. Recorded as `<environment_context><shell>` (`zsh`, `bash`, `sh`, `powershell`, `cmd`) [52] |
| `write_stdin` | writes to a running `exec_command` session [21 §4.1] |
| `request_user_input` | asks the user (canonical `AskUserQuestion`) [21 §3] |
| `view_image` | image viewing; purpose from the name only **[U]** |
| `multi_agent_v1` (namespace) | subagent tools; the stores show `spawn_agent`, `send_message`, `wait_agent` |
| `get_goal`, `create_goal`, `update_goal` | the thread's persistent goal (`/goal`) [15] |
| `web_search` | hosted Responses tool, on in `cached` mode by default ([Web tools](#web-tools)) [18][21 §3][29][35] |

**Other tools**

| tool | status |
|---|---|
| `apply_patch` | patch-based file edits; **not offered** to the bench's unknown model, used by Codex Desktop [21]. Registered only when the model's catalog sets `apply_patch_tool_type`; no feature switch removes it (`apply_patch_freeform` is `removed`) [38][47]. How far `-s read-only` bounds its writes **[U]** (one `codex exec -s read-only` run asked to edit would show it) |
| `web.run` | a second, standalone web tool ([Web tools](#web-tools)); the Desktop census holds 53 `web__run` calls [21 §2] |
| `local_shell_call` | an older record type, read by mukade from its `action` [22] |
| `multi_agent_v1` / `multi_agent_v2` | the source at the tag puts `spawn_agent`, `send_input`, `resume_agent`, `wait_agent`, `close_agent` in `multi_agent_v1` [46], and `send_message`, `followup_task`, `interrupt_agent`, `list_agents` in `multi_agent_v2`, off by default [38]. Which of the two the stores' `send_message` calls come from **[U]** (the namespace on one such call would settle it) |
| gated in `core/src/tools/spec_plan.rs` | not among the nine offered: `update_plan`, `list_mcp_resources`, `list_mcp_resource_templates`, `read_mcp_resource`, `request_permissions`, `sleep`, `image_gen.imagegen`, `tool_search`, the plugin-install tools, the code-mode `exec` and `wait`, the `multi_agent_v2` tools [38]. MCP servers' tools are not listed |

**No read tool.** `core/src/tools/handlers` at the tag has no `read_file`, `list_dir` or
`grep_files` handler [45]; files are read through `exec_command`, images through `view_image`
[21 §3].

**Codex Desktop "code mode."** Desktop rollouts (`originator: codex_work_desktop`) hold almost every
call as **one custom tool `exec`** whose input is a script such as
`text(await tools.exec_command({cmd:S, yield_time_ms:N}));` or `text(await tools.apply_patch(S));`:
4,844 of 4,908 Codex calls in the census [21 §5]; the 0.164.0 app server also has
`--code-mode-host <URL>` [18]. Rule: [tool conversion §5](../tool-conversion-tables.md#5-reading-codex-desktop-code-mode-exec);
limits: [How mukade reads it](#how-mukade-reads-it).

#### Web tools

- **Hosted `web_search`** (item `web_search_call`) [33][39]
  - Config `web_search` = `disabled`, `cached`, `indexed`, `live`; default `Cached`, `live` under a
    full-access sandbox such as `--yolo` [29][30][35]. Per run `-c web_search="live"` [47].
    `disabled` removes the tool [39] and `web.run` [38].
  - `cached`: "an OpenAI-maintained index without external web access" (the query still goes to
    OpenAI); `live`: "unrestricted live retrieval" [29]. Runs at the provider's side, "separate from
    sandboxed local command networking" [30]. The engine behind `live` is not named **[U]** (a
    provider statement settles it).
- **`exec --json` item** `web_search` [32] ("web searches" is a named item type [31]):
  - `query`: the query; several → the first plus ` ...`; the URL for `open_page`;
    `'pattern' in url` for `find_in_page`; empty for `other` [37].
  - `action.type` = `search`, `open_page`, `find_in_page` or `other`, with `action.query` or
    `action.queries[]`, `action.url`, `action.pattern` [33].
  - Page reading is the same item; a switch closing it while keeping search was not found **[U]** (the
    unread part of the configuration reference [29] could settle it). Which `id` the JSON carries **[U]** (two flattened structs declare `id`) [32]; one
    `item.completed` line would show it.
- **Result URLs** are not in the hosted tool's stream: `results` is `None` [36], `WebSearchAction`
  has no `sources` field [33], the request `include` list does not ask for them [44]. `results` is
  optional opaque JSON filled by the standalone tool [34][41]. Whether it holds URLs **[U]**:
  `exec --json --ephemeral -s read-only --enable standalone_web_search -c web_search="live"` with a
  search prompt, giving an `item.completed` `web_search` with non-empty `results`, would settle it.
- **Standalone `web.run`** (namespace `web`, function `run`; commands `search_query`, `image_query`,
  `open`, `find`) [41]
  - Calls a remote search API with the provider's login, endpoint **[U]**; nothing runs locally [41].
  - Registered when the provider supports web search, the mode is not `disabled`, and the model uses
    `use_responses_lite` or feature `standalone_web_search` is on [38] (`under development`, default
    `false`, `--enable standalone_web_search` [42][47]). Which models use `use_responses_lite`
    **[U]** (a model's catalog entry would show it).
  - `item.started` carries `query: ""`, `action: other`; data arrives at `item.completed`; an `open`
    by result reference (`turn0search0`) reports no URL [41]. That these items reach `exec` as
    `web_search` items **[U]**: only the mapping `results: item.results` was read [34][40]; the probe
    above would show it.
- **Domain allowlist** `[tools.web_search] allowed_domains = ["example.com"]` (also `context_size`,
  `location`): separate from sandbox network rules, not restricting connectors or MCP servers [29];
  passed as `filters` into the hosted spec [39]. Whether it binds `open_page` and `open`, or limits
  opening to URLs seen in the turn's results **[U]**: a standalone run with
  `-c tools.web_search.allowed_domains=["example.com"]` asked to open another host would settle it.

#### Sandbox, approvals and switches that remove tools

| switch | effect |
|---|---|
| `-s/--sandbox` or `sandbox_mode` | `read-only`, `workspace-write`, `danger-full-access`; Seatbelt on macOS, bubblewrap (`bwrap`) on Linux and WSL2, the native Windows sandbox in PowerShell [4][5][18]. `codex exec` defaults to `read-only` [3]; `-c sandbox_mode="read-only"` bounds what a call can do [18][27] |
| `-a/--ask-for-approval` | `on-request \| never` [18]; `untrusted` retired, `on-failure` deprecated [4][5]. `--approve-for-me` routes eligible requests to automatic review (`approvals_reviewer = "auto_review"`) [5][18]; `--dangerously-bypass-approvals-and-sandbox` removes both [18] |
| `--disable shell_tool` | = `-c features.shell_tool=false`: removes `exec_command` and `write_stdin`; the source registers no shell tool with the feature off [38][47]. Not read-only: bound it with `-s read-only` |
| `--disable view_image`, `--disable multi_agent` | close two more groups [38][47] |
| `-c web_search="disabled"` | removes the web tools |

What a real run offers after these switches **[U]** (reading `tools` in the request body of
`codex exec` against a local mock Responses server would settle it).

### Branches, forks and subagents

- **Fork.** `codex fork` and `codex exec fork <SESSION_ID>` start a new session from a previous one
  [18]; app-server `thread/fork` copies the stored history into a new thread id [7]. Parent record: see Lineage ([Session store](#session-store)).
- Redone turns make a paginated thread (same section).
- **Subagents** are on by default (`agents.enabled = true`), spawned on request or when an AGENTS.md
  or skill asks for delegation [14]. `/subagents` switches between agent threads in the CLI.
  - Custom agents: one TOML file per agent in `~/.codex/agents/` or `.codex/agents/`, with `name`,
    `description`, `developer_instructions`. Built-in: `default`, `worker`, `explorer`.
  - Cap: `agents.max_concurrent_threads_per_session` (legacy alias `agents.max_threads`).
  - On disk (Desktop): each subagent is its own rollout, 971 of the 982 scanned [26]. The CLI's
    persistence of subagent threads **[U]**.

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
Responses API only [2][4][6].

- **Models:** recommended `gpt-6-astra`, `gpt-6.1-sol`, `gpt-6.1-terra`, `gpt-6-luna`, chosen with
  `/model`, `-m/--model` or `model = "…"` [6]. With a ChatGPT account, 0.166.0 refuses `gpt-5.1`,
  `gpt-5.1-codex`, `gpt-5.1-codex-max` [49].
- **Local provider:** `--oss --local-provider lmstudio|ollama` [18].
- **Profiles in 0.164.0** are separate files: `-p/--profile <name>` layers
  `$CODEX_HOME/<name>.config.toml` on the base config; a project-local `.codex/config.toml` ignores
  `profile`/`profiles` [4][18]. Whether older inline `[profiles.<name>]` tables are still read
  **[U]**.
- **Per run:** `-c key=value` overrides any key (TOML-parsed); `--strict-config` rejects unknown keys
  [18].

### Plugins and extensions

| kind | install and configure |
|---|---|
| MCP servers | `codex mcp add <NAME> [--env K=V] -- <COMMAND>...` (stdio) or `codex mcp add <NAME> --url <URL> [--bearer-token-env-var VAR]` (streamable HTTP); `codex mcp list\|get\|remove\|login\|logout` [8][18]. Stored as `[mcp_servers.<name>]` (`command`, `args`, `env`, `env_vars`, `cwd` / `url`, `bearer_token_env_var`, `http_headers`; `startup_timeout_sec` 10, `tool_timeout_sec` 60, `enabled`, `required`, `enabled_tools`, `disabled_tools`) in `~/.codex/config.toml` or a trusted project's `.codex/config.toml`; OAuth: `codex mcp login <name>` [8] |
| AGENTS.md | global `$CODEX_HOME/AGENTS.override.md`, else `AGENTS.md`; then one file per directory from the project root down to the cwd (`AGENTS.override.md`, `AGENTS.md`, `project_doc_fallback_filenames`), concatenated root first, up to `project_doc_max_bytes` (32 KiB); `/init` writes a scaffold [2][9] |
| skills | a folder with `SKILL.md` (frontmatter `name`, `description`) in `.agents/skills` (cwd up to the repo root), `$HOME/.agents/skills`, `/etc/codex/skills`, or the bundled system skills; used with `$<skill>`, installed with `$skill-installer <name>`, created with `$skill-creator`; disabled with `[[skills.config]]` `path` + `enabled = false` [10][11] |
| plugins | bundles of skills, MCP servers, browser extensions and hooks [11][12]; `/plugins` in the TUI [12]; `codex plugin add <PLUGIN[@MARKETPLACE]>`, `codex plugin list\|marketplace\|remove` [18]. mukade's bench sets `[features] plugins = false`: with plugins on, every Codex process contacts chatgpt.com and github.com [19 §2.2] |
| prompts | custom prompts as `/prompts:<name>` (Desktop app) [15]; 0.164.0 adds ten numbered `/presets` slots [16]; prompt file location (`~/.codex/prompts`) **[U]** |

### Hooks

On by default; `[features] hooks = false` disables them (`codex_hooks` is a deprecated alias) [13].
mukade neither reads nor writes hook configuration.

| aspect | detail [13] |
|---|---|
| files | `~/.codex/hooks.json`, `~/.codex/config.toml` `[hooks]`, `<repo>/.codex/hooks.json`, `<repo>/.codex/config.toml`; plugin `hooks/hooks.json`; managed `requirements.toml` `[hooks]`. All matching sources load |
| handlers | `command`, `mcp_tool` (`prompt`, `agent` parsed but skipped); `timeout` in seconds (default 600; `SessionEnd`/`Interrupt` 1, max 3); `async: true` |
| matcher | regex on tool name, compaction trigger, start source (`startup\|resume\|clear\|compact\|fork`) or agent type; `UserPromptSubmit`, `Stop`, `Interrupt` ignore it |
| trust | non-managed hooks run only after review, recorded against their hash (`/hooks`); `--dangerously-bypass-hook-trust` skips the check for one run [18] |
| events | tools `PreToolUse`, `PermissionRequest`, `PostToolUse`; compaction `PreCompact`, `PostCompact`; prompt and turn `UserPromptSubmit`, `Stop`, `Interrupt`; session `SessionStart`, `SessionEnd`; subagents `SubagentStart`, `SubagentStop` |

- 0.164.0 added observation-only model-request hooks and hooks for inter-agent messages [16].
- **`notify`** is a separate key: an array of strings naming a command that receives a JSON payload;
  project-local config ignores it [4]. Which events fire it **[U]**.

## How mukade reads it

Code: `src/reader/codex.rs` [22], `src/reader/codex/exec.rs`, `src/reader/codex_state.rs`,
`src/tools/patch.rs`. Without `CODEX_HOME`: `CODEX_HOME is not set: no Codex store to list` /
`…to read` (`list_in()`, `read_with()`).

**Limits, by cost to you: what is left out or kept as text**

1. **Code mode is read by its literal shape (L10).** Literal `text(await tools.X(...));` scripts
   become those calls; any other (variables, `Promise.allSettled`, `ALL_TOOLS`) stays one `exec`
   call in text form ([tool conversion §5](../tool-conversion-tables.md#5-reading-codex-desktop-code-mode-exec);
   725 of 981 split [50]). A split `exec_command` keeps the rollout's declared shell; every Desktop
   rollout seen declares PowerShell, so its shell calls translate only toward a destination that runs
   PowerShell (Codex on Windows does).
2. **Encrypted compaction summaries (0.165)** ([Compaction](#compaction)). An empty `message` is
   never written as an empty user message: it is left out and counted
   (`dropped_unreadable_summaries`), and `--compaction portable` is refused, exit 2
   (`src/writer/plan.rs` `plan()`; `src/hop.rs` `select()`). The history around it travels.
3. **Encrypted messages between agents are left out.** An `agent_message` with an
   `encrypted_content` block (only OpenAI can read it) stays `Meta(SubagentMessage)`, counted
   (`dropped_subagent_messages`); one with readable text only is a user turn where it was received
   (`class_of()`). The root agent's messages to subagents (`send_message`, `followup_task`,
   `spawn_agent`, `message` encrypted the same way) travel as text form with that value replaced by
   `<encrypted by the source's provider, N chars>` (tables §3; `src/tools.rs` `without_encrypted()`).
4. **Not converted:** a thread's subagent threads (each its own session, read by its id), forks, and
   compressed/migrated rollouts (only `*.jsonl` files are listed) **[U]** [19 §10].
5. **`apply_patch` is read at translate time** (`src/tools/patch.rs` `view()`): one added file is a
   write, one updated file with one hunk an edit, several hunks a multi-edit (Pi only); anything
   else stays text form (rules: same §5).

**Limits: rare shapes**

- **A chain of pages** is read by following each `history_base` back to the thread's first file
  (decided with the user on 2026-10-28: the latest version wins). Tested on hand-made files only:
  of 982 rollouts scanned, one was a page, its base a plain file [26] **[U]**.
- **A Codex that renames the `threads` columns** lists its rollouts as before, without the database
  rows, and nothing warns ([`next.md`](../../next.md)).

**Finding a thread**

| what | how | code |
|---|---|---|
| `list` | walks `sessions/` and `archived_sessions/` for `*.jsonl`; one entry per thread at its latest file (in both: its `sessions/` copy). Names from `<CODEX_HOME>/session_index.jsonl`, read **once per list**, newest `thread_name` per id (a rollout carries none: without the catalogue a list shows uuids). `cwd` from the first `session_meta` (up to 1 MiB); time from the database, else the file's last write | `list_in()`, `index_names()`, `peek_cwd()` |
| `dump`, `hop` | an id **prefix** of the thread uuid in the file name. Several files of **one** thread → its latest page; several threads → `Ambiguous`; none → the database rows, by prefix, then `NotFound` | `read_with()`, `read_through_catalogue()`, `rollout_id()` |
| `--from-file` | the rollout named; a page outside any home looks four folders up for its base | `read_file()` |

**The state database** (`src/reader/codex_state.rs` `catalogue()`): the newest `state_<n>.sqlite` of
the home, opened read-only (2 s busy timeout). Missing, locked, damaged or laid out otherwise → an
empty catalogue, and the rollouts are listed as before.

| where the rollout is | listed as |
|---|---|
| under `archived_sessions/` (where `codex archive` moves it) | `archived`, with or without a database; `dump`/`hop` read it there |
| on disk | the transcript. Its row adds the update time (wins over the file's), `archived` when the row says so, the parent, a title when `session_index.jsonl` has none |
| elsewhere: a row whose `rollout_path` exists outside `sessions/` | at that file; `dump`/`hop` read it there |
| gone | the row, `unreadable` set, `location` = the path the row records; `dump`/`hop` refuse it (exit 2) saying so. On the machine of 2026-11-08 this took `mukade list codex` from 0 sessions to 1105, in 61 working directories, in 135 ms [51] |

- **Subagents:** `list` leaves out the threads Codex ran for another (`source` containing
  `subagent`, `thread_source` `guardian_review` or `subagent`, or a child in `thread_spawn_edges`):
  7493 of the 8598 rows. `list_all` (`list --children`, the TUI) keeps them, with `parent` from
  `thread_spawn_edges` and `parent_kind` `Spawn { agent: agent_role }` (`guardian_review` for a
  guardian review, which has no edge and so no parent).
- **Title and time:** the row's `name`, else `title`, else the start of `preview` (one line, 120
  characters); `updated_at_ms`, else `updated_at` (seconds). Columns a Codex version lacks read as
  absent (`read_threads()`, `parent_links()`).

**Paginated threads** (`read_file_with()`, `base_prefix()`, `page_folders()`)

- The **latest version** is read: the latest page after the base's first `end_ordinal_exclusive`
  records (checked against `end_byte_offset`); a base that is itself a page is followed down to the
  thread's first file, up to 64 files (`MAX_PAGES`).
- Earlier files are looked for in the page's own home only, under `sessions/` and
  `archived_sessions/` (an archived page may continue from a base still under `sessions/`); a copy
  beside the home is never taken.
- Each earlier file's conversation records past its cut become `MetaKind::OffBranch`, counted
  (`dropped_off_branch`).
- `--all-branches` (`Branches::All`, `src/hop.rs` `export_all_branches()`) reads every file of the
  thread whole, the first one first. Codex only; another harness is refused (exit 2).
- A missing base, or an offset that does not end a record → an error with the reason. The
  fingerprint covers the bytes read, in order.

**Conversation records**

| source record | read as | code |
|---|---|---|
| line order | file order, never timestamps; a cut-off last line → `MetaKind::Unfinished`; a bad line elsewhere fails with `file:line` | `src/reader/json.rs` `read_jsonl()`, `rows_from_lines()`, `keep_unfinished()` |
| user `message` | `User` with attachments, unless its text, after leading whitespace, **starts with** `<environment_context>`, `<skills_instructions>` or `# AGENTS.md instructions for ` (the project's AGENTS.md, sent by 0.165 Desktop as a user message [26]) → `MetaKind::SystemPrompt`; `<external_codex_apps_open_page>` (app event) → `Internal`; `<turn_aborted>` → `TurnAborted`. A prompt that merely mentions a tag stays the user's words, as does the answer to a question (`<send_user_message_question_reply>`) | `class_of()` |
| `developer`/`system` `message` | `MetaKind::SystemPrompt` | `class_of()` |
| assistant `message`, `reasoning`, calls | one `Step` per model response; `turn_context` or a new `internal_chat_message_metadata_passthrough.turn_id` starts the next; parallel calls stay together | `step_with()` |
| `<turn_aborted>` message, `turn_aborted` event | `TurnAborted`, never user text. The step the aborted turn was writing ends `Aborted` and keeps what it holds, when nothing (a result, a later prompt) came after; an abort naming another `turn_id` leaves it alone. `--interrupted drop` leaves out the turn's steps and results, its prompt stays | `abort_step()` |
| `reasoning` | `summary` text + `Opaque{openai-responses, model, encrypted_content}`; `content` is not read | `step_with()` |
| `compacted` | `Compaction{summary: message, kept_from: next non-Meta entry, trigger: Unknown, tokens_before: latest_token_usage_record.usage.input_tokens}`; the step of the same `turn_id` that wrote the summary is folded in; `replacement_history` stays in `raw` only | `compacted()`, `finish()` |
| `event_msg`, `turn_context`, `world_state`, `token_usage_record` | `Meta`, kept raw (`task_started`/`task_complete` → `Idle`, `thread_settings_applied` → `ModelChange`, the rest `Internal`); models from `thread_settings_applied` | `class_of()`, `push()` |
| `session_meta` | id (`id`, else `session_id`), `cwd`, `cli_version` (harness version), `git.branch`; created/updated from the timestamps; **title**: the newest `session_index.jsonl` `thread_name` for the id (the name the Codex app shows), when the rollout is under `sessions/`; **parent** = `parent_thread_id` (top level, else `source.subagent.thread_spawn`), else `session_id` when it differs from `id`; `parent_kind` = `Spawn { agent }`: `agent_role`, or `guardian_review` [53]; forks not linked | `MetaBuilder::observe()`, `finish()`, `indexed_name()` |

**Tool records**

| source record | read as | code |
|---|---|---|
| `function_call`, `custom_tool_call` | name + arguments (the JSON string parsed) | `step_with()`, `arguments_of()` |
| `local_shell_call`, `web_search_call` | **read from `action`**: named `local_shell` / `web_search`, input = `action`; a web search's id is its `id` (no `call_id`) | `action_of()` |
| code-mode `exec` | split per the rule in tool conversion §5 (id of the n-th call `<id>_<n>`; `raw` untouched); otherwise one `exec` call | `src/reader/codex/exec.rs` `split_code_mode()`, `parse_script()`, `output_of()` |
| shell dialect | `exec_command`/`local_shell` → `Bash` or `PowerShell` from the call's `shell`, else the `<shell>` of `<environment_context>` (for `exec`, the one declared before the call), else `Bash` | `canonical_call()`, `shell_declared()` |
| `function_call_output`, `custom_tool_call_output` | string → text; **array (images, MCP content) → structured blocks kept** (`input_text`/`output_text` → `{"type":"text"}`, others as recorded); `is_error` always `false` (except a failed one-call `exec`) | `results_with()`, `output_item()` |
| `agent_message` | `Meta`, not carried (limit 3) | `class_of()` |

## How mukade writes it

Code: `src/writer/codex.rs` [23]. Without `CODEX_HOME`:
`CODEX_HOME is not set: nowhere to write the Codex rollout` (`write_planned()`).

**Limits, by cost to you**

1. **Only shell calls translate.** Offered set (`src/tools.rs` `offered()`): the [Tools](#tools)
   table less `multi_agent_v1`. A shell call becomes `exec_command {cmd, workdir, justification}`
   only when its dialect is the one `--dest-shell` names, else the one Codex runs: PowerShell on
   Windows, POSIX elsewhere (`src/harnesses/codex.rs` `default_shell()`). `timeout` is dropped,
   `yield_time_ms` not emitted, a background call is text form (`src/harnesses/codex.rs`
   `translate()`, `other_dialect()` first;
   [tool conversion §4.1](../tool-conversion-tables.md)). **Codex has no read or edit tool in the
   pinned bench**, so `Read`, `Write`, `Edit`, `Grep`, `Glob`, web search and fetch travel as text
   [21 §3]; Codex's patch form is not written (`edit()`).
2. **`cli_version` is required.** Codex refuses a rollout whose `session_meta` has none, so the write
   fails without one:
   ``Codex needs the destination's cli_version (`--dest-version`): it refuses a rollout whose session_meta has none``.
   mukade takes `--dest-version`, else the first version token of `<exe> --version`
   (`codex-cli 0.164.0` → `0.164.0`), cached per binary; an npm launcher's `package.json` is read
   without starting it. Pass `--dest-version` when the probe fails [25] (`src/cli.rs`
   `write_opts()`; `src/exe.rs` `version_of()`, `first_version()`).
3. **Reasoning.** Default `drop_foreign` ([usage §5](../usage.md#tool-calls-and-reasoning-what-is-converted-and-when)):
   only `openai-responses` blocks that still carry `encrypted_content` are written natively; others
   are dropped and counted. Live-verified: Codex 0.166.0 resumed a written copy and the provider
   refused a one-character tamper of the payload [48]. Reasoning without `encrypted_content` against
   the real API is open (R3) **[U]** [19 §10] (`src/policy.rs` `plan_reasoning()`, `payload_fits()`).
4. **Results and attachments are flattened.** Structured result blocks become their text; non-text
   blocks are dropped and counted (`dropped_result_blocks`). A prompt's image is written as
   `{type:"input_image", image_url:"data:<mime>;base64,<data>"}`; another attachment is dropped and
   counted (`dropped_attachments`) (`src/writer/plan.rs` `carry_result_content()`,
   `carry_attachments()`; `src/writer/codex.rs` `image_block()`).
5. **Resume:** keep `CODEX_HOME` set for the resume process and run it from the session's cwd
   [19 §5][25]. A written thread gets no `session_index.jsonl` name.

**What a rollout holds** (`rows()`)

| writes | detail |
|---|---|
| file | new uuid (or `--dest-id`, a uuid); `<CODEX_HOME>/sessions/YYYY/MM/DD/rollout-YYYY-MM-DDTHH-MM-SS-<uuid>.jsonl`, folder and stamp from one UTC instant (`src/writer/codex.rs` `rollout_stamp()`); `--dest-path` names the file instead (absolute, inside `CODEX_HOME`, no `..`: `src/harnesses/codex.rs` `validate_write()`). Lines `{timestamp, ordinal, type, payload}`, `ordinal` from 0 |
| `session_meta` | `base_instructions: null`, `id`, `timestamp`, `cwd`, `originator: "codex_cli_rs"`, `source: "cli"`, `cli_version`, `model_provider` (when known), `git.branch`; **no `session_id`** (the key set Codex accepts for a hand-written rollout, re-probed on 0.164.0 on 2026-10-28) |
| environment | a user message `<environment_context>` with `<cwd>` and the destination's shell (`--dest-shell`, else the platform's): `<shell>bash</shell>` (POSIX) or `<shell>powershell</shell>`; none with `--dest-shell none` |
| each prompt | `event_msg` `task_started {turn_id:"turn-N"}`, then the user `message` (`input_text`) |
| each step | `turn_context {turn_id:"turn-N", model, cwd}`, native `reasoning` (`summary`, `content: null`, `encrypted_content`), assistant `message` (`output_text`, `phase` `final_answer` or `commentary`), `function_call {name, arguments (JSON string), call_id}`; after a completed step `event_msg` `task_complete {turn_id, last_agent_message}` |
| each result | `function_call_output {call_id, output}` as text; a text-form call's result is a user message in the same place |
| `portable` summary | one user message |
| not written | a `session_index.jsonl` line; **the writer never opens `state_5.sqlite`**, so no stale `threads` row (L7). The reader's `list` does open it, read-only |

| setting | source |
|---|---|
| `model_provider` | `--model-provider`, else the top-level `model_provider = "…"` of the destination's `$CODEX_HOME/config.toml`: only keys before the first `[table]`, the key exactly, the text inside the quotes (a trailing `# comment` dropped when unquoted); `[profiles.x]` and dotted `model_providers.x.*` are ignored (`src/harnesses/codex.rs` `configured_model()`, `top_level_string()`) |
| model | `--model`, else the source's |
| executable in the receipt | `[<exe or "codex">, "exec", "resume", <new id>]` (`src/catalog.rs` `resume()`); interactive `[<exe>, "resume", <id>]` (`interactive_resume()`) |
| compaction | `none` (default), `portable`, `summary`, `native` (with `--context-window`; [usage §5](../usage.md#make-a-long-session-smaller)); the library's `DestinationNative` is refused (`src/hop.rs` `select()`) |

**Local message ids keep Codex's own shape.** Every `message` item but the summary and text-form
results carries `msg_<uuid v7, dashed>` (`local_message_id()`). Codex 0.166.0 strips ids it
recognises before replaying a resumed thread but forwards foreign shapes, which the Responses API
refuses (`This request does not support user-supplied input item IDs.`): an earlier
`msg_<uuid v4 simple>` made every written thread unresumable. Live-verified 2026-11-04 [48].

**Native compaction.** `--compaction native` keeps the whole rollout and appends a `compacted`
record (`src/native_compaction.rs` `plan_native_items()`); `portable` writes the summary as a user
message. Its `replacement_history` holds the deterministic summary as the first user input item,
then the retained response items with their Mukade-generated top-level `id` removed
(`without_input_id()`): live Codex 0.166.0 refused user-supplied input item IDs and ignored the
record's `message` on replay. Verified live 2026-11-04 by resuming an oversized session and
answering from the summary [19 §4.4] (`docs/verification/native-compaction-live.md`).

## How mukade summarizes it

With `--compaction summary` and Codex as the source, mukade runs (`src/summary.rs`
`summary_command()`):

```text
codex exec fork <id> --ephemeral --skip-git-repo-check -c sandbox_mode="read-only" -o <file> [-m <id>] <prompt>
```

- **Where:** `CODEX_HOME` set to the source store; from the session's cwd when it still exists;
  `<file>` is `last-message.txt` in `mukade-data/tmp/summary-<pid>-<time>/`, removed afterwards
  (`summarize()`).
- **Model and deadline:** `-m` is `--summary-model`, else the session's own last model;
  `--summary-timeout` (default 300 s) ([usage §5](../usage.md#summary-the-sources-own-model-writes-one)).
- **Windows `.cmd` launcher** (npm): the prompt must be one line of ASCII letters, digits, spaces and
  `.,;:()`, else the run is refused; name the native executable instead.
- The real model's behaviour: mock only **[U]** [27].

## Open points

1. **Codex summary with its real model [U]**: only a mock so far (quota); one `--compaction summary`
   run against the real provider settles it [27].
2. **A live hop of a real Desktop thread with code mode**: the census passed (every split call has
   one result) [50]; hopping one into a destination and resuming it settles it.
3. **A declared `cmd`, a call's own shell path**: the reader takes `<shell>cmd</shell>` as `Bash`
   and ignores a call's `shell` given as a path (`src/reader/codex.rs` `shell_declared()`). Reading
   them by name settles it in code. Desktop with its agent in WSL **[U]**: which shell it declares.
4. **Chain of pages [U]** (reader, rare shapes): a real thread redone twice settles it.
5. **Reasoning without `encrypted_content` (R3) [U]** [19 §10]: resuming such a rollout against the
   real Responses API settles it.

The rest, with the checklist: [`next.md`](../../next.md).

## Sources

All accessed 2026-10-28 unless a source says otherwise.

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
20. Repository: [`docs/reference/harness-session-persistence-reference.md`](../reference/harness-session-persistence-reference.md), the *OpenAI Codex* dossier under *Local and open-format harness dossiers*
21. Repository: [`docs/tool-conversion-tables.md`](../tool-conversion-tables.md) §2–§6
22. Repository: [`src/reader/codex.rs`](../../src/reader/codex.rs), `src/reader/codex/exec.rs`, `src/reader/codex_state.rs`, `src/reader/json.rs`
23. Repository: [`src/writer/codex.rs`](../../src/writer/codex.rs), `src/writer/plan.rs`, `src/policy.rs`, `src/tools.rs`, `src/tools/patch.rs`, `src/harnesses/codex.rs` (the adapter), `src/model/tool.rs`, `src/hop.rs`
24. Repository: [`src/exe.rs`](../../src/exe.rs), [`src/cli.rs`](../../src/cli.rs) (`write_opts()`), `src/harnesses/codex.rs` (`configured_model()`, `top_level_string()`), `src/detect.rs`, `src/detect/codex.rs`, [`catalog/harnesses.json`](../../catalog/harnesses.json)
25. Repository: [`README.md`](../../README.md), [`docs/usage.md`](../usage.md) §2
26. Two real Codex Desktop 0.165.0-alpha.1.2 threads of the Daodan project, read on 2026-10-28 with their `state_5.sqlite` and `thread_history_1.sqlite` rows (read only), and a scan of the 982 rollouts on that machine (971 subagent rollouts, whose `session_id` is the spawning thread's and whose `state_5.sqlite` row is keyed by their own `id`): thread `01a181e2-…` (17.8 MB, 5 compactions, all with an empty `message`) and thread `01a181df-…` (a base file plus one page, `history_base.end_ordinal_exclusive` 511 of 1064). Not in the repository: the user's own conversations.
27. `docs/verification/source-model-summary-live.md`: the live check of 2026-10-28 (throwaway sessions, source hashed before and after; Codex against a local mock)
28. Official Unix install script, https://chatgpt.com/codex/install.sh, accessed 2026-10-30 (`CODEX_INSTALL_DIR` default `$HOME/.local/bin`, standalone release layout, existing-install discovery).

Sources [29]-[46] were read on 2026-11-01 through summarising fetches (a small model returns
summaries and quotes, so the wording is as returned). They back the web-tool facts (Ways to run it,
Ephemeral runs, Tools, Web tools), checked against 0.164.0: the installed CLI [47] and the source at
tag `rust-v0.164.0` [32]-[46]. [29]-[31] are pages cited above, reread for web search; [32]-[46] are
files of `openai/codex` at that tag, under `codex-rs/`, at
`https://raw.githubusercontent.com/openai/codex/rust-v0.164.0/codex-rs/` unless a URL is given.

29. OpenAI, *Configuration reference*, https://learn.chatgpt.com/docs/config-file/config-reference (keys `web_search` and `tools.web_search`; first 100000 of 156888 characters read), accessed 2026-11-01.
30. OpenAI, *Web search*, https://learn.chatgpt.com/docs/web-search, accessed 2026-11-01.
31. OpenAI, *Non-interactive mode*, https://learn.chatgpt.com/docs/non-interactive-mode (`codex exec`, `--json` item types), accessed 2026-11-01.
32. `exec/src/exec_events.rs` (`ThreadItemDetails`, the `web_search` item), accessed 2026-11-01.
33. `protocol/src/models.rs` (`WebSearchAction`, `ResponseItem::WebSearchCall`; first 100000 of 167600 characters read), accessed 2026-11-01.
34. `protocol/src/items.rs` (`WebSearchItem.results`), accessed 2026-11-01.
35. `protocol/src/config_types.rs` (`WebSearchMode` default `Cached`, `WebSearchToolConfig`), accessed 2026-11-01.
36. `core/src/event_mapping.rs` (`parse_turn_item`), accessed 2026-11-01.
37. `core/src/web_search.rs` (`web_search_action_detail`), accessed 2026-11-01.
38. `core/src/tools/spec_plan.rs` (which tools are registered, and under which feature or capability), accessed 2026-11-01.
39. `core/src/tools/hosted_spec.rs` (`create_web_search_tool`), accessed 2026-11-01.
40. `exec/src/event_processor_with_jsonl_output.rs` (`map_item_with_id`), accessed 2026-11-01.
41. `ext/web-search/src/tool.rs` and `extension.rs` (the standalone `web.run` tool), accessed 2026-11-01.
42. `features/src/lib.rs` (feature `standalone_web_search`), accessed 2026-11-01.
43. `exec/src/cli.rs` (the `exec` flags), accessed 2026-11-01.
44. `core/src/client.rs` (the request `include` list; first 100000 of 125901 characters read), accessed 2026-11-01.
45. https://github.com/openai/codex/tree/rust-v0.164.0/codex-rs/core/src/tools/handlers (directory listing, which the page truncates behind "View all files"), accessed 2026-11-01.
46. `core/src/tools/handlers/multi_agents_spec.rs` (the tools of `multi_agent_v1`), accessed 2026-11-01.
47. Installed `codex` binary, accessed 2026-11-01: `codex --version` (`codex-cli 0.164.0`); `codex --help`; `codex exec --help`; `codex exec resume --help`; `codex features list`; `codex debug --help`; `codex sandbox --help`.
48. `docs/verification/codex-encrypted-reasoning-live.md`: the live Codex → Codex hop of 2026-11-04 (`codex-cli 0.166.0`, real ChatGPT login, `gpt-6.1-sol` at `xhigh`): encrypted reasoning carried byte-identical, resume answered, tampered payload refused (`400 invalid_encrypted_content`), and the message-id findings.
49. Installed `codex` binary, accessed 2026-11-04: `codex --version` (`codex-cli 0.166.0`); `codex delete --help` (permanent deletion by id or name; used with `--force` for the test threads of [48], no residue). Live refusal the same day: `The 'gpt-5.1' model is not supported when using Codex with a ChatGPT account.` (also `gpt-5.1-codex`, `gpt-5.1-codex-max`); the item-ID refusal is quoted under *How mukade writes it*.
50. [`docs/verification/2026-11-11-codex-code-mode.md`](../verification/2026-11-11-codex-code-mode.md): read-only code-mode census of 2026-11-11 (34 rollouts of Codex 0.167.0, 0.167.0-alpha.3.2, 0.167.1; 981 `exec` calls, 725 split into 1,139 calls; per-destination counts).
51. [`docs/verification/listing-coverage-2026-11-08.md`](../verification/listing-coverage-2026-11-08.md): listing coverage of 2026-11-08 (Codex 0.166; 8598 `threads` rows, none with an existing rollout; 1105 listed threads, 7493 subagent and guardian threads left out).
52. Codex CLI 0.167.1 and Desktop 26.1106 code, read only, 2026-11-12: `shell_detect.rs` (the shell per platform), the `<environment_context>` and `world_state` shell names. Decisions: [`docs/superpowers/specs/2026-11-12-destination-shell-design.md`](../superpowers/specs/2026-11-12-destination-shell-design.md).
53. The author's Codex home, read only for shape (no content copied), 2026-11-12, 0.167.0-alpha.3.2 to 0.167.1: 19 subagent and 6 guardian `session_meta` headers; `state_5.sqlite` columns (`agent_role`, `thread_source`) and `thread_spawn_edges`; 320 `agent_message` rows in 29 rollouts (265 with `encrypted_content`). Decisions: [`docs/superpowers/specs/2026-11-12-subagents-design.md`](../superpowers/specs/2026-11-12-subagents-design.md).
