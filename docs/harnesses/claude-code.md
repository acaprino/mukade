# Claude Code

| | |
|---|---|
| **Documentation updated** | 2026-11-01 |
| **Version described** | 2.1.311 (the version mukade's readers and writers were verified against) [1][24] |
| **Vendor** | Anthropic PBC [25] |
| **Website** | https://code.claude.com (documentation); https://github.com/anthropics/claude-code (releases, issues) [20][24] |
| **License / source** | Proprietary: "© Anthropic PBC. All rights reserved. Use is subject to Anthropic's Commercial Terms of Service." [25] |
| **mukade name** | `claude_code` |

Version note: the binary installed on the verification machine now reports `2.1.316 (Claude Code)` and npm's `latest` is `2.1.316` [35][36]. On-disk facts and mukade behaviour below were verified on 2.1.311 [1]; the entry points were checked against the 2.1.312 `--help` [9], and only the tool and restriction flags added to section 3 were read from the 2.1.316 `--help` [35]. Also read on 2.1.316: text found in the installed binary about how `WebSearch` and `WebFetch` run, the shape of the search `results` object and some tool names [37]; it is internal code, not a documented contract. These are the only facts checked on 2.1.316, and none of them is mukade behaviour verified there. Every other fact added on 2026-11-01 comes from the web documentation, was not run, and is subject to the rule that follows: the web documentation describes the current release (the pages read name versions up to v2.1.310 [29]; which release they track is **[U]**), so a feature it names may be newer than 2.1.311 **[U]** per feature.

## 1. Introduction

Claude Code is Anthropic's agentic coding tool: it reads a codebase, edits files, runs commands and talks to development tools through MCP [20]. It is aimed at developers working in a terminal, in an IDE, in the Claude Desktop app, on the web, or in CI [20]. Its sessions are local JSONL transcripts, which is what mukade reads and writes [1].

## 2. Description

- **Agent loop.** The model plans, calls tools (read, edit, shell, search, web, subagents), observes the results and repeats until the task is done; the same loop, tools and context management are exposed as a library by the Agent SDK [21].
- **Model and wire protocol.** It talks to Claude models over the Anthropic Messages protocol; mukade treats Claude Code as an `anthropic-messages` destination [7]. Providers: the Anthropic API, Amazon Bedrock, Google Cloud's Agent Platform, Microsoft Foundry, Claude Platform on AWS, or any gateway reached through `ANTHROPIC_BASE_URL` [16][22][23] (see section 5, Models and providers).
- **State.** Every conversation is appended to a transcript under `<config dir>/projects/`; checkpoints of edited files sit beside it so `/rewind` can restore code [18][19].
- **Extensibility.** Skills, slash commands, subagents, hooks, MCP servers and plugins, all configured in files under `.claude/` and `~/.claude/` [18].
- **Permissions.** Tools ask first unless a permission rule or mode allows them; modes are `acceptEdits`, `auto`, `bypassPermissions`, `manual`, `dontAsk` and `plan` [9].

## 3. Entry points

| Use | Exact command |
|---|---|
| Interactive session in the current directory | `claude` |
| Interactive session with a first prompt | `claude "<prompt>"` [9] |
| Non-interactive (print) | `claude -p "<prompt>"`; `--output-format text\|json\|stream-json`, `--input-format text\|stream-json` [9] |
| Continue the most recent conversation of this directory | `claude -c` / `claude --continue` [9] |
| Resume by id or name, or pick interactively | `claude -r [value]` / `claude --resume [value]` [9] |
| Resume non-interactively with a prompt (what mukade runs) | `claude -p --resume <id> "<prompt>"`: same id, appends to the same file, works from any cwd [1] |
| Compact a session non-interactively | `claude -p --resume <id> "/compact"` [1] |
| Resume under a new id | add `--fork-session` to `--resume` or `--continue` [9] |
| Fix the session id / name it | `--session-id <uuid>`, `-n, --name <name>` [9] |
| No transcript written (print mode only) | `--no-session-persistence` [9] |
| Choose the tools one run is offered | `--tools "<list>"` takes names from the built-in set (`--tools ""` offers none); `--allowedTools "<rules>"` allows the named tools or rules, `--disallowedTools "<rules>"` denies them (a bare tool name removes the tool from the model's context). `--tools` and `--allowedTools` take several values and swallow a prompt that follows them [35][31][32] |
| Restrict one run | `--restricted` removes the code-running tools and `WebFetch` unless `--tools` names them, ignores the user, project and local settings files (`--settings` still applies), confines file tools to the working directories and refuses `bypassPermissions`; `--strict-mcp-config` with no `--mcp-config` loads no MCP server; `--safe-mode` disables every customization (section 6) [35][9]. claude.ai connectors and plugins are separate from `.mcp.json` servers [39]; that `--strict-mcp-config` or `--disable-slash-commands` excludes them is **[U]**, and empty `mcp_servers` and `plugins` in the `system/init` line of a run would settle it |
| Permission prompts in print mode | `--permission-prompts host\|none`; `none` needs v2.1.259+ [31]. What `host` does was not read **[U]**; the `--help` text of the flag would settle it |
| Hook events in the stream | `--include-hook-events` is listed by `--help` [35]; what it adds to the stream was not read **[U]**; a `-p --output-format stream-json` run with a `PreToolUse` hook would show it |
| Resume a session linked to a pull request | `--from-pr [value]` [9] |
| Background sessions | `claude --bg`, then `claude agents`, `claude attach <id\|name>`, `claude logs <id\|name>`, `claude stop <id>`, `claude rm <id>`, `claude respawn [id]` [9] |
| Isolated git worktree | `claude -w [name]` / `--worktree [name]` [9] |
| Desktop app, IDE, browser | `--desktop`, `--ide`, `--chrome`; `--cloud`, `--teleport [session]`, `--remote-control [name]` [9] |
| Management subcommands | `auth`, `auto-mode`, `doctor`, `gateway`, `import`, `install`, `mcp`, `plugin`, `purge`, `setup-token`, `ultrareview`, `update` [9] |

- **IDE surfaces.** VS Code extension `anthropic.claude-code`; JetBrains plugin from the JetBrains Marketplace (it needs the CLI installed separately); the Desktop app; the web; Slack; GitHub Actions and GitLab CI/CD [20].
- **SDK.** `npm install @anthropic-ai/claude-agent-sdk` (TypeScript), `pip install claude-agent-sdk` or `uv add claude-agent-sdk` (Python). Both bundle a native Claude Code binary [22]. Other languages run `claude -p --output-format json` as a subprocess [21].
- Without a prompt, mukade only prints the resume command; with `--resume-prompt` it runs `claude -p --resume <id>` and that makes a model call [8].

## 4. Harness type

### Vendor

Anthropic PBC [25]. Coding agent with a terminal CLI, IDE extensions, a desktop app and a hosted web surface [20].

### Website and documentation

- Documentation: https://code.claude.com/docs/en/overview (index of all pages: https://code.claude.com/docs/llms.txt) [20]
- Repository (releases, issues, plugins marketplace sources): https://github.com/anthropics/claude-code [24][26]
- npm package: `@anthropic-ai/claude-code` [26]

### Installation

Requirements: macOS 13.0+, Windows 10 1809+ or Windows Server 2019+, Ubuntu 20.04+, Debian 10+ or Alpine Linux 3.19+; 4 GB+ RAM; x64 or ARM64; internet access [10].

| Method | Command |
|---|---|
| Native, macOS / Linux / WSL (recommended) | `curl -fsSL https://claude.ai/install.sh \| bash` [10] |
| Native, Windows PowerShell | `irm https://claude.ai/install.ps1 \| iex` [10] |
| Native, Windows CMD | `curl -fsSL https://claude.ai/install.cmd -o install.cmd && install.cmd && del install.cmd` [10] |
| Native, stable channel | `curl -fsSL https://claude.ai/install.sh \| bash -s stable` (PowerShell and CMD variants on the same page) [10] |
| Native, from an installed copy | `claude install [target]` where target is `stable`, `latest` or a version [9] |
| Homebrew (macOS / Linux) | `brew install --cask claude-code` (stable) or `claude-code@latest` [10] |
| WinGet (Windows) | `winget install Anthropic.ClaudeCode` [10] |
| apt / dnf / apk | signed repositories; `sudo apt install claude-code`, `sudo dnf install claude-code`, `apk add claude-code` after adding the repository and key as the setup page describes [10] |
| npm (Node.js 22+) | `npm install -g @anthropic-ai/claude-code`; installs the same native binary through a per-platform optional dependency [10][26] |

- Update: native installs update themselves in the background; `claude update` (alias `claude upgrade`) forces a check; Homebrew, WinGet, apt, dnf and apk do not self-update (`brew upgrade claude-code`, `winget upgrade Anthropic.ClaudeCode`); npm: `npm install -g @anthropic-ai/claude-code@latest` [9][10]. `DISABLE_AUTOUPDATER=1` stops the background check [16].
- Launcher location: native installs put it at `~/.local/bin/claude` (a symlink into `~/.local/share/claude/versions/` on macOS and Linux) [10]; on this machine `C:\Users\alfio\.local\bin\claude.exe` [1][6].

### Canonical locations and finding an existing install

| what | Windows | macOS / Linux |
|---|---|---|
| Native-install launcher | `%USERPROFILE%\.local\bin\claude.exe` | `~/.local/bin/claude` |
| Native-install versions | `%USERPROFILE%\.local\share\claude\versions\` | `~/.local/share/claude/versions/` |
| Default session/config store | `%USERPROFILE%\.claude` | `~/.claude` |

The macOS/Linux launcher is a symlink into the versions directory; on Windows the same native layout
is used under `%USERPROFILE%` [28]. The store is `~/.claude` on every OS unless `CLAUDE_CONFIG_DIR`
moves it [16][18]. Homebrew, apt/dnf/apk, WinGet and npm installs are found through `PATH` and their
package manager, not by assuming the native path: use `command -v claude` (macOS/Linux),
`Get-Command claude -All` (PowerShell), `brew --prefix` or `npm prefix -g`, then `claude --version`.
`claude doctor` also reports installation health without starting a session [28].

`mukade harnesses` reports the one executable it resolves, not every copy on disk. See the
[harness catalog](../reference/harness-catalog.md#finding-existing-installations) for the shared
per-OS discovery procedure and for selecting a non-default installation in mukade.

- Windows: Git for Windows is optional; with it the Bash tool uses Git Bash, without it Claude Code uses the PowerShell tool [10].
- Alpine / musl: needs `bash`, `curl`, `libgcc`, `libstdc++`, `ripgrep` and `USE_BUILTIN_RIPGREP=0` [10].
- Uninstall: see the setup page [10] (not reproduced here **[U]**).

### Licence

Proprietary; use is subject to Anthropic's Commercial Terms of Service [25]. The npm `license` field reads `SEE LICENSE IN README.md` [26]. The Agent SDK is governed by the same Commercial Terms unless a component's own LICENSE says otherwise [21]. Whether the repository publishes the CLI's source was not verified **[U]**; the repository is used here only for releases and issues.

## 5. Features

### Session store

- **Location.** `<CLAUDE_CONFIG_DIR>/projects/<project>/<session-id>.jsonl`; the default config directory is `~/.claude`, and `CLAUDE_CONFIG_DIR` overrides it ("all settings, session history, and plugins are stored under this path") [16][18]. On Windows this is `%USERPROFILE%\.claude` (an OS-path inference, not an Anthropic contract [2]).
- **Project folder name.** Claude Code 2.1.311 derives it from the working directory (`wL`): every UTF-16 unit outside `[a-zA-Z0-9]` becomes `-` (`C:\a\b` becomes `C--a-b`); a name over 200 characters is cut at 200 and suffixed with `-` and the base-36 hash of the path [6].
- **Format.** JSON Lines, one record per line. Anthropic publishes no versioned schema for it; treat it as an internal format and keep unknown records verbatim [2]. No index file is needed: a session file written outside Claude Code resumes by id [1].
- **Record kinds seen in 2.1.311** [1][4]:
  - `user` rows (prompts; tool results travel in `user` rows as `tool_result` blocks; slash-command rows `<command-name>`, `<local-command-stdout>`, `<local-command-caveat>` with `isMeta`);
  - `assistant` rows: **one row per content block**, all sharing `message.id` and chained by `parentUuid`; blocks are `thinking` (with `signature`), `redacted_thinking` (with `data`), `text`, `tool_use`;
  - `system` rows, notably `subtype: "compact_boundary"`;
  - `attachment` rows (files re-injected after a compaction), request/cost/telemetry rows (`api-request` among them), and sidechain rows (`isSidechain: true`) for subagents.
  Common fields: `uuid`, `parentUuid`, `logicalParentUuid`, `sessionId`, `cwd`, `gitBranch`, `version`, `slug`, `timestamp`.
- **Tool results.** Parallel calls: the `tool_use` rows are chained, and each `tool_result` row's `parentUuid` is its own `tool_use` row, in completion order; pairing is by `tool_use_id`, not by walking parents [1].
- **Interrupted turn.** A killed process leaves the user row; on the next load Claude Code writes a synthetic assistant `"No response requested."` with `model: "<synthetic>"` [1].
- **Beside the transcript.** `<session>/subagents/` (subagent transcripts, same format), `<session>/tool-results/` (large outputs), `file-history/<session>/` (pre-edit snapshots for `/rewind`, the 100 most recent checkpoints), `history.jsonl` (typed prompts), `~/.claude.json` (account, personal MCP servers, per-project state) [17][18].
- **Retention.** Files older than `cleanupPeriodDays` are deleted by a sweep: default 30, minimum 1 [18]. Transcripts are not encrypted; a credential a tool printed is in the file [18].

### Compaction

- **Manual and automatic.** `/compact` (also `claude -p --resume <id> "/compact"`); automatic when the conversation approaches the context limit [1][23]. Controls: `DISABLE_AUTO_COMPACT=1` (automatic off, `/compact` stays), `DISABLE_COMPACT=1` (both off), `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE` (1-100, can only lower the threshold), `--autocompact <auto|tokens>` (100k to 1M), `autoCompactWindow` [9][16][23]. Since v2.1.311 `DISABLE_COMPACT` in settings files can only turn compaction off, not back on [24]. The default threshold of 2.1.311 was not read **[U]**.
- **Targeted compaction.** `/rewind` offers "Summarize from here" and "Summarize up to here", which compress part of the conversation; the original messages stay in the transcript [19].
- **Record on disk** [1]: `system` row `subtype: "compact_boundary"` with `parentUuid: null`, `logicalParentUuid: <old leaf>` and `compactMetadata{trigger: "manual"|"auto", preTokens, postTokens, preservedSegment{headUuid, anchorUuid, tailUuid}, preservedMessages}`; then a `user` row with `isCompactSummary: true`, `isVisibleInTranscriptOnly: true` holding "This session is being continued from a previous conversation … Summary: …"; then the command rows of `/compact` and `attachment` rows for files read earlier. The last assistant reply stays in place and is hung under the summary by `preservedSegment`.
- **Resume.** Before a compaction the model receives everything, thinking with its signature; after it, only the summary, the preserved tail and later turns. Earlier tools and thinking are not sent; files read earlier return as a system message [1].
- **Without a terminal**, `claude -p --resume <id> "/compact"` compacts the session it resumes: the rows land in that session's own transcript [1]. Compacting without touching it means forking first (`--fork-session`) **[U]**.

### Ephemeral runs

- **No transcript.** `--no-session-persistence` (print mode only) writes no session file [9]. With
  `--resume <id>` the resumed transcript stays byte-identical and no file is added to the project
  folder [27]. Hooks still run: the user's SessionStart hooks rewrote `session-env/<id>/` for the
  resumed id [27].
- **Under a new id.** `--fork-session` with `--resume` or `--continue` writes the continuation to a
  new file and leaves the original as it was [9][19].
- **No tools.** `--tools ""` offers none; `EndConversation` is the exception no `--tools` list removes, but `-p` runs do not offer it [29]. The option takes several values, so it must come before
  another option: after `--resume` it swallows the prompt and Claude Code answers `Error: No
  deferred tool marker found in the resumed session …` [27]. A history holding tool calls is
  accepted with no tools offered (first-party API, `claude-sonnet-5`) [27].
- **`--bare` is not for subscription logins**: it skips hooks, but authenticates only with
  `ANTHROPIC_API_KEY` or an `apiKeyHelper`, never OAuth [9].
- **Output.** `--output-format json` prints one object: `result`, `is_error`, `subtype`
  (`success`), `session_id`, `total_cost_usd`, `usage`, `modelUsage` [27]. With stdin left open,
  print mode waits 3 s for it (`Warning: no stdin data received in 3s`) [27].

### Built-in tools

Tool names are the exact strings used in permission rules, subagent tool lists and hook matchers [12]. Documented set, grouped:

| Group | Tools |
|---|---|
| Files | `Read`, `Write`, `Edit`, `NotebookEdit` (Jupyter cells) |
| Shell | `Bash`; `PowerShell` (default on native Windows without Git Bash; opt-in elsewhere through `CLAUDE_CODE_USE_POWERSHELL_TOOL=1`); `Monitor` (background command streamed line by line) |
| Search | `Glob`, `Grep` (default on Windows; on macOS, Linux and WSL the shell runs embedded `bfs` and `ugrep` instead, and the two tools are absent unless `--tools` or `--allowedTools` names them (naming either in `--allowedTools` restores both; an allow rule in a settings file does not) or Bash is removed [29]), `LSP` (code intelligence through language servers), `ToolSearch` (loads deferred tools) |
| Web | `WebFetch`, `WebSearch` |
| Agents | `Agent` (subagent), `SendMessage`, `ListAgents`, `TaskStop`, `Workflow` (script that orchestrates subagents), `Skill` (runs a skill) |
| Tasks and scheduling | `TaskCreate`, `TaskGet`, `TaskList`, `TaskUpdate` (replace `TodoWrite`, which is off by default), `CronCreate`, `CronDelete`, `CronList`, `ScheduleWakeup`, `RemoteTrigger` |
| Planning and worktrees | `EnterPlanMode`, `ExitPlanMode`, `EnterWorktree`, `ExitWorktree`, `AskUserQuestion` |
| MCP | `ListMcpResourcesTool`, `ReadMcpResourceTool`, `WaitForMcpServers`; tools of MCP servers are `mcp__<server>__<tool>` [3][12] |
| Other | `Artifact`, `ReportFindings`, `PushNotification`, `SendFeedback`, `SendUserFile`, `ShareOnboardingGuide`, `SubagentHandback` (auto mode), `EndConversation` |

`TaskOutput` was removed in v2.1.277 [12]. The table omits tools that the SDK reference names: `ReadMcpResourceDir`, `Projects`, `RefreshMcpTools` (needs `CLAUDE_CODE_ENABLE_REFRESH_MCP_TOOLS=1`), `ShowOnboardingRolePicker` and `StructuredOutput`; the same page marks `REPL` as removed [32]. The 2.1.316 binary also holds the name strings `ArtifactComments`, `ArtifactData`, `ArtifactCheck`, `TeamCreate` and `TeamDelete`; a name found there does not show that the tool is offered, which the `tools` list of a `system/init` line would [37]. The set the model is offered depends on version, platform, model and settings. In the mukade bench (2.1.311, default settings, no plugins) 23 tools were offered: `Agent`, `Bash`, `CronCreate`, `CronDelete`, `CronList`, `Edit`, `EnterWorktree`, `ExitWorktree`, `Glob`, `Grep`, `ListAgents`, `Monitor`, `NotebookEdit`, `Read`, `ReportFindings`, `ScheduleWakeup`, `SendMessage`, `Skill`, `TaskStop`, `WebFetch`, `WebSearch`, `Workflow`, `Write` [3]. For how each call maps to Codex, OpenCode and Pi, see [../tool-conversion-tables.md](../tool-conversion-tables.md); the table is not repeated here.

#### Web tools

- **`WebSearch` runs at the provider.** Input `{query, allowed_domains?, blocked_domains?}`, the two lists not combinable in one call [29][32]. It "runs a query against Anthropic's web search backend and returns result titles and URLs" without fetching the pages, and is not exposed on Amazon Bedrock [29]. In the 2.1.316 binary a call makes a nested Messages request with the `web_search_20250305` server tool (`max_uses: 8`): a second model call, and up to eight backend searches per call [37]. Whether every login type (a subscription login) may run it is not stated **[U]**; a `-p` run on such a login would settle it, and the mukade bench (2.1.311) was offered the tool [3].
- **`WebFetch` runs locally.** Input `{url, prompt, offset?}` [32]. It fetches the page, converts HTML to Markdown and "runs the prompt against the content in a separate model call"; the model receives that result, not the raw page [29]. The hostname goes to `api.anthropic.com` for a blocklist check first, which `skipWebFetchPreflight: true` skips [34]. It refuses `localhost` and hosts without a dot, and does not follow a redirect to another host: the result begins `REDIRECT DETECTED: The URL redirects to a location that was not fetched automatically` and the model must call again [29][37].
- **Stream, search.** An `assistant` line holds a `tool_use` block `name:"WebSearch"` with `input.query`, `input.allowed_domains`, `input.blocked_domains` [32][33]. The answer is a `user` line with a `tool_result` block and a top-level `tool_use_result`: `query`; `results[]`, one `{tool_use_id, content:[{title, url}]}` per backend search or a plain string (the nested model's summary); `durationSeconds`; `searchCount`; an error is the string `Web search error: <error_code>` [32][37]. The refined queries are counted by `searchCount`, not listed, and the nested `server_tool_use` / `web_search_tool_result` blocks do not reach the outer stream [37]; `encrypted_content` and `page_age` of the API result are not in the output type [32]. Whether the binary's progress records (`query_update`, `search_results_received`) reach stream-json or a transcript is **[U]**; `grep -c query_update` on a saved run would settle it.
- **Stream, fetch.** `tool_use` `name:"WebFetch"` with `input.url`, `input.prompt`, `input.offset?` [32][33]; the `tool_use_result` has `url`, `code`, `codeText`, `bytes`, `durationMs` and `result` (the extracted text); a background call returns `{detachedToolCall: true}` (v2.1.287+) [32]. In the binary `url` is assigned from the request variable, not from a redirect target; where that variable comes from was not traced **[U]** [37].
- **What is established about these lines.** The field lists are the SDK's message types [32]; a refused call also arrives as a `system/permission_denied` line (`tool_name`, `tool_use_id`, `decision_reason`) and in the final `result`'s `permission_denials` [31][32]. That the CLI's stdout lines carry exactly these objects is consistent with the headless page but stated in no single sentence **[U]**, and so is whether `--verbose` is required with `--output-format stream-json` in `-p` (the page's example uses it) [31]. One such run with `WebSearch` allowed, with and without `--verbose`, would settle both.
- **Allow or deny `WebSearch` for one run.** Allow: `--allowedTools "WebSearch"`; the rule has no specifier form [29][31]. Deny: `--disallowedTools "WebSearch"` removes it from the model's context [32], or leave it out of `--tools` [35]. In `-p` a call that needs approval and is not allowed is denied [31]. No environment variable that disables search was found **[U]** (`CLAUDE_CODE_DISABLE_WEB_FETCH=1` exists for `WebFetch`); a search of the installed binary's variable names would settle it. The per-turn limit resets on each `claude -p` run and is raised with `CLAUDE_CODE_MAX_WEB_SEARCHES_PER_SESSION`; its `-p` default, and whether it can be lowered, are **[U]** [29].
- **Allow or deny `WebFetch` for one run.** It asks in Manual mode except for built-in preapproved documentation domains [29]. Allow: `--allowedTools "WebFetch(domain:docs.example.com)"`. Deny: a bare `WebFetch` in `--disallowedTools` removes the tool, denying `WebFetch(domain:*)` keeps it and refuses every fetch, `CLAUDE_CODE_DISABLE_WEB_FETCH=1` (v2.1.285+) turns it off, and `--restricted` removes it unless `--tools` names it [29][30][35].
- **`WebFetch` is restricted by host only.** `WebFetch(domain:example.com)` matches that host only, `WebFetch(domain:*.example.com)` its subdomains at any depth but not the apex, `WebFetch(domain:*)` every host; matching is case-insensitive, deny beats ask beats allow, an explicit rule beats the preapproved set, and elsewhere `*` matches one label [30][29]. A rule on the `url` field or on a path is not supported [30]; an exact-URL allowlist needs a `PreToolUse` hook (section 7).
- **Read tools.** `Read`, `Glob` and `Grep` are read-only (a read of a network path asks first). Switch for one run: name them in `--tools`; `Read(<path>)` rules scope or deny paths for all three; `--disallowedTools "Read"` or `"Glob Grep"` removes them; the `Glob`/`Grep` platform rule is in the table above [29][30][32].
- **No web sample in the repository.** The one Claude fixture that mentions `WebSearch` lists the tool among its tool schemas and holds no web `tool_use` or `tool_result` block; its `usage.server_tool_use` counters read `web_search_requests: 0` and `web_fetch_requests: 0` [38]. The field lists above therefore come from the SDK reference and the binary, not from a mukade fixture.

### Branches, rewind and subagents

- **Checkpoints and rewind.** Each prompt that starts a turn creates a checkpoint of the files Claude edits; `/rewind` (or `Esc` twice on an empty prompt) restores code, conversation or both. Changes made only by Bash and edits by subagents are not tracked. Checkpoints are saved with the conversation, so `/rewind` works after a resume [19].
- **Rewound branches stay in the file.** A rewind or an edited prompt starts a new branch from an earlier row and leaves the abandoned rows in the transcript; Claude Code resumes the branch of the last row [4]. Release 2.1.311 fixed transcript ordering and "taken back" messages [24].
- **Branching a session.** `/branch`, or `claude --continue --fork-session`, keeps the original intact [19]; `--fork-session` gives a new id and writes **no lineage field** [1].
- **Subagents.** The `Agent` tool spawns a subagent with its own context. Subagent transcripts live in `<session>/subagents/` in the same format [18]; in the main file they appear as `isSidechain: true` rows [4]. Definitions: `agents/*.md` (project or global) or `--agents <json-or-file>` [9][18]. Background sessions: `claude --bg` [9].
- **Not exercised by the mukade bench:** auto-compaction by threshold, a second compaction, a rewind or fork in a real source, subagent sessions **[U]** [1].

### Models and providers

- **Models.** Aliases `default`, `best`, `fable`, `sonnet`, `opus`, `haiku`, `sonnet[1m]`, `opus[1m]`, `opusplan`, or a full model name. Precedence: `/model` in a session, `--model`, `ANTHROPIC_MODEL`, the `model` setting, `ANTHROPIC_DEFAULT_MODEL` [23]. Also `--effort low|medium|high|xhigh|max` and `--fallback-model <model>` [9]. Which models are available to 2.1.311 specifically was not verified **[U]**.
- **Providers.** Anthropic API (`ANTHROPIC_API_KEY`, or a Claude subscription login), Amazon Bedrock (`CLAUDE_CODE_USE_BEDROCK`), Google Cloud's Agent Platform (`CLAUDE_CODE_USE_VERTEX`), Microsoft Foundry (`CLAUDE_CODE_USE_FOUNDRY`), Claude Platform on AWS (`CLAUDE_CODE_USE_ANTHROPIC_AWS=1`) [16][22][23]. `ANTHROPIC_BASE_URL` routes requests through a proxy or gateway; it changes where requests go, not which model answers [16][23].
- **Authentication.** `claude auth login|logout|status`, `claude setup-token` [9].

## 6. Plugins and extensions

- **Plugin layout.** `.claude-plugin/plugin.json` (only `name` is required) plus components at the plugin root: `skills/<name>/SKILL.md`, `commands/` (flat Markdown files; skills are preferred for new plugins), `agents/`, `hooks/hooks.json`, `.mcp.json`, `.lsp.json`, `output-styles/`, `monitors/monitors.json`, `bin/` (on the Bash tool's `PATH` while the plugin is enabled) [13].
- **Install.** In a session `/plugin install <plugin>@<marketplace>` (opens the `/plugin` panel); from a shell `claude plugin install <plugin>[@<marketplace>] [--scope project|local]` (user scope by default). Other verbs: `list`, `enable`, `disable`, `uninstall`, `update`, `validate`, `details`, `marketplace` [9][14].
- **Marketplaces.** `/plugin marketplace add <owner/repo | git URL | path | https URL to marketplace.json>`, or `claude plugin marketplace add <source>`. Anthropic's official marketplace is `claude-plugins-official`; interactive sessions register it on first start [14].
- **Scope and record.** User scope writes `enabledPlugins` in `~/.claude/settings.json`; project scope in `.claude/settings.json`; local scope in `.claude/settings.local.json` [14]. Plugin files, cloned marketplaces and the `installed_plugins.json` install record are under `~/.claude/plugins` [18]. For a single session: `--plugin-dir <path>` or `--plugin-url <url>` [9].
- **MCP servers.** `claude mcp add --transport http <name> <url>`; stdio: `claude mcp add [options] <name> -- <command> [args...]`; SSE is deprecated; WebSocket only through `claude mcp add-json`. Scopes: local (default) and user are stored in `~/.claude.json`, project in `.mcp.json` at the project root [15]. Also `claude mcp list|get|remove|login|logout|serve` [9]. Plugin-bundled servers are named `mcp__plugin_<plugin>_<server>__<tool>` [15].
- **Skills, commands, agents.** Markdown files with YAML frontmatter in `skills/`, `commands/` and `agents/` under a project's `.claude/` or under `~/.claude/` [18]; a skill runs through the `Skill` tool and does not add a tool entry [12]. `--disable-slash-commands` turns skills and commands off; `--safe-mode` disables every customization [9].
- **Importing configuration.** `claude import [codex|gemini|cursor]` imports configuration from another agent (`--dry-run` previews) [9]. It imports configuration, not sessions; mukade does the sessions.

## 7. Hooks

- **Configured in** `~/.claude/settings.json`, `.claude/settings.json`, `.claude/settings.local.json`, managed policy settings, a plugin's `hooks/hooks.json`, skill frontmatter (while the skill is active) and subagent frontmatter (a `Stop` hook there becomes `SubagentStop`). Entries merge across levels [11]. Structure: event, then matcher group (`matcher`), then a `hooks` array of handlers [11].
- **Events (33)** [11]:

| Area | Events |
|---|---|
| Session | `SessionStart`, `Setup`, `SessionEnd`, `InstructionsLoaded`, `ConfigChange`, `CwdChanged`, `DirectoryAdded`, `FileChanged` |
| Prompt | `UserPromptSubmit`, `UserPromptExpansion` |
| Tools and permissions | `PreToolUse`, `PermissionRequest`, `PermissionDenied`, `PostToolUse`, `PostToolUseFailure`, `PostToolBatch` |
| Agents and tasks | `SubagentStart`, `SubagentStop`, `TaskCreated`, `TaskCompleted`, `TeammateIdle` |
| Turn and UI | `Stop`, `StopFailure`, `Notification`, `MessageDisplay` |
| Worktrees | `WorktreeCreate`, `WorktreeRemove` |
| Compaction and model | `PreCompact`, `PostCompact`, `PreModelSwitch`, `PostModelSwitch` |
| MCP | `Elicitation`, `ElicitationResult` |

- **Handler types:** `command` (shell command), `http` (POSTs the JSON), `mcp_tool`, `prompt` (single-turn model check) and `agent` (subagent; experimental). Common fields: `type`, `if`, `timeout`, `statusMessage`, `once` [11].
- **Input.** Command hooks get the event JSON on stdin, HTTP hooks as the POST body. Common fields: `session_id`, `prompt_id`, `transcript_path`, `cwd`, `scratchpad_dir`, `permission_mode`, `effort`, `hook_event_name`; tool events add `tool_name`, `tool_input`, `tool_use_id` [11]. Per-event fields beyond these were not read **[U]** (the page was fetched only in part).
- **Exit codes.** `0` success (JSON on stdout is applied); `2` blocking error on events that can block, stderr is fed back (ignored on `PermissionRequest`); any other code is a non-blocking error; `onFailure: "block"` (v2.1.295+) makes a failure or timeout block [11].
- **JSON output.** `continue: false` stops Claude; `decision: "block"` with `reason` on prompt, post-tool, `Stop`, `PreCompact` and similar events; `hookSpecificOutput` carries `permissionDecision` (`allow`, `deny`, `ask`, `defer`) for `PreToolUse`, `additionalContext`, `updatedInput` and others [11].
- **Web tools in hooks.** A `PreToolUse` hook sees `tool_input.url` for `WebFetch` and can answer `permissionDecision: "deny"`; for `WebSearch` it sees `query`, `allowed_domains`, `blocked_domains` and an optional `mode` (`"standard"` or `"extended"`, v2.1.282+) [33]. Hooks run in `-p` runs [31]. Whether a hook runs under `--restricted` is **[U]** (a `-p --restricted` run with a hook that logs its stdin would settle it); the `PostToolUse` `tool_response` shape for `WebSearch` is **[U]** (the same logging hook on a search would show it).
- `--bare` and `--safe-mode` skip hooks [9]. mukade neither reads nor writes hook configuration.

## 8. How mukade reads and writes it

Sources: `src/reader/claude_code.rs`, `src/writer/claude_code.rs`, `src/writer/plan.rs`, `src/exe.rs`, `src/roots.rs`, `src/policy.rs`, `src/detect.rs`, `src/hop.rs`, `docs/usage.md`, spec §2.1 and §6.2 [1][4][5][6][7][8].

| Item | Value |
|---|---|
| CLI name | `claude_code` |
| Store variable | `CLAUDE_CONFIG_DIR` (sessions under `projects/`); required explicitly for read and write, never defaulted [6][8] |
| Executable flag / variable | `--claude-exe` / `MUKADE_CLAUDE_EXE` [6][8] |
| Executable resolution | flag, then `MUKADE_CLAUDE_EXE`, then `PATH`, then the native launcher `~/.local/bin/claude[.exe]` from [`catalog/harnesses.json`](../../catalog/harnesses.json), then `claude` [6] |
| Version stamp | `--dest-version`, else the first version printed by the resolved executable's `--version` [7] |
| Detection | a directory with at least one `projects/<project>/*.jsonl` is Claude Code [7] |
| Resume command written | `claude -p --resume <id>` (the resolved executable replaces `claude`) [5] |

**Reader (`src/reader/claude_code.rs`, `src/reader/claude_code/order.rs`)** [4]:
- `list` reads `projects/*/*.jsonl` and peeks at each file's **head and tail** — 128 KB then 384 KB, a file no bigger than the two together read whole — the way Claude Code's own picker reads a session [9]: `sessionId`, `cwd`, timestamp, and the title, which is the picker's (see §8 "How `/resume` lists it"): the last `custom-title` row, else the `lastPrompt` of the last `last-prompt` row, else the first user prompt, else the `slug` of a compact_boundary row, each folded to one line of at most 200 characters. In a file bigger than the two windows a `custom-title` written between them is missed and the name falls back to the last prompt or the first one. `read` matches the file name by prefix of the id and refuses an ambiguous prefix. Subagent folders are not listed. A session of bookkeeping rows only (`/help`, `/exit`, their `<local-command-*>` output) has no prompt to be named by and stays untitled, as it is dropped from Claude Code's own list.
- **Selected branch only.** `last-prompt.leafUuid` is the checkpoint; a later continuation or a new user branch can supersede it. A new branch only overrides a checkpoint whose leaf was already recorded before it; a checkpoint flushed before its own leaf is not a rewind ordering barrier. Mixed result/text user rows can start a new branch too. Without a checkpoint the last non-sidechain user or assistant with a UUID selects the branch. The reader walks `parentUuid`, and `logicalParentUuid` across compaction; rows outside the branch are kept as `Meta(OffBranch)` and counted as `dropped_off_branch`. This selection is covered by synthetic tests, not a new live rewind verification.
- **Rows merged by `message.id`** into one `Step`, including a response interleaved with its tool results; results are paired by `tool_use_id` (L5). Recovered siblings and UUID-less blocks/results are placed using their dependencies, with file position only as a tie-breaker. Unanchored conversation records, missing parents, cycles, duplicate UUIDs and ambiguous recovered results are refused.
- **Meta, never conversation:** `<synthetic>` assistant replies (`Meta(Synthetic)`), `isMeta` and `isVisibleInTranscriptOnly` rows and `<local-command-*>` / `<command-name>` / `<command-message>` rows (`Meta(Command)`; the `isMeta` row after a prompt-type slash command is the expanded prompt and becomes a user turn), `attachment` rows, other `system` rows, unknown row types, and subagent (`isSidechain`) rows. A last line cut off mid-write is kept as an unfinished record.
- **Compaction:** `compact_boundary` plus the `isCompactSummary` row become one `Compaction` entry (`summary`, `trigger`, `tokens_before`, `kept_from`). Preserved message lists and segments are checked; a retained UUID inside a merged response resolves to that response. A uniquely linked summary without a UUID can be recovered. Malformed, missing or contradictory preserved conversation records are refused.
- **Reasoning:** `thinking` keeps its `signature`; `redacted_thinking` keeps its `data` as the opaque payload; both are tagged `anthropic-messages` with the step's model.
- **Step end** from `stop_reason`: `tool_use`, `end_turn`/`stop_sequence`/`max_tokens`/`length` completed, `refusal` error. Session metadata (`sessionId`, `cwd`, `gitBranch`, `version`, models) comes from the rows; **title** is the picker's — the last `custom-title`, else the last `last-prompt` row's `lastPrompt`, else the first user prompt (never a sidechain's, a command's or a summary row), else a compact_boundary `slug`; `parent` is always empty because `--fork-session` leaves no lineage.

**Writer (`src/writer/claude_code.rs`)** [5]:
- Writes `<CLAUDE_CONFIG_DIR>/projects/<project folder>/<new uuid>.jsonl`, the folder named by `claude_project_dir_name` (the `wL` rule of section 5); `--cwd` or the source's `cwd` is required. No index file is written. The source is never touched.
- One row per content block, all rows of a step sharing a fresh `msg_<uuid>` `message.id`, chained by `parentUuid` in file order, **except** that each `tool_result` row's parent is its own `tool_use` row (otherwise Claude Code substitutes `[Tool result missing due to internal error]`, spec §6.2). Rows carry `isSidechain: false`, `sessionId`, `cwd`, a monotone timestamp, and `version` when the destination version is known.
- **`redacted_thinking` round-trips:** a reasoning block with only an encrypted payload is written as `{"type":"redacted_thinking","data":…}`; one with a signature as `thinking` with its `signature`. Under the default `DropForeign` policy a payload is carried only when it came from the same protocol and is unmodified; otherwise it is dropped or written as labelled text, and the manifest says which [5][7].
- **Tool ids** are rewritten to `^[a-zA-Z0-9_-]+$` for Anthropic destinations: each other character becomes `_`, a collision gets `_2`, `_3`…, an empty id becomes `toolu_mukade`; the same id is used for the call and its result and counted as `rewritten_call_ids` [5].
- **Tools:** calls become Claude's own `Bash`, `PowerShell`, `Read`, `Write`, `Edit`, `Grep`, `Glob`, `WebFetch`, `WebSearch` when equivalent; subagent, skill, question, MCP and harness-internal calls travel as text form, following the tables [3][7]. Result content is flattened to text; non-text blocks are dropped and counted (`dropped_result_blocks`).
- **Compaction** is written as a user message (`[mukade handoff v1] Summary of the earlier conversation, written by the source agent's own compaction:` and the summary), then the tail; no native `compact_boundary` is written [1][7].
- **How `/resume` lists it.** The session ends as a native one does: a `{"type":"custom-title","customTitle":…,"sessionId":…}` row when the session has a name (the source's, e.g. a Codex thread's name, or the caller's `WriteOpts.title`), then `{"type":"last-prompt","lastPrompt":…,"leafUuid":…,"sessionId":…}` naming the last user prompt (one line, at most 200 characters) and the last row. Claude Code 2.1.312's picker titles a session by its custom title, else by that `lastPrompt`, else by its first user prompt (read from its own code, 2026-10-28) [9]. Without those rows a session written from Codex was listed under the AGENTS.md text Codex injects as the first user message, and could not be told apart.
- Orphan and duplicate results, empty steps and unanswered calls are dropped or moved to text and counted in the manifest [5].

**Known limits** [1][4]:
- A hand-written native compaction record for Claude Code is **[U]**: the spec §4.4 marks it as not yet built.
- Not exercised by a bench run: auto-compaction by threshold, a second compaction, rewind or fork in a real source (the reader follows the resumed branch, tested on hand-made records), subagent sessions, images and attachments toward Claude Code.
- On resume of an interrupted last turn Claude Code inserts its synthetic `No response requested.`; the manifest records `interrupted_last_turn`. Run the resume from the session's `cwd`: from elsewhere Claude Code appends rows with the new `cwd` [1].
- mukade does not create Claude Code's authentication or onboarding configuration [8].

### How mukade summarizes it

With `--compaction summary` and Claude Code as the source, mukade runs
`claude -p --tools "" --resume <id> --no-session-persistence --output-format json [--model <id>] <prompt>`
from the session's directory (mukade's own when it is gone), with `CLAUDE_CONFIG_DIR` set to the
source store, and reads the summary from the JSON `result`; `is_error: true` or a `subtype` other
than `success` is a failed summary. The transcript is left as it was; the user's SessionStart hooks
still run [27]. The JSON's `total_cost_usd`, `usage` and `modelUsage` go to the manifest's
`options.summary.usage` [5][27].

## 9. Sources

[9] Claude Code 2.1.312's own JavaScript, embedded in `claude.exe`, read on 2026-10-28: the
`/resume` list is built from each session file's head and tail (first prompt, `lastPrompt`,
`customTitle`, `aiTitle`), and drops sessions that are sidechains, team or daemon sessions, `/loop`
sessions, of some entrypoints, superseded ("continued in"), recorded under another directory with
the same project slug, or made of bookkeeping rows only (no `"parentUuid":`).

All accessed 2026-10-28 unless a source says otherwise.

Local (this repository and this machine):

1. `docs/session-interchange-spec.md` §0, §2.1, §3, §4, §5, §6, §7, §8, §10 (Claude Code 2.1.311 verified records and commands).
2. `docs/reference/harness-session-persistence-reference.md` (Claude Code dossier, secondary research report).
3. `docs/tool-conversion-tables.md` §2 to §4 (tools offered and used, name and argument tables).
4. `src/reader/claude_code.rs`, `src/reader/claude_code/order.rs`.
5. `src/writer/claude_code.rs`, `src/writer/plan.rs`.
6. `src/exe.rs`, `src/roots.rs` (`claude_project_dir_name`).
7. `src/policy.rs`, `src/detect.rs`, `src/hop.rs`, `src/cli.rs`.
8. `README.md`, `docs/usage.md`.
9. Installed binary `C:\Users\alfio\.local\bin\claude.exe`: `claude --version` (2.1.312), `claude --help`, `claude plugin --help`, `claude mcp --help`, `claude import --help`, `claude install --help`, `claude agents --help`, `claude auth --help`.

Web:

10. Advanced setup, https://code.claude.com/docs/en/setup
11. Hooks reference, https://code.claude.com/docs/en/hooks (first 100,000 of 282,369 characters)
12. Tools reference, https://code.claude.com/docs/en/tools-reference
13. Plugins reference, https://code.claude.com/docs/en/plugins-reference
14. Discover and install plugins, https://code.claude.com/docs/en/discover-plugins
15. Connect to tools via MCP, https://code.claude.com/docs/en/mcp
16. Environment variables, https://code.claude.com/docs/en/env-vars
17. Settings files and precedence, https://code.claude.com/docs/en/settings (`~/.claude.json`, settings locations)
18. The `.claude` directory, https://code.claude.com/docs/en/claude-directory
19. Checkpointing, https://code.claude.com/docs/en/checkpointing
20. Claude Code overview, https://code.claude.com/docs/en/overview
21. Agent SDK overview, https://code.claude.com/docs/en/agent-sdk/overview
22. Agent SDK quickstart, https://code.claude.com/docs/en/agent-sdk/quickstart
23. Model configuration, https://code.claude.com/docs/en/model-config
24. Release v2.1.311, https://github.com/anthropics/claude-code/releases/tag/v2.1.311 (commit `08c6a4f5121ae140d3c1b5c96e3e5ae53587b54b`)
25. License file, https://raw.githubusercontent.com/anthropics/claude-code/main/LICENSE.md
26. npm registry, https://registry.npmjs.org/@anthropic-ai/claude-code/latest (`version` 2.1.312, `license` "SEE LICENSE IN README.md", `engines.node` ">=22.0.0", `bin.claude` "bin/claude.exe")
27. `docs/verification/source-model-summary-live.md`: the live check of 2026-10-28 (throwaway sessions, source hashed before and after; Codex against a local mock)
28. Advanced setup and the native installers, https://code.claude.com/docs/en/setup and https://claude.ai/install.sh, accessed 2026-10-30 (canonical launcher, versions directory, `claude doctor`).
29. https://code.claude.com/docs/en/tools-reference, accessed 2026-11-01.
30. https://code.claude.com/docs/en/permissions, accessed 2026-11-01.
31. https://code.claude.com/docs/en/headless, accessed 2026-11-01.
32. https://code.claude.com/docs/en/agent-sdk/typescript (characters 0 to 300000 of 329464), accessed 2026-11-01.
33. https://code.claude.com/docs/en/hooks (characters 0 to 200000 of 284258), accessed 2026-11-01.
34. https://code.claude.com/docs/en/data-usage ("WebFetch domain safety check"), accessed 2026-11-01.
35. `claude --version` and `claude --help` of the installed CLI (2.1.316) on this machine, accessed 2026-11-01.
36. https://registry.npmjs.org/@anthropic-ai/claude-code/latest (`version` 2.1.316), accessed 2026-11-01.
37. `C:\Users\alfio\.local\share\claude\versions\2.1.316` (minified JS, read only; text found with `grep -a`: `web_search_20250305`, `Web search results for query`, `REDIRECT DETECTED`, tool names), accessed 2026-11-01.
38. `fixtures/claude_code/source/c45db7bc-542c-4201-89de-b590c6fdb797.jsonl` (searched for web tool blocks and `server_tool_use` counters), accessed 2026-11-01.
39. https://code.claude.com/docs/en/security, accessed 2026-11-01.
