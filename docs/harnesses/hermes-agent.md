# Hermes Agent

Hermes Agent is Nous Research's MIT-licensed Python agent, with a terminal front end, a background gateway per profile and an ACP mode [1][R 23].
mukade reads its sessions from `state.db` and writes them through Hermes's own `sessions import`: list, dump, hop in and out, resume. Verified on v0.23.1 on this machine.

Evidence: `[n]` = source n in [Sources](#sources); `[S n]` = Hermes source at tag `v0.22.5`; `[R n]` = Shinatama research, nothing run; **[V]** = observed on this machine; **[U]** = not established (the text says what would settle it).

## Status in mukade

| | |
|---|---|
| **Read** (`list`, `dump`) | ✅ `<HERMES_HOME>/state.db`, opened read-only; schema 32 and later (33 verified) with the columns the reader needs. `list` shows what Hermes's own `sessions list` shows; `--children` (and the TUI) add the sessions of the hidden sources and the subagents [37] |
| **Write** | ⚠️ through `hermes sessions import --from claude`, never by writing `state.db`. Only user and assistant text arrives; every tool call is text; no model, no reasoning, no image. Takes about 70 s (the import starts the gateway) [33][35] |
| **Hop as source** | ✅ `mukade hop hermes <id> <dst>`; `--all-branches` does not apply |
| **Hop as destination** | ⚠️ works with the losses above; `--dest-id`, `--dest-agent` and `--metadata-file` are refused as a command-line mistake (exit 1, as toward every destination that does not take them), `--dest-path` and `--compaction native` as unsupported (exit 2); each before anything runs |
| **Resume** | ✅ receipt: `hermes chat -Q --resume <id> -q`; the TUI runs `hermes --resume <id>` |
| **Summary by its own model** | ❌ `--compaction summary` from Hermes is refused, with the reason (How mukade summarizes it) |
| **Native compaction** | ⚠️ read: a stored summary row (`_compressed_summary = 1`) is a compaction entry, so `--compaction portable` works; never written (the import has no such record) |
| **Subagents** | ⚠️ read: `delegate_task` sessions are children (`list --children`, `dump --children`); a `/branch` copy is a fork. Written: `hop --children` imports each child as a session of its own, unlinked |
| **Notable gaps** | ⚠️ a written session has no model and no tool result of its own; its folder is kept only when it exists here; Hermes may leave the session untitled |
| **Not supported** | ❌ a named profile is another installation: pass its folder as `--root` / `HERMES_HOME` |
| **mukade name** | `hermes` (`mukade list hermes`, `mukade hop <src> <id> hermes`) [33] |
| **Versions** | v0.23.1 (schema 33) read and written live on 2026-11-12 [35][36]; v0.22.5 (schema 32) by source only [34] |

## Quick start

```powershell
# 1. Point mukade at the store
$env:HERMES_HOME = "$env:LOCALAPPDATA\hermes"      # macOS/Linux: export HERMES_HOME=~/.hermes

# 2. Find and inspect a session
mukade list hermes
mukade dump hermes YOUR_HERMES_SESSION_ID --output C:\work\session.json

# 3. Hop out of Hermes
mukade hop hermes YOUR_HERMES_SESSION_ID claude_code --cwd 'C:\work\my-project'

# 4. Hop into Hermes (the import takes about a minute), then continue it
mukade hop claude_code YOUR_CLAUDE_SESSION_ID hermes
hermes --resume NEW_SESSION_ID
```

Inspect the store by hand: `hermes sessions list` (a text table), `hermes sessions export - --session-id <id> --format jsonl`, `sqlite3 -readonly "<home>/state.db" "SELECT * FROM schema_version"`. A read-only open of a WAL database can create `-wal` and `-shm` sidecars [32].

## Install and locate

| | |
|---|---|
| Vendor | Nous Research; repository `NousResearch/hermes-agent` [1][2] |
| Licence, source | MIT; <https://github.com/NousResearch/hermes-agent> [1] |
| Docs | <https://hermes-agent.nousresearch.com/docs/> [1] |
| Version described | v0.23.1 (commit `3b421578588243f920886e6fe79a6faa87ea026e`, released 2026-11-10), installed and exercised here on 2026-11-12 [34][36]. The tool tables below come from tag `v0.22.5` (commit `bf0203a794a07aac3ff764972ab9087baa4e4f4e`) and the docs site [2] |
| `hermes --version` | `Hermes Agent v0.23.1 (2026.9.24) · stable · …`: mukade parses `^Hermes Agent v(\S+)`; the date in parentheses is a stale constant [34] |
| Runtime | Python 3.11 to 3.14; installed by its own script with a bundled uv [1][R 23] |
| Agent loop | `hermes chat` finds or starts the profile's background gateway, creates or resumes the session there and attaches over a local WebSocket. Closing the terminal only detaches; Ctrl+C stops the turn. Launch flags are fixed at session creation [6][R 23] |
| Front ends | classic CLI (the default); `hermes chat --tui`, an alternate-screen Node TUI; the ACP server `hermes-acp`; messaging-platform toolsets and a desktop app [R 23][S 11] |
| Windows | native: a UTF-8 console shim, the gateway detached through `pythonw`. mukade sets `PYTHONIOENCODING=utf-8` for the import, whose confirmation line has a check mark [R 23][35] |

**Docs against the tag.** The docs site shows current documentation, which differs from the tag: its `browser` toolset lists 4 tools [5], the tag 14 [S 11]. The tag wins [29]: this page takes behaviour from the docs and tool names from the tag.

**Install.** Hermes's own script, which bundles uv [R 23]. **[U]**: the script's URL and command on every OS, update and side-by-side installs.

**Locations**

| what | Windows | macOS / Linux |
|---|---|---|
| Launcher | `%LOCALAPPDATA%\hermes\bin\hermes.exe` **[V]**; the tree's own `%LOCALAPPDATA%\hermes\hermes-agent\.hermes\bin\hermes.exe` [34]; not on `PATH` in a fresh shell | `~/.local/bin/hermes` **[U]** |
| Hermes home (default) | `%LOCALAPPDATA%\hermes` **[V]** | `~/.hermes` [34] |
| Move the home | `HERMES_HOME` | `HERMES_HOME` |
| Session database | `<home>\state.db` **[V]** | `<home>/state.db` |
| Named profile | `<root>\profiles\<name>\`, its own `state.db`; `<root>/active_profile` names the sticky one [34] | the same |
| Secrets and login | `<home>\.env`, `<home>\auth.json` | the same |

- **One lever.** `HERMES_HOME` and the profile (`-p`) move settings, hooks and the session store together [R 23].
- **Find an install.** `mukade harnesses` (binary, version, store, session count), or `Get-Command hermes -All`, then `hermes --version`.
- **mukade discovery.** Catalog entry `hermes`; flag `--hermes-exe`, variable `MUKADE_HERMES_EXE`; store variable `HERMES_HOME` (default `%LOCALAPPDATA%\hermes` on Windows, else `~/.hermes`); `detect`, `scan` and `--root` recognize a home by its `state.db`. The shared procedure is in the [harness catalog](../reference/harness-catalog.md#finding-existing-installations).
- **Paid parts.** The Nous Tool Gateway needs a paid Nous subscription [9][S 12].

## Session store

Read on this machine (schema 33) and in the v0.23.1 source [34][36].

| path | holds |
|---|---|
| `<home>/state.db` | every session of the profile: SQLite, FTS5, WAL |
| `<root>/profiles/<name>/state.db` | a named profile's own sessions |
| `<home>/sessions/` | a JSONL fallback copy [R 24] |
| `state.db-wal`, `state.db-shm` | WAL sidecars |
| `cache/delegation/live/` | sub-agent logs, pruned after 7 days [R 23] |

**Schema version.** `schema_version` holds one row: 32 at v0.22.5, 33 at v0.23.1 (data-only: re-tags delegate rows that lost `_delegate_from`). The number is not a shape contract: columns are added under the same number, and Hermes never refuses a newer schema. mukade accepts 32 and later and checks the columns it needs (`PRAGMA table_info`), refusing a store that lacks one by naming it [34].

**Tables** (`SCHEMA_SQL`, identical at both tags)

| table | columns mukade uses |
|---|---|
| `sessions` | `id`, `source`, `created_source`, `title`, `cwd`, `git_branch`, `model`, `model_config` (JSON), `parent_session_id`, `started_at`, `ended_at`, `end_reason`, `last_activity_at`, `archived`, `auto_archived` |
| `messages` | `id`, `session_id`, `timestamp`, `role`, `content`, `reasoning`, `reasoning_content`, `reasoning_details`, `codex_reasoning_items`, `tool_calls`, `tool_call_id`, `tool_name`, `active`, `compacted`, `_compressed_summary`, `display_kind` |

**Row contents**

| item | shape |
|---|---|
| order | `messages.id`, never the timestamp (an epoch float) |
| `content` | a string as is; a list of parts as `\x00json:` + JSON; NULL, BLOB and numbers are legal, so mukade reads cells by type |
| roles | `user`, `assistant`, `tool`; the system prompt is `sessions.system_prompt`, not a row |
| `tool_calls` | JSON array of `{id, type:"function", function:{name, arguments: <JSON string>}}` |
| tool result | `role='tool'`, `tool_call_id`, `tool_name`; results of web and MCP tools are wrapped in `<untrusted_tool_result>` |
| reasoning | `reasoning` and `reasoning_content` plain text (the latter may be a single space); `reasoning_details` and `codex_reasoning_items` opaque provider payloads, only on assistant rows |
| `active` / `compacted` | `1/0` live conversation; `0/1` archived by an in-place compaction; `0/0` rewound or superseded |

**Ids and sources**
- Ids: `YYYYMMDD_HHMMSS_<6 hex>` (CLI, TUI, branch), 12 hex for an import, `local-<64 hex>` (gateway creation, cron, kanban), `cron_…`, `bg_…`, `room_…`, `api_…`, a uuid4 (ACP). The caller cannot choose one [34].
- `sessions.source`: `cli`, `tui`, `acp`, `tool`, `oneshot`, `subagent`, `cron`, `kanban`, gateway platforms, and `local` with the creator in `created_source` (every creation label except `tool` and `oneshot` is stored as `local`). Hermes's pickers hide `kanban`, `tool` and `oneshot` [34].
- Retention: nothing is pruned unless `sessions.auto_prune` is on (90 days) [R 24].

**Lineage**

| kind | recorded as |
|---|---|
| subagent (`delegate_task`) | its own row, `source='subagent'`, `parent_session_id`, `model_config._delegate_from = <parent>` (`__orphaned__` when the parent is gone) |
| `/branch` (`/fork`) | a new row with `parent_session_id` and `model_config._branched_from`; the parent's active rows up to a boundary are **copied** into the child |
| legacy rotating compaction | the old session has `end_reason='compression'` and a child continues it (only with `compression.in_place: false`) |
| reset continuation | `model_config._reset_from`, or the parent's `end_reason` is `session_reset`, `idle`, `daily`, … |

**Writer.** The profile's gateway is the only writer of `state.db`. The one supported way in is `hermes sessions import --from claude|codex <file>` (How mukade writes it). Hermes may be running while mukade reads: the reader retries once on a transient `SQLITE_IOERR` or `SQLITE_BUSY`, as Hermes's own readers do [34].

**Reading routes.** `hermes sessions export <out|-> --session-id <id> --format jsonl|md|html|trace` writes one line per session: the `sessions` row plus its `messages`, `content` decoded, JSON columns parsed. mukade reads `state.db` directly, and reads such an export with `--from-file` [34].

## Features

### Compaction

- **Trigger.** Automatic at 50% of the context window by default (`compression.threshold: 0.50`, floored at 0.75 for windows under 512K) [34]. A manual command and a way to compact headless are **[U]**. The `post_compaction` hook event exists (Hooks).
- **On disk.** In place by default (`compression.in_place: true`): the session keeps its id; in one transaction the active rows become `active = 0, compacted = 1` and the compacted list (head, summary, verbatim tail) is inserted as fresh active rows with new ids, keeping each carried row's `message_uid`. The legacy path (`in_place: false`) rotates the id: the old session ends with `end_reason = 'compression'` and a child continues it [34].
- **Summary.** Stored: a row with `_compressed_summary = 1`, role `user` or `assistant`, content = a prefix (`[CONTEXT COMPACTION — REFERENCE ONLY] Earlier turns were compacted …`), the text, and an end marker. When merged into the tail, the summary sits inside the first tail row's content between `[PRIOR CONTEXT …]` delimiters. mukade reads the row as a compaction entry and strips the framing (How mukade reads it) [34].
- **Not a deletion.** Hermes's documentation warns that compression shortens the active context but is not a privacy deletion [28].

### Ephemeral runs

`-q` runs on v0.23.1 with the free OpenRouter model were made on 2026-11-12 [36]; the rest is from the docs and source.

| goal | command | notes |
|---|---|---|
| first turn on a terminal | `hermes chat -q "<text>"` | sends the text and stays open [S 15][R 23]. No positional prompt: a bare word is a subcommand. `-q` size limit: not found [R 23] |
| one turn, headless | `hermes chat --oneshot -q "<text>" --format stream-json` | answers and exits; `-q` without a TTY also runs one turn and exits [S 14][S 15]. `--oneshot` and `--format` are not on the docs CLI page [6] |
| final text only | `hermes chat -z "<prompt>"` | prints only the final text [R 23] |
| prompt from a file | `hermes chat --query-file <path>` (`-` for stdin) | never shell-interpreted [6][R 23] |
| one turn in a resumed session | `hermes chat --resume <id> --oneshot -q "…"` | [32]; launch flags stay frozen on resume [6] |

- **Session id.** Not chosen by the caller: in `system/init` (`session_id`), in `result.session_id` of `--format stream-json`, and as a `session_id: …` line after the answer with `-Q --pass-session-id` **[V]** [S 14][S 15][R 23].
- **Does a one-shot leave a session?** Yes **[V]**: `-q` / `-z` runs left `source = oneshot` sessions in this machine's store, and no flag prevents it [36]. A resumed question appends to the session it resumes (How mukade summarizes it).
- **Isolation flags.** `--ignore-user-config` (needs `--model` and `--provider`), `--ignore-rules`, `--safe-mode` [6]; for a scratch store, `HERMES_HOME` or the profile (Install and locate).
- **Approvals and tools.** No `--yolo` (or `HERMES_YOLO_MODE=1`) for a read-only run [R 23]; one-shot approval defaults and `--toolsets` limits: Tools, Approvals and read-only switches.

**`--format stream-json` events.** One JSON object per stdout line, each with a `timestamp` in milliseconds (`hermes_cli/stream_json.py`) [S 14]:

| event | fields |
|---|---|
| `system/init` | `model`, `session_id` |
| `text` | `text` |
| `tool_use` | `name`, `tool_call_id?`, `input?`: the full argument object, not truncated |
| `tool_result` | `name`, `tool_call_id?`, `output` (cut to 5000 characters plus `...`), `duration_ms`, `is_error` |
| `result` | `session_id`, `exit_code`, `text`, `tokens {input, output, total, cache_read, cache_write}`, `duration_ms`, `error?` |

Whether these callbacks stay wired in quiet `-q` mode is **[U]**: `_configure_quiet_agent` nulls some and the order of `attach()` is not shown in the page read [S 15]. A run in a scratch `HERMES_HOME` with `--toolsets search` would settle it [29].

**ACP.** `hermes-acp` is named as a toolset and as the ACP mode [S 11][R 23]; its launch command is **[U]**. Its toolset is the coding posture minus `clarify`. `web_search` and `web_extract` get tool kind `fetch`; the title is `web_search: <args preview, 80 characters>`; `rawInput` is set only for tools the adapter does not know; `rawOutput` is empty for known tools; completion text shows at most 10 results [S 20]. This carries less than `stream-json`; the exact `session/update` fields are **[U]**.

**Hidden traffic**
- No network-off switch was found **[U]**. MCP toolsets (`mcp-<server>`) may join a turn with an explicit `--toolsets` **[U]**.
- `execute_code` can call `web_search` and `terminal` [S 22]: keep `code_execution` off.
- No hidden queries for the web tools: `tool_use.input` is complete and uncut [S 14]. Only truncation: 5000 characters of results in `stream-json`; 80 and 10 in ACP [S 14][S 20].
- `x_search` (off by default) runs inside xAI's tool; its result still carries `answer`, `citations` and `inline_citations {url, title}` [S 22].
- Whether a `delegate_task` sub-agent's calls appear in the parent's stream is **[U]**.

### Tools

**86 tool names** = 51 in the shared core list `_HERMES_CORE_TOOLS` + 31 more in other `TOOLSETS` entries (82 in `toolsets.py` [S 11]) + `kanban_schedule` (registered in `kanban_tools.py`, in no `toolsets.py` list read) + 3 tool-search bridge tools. Built from `toolsets.py` at the tag, each tool's registration file [S 22], then the docs tools reference [4] [29]. Not run: the count a model sees varies with each tool's availability check, keys and platform [29]. Only v0.22.5 was read; other versions **[U]**.

- **Default set.** `hermes-cli` = the 51 core tools, minus tools whose availability check fails [S 17], minus the default-off toolsets spotify, discord, discord_admin, video, video_gen, x_search, a2a and kanban [S 19].
- **ACP set.** `hermes-acp` also drops `image_generate`, `text_to_speech`, `cronjob_manage`, `computer_use`, `clarify` and `kanban_*` [S 11].
- **Switches.** `hermes chat --toolsets <names>` (comma list, no spaces), `agent.disabled_toolsets`, `hermes tools`, `/tools disable` [3][5].
- **Not built in.** Honcho, Home Assistant and Spotify are catalog plugins in v0.22.5 [4]. MCP tools form one toolset `mcp-<server>` per configured server [5]; the name a model sees is **[U]**.

Columns: `class` = the research report's grouping (web, read, write, shell, agent, interaction, other); `RO` = its read-only judgement of each handler [29]. Columns: `class` = the research report's grouping (web, read, write, shell, agent, interaction, other); `RO` = its read-only judgement of each handler [29]. Citations: `[S 11]` name in `toolsets.py`; `[S 12]`, `[S 18]`, `[S 22]` registration file; `[4]` docs tools reference. Parameters **[U]**: registration file not read.

**Core, on in `hermes-cli`** (registration file [S 22] and docs [4] unless the row says otherwise)

| toolset | tool | does; main parameters | class | RO |
|---|---|---|---|---|
| web, search | `web_search` | web lookup; `query`, `limit` 1-100 (default 5); returns title, URL, description [S 12][4] | web | no |
| web | `web_extract` | page text for URLs; `urls` (max 5, required), `char_limit` per page (min 2000, default 15000) [S 12][4] | web | no |
| file | `read_file` | text file with line numbers; `path`, `offset`, `limit` | read | yes |
| file | `write_file` | replaces a whole file; `path`, `content`; refuses without a prior read | write | no |
| file | `patch` | find-and-replace edit; `path`, `old_string`, `new_string`, `replace_all` (V4A `mode`/`patch` for OpenAI-family models) | write | no |
| file | `search_files` | regex search of contents or find by name; `pattern`, `target` content/files, `path`, `file_glob`, `limit`, `output_mode`, `context` | read | yes |
| terminal | `terminal` | shell command on the configured backend; `command`, `timeout`, `workdir`, `background` (returns a session_id); dangerous commands need approval [7] | shell | no |
| terminal | `process_manage` | background processes; `action` list/poll/log/wait/kill/write/submit/close/handoff, `session_id` | shell | no |
| code_execution | `execute_code` | Python calling Hermes tools (`web_search`, `web_extract`, `read_file`, `write_file`, `search_files`, `patch`, `terminal`); `code`, `reset`; on if the backend check passes | shell | no |
| delegation | `delegate_task` | sub-agents in isolated contexts inheriting the parent's toolsets; `tasks[].goal` (required) with `context`; `action` spawn/list/steer/stop | agent | no |
| vision | `vision_analyze` | shows the model an image; `image_url` (URL, path or data URL), `question`, `region` crop; on if the model is image-capable or an auxiliary vision model resolves | web | no |
| image_gen | `image_generate` | image from text; `prompt`, `aspect_ratio` (editing parameters depend on the backend); on with a provider | web | no |
| tts | `text_to_speech` | text to audio file; `text`, `output_path`, `speed` | other | no |
| skills | `skills_list` | skill names and descriptions; `category` | read | yes |
| skills | `skill_view` | loads a skill or linked file; `name`, `file_path` | read | yes |
| skills | `skill_manage` | creates, patches, deletes skills; `operations[]` with `name` and `action` create/patch/write_file/remove_file/delete; `skills.write_approval` stages writes [S 21] | write | no |
| memory | `memory` | durable cross-session facts; `target` memory/user, `action` add/patch/remove, `content`, `old_text`; `memory.write_approval` | write | no |
| session_search | `session_search` | searches or scrolls past sessions (FTS5); `query`, `limit`, `after`, `before`, `session_id` | read | yes |
| todo | `todo_list` | task list for multi-step work; `todos[]` with `id`, `content`, `status`; `merge` | other | yes |
| clarify | `clarify` | asks the user 1-5 questions; `questions[]` with `question`, `choices`, `multi_select`; not in `hermes-acp` | interaction | yes |
| cronjob | `cronjob_manage` | scheduled jobs; `action` create/list/update/pause/resume/remove/run, `prompt`, `schedule`, `job_id`; on in interactive and gateway sessions | other | no |
| computer_use | `computer_use` | background desktop control through `cua-driver`; parameters **[U]**; not enabled by `all` [5] | other | no |
| connections | `manage_connections` | connector accounts; `action` display/status/connect/reconnect/rename and MCP install/enable/authorize; service-gated [S 11][4] | other | no |

**Browser** (toolset `browser`, 14 tools; all class web, RO no; on if the agent-browser or CDP backend check passes). The first ten were read in `browser_tool.py` [S 22] (the report does not mark them as in the docs tools reference); the last four in `toolsets.py` and the docs [S 11][4][29].

| tool | does; main parameters |
|---|---|
| `browser_navigate` | opens a URL; `url` |
| `browser_snapshot` | text accessibility-tree snapshot; `full` |
| `browser_click` | clicks by snapshot ref; `ref` |
| `browser_type` | types into an input; `ref`, `text` |
| `browser_scroll` | scrolls; `direction` up/down |
| `browser_back` | history back; no parameters |
| `browser_press` | presses a key; `key` |
| `browser_get_images` | page images with URL and alt text; no parameters |
| `browser_vision` | screenshot analysed by a vision model; `question`, `annotate` |
| `browser_console` | console output and JS errors, or evaluates an expression; `clear`, `expression` |
| `browser_cdp` | raw Chrome DevTools Protocol command; needs a CDP endpoint; parameters **[U]** |
| `browser_dialog` | answers a blocking JS dialog; `action` accept/dismiss; needs a CDP endpoint |
| `browser_vault` | saved logins, cards, codes; `action` list/fill/save_login/enter_code/unlock; deferred behind the bridge tools by default [S 11][S 18] |
| `browser_exec` | Python with browser helpers in a session; replaces the other browser tools when `browser.backend` is browser-use [S 11]; parameters **[U]** |

**Off by default: X search, video, kanban** (kanban rows [S 22][4]; switch `kanban`, **off** unless the row says otherwise)

| toolset | tool | does; main parameters | class | RO |
|---|---|---|---|---|
| x_search | `x_search` | X posts through xAI's built-in tool (provider side); `query`, `allowed_x_handles`, `excluded_x_handles`, `from_date`, `to_date` [S 22][4] | web | no |
| video | `video_analyze` | captions, scenes, timestamps from a video URL or path; parameters **[U]** [S 11][4] | web | no |
| video_gen | `video_generate` | text-to-video or image-to-video; `image_url`; other parameters **[U]** [S 11][4] | web | no |
| video_gen | `xai_video_edit`, `xai_video_extend` | edits or extends a video given a public HTTPS MP4 `video_url`; other parameters **[U]** [S 11][4] | web | no |
| kanban | `kanban_show` | full state of a task (row, parents, comments, runs, events); `task_id`, `board`; off unless a dispatcher worker | other | yes |
| kanban | `kanban_list` | task summaries with filters; `assignee`, `status`, `limit`; orchestrator only | other | yes |
| kanban | `kanban_complete` | marks the task done with a handoff; `summary`, `result`, `metadata` | other | no |
| kanban | `kanban_block` | blocks the task; `reason`, `kind` | other | no |
| kanban | `kanban_request_review` | moves the task into review; `summary`, `reviewer` | other | no |
| kanban | `kanban_request_changes` | returns a reviewed task to its implementer; `reason` | other | no |
| kanban | `kanban_heartbeat` | signals liveness, extends the claim; `note` | other | no |
| kanban | `kanban_comment` | appends a comment; `task_id`, `body` | other | no |
| kanban | `kanban_create` | creates a child task; `title`, `assignee`, `body`, `parents` | other | no |
| kanban | `kanban_link` | adds a parent-child dependency; `parent_id`, `child_id` | other | no |
| kanban | `kanban_unblock` | moves a blocked task to ready; `task_id`; orchestrator only | other | no |
| kanban | `kanban_attach` | attaches an inline base64 file (25 MB cap [4]); `filename`, `content_base64` | other | no |
| kanban | `kanban_attach_url` | attaches a file fetched from an http(s) URL (25 MB cap [4]); `url`, `filename` | web | no |
| kanban | `kanban_attachments` | lists a task's attachments (read-only per its handler); `task_id` | other | yes |
| kanban **[U]** | `kanban_schedule` | parks the task until an orchestrator re-gates it; `reason`. Registered [S 22] but in no `toolsets.py` list read: offered **[U]** | other | no |

**Chat platforms and desktop app only** (names [S 11], docs [4] except `setup_beat`; parameters **[U]** unless named)

| toolset | tool | does | class | RO |
|---|---|---|---|---|
| discord | `discord` | reads and takes part in Discord servers; **off**; needs `DISCORD_BOT_TOKEN` | other | no |
| discord_admin | `discord_admin` | manages Discord servers through REST; **off** | other | no |
| feishu_doc | `feishu_doc_read` | reads a Feishu/Lark document's text (comment handler only); platform only | web | no |
| feishu_drive | `feishu_drive_add_comment`, `feishu_drive_list_comments`, `feishu_drive_list_comment_replies`, `feishu_drive_reply_comment` | in that order: adds a comment to a drive file, lists its comments, lists replies, replies; platform only | web | no |
| yuanbao | `yb_query_group_info`, `yb_query_group_members`, `yb_send_dm`, `yb_search_sticker`, `yb_send_sticker` | in that order: group info, members, direct message, sticker search, send sticker; platform only | web | no |
| project | `desktop_project` | creates, lists, switches desktop projects (`action`); desktop only | other | no |
| desktop_ui | `read_terminal`, `close_terminal` | terminal pane affordances; effect **[U]**; desktop only | other | **[U]** |
| desktop_ui | `desktop_preview`, `drive_preview`, `annotate_preview` | preview pane affordances; effect **[U]**; desktop only | other | **[U]** |
| desktop_ui | `read_window_below`, `focus_pane`, `react_to_message` | window affordance, pane focus, message reaction; effect **[U]**; desktop only | other | **[U]** |
| desktop_ui | `gui_tour`, `show_tip`, `set_theme` | guided tour, tip, theme change; effect **[U]**; desktop only | other | **[U]** |
| onboarding | `setup_beat` | first-run onboarding cards; desktop only [S 11] | other | **[U]** |
| catalog | `manage_catalog` | searches and installs catalog plugins and hub skills through an approval card (`search`, `install`); desktop only | other | no |

**Tool-search bridge** [S 18] (switch `tools.tool_search.enabled` auto/on/off; on when any tool is deferrable)

| tool | does | class | RO |
|---|---|---|---|
| `tool_search` | finds deferred tools by query (up to 7 queries per call) | other | yes |
| `tool_describe` | loads full schemas of deferred tools (up to 10 names per call) | other | yes |
| `tool_call` | invokes a deferred tool as `{name, arguments}` | other | no |

**Needs a key or an optional dependency** [29]

| tool | needs |
|---|---|
| `x_search` | `XAI_API_KEY` or xAI OAuth |
| `image_generate` | `FAL_KEY`, the Nous gateway, Krea or a plugin |
| `text_to_speech` | the default `edge` needs the `edge-tts` extra; the others their key or package |
| `browser_*` | the agent-browser CLI, Node, Chromium, or a cloud or CDP backend |
| `discord` | `DISCORD_BOT_TOKEN` [4] |
| `feishu_*`, `yb_*` | platform credentials [4] |
| `video_generate` | `XAI_API_KEY` or `FAL_KEY` [4] |
| `session_search` | the SQLite state database |
| `cronjob_manage` | an interactive, gateway or cron session |
| `kanban_*` | `HERMES_KANBAN_TASK` or the profile's `kanban` toolset |
| `terminal` | the configured backend's requirements |

**Web search and page reading** (examined most closely by the research report [29])
- **Names.** Model-visible `web_search` (`tool_use.name` in the stream) [S 12][S 14]; parameters in the Core table. `web_extract` returns markdown or text with no LLM summary and also reads PDFs [S 12][4].
- **Where it runs.** In the Hermes process, as HTTP to a backend, not at the model provider [S 12]. Backend: `web.search_backend`, else `web.backend`, else the first that autodetects: tavily, perplexity, exa, parallel, keenable, firecrawl (a key, or the Nous gateway), searxng (`SEARXNG_URL`), brave-free (`BRAVE_SEARCH_API_KEY`), ddgs (a package, no key), then plugin providers; last fallback `firecrawl` [S 12]. `web.backend: nous` uses the paid Nous Tool Gateway (Install and locate), no separate key [S 12][9].
- **Account.** One backend key (`EXA_API_KEY`, `PARALLEL_API_KEY`, `TAVILY_API_KEY`, `PERPLEXITY_API_KEY`, `KEENABLE_API_KEY`, `FIRECRAWL_API_KEY` or `FIRECRAWL_API_URL`), the Nous subscription, or a keyless tier (`ddgs`, `SEARXNG_URL`) [S 12][4]. With none, the tool is not offered (`check_fn=check_web_api_key`) [S 12][S 17].
- **Switch.** `web` is in the shared core list, so `hermes-cli` has it; also `hermes-acp`, `hermes-api-server`, `hermes-webhook`, `coding`, `debugging` and `safe` [S 11][5]. Off with `agent.disabled_toolsets: [web]` or `[search]` (removed per tool, last) [10][S 19][S 16], `hermes tools`, or `/tools disable` [5].

**Web results and limits**
- **Result shape.** `web_search` returns a JSON string `{"success":..,"data":{"web":[{title,url,description,position}]}}`, read at `main`; the tag's shape is **[U]** [S 12]. A stream-json consumer sees it only under 5000 characters (a `limit` near 100 is cut) [S 14]. `web_extract` returns entries `{url,title,content,error}`, cut at 5000 in the stream [S 13].
- **Page-reading limits.** None per turn, none tied to search results [S 13]. Only a denylist, `security.website_blocklist: {enabled: true, domains: ["*.internal.company.com"], shared_files: ["/path/list.txt"]}`, and an always-on floor against private, loopback, link-local, CGNAT and cloud-metadata addresses (`security.allow_private_urls`, default false) [7]. URLs that look like they carry an API key are refused [S 13].
- **Fail-open check.** `_extract_safe_urls` treats an exception in the policy check as "not blocked" [S 13].
- **Untrusted content.** No scanning of web content or tool results is documented; only context files (`AGENTS.md`, `.cursorrules`, `SOUL.md`) are scanned [7]. The `web_search` and `web_extract` schemas carry no untrusted-content warning [S 12].

**Approvals and read-only switches**
- Dangerous `terminal` commands need approval [29][7]; `approvals.single_query_mode` defaults to `deny` for one-shot runs [7]; `--yolo` or `HERMES_YOLO_MODE=1` turns approvals off [R 23].
- `--toolsets` names whole toolsets and the registry returns only the requested tool names [3][S 16][S 17]; a bare tool name is refused (`Unknown toolset`) [S 16].
- No built-in toolset holds the read tools and search without the write tools [5][S 11]; no answer-only flag exists; which toolset removes tools and write access is planned check HM7 [R 23][26].

| `--toolsets` | tools left | note |
|---|---|---|
| `search` | `web_search` | [S 11] |
| `web` | `web_search`, `web_extract` | [S 11][5] |
| `search,file` | `web_search`, `read_file`, `search_files`, `write_file`, `patch` | removes terminal, `execute_code`, browser, delegation, memory, skills and `web_extract`; still has the two write tools [S 11] |
| `safe` | `web_search`, `web_extract`, `vision_analyze`, `image_generate` | no file tools [S 11] |
| `hermes-webhook` | `web_search`, `web_extract`, `vision_analyze`, `clarify` | its source comment says payloads are untrusted [S 11] |

| question | status | what would settle it |
|---|---|---|
| choose the tools of a turn | Established for a new session per turn: `--toolsets` selects whole toolsets [3][S 16][S 17]; not for a resumed session (launch flags frozen, `/tools` not an allowed slash command) [6]. MCP or plugin tools joining a `--toolsets file` turn: **[U]** | `hermes chat --oneshot -q "list your tools" --toolsets file --format stream-json` in a scratch `HERMES_HOME`; the model should name no web tool (a model turn; not run) [29] |
| see the queries and pages a turn used | Established from source: `web_search` query in `tool_use.input.query`, URLs in `tool_result.output`; `web_extract` URLs in `input.urls`, text in `output`; cut at 5000 characters [S 14]. **[U]**: callbacks in quiet `-q`; the result shape at the tag | `... --toolsets search`; observe the query and URLs (a search; not run) [29] |
| a turn without write tools | **[U]**. (a) `custom_toolsets` in `config.yaml` is documented for toolset names [5], but no loader was found in `tools_config.py` [S 19] and `create_custom_toolset(name, description, tools, includes)` takes tool names [S 11]: syntax **[U]**. (b) A shell `pre_tool_call` hook can block or fail closed [8]; payload and matcher **[U]**. (c) `agent.disabled_toolsets` removes by toolset name only; no toolset holds just `write_file` and `patch` [S 16][S 11] | a scratch config with the candidate `custom_toolsets` entry, then `hermes tools` and `hermes hooks list` (no model turn); or `hermes hooks test pre_tool_call` to read the payload [29] |
| read only URLs a search returned | **[U]**: no flag or setting [S 13]; only a denylist [7]. A hook keeping the URLs seen in `post_tool_call` results needs the shell payload to carry them; the plugin payload has `result` [8] | `hermes hooks test post_tool_call` with a logging hook; read its stdin fields [29] |
| stateless run and resume by id | Established for stateless runs [S 15] and resume by id [6]; a read-only run depends on the third row | as above |

### Branches, forks and subagents

- **Fork.** `/branch` (alias `/fork`) inside a session: a new row with `parent_session_id` and `model_config._branched_from`, and the parent's active rows up to a boundary copied into it [34]. mukade lists it as a session of its own, `kind: fork`.
- **Resume.** `hermes chat --resume <id|title|latest>`, `--continue [title]`; the design draft also writes
  `hermes --resume <id|title|latest>` and `hermes -c [name]` [32]. A resume moves back to the session's
  recorded directory unless `--no-restore-cwd` [6][R 23]. One session resumed in two terminals is planned
  check HM6 [26]. What `hermes -c` does with a compression chain is **[U]** [32].
- **Sub-agents.** The `delegate_task` tool (Tools) [S 22]. Each run is its own session row: `source = 'subagent'`, `parent_session_id`, and `model_config._delegate_from = <parent>`; hidden from Hermes's pickers [34]. mukade lists them with `--children` and carries them with `hop --children` (`kind: spawn`). Logs under `cache/delegation/live/` are pruned after 7 days; Ctrl+T opens a roster in the classic CLI [R 23]. The delegation backends are pluggable, so a sub-agent's work must not be assumed inlined as text [28]. Whether its calls appear in the parent's `stream-json` is **[U]**.
- **Kanban.** The 15 `kanban_*` tools are off by default and belong to dispatcher workers and an
  orchestrator [S 22].

### Models and providers

- **Providers.** Many, among them OpenRouter, Nous, OpenAI Codex, Anthropic, Gemini and Bedrock [R 23]. The
  complete list is **[U]**.
- **Choosing and signing in.** `hermes model`, `hermes setup`, `hermes auth add <provider> [--type
  oauth|--api-key]`. Status without secrets: `hermes status`, `hermes auth list`, `hermes auth status
  <provider>`. Secrets live in `<home>/.env` and `auth.json` [R 23]. `/model` changes the model for the
  session [R 23]. `--model` and `--provider` exist as flags: `--ignore-user-config` needs them [6].
- **What is recorded.** `sessions` has `model` and `model_config` [R 24]; the stream has `model` in
  `system/init` and `tokens {input, output, total, cache_read, cache_write}` in `result` [S 14].
- **Reasoning.** Which provider or protocol each assistant row used, and how reasoning is stored
  (`reasoning`; in the design draft also `reasoning_content`, `reasoning_details`, `codex_reasoning_items`,
  which may hold signed or encrypted payloads on some routes), is **[U]** [R 24][32]. A secondary report
  calls reasoning persistence not a stable cross-model replay contract [28].

### Plugins and extensions

| mechanism | where it lives | how it is added |
|---|---|---|
| MCP client | `mcp_servers:` in the profile's `config.yaml` (stdio or url) | `hermes mcp add\|list\|remove\|test` [R 23]. Each server becomes a toolset `mcp-<server>` [5]. No per-launch MCP switch; ACP attaches `mcpServers` per session at `session/new` [R 23] |
| skills | `<home>/skills/<name>/SKILL.md` (agentskills.io format); `skills.external_dirs` (for example `~/.agents/skills`); a project's `.hermes/skills/` or `.agents/skills/` only after `hermes skills trust` [R 23] | the folder; the `skill_manage` tool creates, patches and deletes skills, and `skills.write_approval` stages those writes [S 21][S 22] |
| plugins | `<home>/plugins/`: `plugin.yaml` plus Python, off until enabled [R 23] | the enabling command is **[U]** |
| Agent Plugins v1.0.0 packages | a package with `plugin.json`, `skills/` and `mcp.json` [R 23] | the install command is **[U]** |
| catalog plugins and hub skills | the catalog; Honcho, Home Assistant and Spotify are catalog plugins [4] | the `manage_catalog` tool, through an approval card, in desktop sessions only [S 11][4] |
| search providers | plugin providers come after the built-in search backends [S 12] | `web.search_backend` or `web.backend` [S 12] |
| custom toolsets | `custom_toolsets` in `config.yaml` is documented [5]; no loader was found in `tools_config.py` [S 19] | whether it works, and its syntax, is **[U]** |
| context files | `.hermes.md`/`HERMES.md`, `AGENTS.override.md`, `AGENTS.md` (merged from the git root down), `CLAUDE.md`, `.cursorrules`, `.cursor/rules/*.mdc`: only the first project file type found is loaded; `SOUL.md` always, from the Hermes home only [R 23] | placed in the project or the home. Context files are scanned for prompt injection; web content and tool results are not [7] |

Claude Code plugins and marketplaces are not loaded [R 23]. The Nous Tool Gateway is a paid service, not an
extension [9].

### Hooks

Shell hooks live in the profile's `config.yaml` under `hooks:`, each with a `matcher`, a `command`, a
`timeout` and `fail_closed` [R 23]. A hook gets a JSON payload on stdin with `session_id` and `cwd`, and
answers in a block-or-modify format compatible with Claude Code's [R 23]. The other payload fields and the
matcher syntax are **[U]**: the hooks page's shell-hook section was not reached (first 100000 of 149287
characters read) [8]. `hermes hooks list` and `hermes hooks test <event>` (shows the shell-hook payload) are
the cheap probes; neither was run [29].

| group | events [R 23] |
|---|---|
| tools | `pre_tool_call`, `post_tool_call` |
| model calls | `pre_llm_call`, `post_llm_call` |
| session | `on_session_` followed by `start`, `end`, `finalize`, `reset` or `delete` |
| compaction | `post_compaction` |
| sub-agents | `subagent_` followed by `start` or `stop` |
| commands | `pre_command` |

- **Blocking.** A `pre_tool_call` shell hook can block a call or fail closed [8].
- **Consent.** Each new pair of event and command is approved once and recorded in
  `shell-hooks-allowlist.json`; `--accept-hooks` or `HERMES_ACCEPT_HOOKS=1` skips the approval [R 23].
- **No per-launch switch.** Only `HERMES_HOME` and the profile change hooks, and both also switch the
  session store [R 23].
- **Plugin hooks.** A plugin's `post_tool_call` payload carries `result` [8]; whether a shell hook's does is
  **[U]**.
- **mukade** neither reads nor writes hook configuration: it concerns sessions only.

## How mukade reads it

Code: `src/reader/hermes.rs`, adapter `src/harnesses/hermes.rs`, detector `src/detect/hermes.rs`; tests `tests/hermes_reader.rs` over a `state.db` the test builds from the v0.23.1 schema subset (`tests/support/hermes_db.rs`), and the ignored census `tests/hermes_live.rs` over the real store [37].

**What you may miss**

- **The gate.** `schema_version` below 32, or a missing required column, is refused naming it. Required: `sessions` `id`, `source`, `parent_session_id`, `model_config`, `started_at`, `end_reason`; `messages` `id`, `session_id`, `role`, `content`, `tool_call_id`, `tool_calls`, `timestamp`, `active`.
- **Read-only.** The store is opened with `SQLITE_OPEN_READ_ONLY` and a 2 s busy timeout, never `immutable=1`: a gateway may be writing. One retry on a transient `SQLITE_IOERR` / `SQLITE_BUSY`.
- **One conversation.** The `active = 1` rows in `id` order are the conversation. `active = 0` rows are kept as `Meta(OffBranch)`: archived by a compaction, or rewound. A `/branch` child holds a copy of its parent's rows, so a hop of the child carries them once.
- **A rotated chain is one session.** A legacy compaction that rotated the id (`end_reason = 'compression'`) lists only its tip; `read` follows `parent_session_id` while the parent ended that way and joins the segments, root first.
- **Unknown cells.** NULL, BLOB and numeric `content` cells are legal and read by type; a `\x00json:` cell is a list of parts.

**Finding sessions**

- **Listing.** Every session of the store, newest first by `last_activity_at`, else `ended_at`, else `started_at`: `id`, title (the stored one, else the first user text), `cwd`, update time, `archived` (`archived` or `auto_archived`). `list` leaves out what Hermes's pickers leave out (`INTERNAL_LISTING_SOURCES` = `kanban`, `tool`, `oneshot`: workers, integrations and finite `-q` / `-z` runs, which `hermes sessions list` hides too, and shows with `--source oneshot`), the subagent sessions and the superseded half of a rotated chain; `list --children` (the explorer's listing) adds all of them. A hidden session is read by its id like any other. A `local` session is a person's and is listed. A `/branch` copy is a session of its own with `kind: fork`.
- **Ids.** `read` takes an exact id or the only id it is the start of.
- **`--from-file`.** Reads one session from a `hermes sessions export --format jsonl` file.

**How rows become entries**

| row | becomes |
|---|---|
| `user` | a user entry; typed by the person unless its `display_kind` marks Hermes scaffolding (`hidden`, `internal_notification`, `async_delegation_complete`, `process_complete`, `auto_continue`, `model_switch`, `personality_switch`: a marker, not a prompt) |
| `assistant` | a step: text, reasoning, and its `tool_calls` |
| `tool` rows after a step | one results entry, paired by `tool_call_id` |
| `_compressed_summary = 1` | a compaction entry; the summary text without Hermes's reference-only prefix, merge delimiters and end marker |
| `active = 0` | `Meta(OffBranch)` |
| `reasoning`, `reasoning_content` | reasoning text; `reasoning_details` and `codex_reasoning_items` stay in the entry's raw record, kept as an opaque payload when their format names one protocol |

- **Tool names.** `CanonicalTool::from_hermes` (`src/model/tool.rs`): `read_file` → `Read`, `write_file` → `Write`, `patch` → `Edit`, `terminal` → `Bash`, `search_files` → `Grep` (a `target` of `files` is a find), `web_search` → `WebSearch`, `delegate_task` → `Agent`. Any other name stays itself and travels as text. See [tool conversion](../tool-conversion-tables.md).
- **Chat origin.** A user row with no scaffolding `display_kind` is `Typed`; the `mukade chat` observer shows only those.

## How mukade writes it

Code: `src/writer/hermes.rs`; live record [35]. Hermes has no way to add a session except its own import, so mukade never opens `state.db` for writing.

**What you may lose**

- **Tools are text.** The import keeps a tool call as `[ran tool: <name>]` and drops the result [33][35]. So the session is planned with no tool offered: every call, its arguments and its result are written as text inside the turns. `tool_calls` is 0 in the manifest and `tools_text_form` counts them.
- **User and assistant rows only.** Consecutive turns of one role are merged; a user turn that opens with a context tag (`<system-reminder>`, `<environment_context>`, …) is dropped as injected context.
- **No model, no reasoning, no image.** Hermes records no model for an import; reasoning is dropped (`--reasoning` has nothing to carry); an image is written as `[image]`.
- **The folder.** Hermes keeps the working directory only when it exists, so a session whose folder is not one here names none (the receipt's `cwd` is null).
- **The id and the title** are Hermes's: `YYYYMMDD_HHMMSS_<hex>`, and `Imported from Claude Code: <first line>` when Hermes titles it in time. `--dest-id`, `--dest-agent` and `--metadata-file` are refused as a command-line mistake (exit 1, as toward every destination that does not take them), `--dest-path` and `--compaction native` as unsupported (exit 2); each before anything runs.

**How**

1. mukade writes the session as a Claude Code JSONL (user and assistant rows) in its own temporary folder, planned with no tool offered.
2. It runs `hermes sessions import --from claude <file>` with `HERMES_HOME` set to the destination and `PYTHONIOENCODING=utf-8`, under a 180 s deadline.
3. It parses `Imported Claude Code session as <id>` from the output, deletes its file (whatever came of the import) and returns the id. An import that failed or did not confirm may still have written: the error is the uncertain-outcome kind (exit 5) and names `state.db`.
4. Resume: `hermes chat -Q --resume <id> -q` (the receipt's command), interactive `hermes --resume <id>`.

**Verified** on v0.23.1: a Claude Code fixture hopped into the real store read back with mukade's reader, its user and assistant texts intact [35].

## How mukade summarizes it

**Refused.** `--compaction summary` from a Hermes session exits 2 with the reason. The run every other harness uses puts the session in front of its model and saves nothing; Hermes has no such run.

- `hermes chat --resume <id> -q <prompt> -Q` is the only way to show the model the session, and it **appends** the question and its answer to that session. Checked on 2026-11-12: a session of 2 messages had 4 after one resumed question [38].
- Without `--resume` the model cannot see the session. No flag turns persistence off, a `-z` / `--oneshot` run stores a session (`source = oneshot`), and the rows a run adds cannot be removed (`hermes sessions` has no message delete).
- Use `--compaction portable` (the stored summary row travels, when the session compacted) or `native`; or ask Hermes yourself and hop with the text.

## Open points

Ranked by what they cost a user. The rest: [`../../next.md`](../../next.md).

1. **Reasoning payloads [U].** `reasoning_details` and `codex_reasoning_items` are read as opaque payloads; whether a destination accepts them is untested (the Hermes writer drops reasoning).
2. **Resuming a written session with a model [U].** The import and the read-back are verified; `hermes --resume <id>` with a provider has not been run on a mukade-written session. A free OpenRouter model settles it.
3. **Untitled imports.** One import was left without a title; the earlier one had `Imported from Claude Code: …`. Why is not established (a race with Hermes's title worker is a guess).
4. **A cwd that exists.** The import keeps the folder only when it exists; verified by tests with a stand-in, not live (the live fixture's folder is not on this machine).
5. **v0.22.5 (schema 32) [U].** Read by source and by the schema, never on a store that version wrote.
6. **Profiles [U].** A named profile is read by pointing `HERMES_HOME` at its folder; whether `HERMES_HOME` alone keeps every Hermes process inside it is untested.

## Sources

All web pages and raw files accessed 2026-11-01 unless a source says otherwise. Sources 1 to 10: the Hermes README, the GitHub release record and the Hermes docs site (`<docs>` = `https://hermes-agent.nousresearch.com/docs`). Sources 11 to 22: Hermes files at tag `v0.22.5` (`<raw>` = `https://raw.githubusercontent.com/NousResearch/hermes-agent/v0.22.5`), each read through a web fetch tool's page extraction, which gives function names but no line numbers and may drop lines. Sources 23 to 26, 29: documents of the Shinatama repository, a separate project; their facts are research, labelled R, and no longer the basis of anything mukade does. Sources 33 to 38: this repository and this machine, 2026-11-12.

1. Nous Research, *hermes-agent* README, <https://github.com/NousResearch/hermes-agent>.
2. GitHub releases API for `NousResearch/hermes-agent`, <https://api.github.com/repos/NousResearch/hermes-agent/releases?per_page=5> (tag `v0.22.5`, commit `bf0203a794a07aac3ff764972ab9087baa4e4f4e`, `published_at` 2026-10-23T05:28:45Z).
3. Hermes docs, *Tools*, `<docs>/user-guide/features/tools`.
4. Hermes docs, *Tools reference*, `<docs>/reference/tools-reference`.
5. Hermes docs, *Toolsets reference*, `<docs>/reference/toolsets-reference`.
6. Hermes docs, *CLI*, `<docs>/user-guide/cli`.
7. Hermes docs, *Security*, `<docs>/user-guide/security`.
8. Hermes docs, *Hooks*, `<docs>/user-guide/features/hooks` (the first 100000 of 149287 characters; the shell-hook section was not reached).
9. Hermes docs, *Tool gateway*, `<docs>/user-guide/features/tool-gateway`.
10. Hermes docs, *Configuration*, `<docs>/user-guide/configuration` (the first 100000 of 284048 characters).
11. `<raw>/toolsets.py`.
12. `<raw>/tools/web_tools.py` (the `web_search` return shape was read at `main`, not at the tag).
13. `<raw>/tools/web_tools_extract.py`.
14. `<raw>/hermes_cli/stream_json.py`.
15. `<raw>/hermes_cli/cli_single_query.py`.
16. `<raw>/model_tools.py`.
17. `<raw>/tools/registry.py`.
18. `<raw>/tools/tool_search.py`.
19. `<raw>/hermes_cli/tools_config.py`.
20. `<raw>/acp_adapter/tools.py`.
21. `<raw>/tools/write_approval.py`.
22. `<raw>/tools/<file>.py` for each tool of the Tools tables: `file_tools`, `browser_tool`, `terminal_tool`, `process_registry`, `vision_tools`, `code_execution_tool`, `image_generation_tool`, `skills_tool`, `skill_manager_tool`, `memory_tool`, `session_search_tool`, `delegate_tool`, `cronjob_tools`, `todo_tool`, `clarify_tool`, `tts_tool`, `computer_use_tool`, `x_search_tool`, `kanban_tools`.
23. Shinatama technical design §5.7, research task: `D:\Projects\shinatama\docs\technical-design.md`, the "Hermes Agent" entry (lines 1030-1094) and the Hermes lines of §5.6 (844-847) and §5.7 (1167-1203). Facts from the research of 2026-10-31 on the release, its source and its docs; nothing run there; Shinatama's label is R.
24. Shinatama technical design §5.3, research task: `D:\Projects\shinatama\docs\technical-design.md` lines 487-505, the Hermes store entry: "R: v0.22.5 source, 2026-10-31; not observed here, Hermes is not installed on this machine".
25. Shinatama mukade handover §9, "Hermes Agent and DeepSeek Harness (requested 2026-10-31)": `D:\Projects\shinatama\docs\mukade-handover.md` lines 272-294.
26. Shinatama verification register, rows HM1 to HM14: `D:\Projects\shinatama\docs\verification-register.md` lines 218-231. They are checks still to be run, not results.
27. mukade, [`adr/ADR-0002-hermes-and-deepseek-harness-read-first.md`](../adr/ADR-0002-hermes-and-deepseek-harness-read-first.md): amended 2026-11-12: Accepted with changes (Hermes is a destination through its import; schemas 32 and 33).
28. mukade, [`reference/harness-session-persistence-reference.md`](../reference/harness-session-persistence-reference.md), entry "Hermes Agent — `v0.22.5`". A secondary research report pasted by the user on 2026-10-27; its own header says its claims were not re-run here.
29. Shinatama research report "Hermes Agent" of the web-tools run, 2026-11-01: `D:\Projects\shinatama\.daodan\runs\init-467ba8d96c1b4104b3ac1edb707caffa\web-tools\hermes-agent.md` (the 86-row tool list, web search and page reading, headless and ACP events, toolset switches, verdict). Its `[S]` and `[D]` references are sources 11 to 22 and 3 to 10 above.
30. Search of the mukade tree, 2026-11-01 (no Hermes code then): superseded by the implementation, source 37.
31. `Get-Command hermes -All`, 2026-11-01: not installed then; installed 2026-11-12, source 36.
32. mukade, [`superpowers/specs/2026-10-31-hermes-deepseek-harness-support-design.md`](../superpowers/specs/2026-10-31-hermes-deepseek-harness-support-design.md) §4.1, §5.2, §5.3, §5.4, §5.5, §5.6, §6, §7, §8: the first design draft (superseded by source 33), which reads Hermes's session-storage and sessions documentation; what it marks **[U]** was not exercised against a live install.
33. mukade, [`superpowers/specs/2026-11-12-hermes-deepseek-design.md`](../superpowers/specs/2026-11-12-hermes-deepseek-design.md): the decisions for the reader, the writer and the harness id; it supersedes ADR-0002's source-only, schema-31 plan.
34. Research of 2026-11-12 on Hermes v0.23.1 (installed copy, byte-identical to the tag for the state modules) and v0.22.5: `hermes_state_common.py`, `hermes_state_schema.py`, `hermes_state_messages.py`, `hermes_state_ids.py`, `context_compressor.py`, `sessions_cmd.py` at file:line, `hermes --version`, `hermes sessions export --help`. The report is kept in the run's scratchpad, not in the repository. [V]
35. mukade, [`verification/2026-11-12-hermes-writer-live.md`](../verification/2026-11-12-hermes-writer-live.md): a Claude Code fixture hopped into the real store and read back. [V]
36. This machine, 2026-11-12: Hermes v0.23.1 under `%LOCALAPPDATA%\hermes`; `hermes chat --help`, `hermes sessions list`, runs with the free OpenRouter model `cohere/north-mini-code:free`, and the store read read-only. [V]
37. mukade code and tests: `src/reader/hermes.rs`, `src/writer/hermes.rs`, `src/harnesses/hermes.rs`, `src/detect/hermes.rs`, `tests/hermes_reader.rs`, `tests/writer_hermes.rs`, `tests/hermes_live.rs`. [V]
38. This machine, 2026-11-12: `hermes chat --resume <id> -q "Write a one-sentence summary of this session" -Q` on a session of 2 messages (`mukade dump hermes <id>`): 4 messages after. [V]
