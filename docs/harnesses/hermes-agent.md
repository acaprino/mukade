# Hermes Agent

| | |
|---|---|
| **Documentation updated** | 2026-11-01 |
| **Version described** | v0.22.5 (tag `v0.22.5`, commit `bf0203a794a07aac3ff764972ab9087baa4e4f4e`, published 2026-10-23T05:28:45Z) [2]. Described from documentation and source only: Hermes is not installed on this machine and was never run here [V][29][31] |
| **Vendor** | Nous Research [1] |
| **Website** | <https://hermes-agent.nousresearch.com/docs/> [1] |
| **License / source** | MIT; <https://github.com/NousResearch/hermes-agent> [1] |
| **mukade name** | none: mukade does not support this harness yet (§8). ADR-0002, Proposed, uses `hermes` in its examples [27] |

Evidence labels: `[n]` cites §9 (documentation, or a file of the mukade or Shinatama repositories). `[S n]` = read in the Hermes source at tag `v0.22.5` (§9, sources 11 to 22), through a web fetch tool's page extraction, which gives function names but no line numbers and may drop lines: re-read the file in a clone before relying on it. `[R n]` = Shinatama's own label for a fact from its research task of 2026-10-31 on the release, its source and its docs, with nothing run; it is kept wherever a fact rests on that research alone (sources 23 to 25). **[V]** = observed on this machine. **[U]** = not established; the text says what would settle it.

## 1. Introduction

Hermes Agent is Nous Research's agent: MIT-licensed, written in Python (3.11 to 3.14) and installed by its
own script with a bundled uv [1][R 23]. The command `hermes chat` does not run the agent in the terminal:
it finds or starts the profile's background *gateway*, creates or resumes the session there and attaches
over a local WebSocket [R 23]. The same agent is also an ACP server (`hermes-acp`) [R 23] and has toolsets
for messaging platforms and a desktop app (Discord, Feishu/Lark, Yuanbao; `desktop_ui`, `project`,
`onboarding`, `catalog`) [S 11]. It offers 86 tool names, grouped in toolsets (§5) [29]. Sessions live in
one SQLite file per profile, `state.db` [R 24][32].

This page describes v0.22.5 from its documentation and source. Hermes is not installed on this machine,
and neither `--version` nor `--help` was ever run, so no behaviour below was observed [V][29][31].

**mukade status.** mukade does not support Hermes: there is no reader, no writer, no catalog entry, no
fixture and no test [V][30]. ADR-0002 (Proposed, version 1, 2026-10-31, not accepted) would add it as a
source-only harness: mukade reads Hermes sessions and never writes one [27]. §8 gives the detail.

## 2. Description

| aspect | v0.22.5 |
|---|---|
| agent loop | `hermes chat` creates or resumes the session in the profile's gateway and attaches over a local WebSocket. Closing the terminal only detaches and the turn keeps running; Ctrl+C stops it. Launch flags are fixed when the session is created; a resume cannot change them [6][R 23] |
| tools | 86 tool names in toolsets. The default CLI toolset is `hermes-cli`: the 51 core tools, minus those whose availability check fails and minus the default-off toolsets (§5) [29][S 11][S 19] |
| approvals | dangerous `terminal` commands need approval [29][7]; `approvals.single_query_mode` defaults to `deny` for one-shot runs [7]; `--yolo` or `HERMES_YOLO_MODE=1` turns approvals off [R 23] |
| context files | `.hermes.md`/`HERMES.md`, `AGENTS.override.md`, `AGENTS.md` (merged from the git root down), `CLAUDE.md`, `.cursorrules`, `.cursor/rules/*.mdc`; only the first project file type found is loaded; `SOUL.md` always, from the Hermes home only [R 23]. Context files are scanned for prompt injection; web content and tool results are not [7] |
| compaction | at 50% of the context window by default, in place (§5) [R 23] |
| sub-agents | the `delegate_task` tool; each run is its own session row, hidden from pickers (§5) [R 24][S 22] |
| models | many providers, chosen with `hermes model`, `hermes setup` or `hermes auth add`; `/model` changes the model for the session (§5) [R 23] |
| terminal UI | the classic CLI (prompt_toolkit) is the default and writes to normal scrollback; Ctrl+T opens a full-screen sub-agent roster. `--tui` is an alternate-screen Node TUI with mouse tracking on by default [R 23] |
| Windows | native: a UTF-8 console shim, and the gateway runs detached through `pythonw` [R 23] |

## 3. Entry points

None of the commands below was run: Hermes is not installed here [V][31].

| entry point | exact command | notes |
|---|---|---|
| interactive | `hermes chat` | the classic CLI attached to the profile's gateway; `--tui` starts the alternate-screen TUI [R 23] |
| first turn on a terminal | `hermes chat -q "<text>"` | sends the text as the first turn and stays open [S 15][R 23]. There is no positional prompt: a bare word is a subcommand [R 23]. Size limit of `-q`: not found [R 23] |
| one turn, headless | `hermes chat --oneshot -q "<text>" --format stream-json` | answers and exits; `-q` without a TTY also runs one turn and exits [S 14][S 15]. `--oneshot` and `--format` are not on the docs CLI page [6] |
| final text only | `hermes chat -z "<prompt>"` | prints only the final text [R 23] |
| prompt from a file | `hermes chat --query-file <path>` (`-` for stdin) | takes the first turn from a file, never shell-interpreted [6][R 23] |
| resume | `hermes chat --resume <id\|title\|latest>`, `--continue [title]` | the id is not chosen by the caller (see below); a resume moves back to the session's recorded directory unless `--no-restore-cwd` [6][R 23] |
| fork | `/branch` (alias `/fork`) inside a session | [R 23] |
| list sessions | `hermes sessions list` | a text table, no JSON [R 23] |
| export a session | `hermes sessions export <out\|-> --session-id <id> --format jsonl\|md\|html\|trace` | `trace` is Claude Code JSONL [R 23]; the row shape of `jsonl` is **[U]** |
| import foreign sessions | `hermes sessions import --from claude\|codex [path]`, `hermes --resume @claude` | imports Claude Code or Codex sessions, condensing tool activity; `--resume @claude` imports and resumes in one step [R 23]. The mukade design draft states that *no command imports a session*, and `hermes import-agent` imports setups (config, skills), not transcripts [32]: the two statements are not reconciled **[U]**; `hermes sessions import --help` on v0.22.5 would settle it |
| restore a backup | `hermes import <zip>` | restores a whole backup over the Hermes home [27][32] |
| ACP | `hermes-acp` | named as the toolset and as the ACP mode [S 11][R 23]; the launch command **[U]** |
| MCP | `hermes mcp add\|list\|remove\|test` | [R 23] |
| login and status | `hermes model`, `hermes setup`, `hermes auth add <provider> [--type oauth\|--api-key]`; `hermes status`, `hermes auth list`, `hermes auth status <provider>` | status commands print no secrets [R 23] |
| tools | `hermes chat --toolsets <names>` (comma list, no spaces), `hermes tools`, `/tools disable` | [3][5] |
| hooks | `hermes hooks list`, `hermes hooks test <event>` | named as probes in the research report; not run (§7) [29] |

**Learning the session id.** The caller cannot choose it. It is in the `system/init` event (`session_id`)
and in `result.session_id` of `--format stream-json`, and it is printed on stderr as `session_id: ...`
[S 14][S 15][R 23].

**Event stream.** `--format stream-json` writes one JSON object per stdout line, each with a `timestamp`
in milliseconds (`hermes_cli/stream_json.py`) [S 14]:

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

**ACP.** Under `hermes-acp` (the coding posture minus `clarify`), `web_search` and `web_extract` get the
tool kind `fetch`; the title is `web_search: <args preview, 80 characters>`; `rawInput` is set only for
tools the adapter does not know; `rawOutput` is empty for known tools; completion text shows at most 10
results [S 20]. This carries less than `stream-json`. The exact `session/update` fields are **[U]**.

## 4. Harness type

An open-source (MIT) Python agent with a terminal front end, a background gateway per profile, an ACP mode
and an installer of its own [1][R 23]. It needs Python 3.11 to 3.14; the installer brings its own uv
[R 23].

### Vendor

Nous Research, under the MIT licence [1]. The repository is `NousResearch/hermes-agent` [1][2].

### Website and documentation

Docs: <https://hermes-agent.nousresearch.com/docs/> [1]. Source and releases:
<https://github.com/NousResearch/hermes-agent>, tag `v0.22.5`, commit
`bf0203a794a07aac3ff764972ab9087baa4e4f4e`, published 2026-10-23T05:28:45Z according to the GitHub releases
API [2]; Shinatama's design gives 2026-10-22 [R 23], and the persistence reference says the release notes
state 2026-10-22 and GitHub shows 2026-10-23 [28].

The docs site shows the current documentation, which differs from the tag: its `browser` toolset lists
4 tools [5], the tag lists 14 [S 11]. Where they differ, the tag wins [29]. This page therefore takes
behaviour from the docs and tool names from the tag. Two docs pages were read only in part: the hooks page
(the first 100000 of 149287 characters; the shell-hook section was not reached) [8] and the configuration
page (the first 100000 of 284048 characters) [10].

### Installation

| OS | method | exact command |
|---|---|---|
| macOS, Linux, Windows | Hermes's own install script, which bundles uv; Python 3.11 to 3.14 [R 23] | **[U]**: the script's URL and command are not in the sources read for this page. The README [1] and the docs site [6] would settle it |
| Windows | the same script; Hermes runs natively, with a UTF-8 console shim, and the gateway runs detached through `pythonw` [R 23][32] | **[U]** |

How to update, uninstall, or install side by side is **[U]**. After installing, sign in with `hermes model`,
`hermes setup` or `hermes auth add <provider> [--type oauth|--api-key]`; check without printing secrets with
`hermes status`, `hermes auth list` or `hermes auth status <provider>` [R 23]. Whether
`%LOCALAPPDATA%\hermes\bin\hermes` is a native executable or a shim is a planned check, HM1 [R 23][26].

### Canonical locations and finding an existing install

| what | Windows | macOS / Linux |
|---|---|---|
| Launcher | `%LOCALAPPDATA%\hermes\bin\hermes` (native executable or shim: **[U]**, HM1) [R 23] | **[U]** |
| Hermes home (default) | `%LOCALAPPDATA%\hermes` [R 24][32] | `~/.hermes` [R 24][32] |
| Move the home | `HERMES_HOME` [R 24] | `HERMES_HOME` [R 24] |
| Session database | `<home>\state.db` [R 24][32] | `<home>/state.db` [R 24][32] |
| Named profile | `<root>\profiles\<name>\`, with its own `state.db` [32] | `<root>/profiles/<name>/`, with its own `state.db` [32] |
| Secrets and login | `<home>\.env`, `<home>\auth.json` [R 23] | `<home>/.env`, `<home>/auth.json` [R 23] |
| Skills, plugins | `<home>\skills\`, `<home>\plugins\` [R 23] | `<home>/skills/`, `<home>/plugins/` [R 23] |
| Settings, hook approvals | the profile's `config.yaml`, and `shell-hooks-allowlist.json`; exact paths **[U]** [R 23] | the same **[U]** |

`HERMES_HOME` and the profile (`-p`) are the only levers that move settings, hooks and the session store
together; there is no per-launch switch for hooks or MCP [R 23]. Whether `HERMES_HOME` alone keeps
everything inside it is a planned check, HM9 [26]. To find an existing install: `Get-Command hermes -All`
(PowerShell) or `command -v hermes` (macOS/Linux), then `hermes status`. `hermes --version` was not run
and its output is **[U]**. On this machine `Get-Command hermes` found nothing on 2026-11-01 [V][31], and
the research that precedes this page found nothing with `where hermes` [29].

`mukade harnesses` does not list Hermes: `catalog/harnesses.json` has no Hermes entry [V][30]. The shared
per-OS discovery procedure is in the
[harness catalog](../reference/harness-catalog.md#finding-existing-installations).

### Licence

MIT [1]. The Nous Tool Gateway (search through Perplexity, page extraction through Firecrawl) needs a paid
Nous subscription [9][S 12]. The licence of the desktop app that the `desktop_ui` toolset implies was not
checked **[U]**.

## 5. Features

### Session store

Nothing in this subsection was observed: Hermes is not installed here, so no `state.db` was opened [V][31].
It rests on Shinatama's research task of 2026-10-31 on the v0.22.5 source [R 24] and on the mukade design
draft, which read Hermes's session-storage documentation [32].

- **File.** `<home>/state.db`, SQLite with FTS5 and WAL; the home is `HERMES_HOME`, default `~/.hermes`
  (`%LOCALAPPDATA%\hermes` on Windows). Each profile (`-p`) has its own `state.db`, in
  `<root>/profiles/<name>/` [R 24][32]. A JSONL copy under `sessions/` is written only as a fallback
  [R 24].
- **Schema version.** Two sources disagree. Shinatama's reading of the v0.22.5 source says **32** [R 24];
  ADR-0002 and the mukade design draft say **31**, the draft reading it from Hermes's session-storage
  documentation, in a `schema_version` table [27][32]. This is **[U]**. Reading the version from a `state.db` created by v0.22.5, or `SCHEMA_SQL` in
  the source at the tag, would settle it.
- **Tables.** `sessions` and `messages`. Shinatama's research names the columns below and cuts the
  `sessions` list with "…" [R 24]. The design draft adds the columns in the last column, from an abridged
  documentation page; the full set in `SCHEMA_SQL` is **[U]** [32].

| table | columns named by Shinatama's research [R 24] | further columns named by the mukade design draft [32] |
|---|---|---|
| `sessions` | `id`, `source`, `profile_name`, `cwd`, `git_branch`, `git_repo_root`, `title`, `parent_session_id`, `model`, `model_config`, `started_at`, `ended_at`, `end_reason`, `last_activity_at`, `message_count`, `archived`, `hidden`, `pinned`, … | `system_prompt`, token and cost counters |
| `messages` | `id`, `session_id`, `timestamp`, `role`, `content`, `reasoning`, `tool_calls`, `tool_call_id`, `tool_name`, `active`, `compacted` | `reasoning_content`, `reasoning_details`, `codex_reasoning_items`, `codex_message_items`, `message_uid`, `tool_call_uids`, `api_content` |

- **Values.** Session ids look like `YYYYMMDD_HHMMSS_<hex>` [R 24]. `messages.id` is an autoincrement
  integer, `timestamp` an epoch float, and `tool_calls` a JSON string [32]. The role names, and the JSON
  shape of `content` and `tool_calls`, are **[U]** [R 24].
- **Source column.** `sessions.source` holds `cli`, `telegram` and other gateway sources, and `oneshot`,
  `tool` and `kanban` for runs that Hermes's pickers hide [32]. That a one-shot run therefore leaves a
  session row is read from documentation only **[U]**.
- **Writer.** The profile's gateway is documented as the only writer of canonical sessions, and ADR-0002
  and the design draft state that no command imports a session [27][32] (Shinatama's research reports
  `hermes sessions import --from claude|codex`; see §3). Opening a WAL database read-only can still create `-wal` and
  `-shm` sidecars [32]. Whether a read-only open works while the gateway writes is a planned check, HM3
  [26].
- **Other stores.** A secondary research report also names `sessions.json` (gateway session mappings,
  rewritten atomically), `kanban.db` and `response_store.db` [28]; none was opened **[U]**.
- **Retention.** No pruning by default; `sessions.auto_prune` prunes at 90 days when turned on [R 24].
- **Lineage.** `sessions.parent_session_id` links a session to its parent. A sub-agent run is its own
  session row with `parent_session_id` and a `_delegate_from` marker in `model_config`, hidden from
  pickers [R 24]. The design draft adds that compression can split a session into a child linked the same
  way [32]; Shinatama's research says compaction keeps the id (next subsection): **[U]**.

**Finding and reading sessions.**
- `hermes sessions list` prints a text table, no JSON [R 23].
- `hermes sessions export - --session-id <id> --format jsonl` writes one session to stdout; `md`, `html` and
  `trace` (Claude Code JSONL) are the other formats [R 23]. A secondary report mentions a filtered
  user-prompts view [28]. Shinatama's research treats this export as the primary source and the SQLite
  file as a read-only fallback for listing and search, because the schema is versioned and moves [R 24];
  the mukade design draft reads `state.db` directly and uses the export only as a cross-check [32].
  The shape of an export row is **[U]**.
- The file itself: `<home>/state.db` opened read-only. A hand-made schema-31 database is all the design
  draft has to build a reader against; a real session is awaited from the user [32].

**Shinatama's intended mapping** to its own canonical transcript (not to mukade's interchange entries)
[R 24]:

| Hermes rows | Shinatama canonical |
|---|---|
| `user` and `assistant` rows | `user_text`, `assistant_text` |
| `reasoning` | `reasoning` |
| `tool_calls` | `tool_call` |
| rows with a `tool_call_id` | `tool_result` |
| rows with `active=0, compacted=1` | not followed; the summary is marked as `compaction` |
| a sub-agent session | one tool pair |

How a summary row is told apart in `messages` is **[U]**: the design draft says upstream documents no stored
summary row [32].

### Compaction

- **Trigger.** Automatic at 50% of the context window by default [R 23]. The setting that holds the
  threshold, any manual command, and a way to compact without a terminal are **[U]**: the sources read
  do not name them. The `post_compaction` hook event exists (§7) [R 23].
- **In place.** The session keeps its id; the rows it replaced stay in `messages` with
  `active=0, compacted=1` [R 24]. The design draft describes the same pattern (old rows get `active=0`, the
  retained context is inserted as `active=1`, a protected message can appear in both generations) and adds
  that compression can also split a session into a child linked by `parent_session_id` [32]. Which of the two
  happens in v0.22.5, or both, is **[U]**; opening a `state.db` made by v0.22.5 after a compaction would
  settle it.
- **Summary.** Shinatama plans to mark the summary as a compaction [R 24]; the design draft says upstream
  documents no stored summary row, so a Hermes session would yield no compaction entry [32]. Where the
  summary text lives, if it is stored, is **[U]**.
- **Not a deletion.** Hermes's documentation warns that compression shortens the active context but is
  not a privacy deletion [28].

### Ephemeral runs

Nothing here was run [V][31]; every statement is from documentation or source.

- **One turn, no terminal.** `hermes chat --oneshot -q "<text>" --format stream-json`, `-z "<prompt>"`, or `-q`
  without a TTY (§3) [S 14][S 15]. The session id comes back in `system/init`, in `result` and on stderr, not
  from the caller (§3).
- **Does a one-shot leave a session?** **[U]**. `sessions.source` has a `oneshot` value that pickers hide
  [32], which suggests it does; no flag that prevents it was found. A one-shot turn in a scratch
  `HERMES_HOME`, with `state.db` listed before and after, would settle it.
- **Isolation flags.** `--ignore-user-config` (needs `--model` and `--provider`), `--ignore-rules`,
  `--safe-mode` [6]. The only lever that moves settings, hooks and the session store together is
  `HERMES_HOME` or the profile [R 23].
- **Approvals.** `approvals.single_query_mode: deny` is the default for one-shot runs [7]. Do not pass
  `--yolo` (or set `HERMES_YOLO_MODE=1`) for a read-only run [R 23].
- **Limiting the tools of a turn.** `--toolsets` names whole toolsets, and the registry returns only the
  requested tool names [3][S 16][S 17]. A bare tool name is refused (`Unknown toolset`) [S 16]. The
  toolsets that matter for a search-only or read-only turn:

| `--toolsets` | tools left | note |
|---|---|---|
| `search` | `web_search` | [S 11] |
| `web` | `web_search`, `web_extract` | [S 11][5] |
| `search,file` | `web_search`, `read_file`, `search_files`, `write_file`, `patch` | removes terminal, `execute_code`, browser, delegation, memory, skills and `web_extract`; still has the two write tools [S 11] |
| `safe` | `web_search`, `web_extract`, `vision_analyze`, `image_generate` | no file tools [S 11] |
| `hermes-webhook` | `web_search`, `web_extract`, `vision_analyze`, `clarify` | its source comment says payloads are untrusted [S 11] |

No built-in toolset holds the read tools and search without the write tools [5][S 11]. There is no
answer-only flag; which toolset removes tools and write access is a planned check, HM7 [R 23][26].

| question | status | what would settle it |
|---|---|---|
| choose the tools of a turn | Established for a new session per turn: `--toolsets` selects whole toolsets [3][S 16][S 17]. Not established for a resumed session: launch flags are frozen and `/tools` is not an allowed slash command there [6]. Whether MCP or plugin tools join a `--toolsets file` turn: **[U]** | `hermes chat --oneshot -q "list your tools" --toolsets file --format stream-json` in a scratch `HERMES_HOME`; the model should name no web tool. Names a model turn; not run [29] |
| see the queries and pages a turn used | Established from source for `web_search` (query in `tool_use.input.query`, URLs in `tool_result.output`) and `web_extract` (URLs in `input.urls`, text in `output`), cut at 5000 characters [S 14]. **[U]**: whether the callbacks stay wired in quiet `-q`; the `web_search` result shape at the tag | `... --toolsets search`; observe the query and the URLs. Names a search; not run [29] |
| a turn without write tools | **[U]**. (a) `custom_toolsets` in `config.yaml` is documented for toolset names [5], but no loader was found in `tools_config.py` [S 19], and `create_custom_toolset(name, description, tools, includes)` takes tool names [S 11]: syntax **[U]**. (b) A shell `pre_tool_call` hook can block or fail closed [8]; payload and matcher **[U]**. (c) `agent.disabled_toolsets` removes tools by toolset name only, and no toolset holds just `write_file` and `patch` [S 16][S 11] | a scratch config with the candidate `custom_toolsets` entry, then `hermes tools` and `hermes hooks list` (no model turn); or `hermes hooks test pre_tool_call` to read the payload [29] |
| read only URLs a search returned | **[U]**: no flag or setting [S 13]; only a denylist [7]. A hook that keeps the URLs seen in `post_tool_call` results works only if the shell payload carries them; the plugin payload has `result` [8] | `hermes hooks test post_tool_call` with a logging hook; read its stdin fields [29] |
| stateless run and resume by id | Established for stateless runs [S 15] and for resume by id [6]; a read-only run depends on the third row | as above |

- **Other network.** No network-off switch was found **[U]**. MCP servers (`mcp-<server>` toolsets) may
  join a turn that has an explicit `--toolsets` **[U]**. `execute_code` can call `web_search` and
  `terminal` [S 22], so keep `code_execution` off.
- **Hidden queries.** None found for the web tools: `tool_use.input` is complete and uncut [S 14]. Only
  truncation: 5000 characters of results in `stream-json`, and 80 and 10 in ACP [S 14][S 20].
  `x_search` (off by default) runs inside xAI's tool; its result still carries `answer`, `citations` and
  `inline_citations {url, title}` [S 22]. Whether a `delegate_task` sub-agent's calls appear in the parent's
  stream is **[U]**.

### Built-in tools

The complete list: **86 rows** = 51 tools in the shared core list `_HERMES_CORE_TOOLS` + 31 more named in
other `TOOLSETS` entries (82 in all, read in `toolsets.py` [S 11]) + `kanban_schedule` (registered in
`kanban_tools.py` but in no `toolsets.py` list that was read) + the 3 tool-search bridge tools. It was built
from `toolsets.py` at the tag, then one registration file per tool [S 22], then the docs tools reference [4]
[29]. Not run: the real count a model sees varies with each tool's availability check, with keys and with the
platform [29]. Honcho, Home Assistant and Spotify are catalog plugins in v0.22.5, not built-in tools [4]. MCP
tools form one toolset `mcp-<server>` per configured server [5]; the name a model sees for them is **[U]**.

How to read the tables. `class` is the research report's grouping (web, read, write, shell, agent,
interaction, other). `read-only?` is the report's judgement of each handler [29]; `[U]` where the tool's
effect was not read. `switch` is how to turn the toolset on or off: `--toolsets <name>` on `hermes chat`,
`agent.disabled_toolsets`, `hermes tools` or `/tools disable` [3][5]; **off** means off by default.
`latest version present`: every row is present at v0.22.5, the only version read; no other version was
compared **[U]**. The citation says where the row was read: `[S 11]` the name in `toolsets.py`; `[S 12]`,
`[S 18]`, `[S 22]` the registration file; `[4]` the docs tools reference. Parameters marked `[U]` are those
of tools whose registration file was not read.

**Default toolset** (`hermes-cli`): the 51 core tools, minus every tool whose availability check fails
[S 17], minus the default-off toolsets spotify, discord, discord_admin, video, video_gen, x_search, a2a and
kanban [S 19]. `hermes-acp` also drops `image_generate`, `text_to_speech`, `cronjob_manage`, `computer_use`,
`clarify` and `kanban_*` [S 11]. The toolsets `desktop_ui`, `project`, `onboarding` and `catalog` are only
for desktop-app sessions [S 11]. `browser_vault` is deferred by default behind the bridge tools
[S 11][S 18].

**Needs a key or an optional dependency.** `web_search`, `web_extract` (a backend key, the Nous
subscription, the `ddgs` package or `SEARXNG_URL`); `x_search` (`XAI_API_KEY` or xAI OAuth);
`image_generate` (`FAL_KEY`, the Nous gateway, Krea or a plugin); `text_to_speech` (the default `edge` needs
the `edge-tts` extra, the others their key or package); `vision_analyze` (an image-capable main model or an
auxiliary vision model); `browser_*` (the agent-browser CLI, Node, Chromium or a cloud or CDP backend);
`computer_use` (`cua-driver`); `discord` (`DISCORD_BOT_TOKEN` [4]); `feishu_*` and `yb_*` (platform
credentials [4]); `video_generate` (`XAI_API_KEY` or `FAL_KEY` [4]); `session_search` (the SQLite state
database); `cronjob_manage` (only inside an interactive, gateway or cron session); `kanban_*`
(`HERMES_KANBAN_TASK` or the profile's `kanban` toolset); `terminal` (the configured backend's
requirements) [29].

**Web and search (2 tools)**

| tool | toolset | what it does and main parameters | class | read-only? | switch | latest version present |
|---|---|---|---|---|---|---|
| `web_search` | web, search | Looks up the web; `query` is the search text, `limit` 1-100 (default 5); returns title, URL, description | web | no | `--toolsets web` or `search`; on in `hermes-cli`; `agent.disabled_toolsets` | v0.22.5 [S 12][4] |
| `web_extract` | web | Reads page text for URLs; `urls` (max 5, required), `char_limit` caps each page (min 2000, default 15000) | web | no | `--toolsets web`; on in `hermes-cli` | v0.22.5 [S 12][4] |

**Files (4 tools)**

| tool | toolset | what it does and main parameters | class | read-only? | switch | latest version present |
|---|---|---|---|---|---|---|
| `read_file` | file | Reads a text file with line numbers; `path`, `offset`, `limit` | read | yes | `--toolsets file`; on | v0.22.5 [S 22][4] |
| `write_file` | file | Replaces a whole file; `path`, `content`; refuses without a prior read | write | no | `--toolsets file`; on | v0.22.5 [S 22][4] |
| `patch` | file | Find-and-replace edit; `path`, `old_string`, `new_string`, `replace_all` (V4A `mode`/`patch` for OpenAI-family models) | write | no | `--toolsets file`; on | v0.22.5 [S 22][4] |
| `search_files` | file | Regex search of contents or find files by name; `pattern`, `target` content/files, `path`, `file_glob`, `limit`, `output_mode`, `context` | read | yes | `--toolsets file`; on | v0.22.5 [S 22][4] |

**Shell, code and delegation (4 tools)**

| tool | toolset | what it does and main parameters | class | read-only? | switch | latest version present |
|---|---|---|---|---|---|---|
| `terminal` | terminal | Runs a shell command on the configured backend; `command`, `timeout`, `workdir`, `background` (returns a session_id) | shell | no | `--toolsets terminal`; on; dangerous commands need approval [7] | v0.22.5 [S 22][4] |
| `process_manage` | terminal | Controls background terminal processes; `action` list/poll/log/wait/kill/write/submit/close/handoff, `session_id` | shell | no | `--toolsets terminal`; on | v0.22.5 [S 22][4] |
| `execute_code` | code_execution | Runs Python that calls Hermes tools (`web_search`, `web_extract`, `read_file`, `write_file`, `search_files`, `patch`, `terminal`); `code`, `reset` | shell | no | `--toolsets code_execution`; on if the backend check passes | v0.22.5 [S 22][4] |
| `delegate_task` | delegation | Starts sub-agents in isolated contexts that inherit the parent's toolsets; `tasks[].goal` (required) with `context`; `action` spawn/list/steer/stop | agent | no | `--toolsets delegation`; on | v0.22.5 [S 22][4] |

**Browser (14 tools).** The first ten were read in `browser_tool.py`; the research report does not mark them
as listed in the docs tools reference [4] (the docs' `browser` toolset lists 4 tools, the tag 14 [5][S 11]).
The last four were read in `toolsets.py` and are in the docs [4][29].

| tool | toolset | what it does and main parameters | class | read-only? | switch | latest version present |
|---|---|---|---|---|---|---|
| `browser_navigate` | browser | Opens a URL in the browser; `url` | web | no | `--toolsets browser`; on if the agent-browser or CDP backend check passes | v0.22.5 [S 22] |
| `browser_snapshot` | browser | Text accessibility-tree snapshot of the page; `full` | web | no | as `browser_navigate` | v0.22.5 [S 22] |
| `browser_click` | browser | Clicks an element by its snapshot ref; `ref` | web | no | as `browser_navigate` | v0.22.5 [S 22] |
| `browser_type` | browser | Types into an input; `ref`, `text` | web | no | as `browser_navigate` | v0.22.5 [S 22] |
| `browser_scroll` | browser | Scrolls the page; `direction` up/down | web | no | as `browser_navigate` | v0.22.5 [S 22] |
| `browser_back` | browser | History back; no parameters | web | no | as `browser_navigate` | v0.22.5 [S 22] |
| `browser_press` | browser | Presses a key; `key` | web | no | as `browser_navigate` | v0.22.5 [S 22] |
| `browser_get_images` | browser | Lists the page's images with URL and alt text; no parameters | web | no | as `browser_navigate` | v0.22.5 [S 22] |
| `browser_vision` | browser | Screenshot of the page analysed by a vision model; `question`, `annotate` | web | no | as `browser_navigate` | v0.22.5 [S 22] |
| `browser_console` | browser | Reads console output and JS errors, or evaluates an expression; `clear`, `expression` | web | no | as `browser_navigate` | v0.22.5 [S 22] |
| `browser_cdp` | browser | Raw Chrome DevTools Protocol command; needs a CDP endpoint; parameters **[U]** | web | no | as `browser_navigate` | v0.22.5 [S 11][4] |
| `browser_dialog` | browser | Answers a blocking JS dialog; `action` accept/dismiss; needs a CDP endpoint | web | no | as `browser_navigate` | v0.22.5 [S 11][4] |
| `browser_vault` | browser | Saved logins, cards, codes; `action` list/fill/save_login/enter_code/unlock | web | no | as `browser_navigate`; deferred behind the bridge tools by default | v0.22.5 [S 11][4] |
| `browser_exec` | browser | Runs Python with browser helpers in a session; replaces the other browser tools when `browser.backend` is browser-use [S 11]; parameters **[U]** | web | no | as `browser_navigate` | v0.22.5 [S 11][4] |

**Vision, images and speech (3 tools)**

| tool | toolset | what it does and main parameters | class | read-only? | switch | latest version present |
|---|---|---|---|---|---|---|
| `vision_analyze` | vision | Loads an image into the conversation so the model sees it; `image_url` (URL, path or data URL), `question`, `region` crop | web | no | `--toolsets vision`; on if the model is image-capable or an auxiliary vision model resolves | v0.22.5 [S 22][4] |
| `image_generate` | image_gen | Generates an image from text; `prompt`, `aspect_ratio` (editing parameters depend on the backend) | web | no | `--toolsets image_gen`; on with a provider | v0.22.5 [S 22][4] |
| `text_to_speech` | tts | Converts text to an audio file; `text`, `output_path`, `speed` | other | no | `--toolsets tts`; on | v0.22.5 [S 22][4] |

**Skills, memory, past sessions, tasks and questions (7 tools)**

| tool | toolset | what it does and main parameters | class | read-only? | switch | latest version present |
|---|---|---|---|---|---|---|
| `skills_list` | skills | Lists skill names and descriptions; `category` | read | yes | `--toolsets skills`; on | v0.22.5 [S 22][4] |
| `skill_view` | skills | Loads a skill or a linked file; `name`, `file_path` | read | yes | `--toolsets skills`; on | v0.22.5 [S 22][4] |
| `skill_manage` | skills | Creates, patches or deletes skills; `operations[]`, each with `name` and `action` create/patch/write_file/remove_file/delete | write | no | `--toolsets skills`; on; `skills.write_approval` stages writes [S 21] | v0.22.5 [S 22][4] |
| `memory` | memory | Saves durable facts across sessions; `target` memory/user, `action` add/patch/remove, `content`, `old_text` | write | no | `--toolsets memory`; on; `memory.write_approval` | v0.22.5 [S 22][4] |
| `session_search` | session_search | Searches or scrolls past sessions (FTS5); `query`, `limit`, `after`, `before`, `session_id` | read | yes | `--toolsets session_search`; on | v0.22.5 [S 22][4] |
| `todo_list` | todo | Keeps a task list for multi-step work; `todos[]` with `id`, `content`, `status`; `merge` | other | yes | `--toolsets todo`; on | v0.22.5 [S 22][4] |
| `clarify` | clarify | Asks the user 1-5 questions; `questions[]` with `question`, `choices`, `multi_select` | interaction | yes | `--toolsets clarify`; on; not in `hermes-acp` | v0.22.5 [S 22][4] |

**Scheduling, desktop control and connections (3 tools)**

| tool | toolset | what it does and main parameters | class | read-only? | switch | latest version present |
|---|---|---|---|---|---|---|
| `cronjob_manage` | cronjob | Creates and runs scheduled jobs; `action` create/list/update/pause/resume/remove/run, `prompt`, `schedule`, `job_id` | other | no | `--toolsets cronjob`; on in interactive and gateway sessions | v0.22.5 [S 22][4] |
| `computer_use` | computer_use | Background desktop control through `cua-driver`; parameters **[U]** | other | no | `--toolsets computer_use`; not enabled by `all` [5] | v0.22.5 [S 22][4] |
| `manage_connections` | connections | Shows, connects and renames connector accounts; `action` display/status/connect/reconnect/rename and MCP install/enable/authorize | other | no | `--toolsets connections`; service-gated | v0.22.5 [S 11][4] |

**X search and video (5 tools, all off by default)**

| tool | toolset | what it does and main parameters | class | read-only? | switch | latest version present |
|---|---|---|---|---|---|---|
| `x_search` | x_search | Searches X posts through xAI's built-in tool (provider side); `query`, `allowed_x_handles`, `excluded_x_handles`, `from_date`, `to_date` | web | no | `--toolsets x_search`; **off** | v0.22.5 [S 22][4] |
| `video_analyze` | video | Captions, scenes, timestamps from a video URL or path; parameters **[U]** | web | no | `--toolsets video`; **off** | v0.22.5 [S 11][4] |
| `video_generate` | video_gen | Text-to-video or image-to-video; `image_url`; parameters **[U]** | web | no | `--toolsets video_gen`; **off** | v0.22.5 [S 11][4] |
| `xai_video_edit` | video_gen | Edits a video given a public HTTPS MP4 `video_url`; other parameters **[U]** | web | no | `video_gen`; **off** | v0.22.5 [S 11][4] |
| `xai_video_extend` | video_gen | Extends a video given a public HTTPS MP4 `video_url`; other parameters **[U]** | web | no | `video_gen`; **off** | v0.22.5 [S 11][4] |

**Kanban (15 tools, off by default)**

| tool | toolset | what it does and main parameters | class | read-only? | switch | latest version present |
|---|---|---|---|---|---|---|
| `kanban_show` | kanban | Full state of a task (row, parents, comments, runs, events); `task_id`, `board` | other | yes | `kanban`; **off** unless a dispatcher worker | v0.22.5 [S 22][4] |
| `kanban_list` | kanban | Task summaries with filters; `assignee`, `status`, `limit` | other | yes | `kanban`; orchestrator only | v0.22.5 [S 22][4] |
| `kanban_complete` | kanban | Marks the current task done with a handoff; `summary`, `result`, `metadata` | other | no | `kanban`; **off** | v0.22.5 [S 22][4] |
| `kanban_block` | kanban | Blocks the task with a reason; `reason`, `kind` | other | no | `kanban`; **off** | v0.22.5 [S 22][4] |
| `kanban_request_review` | kanban | Moves the task into review; `summary`, `reviewer` | other | no | `kanban`; **off** | v0.22.5 [S 22][4] |
| `kanban_request_changes` | kanban | Returns a reviewed task to its implementer; `reason` | other | no | `kanban`; **off** | v0.22.5 [S 22][4] |
| `kanban_heartbeat` | kanban | Signals liveness and extends the claim; `note` | other | no | `kanban`; **off** | v0.22.5 [S 22][4] |
| `kanban_comment` | kanban | Appends a comment to a task; `task_id`, `body` | other | no | `kanban`; **off** | v0.22.5 [S 22][4] |
| `kanban_create` | kanban | Creates a child task; `title`, `assignee`, `body`, `parents` | other | no | `kanban`; **off** | v0.22.5 [S 22][4] |
| `kanban_link` | kanban | Adds a parent-child dependency; `parent_id`, `child_id` | other | no | `kanban`; **off** | v0.22.5 [S 22][4] |
| `kanban_unblock` | kanban | Moves a blocked task to ready; `task_id` | other | no | `kanban`; orchestrator only | v0.22.5 [S 22][4] |
| `kanban_attach` | kanban | Attaches an inline base64 file (25 MB cap [4]); `filename`, `content_base64` | other | no | `kanban`; **off** | v0.22.5 [S 22][4] |
| `kanban_attach_url` | kanban | Attaches a file fetched from an http(s) URL (25 MB cap [4]); `url`, `filename` | web | no | `kanban`; **off** | v0.22.5 [S 22][4] |
| `kanban_attachments` | kanban | Lists a task's attachments (read-only per its handler); `task_id` | other | yes | `kanban`; **off** | v0.22.5 [S 22][4] |
| `kanban_schedule` | kanban | Parks the current task until an orchestrator re-gates it; `reason`. Registered, but in no `toolsets.py` list that was read, so whether it is offered is **[U]** | other | no | `kanban` **[U]** | v0.22.5 [S 22] (registered) |

**Chat platforms (12 tools: Discord, Feishu/Lark, Yuanbao)**

| tool | toolset | what it does and main parameters | class | read-only? | switch | latest version present |
|---|---|---|---|---|---|---|
| `discord` | discord | Reads and takes part in Discord servers; parameters **[U]** | other | no | `discord`; **off**; needs `DISCORD_BOT_TOKEN` | v0.22.5 [S 11][4] |
| `discord_admin` | discord_admin | Manages Discord servers through REST; parameters **[U]** | other | no | `discord_admin`; **off** | v0.22.5 [S 11][4] |
| `feishu_doc_read` | feishu_doc | Reads a Feishu/Lark document's text (comment handler only); parameters **[U]** | web | no | `feishu_doc`; platform only | v0.22.5 [S 11][4] |
| `feishu_drive_add_comment` | feishu_drive | Adds a comment to a Feishu drive file; parameters **[U]** | web | no | `feishu_drive`; platform only | v0.22.5 [S 11][4] |
| `feishu_drive_list_comments` | feishu_drive | Lists comments on a Feishu drive file; parameters **[U]** | web | no | `feishu_drive`; platform only | v0.22.5 [S 11][4] |
| `feishu_drive_list_comment_replies` | feishu_drive | Lists replies to a comment; parameters **[U]** | web | no | `feishu_drive`; platform only | v0.22.5 [S 11][4] |
| `feishu_drive_reply_comment` | feishu_drive | Replies to a comment; parameters **[U]** | web | no | `feishu_drive`; platform only | v0.22.5 [S 11][4] |
| `yb_query_group_info` | yuanbao | Reads a Yuanbao group's info; parameters **[U]** | web | no | `yuanbao`; platform only | v0.22.5 [S 11][4] |
| `yb_query_group_members` | yuanbao | Lists a Yuanbao group's members; parameters **[U]** | web | no | `yuanbao`; platform only | v0.22.5 [S 11][4] |
| `yb_send_dm` | yuanbao | Sends a direct message on Yuanbao; parameters **[U]** | web | no | `yuanbao`; platform only | v0.22.5 [S 11][4] |
| `yb_search_sticker` | yuanbao | Searches stickers on Yuanbao; parameters **[U]** | web | no | `yuanbao`; platform only | v0.22.5 [S 11][4] |
| `yb_send_sticker` | yuanbao | Sends a sticker on Yuanbao; parameters **[U]** | web | no | `yuanbao`; platform only | v0.22.5 [S 11][4] |

**Desktop app only (14 tools)**

| tool | toolset | what it does and main parameters | class | read-only? | switch | latest version present |
|---|---|---|---|---|---|---|
| `desktop_project` | project | Creates, lists and switches desktop projects (`action`); desktop sessions only | other | no | `project`; desktop only | v0.22.5 [S 11][4] |
| `read_terminal` | desktop_ui | Desktop-app terminal pane affordance; effect and parameters **[U]** | other | **[U]** | `desktop_ui`; desktop only | v0.22.5 [S 11][4] |
| `close_terminal` | desktop_ui | Desktop-app terminal pane affordance; effect and parameters **[U]** | other | **[U]** | `desktop_ui`; desktop only | v0.22.5 [S 11][4] |
| `desktop_preview` | desktop_ui | Desktop-app preview pane affordance; effect and parameters **[U]** | other | **[U]** | `desktop_ui`; desktop only | v0.22.5 [S 11][4] |
| `drive_preview` | desktop_ui | Desktop-app preview pane affordance; effect and parameters **[U]** | other | **[U]** | `desktop_ui`; desktop only | v0.22.5 [S 11][4] |
| `annotate_preview` | desktop_ui | Desktop-app preview pane affordance; effect and parameters **[U]** | other | **[U]** | `desktop_ui`; desktop only | v0.22.5 [S 11][4] |
| `read_window_below` | desktop_ui | Desktop-app window affordance; effect and parameters **[U]** | other | **[U]** | `desktop_ui`; desktop only | v0.22.5 [S 11][4] |
| `focus_pane` | desktop_ui | Desktop-app pane focus affordance; effect and parameters **[U]** | other | **[U]** | `desktop_ui`; desktop only | v0.22.5 [S 11][4] |
| `react_to_message` | desktop_ui | Desktop-app message reaction; effect and parameters **[U]** | other | **[U]** | `desktop_ui`; desktop only | v0.22.5 [S 11][4] |
| `gui_tour` | desktop_ui | Desktop-app guided tour; effect and parameters **[U]** | other | **[U]** | `desktop_ui`; desktop only | v0.22.5 [S 11][4] |
| `show_tip` | desktop_ui | Desktop-app tip; effect and parameters **[U]** | other | **[U]** | `desktop_ui`; desktop only | v0.22.5 [S 11][4] |
| `set_theme` | desktop_ui | Desktop-app theme change; effect and parameters **[U]** | other | **[U]** | `desktop_ui`; desktop only | v0.22.5 [S 11][4] |
| `setup_beat` | onboarding | First-run onboarding cards; desktop only; parameters **[U]** | other | **[U]** | `onboarding`; desktop only | v0.22.5 [S 11] |
| `manage_catalog` | catalog | Searches and installs catalog plugins and hub skills through an approval card (`search`, `install`); desktop only | other | no | `catalog`; desktop only | v0.22.5 [S 11][4] |

**Tool-search bridge (3 tools)**

| tool | toolset | what it does and main parameters | class | read-only? | switch | latest version present |
|---|---|---|---|---|---|---|
| `tool_search` | bridge | Finds deferred tools by query (up to 7 queries per call) | other | yes | `tools.tool_search.enabled` auto/on/off; on when any tool is deferrable | v0.22.5 [S 18] |
| `tool_describe` | bridge | Loads the full schema of deferred tools (up to 10 names per call) | other | yes | as `tool_search` | v0.22.5 [S 18] |
| `tool_call` | bridge | Invokes a deferred tool as `{name, arguments}` | other | no | as `tool_search` | v0.22.5 [S 18] |

**Web search and page reading in detail** (the part of the tool list that the research report examined most
closely [29]):

- **Name and parameters.** The model-visible name is `web_search` (`tool_use.name` in the stream) [S 12][S 14];
  parameters `query` (required) and `limit` (1-100, default 5) [S 12]. `web_extract` takes `urls` (an array,
  at most 5, required) and `char_limit` (minimum 2000, default 15000), returns markdown or text with no LLM
  summary, and also reads PDFs [S 12][4].
- **Where it runs.** In the Hermes process, as HTTP to a backend, not at the model provider [S 12]. The
  backend is `web.search_backend`, else `web.backend`, else the first that autodetects: tavily, perplexity,
  exa, parallel, keenable, firecrawl (a key, or the Nous gateway), searxng (`SEARXNG_URL`), brave-free
  (`BRAVE_SEARCH_API_KEY`), ddgs (a package, no key), then plugin providers; the last fallback is `firecrawl`
  [S 12]. `web.backend: nous` uses the Nous Tool Gateway: search through Perplexity and extraction through
  Firecrawl, with a paid Nous subscription and no separate key [S 12][9].
- **Account.** One backend key (`EXA_API_KEY`, `PARALLEL_API_KEY`, `TAVILY_API_KEY`, `PERPLEXITY_API_KEY`,
  `KEENABLE_API_KEY`, `FIRECRAWL_API_KEY` or `FIRECRAWL_API_URL`), or the Nous subscription, or a keyless tier
  (the `ddgs` package, `SEARXNG_URL`) [S 12][4]. With none, the tool is not offered
  (`check_fn=check_web_api_key`) [S 12][S 17].
- **Switch.** `web` is in the shared core list, so `hermes-cli` has it; it is also in `hermes-acp`,
  `hermes-api-server`, `hermes-webhook`, `coding`, `debugging` and `safe` [S 11][5]. Turn it off with
  `agent.disabled_toolsets: [web]` or `[search]` (removed per tool, last) [10][S 19][S 16], with
  `hermes tools`, or with `/tools disable` [5].
- **Result shape.** `web_search` returns a JSON string
  `{"success":..,"data":{"web":[{title,url,description,position}]}}`, read at `main`; the tag's shape is
  **[U]** [S 12]. A stream-json consumer sees it only while it is under 5000 characters (a `limit` near 100
  is cut) [S 14]. `web_extract` returns entries `{url,title,content,error}`, cut at 5000 in the stream
  [S 13].
- **Page-reading limits.** None per turn and none tied to search results [S 13]. Only a denylist,
  `security.website_blocklist: {enabled: true, domains: ["*.internal.company.com"], shared_files:
  ["/path/list.txt"]}`, and an always-on floor against private, loopback, link-local, CGNAT and cloud-metadata
  addresses (`security.allow_private_urls`, default false) [7]. URLs that look like they carry an API key are
  refused [S 13]. The policy check in `_extract_safe_urls` treats an exception as "not blocked" [S 13].
- **Untrusted content.** No scanning of web content or tool results is documented; only context files
  (`AGENTS.md`, `.cursorrules`, `SOUL.md`) are scanned [7]. The schemas of `web_search` and `web_extract`
  carry no untrusted-content warning [S 12].

### Branches, forks and subagents

- **Fork.** `/branch` (alias `/fork`) inside a session starts a branch [R 23]. How a fork is recorded in
  `state.db` (for instance through `parent_session_id`) is **[U]**; a fork made in a scratch `HERMES_HOME`,
  then a look at `sessions`, would settle it.
- **Resume.** `hermes chat --resume <id|title|latest>`, `--continue [title]`; the resume goes back to the
  session's recorded directory unless `--no-restore-cwd`; one session resumed in two terminals is a planned
  check, HM6 [6][R 23][26].
- **Sub-agents.** The `delegate_task` tool starts sub-agents in isolated contexts that inherit the parent's
  toolsets; `tasks[].goal` is required, `context` optional, and `action` is spawn, list, steer or stop
  [S 22]. A sub-agent run is its own session row with `parent_session_id` and a `_delegate_from` marker in
  `model_config`, hidden from pickers [R 24]. Its logs under `cache/delegation/live/` are pruned after
  7 days [R 23]. Ctrl+T opens a full-screen roster of sub-agents in the classic CLI [R 23]. The delegation
  backends are pluggable, so a sub-agent's work must not be assumed inlined as text [28]. Whether a
  sub-agent's calls appear in the parent's `stream-json` is **[U]**.
- **Kanban.** The 15 `kanban_*` tools (§5, Built-in tools) are off by default and belong to dispatcher
  workers and an orchestrator [S 22].

### Models and providers

- **Providers.** Many, among them OpenRouter, Nous, OpenAI Codex, Anthropic, Gemini and Bedrock [R 23]. The
  complete list is **[U]**.
- **Choosing and signing in.** `hermes model`, `hermes setup`, `hermes auth add <provider> [--type
  oauth|--api-key]`; status without secrets: `hermes status`, `hermes auth list`, `hermes auth status
  <provider>`. Secrets live in `<home>/.env` and `auth.json` [R 23]. `/model` changes the model for the
  session [R 23]. `--model` and `--provider` exist as flags: `--ignore-user-config` needs them [6].
- **What is recorded.** The `sessions` table has `model` and `model_config` columns [R 24]; the stream has
  the `model` in `system/init` and `tokens {input, output, total, cache_read, cache_write}` in `result`
  [S 14]. Which provider or protocol each assistant row used, and how reasoning is stored (`reasoning`, and
  in the design draft `reasoning_content`, `reasoning_details`, `codex_reasoning_items`, which may hold
  signed or encrypted payloads on some routes), is **[U]** [R 24][32]. A secondary report calls reasoning
  persistence not a stable cross-model replay contract [28].

## 6. Plugins and extensions

| mechanism | where it lives | how it is added |
|---|---|---|
| MCP client | `mcp_servers:` in the profile's `config.yaml` (stdio or url) | `hermes mcp add\|list\|remove\|test` [R 23]. Each configured server becomes a toolset `mcp-<server>` [5]. There is no per-launch MCP switch; ACP attaches `mcpServers` per session at `session/new` [R 23] |
| skills | `<home>/skills/<name>/SKILL.md` (agentskills.io format); `skills.external_dirs` (for example `~/.agents/skills`); a project's `.hermes/skills/` or `.agents/skills/` only after `hermes skills trust` [R 23] | the folder; the `skill_manage` tool creates, patches and deletes skills, and `skills.write_approval` stages those writes [S 21][S 22] |
| plugins | `<home>/plugins/`: `plugin.yaml` plus Python, off until enabled [R 23] | the enabling command is **[U]** |
| Agent Plugins v1.0.0 packages | a package with `plugin.json`, `skills/` and `mcp.json` [R 23] | the install command is **[U]** |
| catalog plugins and hub skills | the catalog | the `manage_catalog` tool searches and installs through an approval card, in desktop sessions only [S 11][4]. Honcho, Home Assistant and Spotify are catalog plugins [4] |
| search providers | plugin providers come after the built-in search backends [S 12] | `web.search_backend` or `web.backend` [S 12] |
| custom toolsets | `custom_toolsets` in `config.yaml` is documented [5]; no loader was found in `tools_config.py` [S 19] | whether it works, and its syntax, is **[U]** |
| context files | `.hermes.md`/`HERMES.md`, `AGENTS.override.md`, `AGENTS.md`, `CLAUDE.md`, `.cursorrules`, `.cursor/rules/*.mdc`; `SOUL.md` from the home [R 23] | placed in the project or the home (§2) |

Claude Code plugins and marketplaces are not loaded [R 23]. The Nous Tool Gateway is a paid service rather
than an extension (§5) [9].

## 7. Hooks

Hermes has shell hooks, set in the profile's `config.yaml` under `hooks:`, each with a `matcher`, a
`command`, a `timeout` and `fail_closed` [R 23]. A hook receives a JSON payload on stdin with `session_id` and
`cwd`, and answers in a block-or-modify format that is compatible with Claude Code's [R 23]. The payload's
other fields and the matcher syntax are **[U]**: the shell-hook section of the hooks page was not reached
(the first 100000 of 149287 characters were read) [8]. The research report names `hermes hooks list` and
`hermes hooks test <event>` (to see the shell-hook payload) as the cheap probes; neither was run [29].

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
- **No per-launch switch.** Hooks cannot be turned on or off for one launch. The only levers are
  `HERMES_HOME` and the profile, and both also switch the session store [R 23].
- **Plugin hooks.** A plugin's `post_tool_call` payload carries `result` [8]; whether a shell hook's payload
  does is **[U]**.
- **mukade** neither reads nor writes hook configuration, and ADR-0002 does not propose it: it concerns
  sessions only [27].

## 8. How mukade reads and writes it

**Status: not implemented.** mukade has no reader, writer, catalog entry, fixture or test for Hermes. Checked
on 2026-11-01 by searching `src/`, `catalog/`, `tests/`, `fixtures/`, `README.md`, `Cargo.toml` and
`docs/`: nothing under `src/`, `catalog/`, `tests/` or `fixtures/` mentions Hermes, and the only existing
documents that do are ADR-0002, the ADR index, the design draft and the persistence reference [V][30].

| | |
|---|---|
| CLI name | none. `HarnessId` has four variants (`ClaudeCode`, `Codex`, `OpenCode`, `Pi`) and `HarnessId::ALL` has four entries (`src/model/harness.rs:8`, `:17`); the unknown-harness error lists the same four (`src/model/harness.rs:44`) [V][30]. ADR-0002's examples write `hermes`, and the design draft proposes that id as the author's own call [27][32] |
| store variable | none: no `HERMES_HOME` handling in `src/`, `catalog/` or `tests/` [V][30] |
| executable | none: `catalog/harnesses.json` has no Hermes entry, and there is no `--hermes-exe` flag or `MUKADE_HERMES_EXE` variable [V][30] |
| detection, `list`, `dump`, `scan`, `harnesses` | not implemented |
| reading a session | not implemented |
| writing a session, hop destination | not implemented, and not intended: ADR-0002 would refuse it (below) [27] |
| tool translation | not implemented: no Hermes names in [`../tool-conversion-tables.md`](../tool-conversion-tables.md) [V][30] |
| resume command in a receipt | none, because mukade would never write a Hermes session [27] |

**What ADR-0002 proposes.** The record is **Proposed**: version 1, 2026-10-31, `accepted_on` empty [27]. For
Hermes it says:

- **Source-only.** mukade reads Hermes sessions and never writes one, until upstream offers a supported way
  to add a session without writing `state.db` behind the owning gateway [27].
- **Refusal first.** A hop or write toward a source-only harness is refused with exit code 2 and the reason,
  before any plan or write, and nothing is written (INV-002; example EX-002: `mukade hop claude_code
  <session-id> hermes`). Whether a harness can be a destination becomes part of its definition in the library
  [27].
- **One verified store version.** The reader accepts only the `state.db` schema it was verified against,
  which the record gives as **31**, and refuses any other naming the version found (INV-003; example EX-003:
  `mukade list hermes`). The sources disagree on the number (§5, schema version) **[U]** [27][R 24].
- **Never open for writing.** No code path opens a Hermes `state.db` for writing (INV-001); a test checks
  that a Hermes fixture opened by mukade is byte-identical afterwards [27].
- **Nominal case.** EX-001: `mukade hop hermes <session-id> claude_code` over a schema-31 `state.db` writes a
  new Claude Code session and the manifest lists every loss, as for the four existing sources [27].
- **Rejected.** Writing `state.db` directly (RA-001: the gateway is the only canonical writer and an upstream
  issue links concurrent writers to corruption), generating a backup zip and running `hermes import`
  (RA-002: it overwrites files in the Hermes home), and read-write behind a bypass flag (RA-005) [27].
- **Consequences.** `HarnessId` grows and `catalog/harnesses.json` gets an entry; a harness capability
  (can be a source, can be a destination) is added to the library; "four harnesses" in the README, the
  specification and the usage guide becomes six, with Hermes marked read-only. `rusqlite` with `bundled` is
  already a dependency (`Cargo.toml`: `rusqlite = { version = "=0.40.2", features = ["bundled"] }`) [27][V].

**The design draft** (`docs/superpowers/specs/2026-10-31-hermes-deepseek-harness-support-design.md`, status
"design draft, awaiting approval") adds choices that it marks as the author's own [32]:

- Open `state.db` read-only through the bundled `rusqlite`, check `schema_version == 31`, and read messages in
  `messages.id` order within a session. Rows with `active=1` are the conversation; rows with `active=0` are
  the generation an in-place compaction archived and become `Meta` (off-branch).
- A Hermes session yields no compaction entry, and `--compaction portable` from it is refused with that
  reason, because upstream documents no stored summary row **[U]**.
- `reasoning` and `reasoning_content` become reasoning text; `reasoning_details` and `codex_reasoning_items`
  stay in `raw` and are reported as a loss, since they may hold signed or encrypted payloads **[U]**.
- `hermes sessions export` is not used at runtime; it is the cross-check in verification.
- `list` shows sessions with source `cli`; `oneshot`, `tool`, `kanban` and gateway sources sit behind a flag.
  A compression child (`parent_session_id`) is the live continuation, so the tip is the listed session. A
  named profile is a separate installation that `scan` finds.
- `--compaction summary` is refused for Hermes in the first phase. Tool names are collected from fixtures
  into `docs/tool-conversion-tables.md` later.
- Open points it records: whether the reader is built against a hand-made schema-31 database or waits for a
  real session; Hermes's tool names and full column list; reading when the directory is read-only or the
  gateway holds the WAL (`mode=ro` against `immutable=1`) [32].

**What Shinatama expects.** Shinatama has hosted Hermes since 2026-10-31, but until a reviewed pin
carries a Hermes reader its adapter declares `list_sessions`, `read_transcript` and `export` unsupported:
Hermes resumes natively within itself and receives seeds, nothing more [R 25]. The reader must be read-only,
take no lock, ignore a torn last record and count unknown records; fixtures come first (planned check HM3)
[R 25][26]. Its handover notes a possible writer after the first version, built on the fact that Hermes has
its own `sessions import --from claude|codex` [R 25]; ADR-0002, if accepted, would keep Hermes source-only
instead [27].

**Open decisions and disagreements.** None is settled by any source read; each says what would settle it.

| question | what the sources say | what would settle it |
|---|---|---|
| store schema version | 31 [27][32]; 32 [R 24] | read the version from a `state.db` made by v0.22.5, or `SCHEMA_SQL` at the tag |
| read route | `state.db` directly [27][32]; Hermes's export as primary, `state.db` as fallback [R 24]; the handover calls the export route the cheaper one [R 25] | a fixture run, planned check HM3 [26] |
| compaction shape | rows kept with `active=0, compacted=1` under the same id [R 24]; old rows archived and also a possible child session [32] | a v0.22.5 `state.db` after a compaction |
| does a session import command exist | `hermes sessions import --from claude\|codex` [R 23]; no command imports a session [27][32] | `hermes sessions import --help` on v0.22.5 |
| where a summary lives | marked as a compaction [R 24]; no stored summary row documented [32] | the same `state.db` |

### How mukade summarizes it

Not implemented. With `--compaction summary`, mukade asks the source's own model for a summary through a
non-interactive run that saves nothing; for Hermes the design draft refuses it in the first phase [32], and
ADR-0002 does not address it [27]. Shinatama's candidate for an answer-only run is
`hermes chat --oneshot -q <prompt> --format stream-json --ignore-user-config --ignore-rules`, but no toolset
that removes the tools and the write access is established, and whether a one-shot leaves a session is
**[U]** (§5, Ephemeral runs) [R 23].

## 9. Sources

All web pages and raw files accessed 2026-11-01 unless a source says otherwise. Sources 1 to 10 are the
Hermes README, the GitHub release record and the Hermes docs site (`<docs>` =
`https://hermes-agent.nousresearch.com/docs`). Sources 11 to 22 are Hermes files at tag `v0.22.5`
(`<raw>` = `https://raw.githubusercontent.com/NousResearch/hermes-agent/v0.22.5`), each read through a web
fetch tool's page extraction. Sources 23 to 25 and 29 are documents of the Shinatama repository
(`D:\Projects\shinatama`), written by research tasks that ran nothing. Repository paths are relative to
`D:\Projects\mukade` unless a source says otherwise.

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
22. `<raw>/tools/<file>.py` for each tool of the §5 tables: `file_tools`, `browser_tool`, `terminal_tool`, `process_registry`, `vision_tools`, `code_execution_tool`, `image_generation_tool`, `skills_tool`, `skill_manager_tool`, `memory_tool`, `session_search_tool`, `delegate_tool`, `cronjob_tools`, `todo_tool`, `clarify_tool`, `tts_tool`, `computer_use_tool`, `x_search_tool`, `kanban_tools`.
23. Shinatama technical design §5.7, research task: `D:\Projects\shinatama\docs\technical-design.md`, the "Hermes Agent" entry (lines 1030-1094) and the Hermes lines of §5.6 (844-847) and §5.7 (1167-1203). Facts from the research of 2026-10-31 on the release, its source and its docs; nothing run there; Shinatama's label is R.
24. Shinatama technical design §5.3, research task: `D:\Projects\shinatama\docs\technical-design.md` lines 487-505, the Hermes store entry: "R: v0.22.5 source, 2026-10-31; not observed here, Hermes is not installed on this machine".
25. Shinatama mukade handover §9, "Hermes Agent and DeepSeek Harness (requested 2026-10-31)": `D:\Projects\shinatama\docs\mukade-handover.md` lines 272-294.
26. Shinatama verification register, rows HM1 to HM14: `D:\Projects\shinatama\docs\verification-register.md` lines 218-231. They are checks still to be run, not results.
27. mukade, [`adr/ADR-0002-hermes-and-deepseek-harness-read-first.md`](../adr/ADR-0002-hermes-and-deepseek-harness-read-first.md): status **Proposed**, version 1, proposed 2026-10-31, `accepted_on` empty.
28. mukade, [`reference/harness-session-persistence-reference.md`](../reference/harness-session-persistence-reference.md), entry "Hermes Agent — `v0.22.5`" (lines 539-559). A secondary research report pasted by the user on 2026-10-27; its own header says its claims were not re-run here.
29. Shinatama research report "Hermes Agent" of the web-tools run, 2026-11-01: `D:\Projects\shinatama\.daodan\runs\init-467ba8d96c1b4104b3ac1edb707caffa\web-tools\hermes-agent.md` (the 86-row tool list, web search and page reading, headless and ACP events, toolset switches, verdict). Its `[S]` and `[D]` references are sources 11 to 22 and 3 to 10 above.
30. Search of the mukade tree on 2026-11-01: `grep -rli hermes docs src catalog tests fixtures README.md Cargo.toml` and `find . -iname "*hermes*"`; reading of `src/model/harness.rs` (lines 5-50) and of `Cargo.toml` (line 22). [V]
31. `Get-Command hermes -All` in PowerShell on this machine (Windows 11), 2026-11-01: not recognised as a command. [V]
32. mukade, [`superpowers/specs/2026-10-31-hermes-deepseek-harness-support-design.md`](../superpowers/specs/2026-10-31-hermes-deepseek-harness-support-design.md) §4.1, §5.3, §5.6, §6, §8: a design draft awaiting approval, which reads Hermes's session-storage and sessions documentation; what it marks **[U]** was not exercised against a live install.
