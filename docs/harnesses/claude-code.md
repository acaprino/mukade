# Claude Code

| | |
|---|---|
| **Documentation updated** | 2026-10-28 |
| **Version described** | 2.1.311 (the version mukade's readers and writers were verified against) [1][24] |
| **Vendor** | Anthropic PBC [25] |
| **Website** | https://code.claude.com (documentation); https://github.com/anthropics/claude-code (releases, issues) [20][24] |
| **License / source** | Proprietary: "© Anthropic PBC. All rights reserved. Use is subject to Anthropic's Commercial Terms of Service." [25] |
| **mukade name** | `claude_code` |

Version note: the binary installed on the verification machine now reports `2.1.312 (Claude Code)` and npm's `latest` is `2.1.312` [9][26]. On-disk facts and mukade behaviour below were verified on 2.1.311 [1]; the entry points were checked against the 2.1.312 `--help`; the web documentation describes the current release, so a feature it names may be newer than 2.1.311 **[U]** per feature.

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
- **No tools.** `--tools ""` offers none. The option takes several values, so it must come before
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
| Search | `Glob`, `Grep` (default on Windows; on macOS, Linux and WSL the shell runs embedded `bfs` and `ugrep` instead), `LSP` (code intelligence through language servers), `ToolSearch` (loads deferred tools) |
| Web | `WebFetch`, `WebSearch` |
| Agents | `Agent` (subagent), `SendMessage`, `ListAgents`, `TaskStop`, `Workflow` (script that orchestrates subagents), `Skill` (runs a skill) |
| Tasks and scheduling | `TaskCreate`, `TaskGet`, `TaskList`, `TaskUpdate` (replace `TodoWrite`, which is off by default), `CronCreate`, `CronDelete`, `CronList`, `ScheduleWakeup`, `RemoteTrigger` |
| Planning and worktrees | `EnterPlanMode`, `ExitPlanMode`, `EnterWorktree`, `ExitWorktree`, `AskUserQuestion` |
| MCP | `ListMcpResourcesTool`, `ReadMcpResourceTool`, `WaitForMcpServers`; tools of MCP servers are `mcp__<server>__<tool>` [3][12] |
| Other | `Artifact`, `ReportFindings`, `PushNotification`, `SendFeedback`, `SendUserFile`, `ShareOnboardingGuide`, `SubagentHandback` (auto mode), `EndConversation` |

`TaskOutput` was removed in v2.1.277 [12]. The set the model is offered depends on version, platform, model and settings. In the mukade bench (2.1.311, default settings, no plugins) 23 tools were offered: `Agent`, `Bash`, `CronCreate`, `CronDelete`, `CronList`, `Edit`, `EnterWorktree`, `ExitWorktree`, `Glob`, `Grep`, `ListAgents`, `Monitor`, `NotebookEdit`, `Read`, `ReportFindings`, `ScheduleWakeup`, `SendMessage`, `Skill`, `TaskStop`, `WebFetch`, `WebSearch`, `Workflow`, `Write` [3]. For how each call maps to Codex, OpenCode and Pi, see [../tool-conversion-tables.md](../tool-conversion-tables.md); the table is not repeated here.

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
- `--bare` and `--safe-mode` skip hooks [9]. mukade neither reads nor writes hook configuration.

## 8. How mukade reads and writes it

Sources: `src/reader/claude_code.rs`, `src/writer/claude_code.rs`, `src/writer/plan.rs`, `src/exe.rs`, `src/roots.rs`, `src/policy.rs`, `src/detect.rs`, `src/hop.rs`, `docs/usage.md`, spec §2.1 and §6.2 [1][4][5][6][7][8].

| Item | Value |
|---|---|
| CLI name | `claude_code` |
| Store variable | `CLAUDE_CONFIG_DIR` (sessions under `projects/`); required explicitly for read and write, never defaulted [6][8] |
| Executable flag / variable | `--claude-exe` / `MUKADE_CLAUDE_EXE` [6][8] |
| Executable resolution | flag, then `MUKADE_CLAUDE_EXE`, then `PATH`, then `~/.local/bin/claude.exe` and `~/.local/bin/claude` (under `USERPROFILE` or `HOME`), then `claude` [6] |
| Version stamp | `--dest-version`, else the first version printed by the resolved executable's `--version` [7] |
| Detection | a directory with at least one `projects/<project>/*.jsonl` is Claude Code [7] |
| Resume command written | `claude -p --resume <id>` (the resolved executable replaces `claude`) [5] |

**Reader (`src/reader/claude_code.rs`)** [4]:
- `list` reads `projects/*/*.jsonl` and peeks at the first 20 lines (`sessionId`, `cwd`, `slug` as title, timestamp); `read` matches the file name by prefix of the id and refuses an ambiguous prefix. Subagent folders are not listed.
- **Resumed branch only.** It walks back from the last row (last row with a `uuid` that is not a sidechain row) by `parentUuid`, and by `logicalParentUuid` across a compaction boundary whose `parentUuid` is null; rows of parallel calls (same `message.id`, results hanging off a `tool_use` row) are taken with the chain. Rows outside that branch are **kept as `Meta(OffBranch)`** and counted as `dropped_off_branch`; they are not written to the destination.
- **Rows merged by `message.id`** into one `Step`; results are paired by `tool_use_id`, never by walking parents (L5).
- **Meta, never conversation:** `<synthetic>` assistant replies (`Meta(Synthetic)`), `isMeta` and `isVisibleInTranscriptOnly` rows and `<local-command-*>` / `<command-name>` / `<command-message>` rows (`Meta(Command)`; the `isMeta` row after a prompt-type slash command is the expanded prompt and becomes a user turn), `attachment` rows, other `system` rows, unknown row types, and subagent (`isSidechain`) rows. A last line cut off mid-write is kept as an unfinished record.
- **Compaction:** `compact_boundary` plus the `isCompactSummary` row become one `Compaction` entry (`summary`, `trigger` manual/auto, `tokens_before` = `preTokens`, `kept_from` = `preservedSegment.headUuid`, or the first conversation entry after the boundary when that id is not an entry).
- **Reasoning:** `thinking` keeps its `signature`; `redacted_thinking` keeps its `data` as the opaque payload; both are tagged `anthropic-messages` with the step's model.
- **Step end** from `stop_reason`: `tool_use`, `end_turn`/`stop_sequence`/`max_tokens`/`length` completed, `refusal` error. Session metadata (`sessionId`, `cwd`, `gitBranch`, `version`, `slug`, models) comes from the rows; `parent` is always empty because `--fork-session` leaves no lineage.

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

## 9. Sources

[9] Claude Code 2.1.312's own JavaScript, embedded in `claude.exe`, read on 2026-10-28: the
`/resume` list is built from each session file's head and tail (first prompt, `lastPrompt`,
`customTitle`, `aiTitle`), and drops sessions that are sidechains, team or daemon sessions, `/loop`
sessions, of some entrypoints, superseded ("continued in"), recorded under another directory with
the same project slug, or made of bookkeeping rows only (no `"parentUuid":`).

All accessed 2026-10-28.

Local (this repository and this machine):

1. `docs/session-interchange-spec.md` §0, §2.1, §3, §4, §5, §6, §7, §8, §10 (Claude Code 2.1.311 verified records and commands).
2. `docs/reference/harness-session-persistence-reference.md` (Claude Code dossier, secondary research report).
3. `docs/tool-conversion-tables.md` §2 to §4 (tools offered and used, name and argument tables).
4. `src/reader/claude_code.rs`.
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
27. `docs/verification/2026-10-28-summary-live.md`: the live check of 2026-10-28 (throwaway sessions, source hashed before and after; Codex against a local mock)
