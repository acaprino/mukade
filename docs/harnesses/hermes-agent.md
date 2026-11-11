# Hermes Agent

Hermes Agent is Nous Research's MIT-licensed Python agent, with a terminal front end, a background gateway
per profile and an ACP mode [1][R 23]. mukade does not support it yet: no reader, no writer; ADR-0002
(Proposed) plans read-first support with Hermes as a source only [27].

Evidence: `[n]` = source n in [Sources](#sources); `[S n]` = Hermes source at tag `v0.22.5`; `[R n]` =
Shinatama research, nothing run; **[V]** = observed on this machine; **[U]** = not established (the text
says what would settle it).

## Status in mukade

| | |
|---|---|
| Read (`list`, `dump`) | ❌ No reader [V][30]. ADR-0002 plans one: `state.db` opened read-only, schema 31 only [27] |
| Write | ❌ No writer. ADR-0002 plans none: Hermes stays source-only [27] |
| Hop as source | ❌ Planned: `mukade hop hermes <session-id> claude_code` (EX-001) [27] |
| Hop as destination | ❌ Planned to stay refused: exit code 2 before any plan or write (EX-002, INV-002) [27] |
| Resume | ❌ No resume command in a receipt: mukade would never write a Hermes session [27] |
| Summary by its own model | ❌ The design draft refuses `--compaction summary` for Hermes in its first phase [32] |
| Native compaction | ❌ Hermes is never a destination, so no native record is written; as a source, the draft refuses `--compaction portable` (no stored summary row [U]) [27][32] |
| Notable gaps | ❌ No `HarnessId`, catalog entry, fixture, test or tool-table row [V][30]. Hermes is not installed here and was never run [V][29][31] |

## Quick start

mukade cannot read Hermes today. `mukade list hermes` stops with
`unknown harness "hermes" (claude_code, codex, opencode, pi)` (`src/model/harness.rs` `HarnessId`,
`FromStr`) [V][30].

Inspect the store by hand (none of this was run: Hermes is not installed here [V][31]):

```sh
hermes status                                   # install status; prints no secrets
hermes sessions list                            # a text table, no JSON
hermes sessions export - --session-id <id> --format jsonl   # or md | html | trace (Claude Code JSONL)
hermes chat --resume <id|title|latest>          # resume it in Hermes
```

To read `state.db` itself, open it read-only, for example `sqlite3 -readonly "<home>/state.db"`, then
`SELECT * FROM schema_version;` (not run either). This settles the schema-version question (Open points).
Even a read-only open of a WAL database can create `-wal` and `-shm` sidecars [32].

Planned mukade commands (ADR-0002, not implemented) [27]:

```sh
mukade list hermes                            # EX-003: refused, naming the version, unless schema_version is 31
mukade hop hermes <session-id> claude_code    # EX-001: a new Claude Code session; the manifest lists every loss
mukade hop claude_code <session-id> hermes    # EX-002: exit code 2, "Hermes is read-only in mukade", nothing written
```

## Install and locate

| | |
|---|---|
| Vendor | Nous Research; repository `NousResearch/hermes-agent` [1][2] |
| Licence, source | MIT; <https://github.com/NousResearch/hermes-agent> [1] |
| Docs | <https://hermes-agent.nousresearch.com/docs/> [1] |
| Version described | v0.22.5: tag `v0.22.5`, commit `bf0203a794a07aac3ff764972ab9087baa4e4f4e`, published 2026-10-23T05:28:45Z by the GitHub releases API [2]. Shinatama's design and the design draft give 2026-10-22 [R 23][32]; the persistence reference: release notes 2026-10-22, GitHub 2026-10-23 [28]. Described from documentation and source only [V][29][31] |
| Page updated | Hermes facts as read on 2026-11-01; mukade facts re-checked on 2026-11-11 [V][30] |
| Runtime | Python 3.11 to 3.14; installed by its own script with a bundled uv [1][R 23] |
| Agent loop | `hermes chat` finds or starts the profile's background gateway, creates or resumes the session there and attaches over a local WebSocket. Closing the terminal only detaches and the turn keeps running; Ctrl+C stops it. Launch flags are fixed when the session is created; a resume cannot change them [6][R 23] |
| Front ends | classic CLI (prompt_toolkit, the default, writes to normal scrollback); `hermes chat --tui`, an alternate-screen Node TUI with mouse tracking on by default; the ACP server `hermes-acp`; toolsets for messaging platforms (Discord, Feishu/Lark, Yuanbao) and a desktop app (`desktop_ui`, `project`, `onboarding`, `catalog`) [R 23][S 11] |
| Windows | native: a UTF-8 console shim; the gateway runs detached through `pythonw` [R 23][32] |
| mukade name | none yet. ADR-0002's examples write `hermes`; the design draft proposes that id as the author's own call [27][32] |

**Docs against the tag.** The docs site shows current documentation, which differs from the tag: its
`browser` toolset lists 4 tools [5], the tag 14 [S 11]. The tag wins [29]: this page takes behaviour from the
docs and tool names from the tag. Read only in part: the hooks page (first 100000 of 149287 characters; the
shell-hook section was not reached) [8] and the configuration page (first 100000 of 284048) [10].

**Install**

| OS | method | exact command |
|---|---|---|
| macOS, Linux, Windows | Hermes's own install script, which bundles uv; Python 3.11 to 3.14 [R 23] | **[U]**: the script's URL and command are not in the sources read. The README [1] and the docs site [6] would settle it |
| Windows | the same script; runs natively (UTF-8 console shim, gateway detached through `pythonw`) [R 23][32] | **[U]** |

Update, uninstall and side-by-side installs: **[U]**.

**Locations**

| what | Windows | macOS / Linux |
|---|---|---|
| Launcher | `%LOCALAPPDATA%\hermes\bin\hermes`; native executable or shim **[U]**, planned check HM1 [R 23][26] | **[U]** |
| Hermes home (default) | `%LOCALAPPDATA%\hermes` [R 24][32] | `~/.hermes` [R 24][32] |
| Move the home | `HERMES_HOME` [R 24] | `HERMES_HOME` [R 24] |
| Session database | `<home>\state.db` [R 24][32] | `<home>/state.db` [R 24][32] |
| Named profile | `<root>\profiles\<name>\`, with its own `state.db` [32] | `<root>/profiles/<name>/`, with its own `state.db` [32] |
| Secrets and login | `<home>\.env`, `<home>\auth.json` [R 23] | `<home>/.env`, `<home>/auth.json` [R 23] |
| Skills, plugins | `<home>\skills\`, `<home>\plugins\` [R 23] | `<home>/skills/`, `<home>/plugins/` [R 23] |
| Settings, hook approvals | the profile's `config.yaml`, and `shell-hooks-allowlist.json`; exact paths **[U]** [R 23] | the same **[U]** |

- **One lever.** `HERMES_HOME` and the profile (`-p`) are the only levers, and they move settings, hooks and
  the session store together; there is no per-launch switch for hooks or MCP [R 23]. Whether `HERMES_HOME`
  alone keeps everything inside it is planned check HM9 [26].
- **Find an install.** `Get-Command hermes -All` (PowerShell) or `command -v hermes` (macOS/Linux), then
  `hermes status`. Neither `hermes --version` nor `--help` was ever run; their output is **[U]**. Here `Get-Command hermes` found
  nothing on 2026-11-01 [V][31], and the earlier research found nothing with `where hermes` [29].
- **mukade discovery.** `mukade harnesses` does not list Hermes: `catalog/harnesses.json` has no entry. The
  detectors under `src/detect/` cover only the four supported harnesses, so `detect`, `scan` and `--root`
  do not recognize a Hermes home [V][30]. The shared per-OS procedure is in the
  [harness catalog](../reference/harness-catalog.md#finding-existing-installations).
- **Paid parts.** The Nous Tool Gateway (search through Perplexity, page extraction through Firecrawl) needs a
  paid Nous subscription [9][S 12]. The licence of the desktop app that `desktop_ui` implies was not
  checked **[U]**.

## Session store

Nothing here was observed: no `state.db` was opened [V][31]. It rests on Shinatama's research of 2026-10-31
on the v0.22.5 source [R 24] and on the design draft, which read Hermes's session-storage documentation [32].

| path | holds |
|---|---|
| `<home>/state.db` | every session of the profile: SQLite with FTS5 and WAL [R 24][32] |
| `<root>/profiles/<name>/state.db` | a named profile's own sessions [R 24][32] |
| `<home>/sessions/` | a JSONL copy, written only as a fallback [R 24] |
| `state.db-wal`, `state.db-shm` | WAL sidecars; a read-only open can create them [32] |
| `cache/delegation/live/` | sub-agent logs, pruned after 7 days [R 23] |
| `sessions.json`, `kanban.db`, `response_store.db` | named by a secondary report: gateway session mappings (rewritten atomically), kanban, responses; none opened **[U]** [28] |

**Schema version.** Two sources disagree **[U]**: **32** from Shinatama's reading of the v0.22.5 source
[R 24]; **31** in ADR-0002 and the design draft, which reads it from Hermes's session-storage documentation,
in a `schema_version` table [27][32]. The version in a `state.db` made by v0.22.5, or `SCHEMA_SQL` at the
tag, would settle it.

**Tables.** Shinatama's research cuts the `sessions` list with "…" [R 24]; the draft's extra columns come
from an abridged documentation page. The full set in `SCHEMA_SQL` is **[U]** [32].

| table | columns named by Shinatama's research [R 24] | further columns named by the design draft [32] |
|---|---|---|
| `sessions` | `id`, `source`, `profile_name`, `cwd`, `git_branch`, `git_repo_root`, `title`, `parent_session_id`, `model`, `model_config`, `started_at`, `ended_at`, `end_reason`, `last_activity_at`, `message_count`, `archived`, `hidden`, `pinned`, … | `system_prompt`, token and cost counters |
| `messages` | `id`, `session_id`, `timestamp`, `role`, `content`, `reasoning`, `tool_calls`, `tool_call_id`, `tool_name`, `active`, `compacted` | `reasoning_content`, `reasoning_details`, `codex_reasoning_items`, `codex_message_items`, `message_uid`, `tool_call_uids`, `api_content` |

**Values and order**
- Session ids look like `YYYYMMDD_HHMMSS_<hex>` [R 24]; the caller cannot choose one (Ephemeral runs).
- `messages.id` is an autoincrement integer and gives the order within a session; `timestamp` is an epoch
  float; `tool_calls` is a JSON string [32]. Role names and the JSON shape of `content` and `tool_calls`:
  **[U]** [R 24].
- `active=1` rows are the conversation; `active=0` rows are a generation a compaction replaced [R 24][32].
- `sessions.source`: `cli`, `telegram` and other gateway sources; `oneshot`, `tool` and `kanban` for runs
  Hermes's pickers hide [32]. That a one-shot run therefore leaves a row is read from documentation only
  **[U]**.
- Retention: no pruning by default; `sessions.auto_prune` prunes at 90 days when turned on [R 24].

**Lineage**
- `sessions.parent_session_id` links a session to its parent [R 24].
- A sub-agent run is its own row with `parent_session_id` and a `_delegate_from` marker in `model_config`,
  hidden from pickers [R 24].
- Compaction: the draft says compression can split a session into a child linked the same way [32];
  Shinatama's research says the id is kept (Compaction) **[U]**.
- A fork (`/branch`): how it is recorded is **[U]** (Branches, forks and subagents).

**Writer.** The profile's gateway is documented as the only writer of canonical sessions; out-of-process
writers go through it [27][32]. ADR-0002 and the draft state that no command imports a session (upstream
issue #8650) [27][32]; Shinatama's research reports `hermes sessions import --from claude|codex` (Open
points) [R 23]. Hermes may be running while mukade reads [32]; whether a read-only open works while the
gateway writes is planned check HM3 [26].

**Reading routes**
- `hermes sessions list`: a text table, no JSON [R 23].
- `hermes sessions export <out|-> --session-id <id> --format jsonl|md|html|trace`: `trace` is Claude Code
  JSONL [R 23]; the shape of a `jsonl` row is **[U]**. A secondary report mentions a filtered user-prompts
  view [28].
- Shinatama treats the export as primary and `state.db` as a read-only fallback for listing and search,
  because the schema is versioned and moves [R 24]. The design draft reads `state.db` directly and uses the
  export only as a cross-check [32].
- A hand-made schema-31 database is all the draft has to build a reader against; a real session is awaited
  from the user [32].

**Shinatama's intended mapping** to its own canonical transcript (not to mukade's entries) [R 24]:

| Hermes rows | Shinatama canonical |
|---|---|
| `user` and `assistant` rows | `user_text`, `assistant_text` |
| `reasoning` | `reasoning` |
| `tool_calls` | `tool_call` |
| rows with a `tool_call_id` | `tool_result` |
| rows with `active=0, compacted=1` | not followed; the summary is marked as `compaction` |
| a sub-agent session | one tool pair |

How a summary row is told apart in `messages` is **[U]**: the draft says upstream documents no stored
summary row [32].

## Features

### Compaction

- **Trigger.** Automatic at 50% of the context window by default [R 23]. The setting that holds the
  threshold, any manual command, and a way to compact headless are **[U]**: the sources read do not name
  them. The `post_compaction` hook event exists (Hooks) [R 23].
- **On disk.** In place: the session keeps its id; the replaced rows stay in `messages` with
  `active=0, compacted=1` [R 24]. The draft describes the same pattern (old rows get `active=0`, the retained
  context is inserted as `active=1`, a protected message can appear in both generations) and adds that
  compression can also split a session into a child linked by `parent_session_id` [32]. Which happens in
  v0.22.5, or both, is **[U]**: a v0.22.5 `state.db` after a compaction would settle it.
- **Summary.** Shinatama plans to mark it as a compaction [R 24]; the draft says upstream documents no stored
  summary row, so a Hermes session yields no compaction entry [32]. Where the summary text lives, if stored,
  is **[U]**.
- **Not a deletion.** Hermes's documentation warns that compression shortens the active context but is not a
  privacy deletion [28].

### Ephemeral runs

Nothing here was run [V][31].

| goal | command | notes |
|---|---|---|
| first turn on a terminal | `hermes chat -q "<text>"` | sends the text and stays open [S 15][R 23]. No positional prompt: a bare word is a subcommand. Size limit of `-q`: not found [R 23] |
| one turn, headless | `hermes chat --oneshot -q "<text>" --format stream-json` | answers and exits; `-q` without a TTY also runs one turn and exits [S 14][S 15]. `--oneshot` and `--format` are not on the docs CLI page [6] |
| final text only | `hermes chat -z "<prompt>"` | prints only the final text [R 23] |
| prompt from a file | `hermes chat --query-file <path>` (`-` for stdin) | never shell-interpreted [6][R 23] |
| one turn in a resumed session | `hermes chat --resume <id> --oneshot -q "…"` | [32]; launch flags stay frozen on resume [6] |

- **Session id.** Not chosen by the caller. It is in `system/init` (`session_id`), in `result.session_id` of
  `--format stream-json`, and on stderr as `session_id: ...` [S 14][S 15][R 23].
- **Does a one-shot leave a session?** **[U]**. `sessions.source` has a `oneshot` value that pickers hide
  [32], which suggests it does; no flag that prevents it was found. A one-shot turn in a scratch
  `HERMES_HOME`, with `state.db` listed before and after, would settle it.
- **Isolation flags.** `--ignore-user-config` (needs `--model` and `--provider`), `--ignore-rules`,
  `--safe-mode` [6]; for a scratch store, `HERMES_HOME` or the profile (Install and locate).
- **Approvals and tools.** Do not pass `--yolo` (or set `HERMES_YOLO_MODE=1`) for a read-only run [R 23].
  One-shot approval defaults and `--toolsets` limits: Tools, approvals and read-only switches.

**`--format stream-json` events.** One JSON object per stdout line, each with a `timestamp` in milliseconds
(`hermes_cli/stream_json.py`) [S 14]:

| event | fields |
|---|---|
| `system/init` | `model`, `session_id` |
| `text` | `text` |
| `tool_use` | `name`, `tool_call_id?`, `input?`: the full argument object, not truncated |
| `tool_result` | `name`, `tool_call_id?`, `output` (cut to 5000 characters plus `...`), `duration_ms`, `is_error` |
| `result` | `session_id`, `exit_code`, `text`, `tokens {input, output, total, cache_read, cache_write}`, `duration_ms`, `error?` |

Whether these callbacks stay wired in quiet `-q` mode is **[U]**: `_configure_quiet_agent` nulls some, and
the order of `attach()` is not shown in the page read [S 15]. A run in a scratch `HERMES_HOME` with
`--toolsets search` would settle it [29].

**ACP.** `hermes-acp` is named as a toolset and as the ACP mode [S 11][R 23]; its launch command is **[U]**.
Its toolset is the coding posture minus `clarify`. `web_search` and `web_extract` get the tool kind `fetch`;
the title is `web_search: <args preview, 80 characters>`; `rawInput` is set only for tools the adapter does
not know; `rawOutput` is empty for known tools; completion text shows at most 10 results [S 20]. This carries
less than `stream-json`. The exact `session/update` fields are **[U]**.

**Hidden traffic**
- No network-off switch was found **[U]**. MCP toolsets (`mcp-<server>`) may join a turn that has an
  explicit `--toolsets` **[U]**.
- `execute_code` can call `web_search` and `terminal` [S 22]: keep `code_execution` off.
- No hidden queries found for the web tools: `tool_use.input` is complete and uncut [S 14]. Only truncation:
  5000 characters of results in `stream-json`; 80 and 10 in ACP [S 14][S 20].
- `x_search` (off by default) runs inside xAI's tool; its result still carries `answer`, `citations` and
  `inline_citations {url, title}` [S 22].
- Whether a `delegate_task` sub-agent's calls appear in the parent's stream is **[U]**.

### Tools

**86 tool names** = 51 in the shared core list `_HERMES_CORE_TOOLS` + 31 more in other `TOOLSETS` entries
(82 in `toolsets.py` [S 11]) + `kanban_schedule` (registered in `kanban_tools.py`, in no `toolsets.py` list
read) + 3 tool-search bridge tools. Built from `toolsets.py` at the tag, then each tool's registration file
[S 22], then the docs tools reference [4] [29]. Not run: the count a model sees varies with each tool's
availability check, keys and platform [29]. Every row is present at v0.22.5, the only version read; no other
version was compared **[U]**.

- **Default set.** `hermes-cli` = the 51 core tools, minus every tool whose availability check fails [S 17],
  minus the default-off toolsets spotify, discord, discord_admin, video, video_gen, x_search, a2a and kanban
  [S 19].
- **ACP set.** `hermes-acp` also drops `image_generate`, `text_to_speech`, `cronjob_manage`,
  `computer_use`, `clarify` and `kanban_*` [S 11].
- **Switches.** `hermes chat --toolsets <names>` (comma list, no spaces), `agent.disabled_toolsets`,
  `hermes tools`, `/tools disable` [3][5].
- **Not built in.** Honcho, Home Assistant and Spotify are catalog plugins in v0.22.5 [4]. MCP tools form one
  toolset `mcp-<server>` per configured server [5]; the name a model sees for them is **[U]**.

Columns: `class` is the research report's grouping (web, read, write, shell, agent, interaction, other);
`RO` is its read-only judgement of each handler [29]. Citations: `[S 11]` name in `toolsets.py`; `[S 12]`,
`[S 18]`, `[S 22]` registration file; `[4]` docs tools reference. Parameters **[U]**: registration file not
read.

**Core, on in `hermes-cli`** (registration file [S 22] and docs [4] unless the row says otherwise)

| toolset | tool | does; main parameters | class | RO |
|---|---|---|---|---|
| web, search | `web_search` | looks up the web; `query`, `limit` 1-100 (default 5); returns title, URL, description [S 12][4] | web | no |
| web | `web_extract` | page text for URLs; `urls` (max 5, required), `char_limit` per page (min 2000, default 15000) [S 12][4] | web | no |
| file | `read_file` | a text file with line numbers; `path`, `offset`, `limit` | read | yes |
| file | `write_file` | replaces a whole file; `path`, `content`; refuses without a prior read | write | no |
| file | `patch` | find-and-replace edit; `path`, `old_string`, `new_string`, `replace_all` (V4A `mode`/`patch` for OpenAI-family models) | write | no |
| file | `search_files` | regex search of contents, or find files by name; `pattern`, `target` content/files, `path`, `file_glob`, `limit`, `output_mode`, `context` | read | yes |
| terminal | `terminal` | a shell command on the configured backend; `command`, `timeout`, `workdir`, `background` (returns a session_id); dangerous commands need approval [7] | shell | no |
| terminal | `process_manage` | background terminal processes; `action` list/poll/log/wait/kill/write/submit/close/handoff, `session_id` | shell | no |
| code_execution | `execute_code` | Python that calls Hermes tools (`web_search`, `web_extract`, `read_file`, `write_file`, `search_files`, `patch`, `terminal`); `code`, `reset`; on if the backend check passes | shell | no |
| delegation | `delegate_task` | sub-agents in isolated contexts that inherit the parent's toolsets; `tasks[].goal` (required) with `context`; `action` spawn/list/steer/stop | agent | no |
| vision | `vision_analyze` | loads an image so the model sees it; `image_url` (URL, path or data URL), `question`, `region` crop; on if the model is image-capable or an auxiliary vision model resolves | web | no |
| image_gen | `image_generate` | an image from text; `prompt`, `aspect_ratio` (editing parameters depend on the backend); on with a provider | web | no |
| tts | `text_to_speech` | text to an audio file; `text`, `output_path`, `speed` | other | no |
| skills | `skills_list` | skill names and descriptions; `category` | read | yes |
| skills | `skill_view` | loads a skill or a linked file; `name`, `file_path` | read | yes |
| skills | `skill_manage` | creates, patches or deletes skills; `operations[]`, each with `name` and `action` create/patch/write_file/remove_file/delete; `skills.write_approval` stages writes [S 21] | write | no |
| memory | `memory` | durable facts across sessions; `target` memory/user, `action` add/patch/remove, `content`, `old_text`; `memory.write_approval` | write | no |
| session_search | `session_search` | searches or scrolls past sessions (FTS5); `query`, `limit`, `after`, `before`, `session_id` | read | yes |
| todo | `todo_list` | a task list for multi-step work; `todos[]` with `id`, `content`, `status`; `merge` | other | yes |
| clarify | `clarify` | asks the user 1-5 questions; `questions[]` with `question`, `choices`, `multi_select`; not in `hermes-acp` | interaction | yes |
| cronjob | `cronjob_manage` | scheduled jobs; `action` create/list/update/pause/resume/remove/run, `prompt`, `schedule`, `job_id`; on in interactive and gateway sessions | other | no |
| computer_use | `computer_use` | background desktop control through `cua-driver`; parameters **[U]**; not enabled by `all` [5] | other | no |
| connections | `manage_connections` | connector accounts; `action` display/status/connect/reconnect/rename and MCP install/enable/authorize; service-gated [S 11][4] | other | no |

**Browser** (toolset `browser`, 14 tools; all class web, RO no; on if the agent-browser or CDP backend check
passes). The first ten were read in `browser_tool.py` [S 22]; the report does not mark them as in the docs
tools reference. The last four were read in `toolsets.py` and are in the docs [S 11][4][29].

| tool | does; main parameters |
|---|---|
| `browser_navigate` | opens a URL; `url` |
| `browser_snapshot` | text accessibility-tree snapshot of the page; `full` |
| `browser_click` | clicks an element by its snapshot ref; `ref` |
| `browser_type` | types into an input; `ref`, `text` |
| `browser_scroll` | scrolls the page; `direction` up/down |
| `browser_back` | history back; no parameters |
| `browser_press` | presses a key; `key` |
| `browser_get_images` | the page's images with URL and alt text; no parameters |
| `browser_vision` | a screenshot analysed by a vision model; `question`, `annotate` |
| `browser_console` | console output and JS errors, or evaluates an expression; `clear`, `expression` |
| `browser_cdp` | a raw Chrome DevTools Protocol command; needs a CDP endpoint; parameters **[U]** |
| `browser_dialog` | answers a blocking JS dialog; `action` accept/dismiss; needs a CDP endpoint |
| `browser_vault` | saved logins, cards, codes; `action` list/fill/save_login/enter_code/unlock; deferred behind the bridge tools by default [S 11][S 18] |
| `browser_exec` | Python with browser helpers in a session; replaces the other browser tools when `browser.backend` is browser-use [S 11]; parameters **[U]** |

**Off by default: X search, video, kanban** (kanban rows [S 22][4]; switch `kanban`, **off** unless the row
says otherwise)

| toolset | tool | does; main parameters | class | RO |
|---|---|---|---|---|
| x_search | `x_search` | X posts through xAI's built-in tool (provider side); `query`, `allowed_x_handles`, `excluded_x_handles`, `from_date`, `to_date` [S 22][4] | web | no |
| video | `video_analyze` | captions, scenes, timestamps from a video URL or path; parameters **[U]** [S 11][4] | web | no |
| video_gen | `video_generate` | text-to-video or image-to-video; `image_url`; other parameters **[U]** [S 11][4] | web | no |
| video_gen | `xai_video_edit`, `xai_video_extend` | edits, or extends, a video given a public HTTPS MP4 `video_url`; other parameters **[U]** [S 11][4] | web | no |
| kanban | `kanban_show` | full state of a task (row, parents, comments, runs, events); `task_id`, `board`; off unless a dispatcher worker | other | yes |
| kanban | `kanban_list` | task summaries with filters; `assignee`, `status`, `limit`; orchestrator only | other | yes |
| kanban | `kanban_complete` | marks the current task done with a handoff; `summary`, `result`, `metadata` | other | no |
| kanban | `kanban_block` | blocks the task with a reason; `reason`, `kind` | other | no |
| kanban | `kanban_request_review` | moves the task into review; `summary`, `reviewer` | other | no |
| kanban | `kanban_request_changes` | returns a reviewed task to its implementer; `reason` | other | no |
| kanban | `kanban_heartbeat` | signals liveness and extends the claim; `note` | other | no |
| kanban | `kanban_comment` | appends a comment to a task; `task_id`, `body` | other | no |
| kanban | `kanban_create` | creates a child task; `title`, `assignee`, `body`, `parents` | other | no |
| kanban | `kanban_link` | adds a parent-child dependency; `parent_id`, `child_id` | other | no |
| kanban | `kanban_unblock` | moves a blocked task to ready; `task_id`; orchestrator only | other | no |
| kanban | `kanban_attach` | attaches an inline base64 file (25 MB cap [4]); `filename`, `content_base64` | other | no |
| kanban | `kanban_attach_url` | attaches a file fetched from an http(s) URL (25 MB cap [4]); `url`, `filename` | web | no |
| kanban | `kanban_attachments` | lists a task's attachments (read-only per its handler); `task_id` | other | yes |
| kanban **[U]** | `kanban_schedule` | parks the current task until an orchestrator re-gates it; `reason`. Registered [S 22], but in no `toolsets.py` list read, so whether it is offered is **[U]** | other | no |

**Chat platforms and desktop app only** (names [S 11], docs [4] except `setup_beat`; parameters **[U]**
unless named)

| toolset | tool | does | class | RO |
|---|---|---|---|---|
| discord | `discord` | reads and takes part in Discord servers; **off**; needs `DISCORD_BOT_TOKEN` | other | no |
| discord_admin | `discord_admin` | manages Discord servers through REST; **off** | other | no |
| feishu_doc | `feishu_doc_read` | reads a Feishu/Lark document's text (comment handler only); platform only | web | no |
| feishu_drive | `feishu_drive_add_comment`, `feishu_drive_list_comments`, `feishu_drive_list_comment_replies`, `feishu_drive_reply_comment` | in that order: adds a comment to a drive file, lists its comments, lists replies to a comment, replies to a comment; platform only | web | no |
| yuanbao | `yb_query_group_info`, `yb_query_group_members`, `yb_send_dm`, `yb_search_sticker`, `yb_send_sticker` | in that order: a group's info, its members, a direct message, sticker search, sending a sticker; platform only | web | no |
| project | `desktop_project` | creates, lists and switches desktop projects (`action`); desktop only | other | no |
| desktop_ui | `read_terminal`, `close_terminal` | terminal pane affordances; effect **[U]**; desktop only | other | **[U]** |
| desktop_ui | `desktop_preview`, `drive_preview`, `annotate_preview` | preview pane affordances; effect **[U]**; desktop only | other | **[U]** |
| desktop_ui | `read_window_below`, `focus_pane`, `react_to_message` | window affordance, pane focus, message reaction; effect **[U]**; desktop only | other | **[U]** |
| desktop_ui | `gui_tour`, `show_tip`, `set_theme` | guided tour, tip, theme change; effect **[U]**; desktop only | other | **[U]** |
| onboarding | `setup_beat` | first-run onboarding cards; desktop only [S 11] | other | **[U]** |
| catalog | `manage_catalog` | searches and installs catalog plugins and hub skills through an approval card (`search`, `install`); desktop only | other | no |

**Tool-search bridge** [S 18] (switch `tools.tool_search.enabled` auto/on/off; on when any tool is
deferrable)

| tool | does | class | RO |
|---|---|---|---|
| `tool_search` | finds deferred tools by query (up to 7 queries per call) | other | yes |
| `tool_describe` | loads the full schema of deferred tools (up to 10 names per call) | other | yes |
| `tool_call` | invokes a deferred tool as `{name, arguments}` | other | no |

**Needs a key or an optional dependency** [29]

| tool | needs |
|---|---|
| `web_search`, `web_extract` | a backend key, the Nous subscription, the `ddgs` package or `SEARXNG_URL` |
| `x_search` | `XAI_API_KEY` or xAI OAuth |
| `image_generate` | `FAL_KEY`, the Nous gateway, Krea or a plugin |
| `text_to_speech` | the default `edge` needs the `edge-tts` extra; the others their key or package |
| `vision_analyze` | an image-capable main model or an auxiliary vision model |
| `browser_*` | the agent-browser CLI, Node, Chromium, or a cloud or CDP backend |
| `computer_use` | `cua-driver` |
| `discord` | `DISCORD_BOT_TOKEN` [4] |
| `feishu_*`, `yb_*` | platform credentials [4] |
| `video_generate` | `XAI_API_KEY` or `FAL_KEY` [4] |
| `session_search` | the SQLite state database |
| `cronjob_manage` | an interactive, gateway or cron session |
| `kanban_*` | `HERMES_KANBAN_TASK` or the profile's `kanban` toolset |
| `terminal` | the configured backend's requirements |

**Web search and page reading** (the part the research report examined most closely [29])
- **Names.** The model-visible name is `web_search` (`tool_use.name` in the stream) [S 12][S 14]; `query`
  (required), `limit` (1-100, default 5) [S 12]. `web_extract` takes `urls` (array, at most 5, required) and
  `char_limit` (minimum 2000, default 15000), returns markdown or text with no LLM summary, and also reads
  PDFs [S 12][4].
- **Where it runs.** In the Hermes process, as HTTP to a backend, not at the model provider [S 12]. Backend:
  `web.search_backend`, else `web.backend`, else the first that autodetects: tavily, perplexity, exa,
  parallel, keenable, firecrawl (a key, or the Nous gateway), searxng (`SEARXNG_URL`), brave-free
  (`BRAVE_SEARCH_API_KEY`), ddgs (a package, no key), then plugin providers; the last fallback is
  `firecrawl` [S 12]. `web.backend: nous` uses the Nous Tool Gateway (paid; Install and locate) with no
  separate key [S 12][9].
- **Account.** One backend key (`EXA_API_KEY`, `PARALLEL_API_KEY`, `TAVILY_API_KEY`, `PERPLEXITY_API_KEY`,
  `KEENABLE_API_KEY`, `FIRECRAWL_API_KEY` or `FIRECRAWL_API_URL`), or the Nous subscription, or a keyless
  tier (the `ddgs` package, `SEARXNG_URL`) [S 12][4]. With none, the tool is not offered
  (`check_fn=check_web_api_key`) [S 12][S 17].
- **Switch.** `web` is in the shared core list, so `hermes-cli` has it; also in `hermes-acp`,
  `hermes-api-server`, `hermes-webhook`, `coding`, `debugging` and `safe` [S 11][5]. Off with
  `agent.disabled_toolsets: [web]` or `[search]` (removed per tool, last) [10][S 19][S 16], with
  `hermes tools`, or with `/tools disable` [5].

**Web results and limits**
- **Result shape.** `web_search` returns a JSON string
  `{"success":..,"data":{"web":[{title,url,description,position}]}}`, read at `main`; the tag's shape is
  **[U]** [S 12]. A stream-json consumer sees it only under 5000 characters (a `limit` near 100 is cut)
  [S 14]. `web_extract` returns entries `{url,title,content,error}`, cut at 5000 in the stream [S 13].
- **Page-reading limits.** None per turn and none tied to search results [S 13]. Only a denylist,
  `security.website_blocklist: {enabled: true, domains: ["*.internal.company.com"], shared_files:
  ["/path/list.txt"]}`, and an always-on floor against private, loopback, link-local, CGNAT and
  cloud-metadata addresses (`security.allow_private_urls`, default false) [7]. URLs that look like they
  carry an API key are refused [S 13].
- **Fail-open check.** The policy check in `_extract_safe_urls` treats an exception as "not blocked" [S 13].
- **Untrusted content.** No scanning of web content or tool results is documented; only context files
  (`AGENTS.md`, `.cursorrules`, `SOUL.md`) are scanned [7]. The `web_search` and `web_extract` schemas carry
  no untrusted-content warning [S 12].

**Approvals and read-only switches**
- Dangerous `terminal` commands need approval [29][7]; `approvals.single_query_mode` defaults to `deny` for
  one-shot runs [7]; `--yolo` or `HERMES_YOLO_MODE=1` turns approvals off [R 23].
- `--toolsets` names whole toolsets, and the registry returns only the requested tool names
  [3][S 16][S 17]. A bare tool name is refused (`Unknown toolset`) [S 16].
- No built-in toolset holds the read tools and search without the write tools [5][S 11]. There is no
  answer-only flag; which toolset removes tools and write access is planned check HM7 [R 23][26].

| `--toolsets` | tools left | note |
|---|---|---|
| `search` | `web_search` | [S 11] |
| `web` | `web_search`, `web_extract` | [S 11][5] |
| `search,file` | `web_search`, `read_file`, `search_files`, `write_file`, `patch` | removes terminal, `execute_code`, browser, delegation, memory, skills and `web_extract`; still has the two write tools [S 11] |
| `safe` | `web_search`, `web_extract`, `vision_analyze`, `image_generate` | no file tools [S 11] |
| `hermes-webhook` | `web_search`, `web_extract`, `vision_analyze`, `clarify` | its source comment says payloads are untrusted [S 11] |

| question | status | what would settle it |
|---|---|---|
| choose the tools of a turn | Established for a new session per turn: `--toolsets` selects whole toolsets [3][S 16][S 17]. Not for a resumed session: launch flags are frozen and `/tools` is not an allowed slash command there [6]. Whether MCP or plugin tools join a `--toolsets file` turn: **[U]** | `hermes chat --oneshot -q "list your tools" --toolsets file --format stream-json` in a scratch `HERMES_HOME`; the model should name no web tool. A model turn; not run [29] |
| see the queries and pages a turn used | Established from source for `web_search` (query in `tool_use.input.query`, URLs in `tool_result.output`) and `web_extract` (URLs in `input.urls`, text in `output`), cut at 5000 characters [S 14]. **[U]**: whether callbacks stay wired in quiet `-q`; the `web_search` result shape at the tag | `... --toolsets search`; observe the query and the URLs. A search; not run [29] |
| a turn without write tools | **[U]**. (a) `custom_toolsets` in `config.yaml` is documented for toolset names [5], but no loader was found in `tools_config.py` [S 19], and `create_custom_toolset(name, description, tools, includes)` takes tool names [S 11]: syntax **[U]**. (b) A shell `pre_tool_call` hook can block or fail closed [8]; payload and matcher **[U]**. (c) `agent.disabled_toolsets` removes tools by toolset name only, and no toolset holds just `write_file` and `patch` [S 16][S 11] | a scratch config with the candidate `custom_toolsets` entry, then `hermes tools` and `hermes hooks list` (no model turn); or `hermes hooks test pre_tool_call` to read the payload [29] |
| read only URLs a search returned | **[U]**: no flag or setting [S 13]; only a denylist [7]. A hook that keeps the URLs seen in `post_tool_call` results works only if the shell payload carries them; the plugin payload has `result` [8] | `hermes hooks test post_tool_call` with a logging hook; read its stdin fields [29] |
| stateless run and resume by id | Established for stateless runs [S 15] and for resume by id [6]; a read-only run depends on the third row | as above |

### Branches, forks and subagents

- **Fork.** `/branch` (alias `/fork`) inside a session [R 23]. How a fork is recorded in `state.db` (for
  instance through `parent_session_id`) is **[U]**: a fork in a scratch `HERMES_HOME`, then a look at
  `sessions`, would settle it.
- **Resume.** `hermes chat --resume <id|title|latest>`, `--continue [title]`; the design draft also writes
  `hermes --resume <id|title|latest>` and `hermes -c [name]` [32]. A resume moves back to the session's
  recorded directory unless `--no-restore-cwd` [6][R 23]. One session resumed in two terminals is planned
  check HM6 [26]. What `hermes -c` does with a compression chain is **[U]** [32].
- **Sub-agents.** The `delegate_task` tool (Tools) [S 22]. Each run is its own session row with
  `parent_session_id` and a `_delegate_from` marker in `model_config`, hidden from pickers [R 24]. Logs under
  `cache/delegation/live/` are pruned after 7 days; Ctrl+T opens a full-screen sub-agent roster in the
  classic CLI [R 23]. The delegation backends are pluggable, so a sub-agent's work must not be assumed
  inlined as text [28]. Whether its calls appear in the parent's `stream-json` is **[U]**.
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
- **mukade** neither reads nor writes hook configuration, and ADR-0002 does not propose it: it concerns
  sessions only [27].

## How mukade reads it

**Today: not at all.** Checked on 2026-11-11 [V][30]:
- Nothing under `src/`, `catalog/`, `tests/` or `fixtures/` mentions Hermes, nor does `Cargo.toml`.
- `src/model/harness.rs` `HarnessId` has four variants (`ClaudeCode`, `Codex`, `OpenCode`, `Pi`),
  `HarnessId::ALL` four entries, and the `FromStr` error lists the same four.
- No `HERMES_HOME` handling; no `catalog/harnesses.json` entry, `--hermes-exe` flag or `MUKADE_HERMES_EXE`
  variable; no Hermes names in [`../tool-conversion-tables.md`](../tool-conversion-tables.md).
- Documents that mention Hermes: `README.md`, `docs/README.md`, the ADR index, ADR-0002, the design draft, the
  persistence reference, the DeepSeek Harness page and this page.

**Proposed** by ADR-0002 (status **Proposed**: version 1, 2026-10-31, `accepted_on` empty) [27] and the
design draft (status "design draft, awaiting approval"; choices marked as the author's own) [32]. Not
implemented. Ranked by what it costs a user:

- **One store version.** The reader accepts only the `state.db` schema it was verified against, given as
  **31**, and refuses any other, naming the version found (INV-003, EX-003) [27]. The sources disagree on the
  number **[U]** [R 24]: if v0.22.5 writes 32, every real store would be refused until the pin is checked.
- **No compaction entry.** A Hermes session yields no compaction entry, and `--compaction portable` from it
  is refused with that reason, because upstream documents no stored summary row **[U]** [32].
- **Reasoning.** `reasoning` and `reasoning_content` become reasoning text; `reasoning_details` and
  `codex_reasoning_items` stay in `raw` and are reported as a loss, since they may hold signed or encrypted
  payloads **[U]**; they are never mapped to an opaque payload until a real session shows what they hold
  [32]. A reasoning block with no payload is dropped as `NoPayload` under the default `--reasoning
  drop_foreign` (`src/policy.rs` `ReasoningPolicy::DropForeign`) and kept as labelled text only with
  `--reasoning as_text` [32].
- **Which sessions `list` shows.** Source `cli`; `oneshot`, `tool`, `kanban` and gateway sources sit behind a
  flag, as Hermes's pickers hide them. A compression child (`parent_session_id`) is the live continuation,
  so the tip is the listed session and the parent chain is reachable through its lineage, as a Pi fork is;
  a hop from a mid-chain session says in the manifest that later segments exist [32].
- **Reading.** `state.db` opened read-only through `rusqlite` with `bundled`, already a dependency
  (`Cargo.toml`: `rusqlite = { version = "=0.40.2", features = ["bundled"] }`) [27][V]; `schema_version == 31` checked;
  messages in `messages.id` order within a session. `active=1` rows are the conversation; `active=0` rows
  become `Kind::Meta` with `MetaKind::OffBranch` (`src/model/kind.rs`); every original row is kept in
  `Entry::raw` (`src/model/mod.rs`) [32].

**Proposed safeguards and plumbing**
- **Never open for writing** (INV-001); a test checks that a Hermes fixture opened by mukade is
  byte-identical afterwards [27]. WAL sidecars can still appear; how to read when the directory is read-only
  or the gateway holds the WAL (`mode=ro` against `immutable=1`) is open [32].
- **Discovery.** Detection recognizes a `state.db` with `sessions`, `messages` and `schema_version`; binary
  `hermes`, with `--hermes-exe` and `MUKADE_HERMES_EXE`. A named profile is a separate installation that
  `scan` finds, so `src/detect/locate.rs` `locate_installation`'s "exactly one" rule needs no change [32].
- **Export.** `hermes sessions export` is not used at runtime (it needs the binary and is a second
  representation); a mukade `dump` is cross-checked against `hermes sessions export --format jsonl` in
  verification [32].
- **Tools.** Hermes tool calls are read-only inputs; the draft has not collected the names **[U]**; they go
  into `docs/tool-conversion-tables.md` once collected from fixtures; a tool with no counterpart travels as
  text with a manifest entry [32].
- **Library changes.** `HarnessId` grows to six (`ALL`, `as_str`, `FromStr` and the error list change);
  `catalog/harnesses.json` gets an entry; a harness capability (can be a source, can be a destination) is
  added; "four harnesses" in the README, the specification and the usage guide becomes six, with Hermes
  marked read-only [27].

**Delivery.** Phase 1: capability, six `HarnessId`s, catalog schema, detect. Phase 2: readers, Hermes first
(read-only SQLite), each with a page, tool-table rows and tests [32]. Author's estimate: foundation about 2
hours, Hermes reader 2-3 hours [32]. Fixtures are recorded real sessions with synthetic content; live
fixtures need Hermes installed and a provider key [27][32].

**What Shinatama expects.** Shinatama has hosted Hermes since 2026-10-31, but until a reviewed pin carries a
Hermes reader its adapter declares `list_sessions`, `read_transcript` and `export` unsupported: Hermes
resumes natively within itself and receives seeds, nothing more [R 25]. The reader must be read-only, take
no lock, ignore a torn last record and count unknown records; fixtures come first (planned check HM3)
[R 25][26].

## How mukade writes it

**Today: no writer.** **Proposed: never** [27].
- **Source-only** until upstream offers a supported way to add a session without writing `state.db` behind
  the owning gateway [27].
- **Refusal first.** A hop or `write` toward Hermes is refused with exit code 2 and the reason, before any
  plan or write; nothing is written and no `state.db` is opened for writing (INV-002, EX-002:
  `mukade hop claude_code <session-id> hermes`) [27]. The check sits in one place shared by the CLI, the TUI
  and the library; the TUI does not offer Hermes as a destination [32].
- **Rejected alternatives.** RA-001: write `state.db` directly (the gateway is the only canonical writer, a
  partial runtime ledger could replay started effects, and an upstream issue links concurrent writers to
  corruption). RA-002: generate a backup zip and run `hermes import <zip>`, which restores a whole backup
  and overwrites files in the Hermes home. RA-005: read-write behind a bypass flag [27].
- **No resume command** in a receipt, because mukade would never write a Hermes session [27].
- **Shinatama's view.** Its handover notes a possible writer after the first version, built on Hermes's own
  `sessions import --from claude|codex` [R 25]; ADR-0002, if accepted, keeps Hermes source-only [27].

## How mukade summarizes it

Not implemented. `--compaction summary` asks the source's own model for a summary through a
non-interactive run that saves nothing.
- **Planned refusal.** The design draft refuses it for Hermes in the first phase [32]; ADR-0002 does not
  address it [27].
- **Candidate run.** Shinatama's answer-only candidate is
  `hermes chat --oneshot -q <prompt> --format stream-json --ignore-user-config --ignore-rules` [R 23]. No
  toolset that removes the tools and the write access is established, and whether a one-shot leaves a
  session is **[U]** (Ephemeral runs).

## Open points

No source read settles these. Ranked by how much they block a reader:

| # | question | what the sources say | what would settle it |
|---|---|---|---|
| 1 | store schema version **[U]** | 31 [27][32]; 32 [R 24] | the version in a `state.db` made by v0.22.5, or `SCHEMA_SQL` at the tag |
| 2 | read route **[U]** | `state.db` directly [27][32]; Hermes's export first, `state.db` as fallback [R 24]; the handover calls the export route cheaper [R 25]. Build against a hand-made schema-31 database or wait for a real session: waiting on the user [32] | a fixture run, planned check HM3 [26] |
| 3 | compaction shape and summary **[U]** | rows kept with `active=0, compacted=1` under the same id, summary marked as a compaction [R 24]; old rows archived, a possible child session, no stored summary row [32] | a v0.22.5 `state.db` after a compaction |
| 4 | does a session import command exist **[U]** | `hermes sessions import --from claude\|codex [path]`, and `hermes --resume @claude` imports and resumes in one step, condensing tool activity [R 23]; no command imports a session, and `hermes import-agent` imports setups (config, skills), not transcripts [27][32] | `hermes sessions import --help` on v0.22.5 |
| 5 | reading beside a live gateway **[U]** | the gateway writes while mukade reads; `mode=ro` against `immutable=1`, and what each risks [32] | planned check HM3 [26] |

Other **[U]** items stay in their sections; planned checks HM1 to HM14 are in the Shinatama verification
register [26]; the project backlog is [`next.md`](../../next.md).

## Sources

All web pages and raw files accessed 2026-11-01 unless a source says otherwise. Sources 1 to 10 are the
Hermes README, the GitHub release record and the Hermes docs site (`<docs>` =
`https://hermes-agent.nousresearch.com/docs`). Sources 11 to 22 are Hermes files at tag `v0.22.5`
(`<raw>` = `https://raw.githubusercontent.com/NousResearch/hermes-agent/v0.22.5`), each read through a web
fetch tool's page extraction, which gives function names but no line numbers and may drop lines: re-read the
file in a clone before relying on it. Sources 23 to 25 and 29 are documents of the Shinatama repository
(`D:\Projects\shinatama`), written by research tasks that ran nothing; `[R n]` is Shinatama's own label for
a fact from its research of 2026-10-31, kept wherever a fact rests on that research alone. Repository paths
are relative to `D:\Projects\mukade` unless a source says otherwise.

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
27. mukade, [`adr/ADR-0002-hermes-and-deepseek-harness-read-first.md`](../adr/ADR-0002-hermes-and-deepseek-harness-read-first.md): status **Proposed**, version 1, proposed 2026-10-31, `accepted_on` empty.
28. mukade, [`reference/harness-session-persistence-reference.md`](../reference/harness-session-persistence-reference.md), entry "Hermes Agent — `v0.22.5`". A secondary research report pasted by the user on 2026-10-27; its own header says its claims were not re-run here.
29. Shinatama research report "Hermes Agent" of the web-tools run, 2026-11-01: `D:\Projects\shinatama\.daodan\runs\init-467ba8d96c1b4104b3ac1edb707caffa\web-tools\hermes-agent.md` (the 86-row tool list, web search and page reading, headless and ACP events, toolset switches, verdict). Its `[S]` and `[D]` references are sources 11 to 22 and 3 to 10 above.
30. Search of the mukade tree, 2026-11-01, repeated on 2026-11-11: `grep -rli hermes docs src catalog tests fixtures README.md Cargo.toml` and `find . -iname "*hermes*"`; reading of `src/model/harness.rs` (`HarnessId`, `HarnessId::ALL`, `FromStr`), `src/detect/` and `Cargo.toml` (`rusqlite`). [V]
31. `Get-Command hermes -All` in PowerShell on this machine (Windows 11), 2026-11-01: not recognised as a command. [V]
32. mukade, [`superpowers/specs/2026-10-31-hermes-deepseek-harness-support-design.md`](../superpowers/specs/2026-10-31-hermes-deepseek-harness-support-design.md) §4.1, §5.2, §5.3, §5.4, §5.5, §5.6, §6, §7, §8: a design draft awaiting approval, which reads Hermes's session-storage and sessions documentation; what it marks **[U]** was not exercised against a live install.
