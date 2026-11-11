# Claude Code

Anthropic's agentic coding tool: it reads a codebase, edits files, runs commands and reaches development tools through MCP, from a terminal, an IDE, the Desktop app, the web or CI [20]. mukade reads and writes its sessions (local JSONL transcripts), and hops them to and from Codex, OpenCode and Pi [1].

## Status in mukade

| | |
|---|---|
| mukade name | `claude_code` |
| Read (`list`, `dump`) | ✅ every `projects/*/*.jsonl`, plus sessions known only from `history.jsonl` or a running process (`unreadable`), with a `live` flag |
| Write | ✅ a new `<uuid>.jsonl` that Claude Code resumes by id; no index file needed [1][5] |
| Hop as source | ✅ the branch Claude Code resumes. ⚠️ `--all-branches` is refused (Codex only) |
| Hop as destination | ✅ resumed live, signed and redacted thinking included (2.1.312) [43]. ⚠️ a `PowerShell` call travels as text form |
| Resume | ✅ `claude -p --resume <id> "<prompt>"` (the receipt's `resume`, plus a prompt) or `claude --resume <id>` interactive: same id, same file [1] |
| Summary by its own model | ✅ `--compaction summary`: `claude -p` with no tools and no transcript; the source is left as it was [27] |
| Native compaction | ✅ read (`compact_boundary`); written by `hop --compaction native`, resumed live on 2.1.318 [42] |
| Notable gaps | subagent transcripts are not listed or carried; a fork has no recorded parent; no archived flag; macOS `live` cannot tell a reused pid **[U]** |

**Which version each fact was checked on.** Page updated 2026-11-11.

| Version | Checked on it |
|---|---|
| 2.1.311 | on-disk facts; mukade's reader and writer [1][24] |
| 2.1.312 | the entry points (`--help`) and the `/resume` picker's own code [9] |
| 2.1.316 | the tool and restriction flags of `--help` [35]; binary text on how `WebSearch` and `WebFetch` run, the search `results` shape, some tool names [37] (internal code, not a documented contract; none of it is mukade behaviour checked there) |
| 2.1.318 | a native compaction written by mukade, resumed live (2026-11-04) [42] |
| 2.1.319 / 2.1.322 | `sessions/<pid>.json` on Windows (2026-11-08) [40] / `live` and `pidDomain` on Linux (2026-11-11) [41] |
| web documentation | read 2026-11-01, not run. It describes the current release (the pages name versions up to v2.1.310 [29]; which release they track is **[U]**), so a feature it names may be newer than 2.1.311 **[U]** per feature |

On 2026-11-01 the installed binary reported `2.1.316 (Claude Code)` and npm's `latest` was `2.1.316` [35][36].

## Quick start

```powershell
# 1. Name the store (required: mukade never assumes ~/.claude)
$env:CLAUDE_CONFIG_DIR = Join-Path $HOME '.claude'      # bash: export CLAUDE_CONFIG_DIR=~/.claude

# 2. Find a session, look at it
mukade list claude_code
mukade dump claude_code <session-id> --output C:\work\session.json

# 3. Hop out (set the destination's store variable too)
$env:PI_CODING_AGENT_DIR = Join-Path $HOME '.pi\agent'
mukade hop claude_code <session-id> pi --cwd 'C:\work\my-project'

# 4. Hop in
$env:CODEX_HOME = Join-Path $HOME '.codex'
mukade hop codex <thread-id> claude_code --cwd 'C:\work\my-project'

# 5. Resume what was written: keep CLAUDE_CONFIG_DIR set, run from the session's cwd
cd C:\work\my-project
claude --resume <new-id>                       # interactive
claude -p --resume <new-id> "<prompt>"         # the receipt's "resume"
```

- `--root <dir>` names the store instead of the variable; with it the harness may be left out of `list`, `dump` and a hop source.
- `--claude-exe <path>` (or `MUKADE_CLAUDE_EXE`) names the binary, e.g. a portable install [6][8].
- Without `--resume-prompt` mukade only prints the resume command. With it, the hop runs `claude -p --resume <id>` and keeps the reply in the receipt; that is a model call [8].

**Claude Code's own session commands** [9]:

| Use | Command |
|---|---|
| Interactive, in the current directory | `claude`, or `claude "<prompt>"` |
| Print mode (non-interactive) | `claude -p "<prompt>"`; `--output-format text\|json\|stream-json`, `--input-format text\|stream-json` |
| Continue this directory's latest | `claude -c` / `claude --continue` |
| Resume by id or name, or pick | `claude -r [value]` / `claude --resume [value]` |
| Resume headless with a prompt (what mukade runs) | `claude -p --resume <id> "<prompt>"`: same id, appends to the same file, works from any cwd [1] |
| Compact headless | `claude -p --resume <id> "/compact"` [1] |
| Resume under a new id | add `--fork-session` to `--resume` or `--continue` |
| Set the id, name the session | `--session-id <uuid>`, `-n, --name <name>` |
| Resume a session linked to a pull request | `--from-pr [value]` |

## Install and locate

**Requirements** [10]: macOS 13.0+, Windows 10 1809+ or Windows Server 2019+, Ubuntu 20.04+, Debian 10+ or Alpine Linux 3.19+; 4 GB+ RAM; x64 or ARM64; internet access.

| Method | Command |
|---|---|
| Native, macOS / Linux / WSL (recommended) | `curl -fsSL https://claude.ai/install.sh \| bash` [10] |
| Native, Windows PowerShell | `irm https://claude.ai/install.ps1 \| iex` [10] |
| Native, Windows CMD | `curl -fsSL https://claude.ai/install.cmd -o install.cmd && install.cmd && del install.cmd` [10] |
| Native, stable channel | `curl -fsSL https://claude.ai/install.sh \| bash -s stable` (PowerShell and CMD variants on the same page) [10] |
| Native, from an installed copy | `claude install [target]`, target `stable`, `latest` or a version [9] |
| Homebrew (macOS / Linux) | `brew install --cask claude-code` (stable) or `claude-code@latest` [10] |
| WinGet (Windows) | `winget install Anthropic.ClaudeCode` [10] |
| apt / dnf / apk | signed repositories: `sudo apt install claude-code`, `sudo dnf install claude-code`, `apk add claude-code`, after adding the repository and key as the setup page describes [10] |
| npm (Node.js 22+) | `npm install -g @anthropic-ai/claude-code`: the same native binary, through a per-platform optional dependency [10][26] |

**Update** [9][10][16]:
- Native installs update themselves in the background; `DISABLE_AUTOUPDATER=1` stops the check.
- `claude update` (alias `claude upgrade`) forces a check.
- Homebrew, WinGet, apt, dnf and apk do not self-update: `brew upgrade claude-code`, `winget upgrade Anthropic.ClaudeCode`.
- npm: `npm install -g @anthropic-ai/claude-code@latest`.
- Uninstall: see the setup page [10] (not reproduced here **[U]**).

**Where it is**:

| What | Windows | macOS / Linux |
|---|---|---|
| Native-install launcher | `%USERPROFILE%\.local\bin\claude.exe` | `~/.local/bin/claude` (a symlink into the versions directory) |
| Native-install versions | `%USERPROFILE%\.local\share\claude\versions\` | `~/.local/share/claude/versions/` |
| Store (sessions and configuration) | `%USERPROFILE%\.claude` | `~/.claude` |

- Sources: [10][28]; on Windows the same native layout is used under `%USERPROFILE%` [28]. On the verification machine: `C:\Users\alfio\.local\bin\claude.exe` [1][6].
- The store is `~/.claude` on every OS unless `CLAUDE_CONFIG_DIR` moves it: "all settings, session history, and plugins are stored under this path" [16][18]. `%USERPROFILE%\.claude` is an OS-path inference, not an Anthropic contract [2].
- Homebrew, apt/dnf/apk, WinGet and npm installs are found through `PATH` and their package manager, not the native path: `command -v claude` (macOS/Linux), `Get-Command claude -All` (PowerShell), `brew --prefix` or `npm prefix -g`, then `claude --version`. `claude doctor` reports installation health without starting a session [28].
- Windows: Git for Windows is optional; with it the Bash tool uses Git Bash, without it Claude Code uses the PowerShell tool [10].
- Alpine / musl: needs `bash`, `curl`, `libgcc`, `libstdc++`, `ripgrep` and `USE_BUILTIN_RIPGREP=0` [10].

**How mukade finds it**:

| What | How |
|---|---|
| Store | `CLAUDE_CONFIG_DIR` (sessions under `projects/`) or `--root <dir>`; required for read and write, never defaulted [6][8] |
| Executable | `--claude-exe` → `MUKADE_CLAUDE_EXE` → `PATH` → the native launcher `~/.local/bin/claude[.exe]` from [`catalog/harnesses.json`](../../catalog/harnesses.json) → `claude` [6] |
| Executable kept beside a store (`scan`, `--dest-root`) | `claude[.exe]`, `bin/claude[.exe]` or `.local/bin/claude[.exe]` in the store folder or up to 3 folders above it (`src/detect/engine.rs` `engine_beside_store()`) |
| Detection (`detect`, `scan`, `--root`) | a folder with at least one `projects/<project>/*.jsonl`; the folder name alone is not evidence [7] (`src/detect/claude_code.rs` `ClaudeCodeLayout`) |
| `mukade harnesses` | the one executable it resolves, not every copy on disk; see the [harness catalog](../reference/harness-catalog.md#finding-existing-installations) for the per-OS search and for choosing a non-default install |

**Other surfaces** [9][20][21][22]:

| Surface | How |
|---|---|
| Background sessions | `claude --bg`, then `claude agents`, `claude attach <id\|name>`, `claude logs <id\|name>`, `claude stop <id>`, `claude rm <id>`, `claude respawn [id]` |
| Isolated git worktree | `claude -w [name]` / `--worktree [name]` |
| Desktop app, IDE, browser, cloud | `--desktop`, `--ide`, `--chrome`; `--cloud`, `--teleport [session]`, `--remote-control [name]` |
| Management subcommands | `auth`, `auto-mode`, `doctor`, `gateway`, `import`, `install`, `mcp`, `plugin`, `purge`, `setup-token`, `ultrareview`, `update` |
| IDEs and elsewhere | VS Code extension `anthropic.claude-code`; JetBrains plugin from the JetBrains Marketplace (needs the CLI installed separately); the Desktop app; the web; Slack; GitHub Actions and GitLab CI/CD [20] |
| Agent SDK | `npm install @anthropic-ai/claude-agent-sdk` (TypeScript), `pip install claude-agent-sdk` or `uv add claude-agent-sdk` (Python); both bundle a native Claude Code binary [22]. Other languages run `claude -p --output-format json` as a subprocess [21] |

**Vendor, licence, sites**:

| | |
|---|---|
| Vendor | Anthropic PBC [25]: a coding agent with a terminal CLI, IDE extensions, a desktop app and a hosted web surface [20] |
| Licence | Proprietary: "© Anthropic PBC. All rights reserved. Use is subject to Anthropic's Commercial Terms of Service." [25]. npm `license`: `SEE LICENSE IN README.md` [26]. The Agent SDK falls under the same terms unless a component's own LICENSE says otherwise [21] |
| Source | whether the repository publishes the CLI's source was not verified **[U]**; it is used here only for releases and issues |
| Documentation | https://code.claude.com/docs/en/overview; index of all pages: https://code.claude.com/docs/llms.txt [20] |
| Repository, package | https://github.com/anthropics/claude-code (releases, issues, plugin marketplace sources) [24][26]; npm `@anthropic-ai/claude-code` [26] |

## Session store

JSON Lines, one record per line. Anthropic publishes no versioned schema: treat it as internal and keep unknown records verbatim [2]. No index file: a session file written outside Claude Code resumes by id [1].

**Layout** (`<config>` = `CLAUDE_CONFIG_DIR`, default `~/.claude`) [16][17][18]:

| Path | Holds |
|---|---|
| `<config>/projects/<project>/<session-id>.jsonl` | the transcript |
| `<config>/projects/<project>/<session-id>/subagents/` | subagent transcripts, same format |
| `<config>/projects/<project>/<session-id>/tool-results/` | large tool outputs |
| `<config>/file-history/<session-id>/` | pre-edit snapshots for `/rewind`, the 100 most recent checkpoints |
| `<config>/history.jsonl` | every typed prompt, all sessions (below) |
| `<config>/sessions/<pid>.json` | one per running session (below) |
| `~/.claude.json` | account, personal MCP servers, per-project state |

**Project folder name** (`src/roots.rs` `claude_project_dir_name()`, Claude Code 2.1.311's `wL`) [6]: every UTF-16 unit outside `[a-zA-Z0-9]` becomes `-` (`C:\a\b` → `C--a-b`); a name over 200 characters is cut at 200 and suffixed with `-` and the base-36 hash of the path.

**Record types** (seen in 2.1.311) [1][4][9]:

| `type` | Meaning | Fields mukade reads |
|---|---|---|
| `user` | a prompt; tool results as `tool_result` blocks; slash-command rows (`<command-name>`, `<local-command-stdout>`, `<local-command-caveat>`, with `isMeta`); the compaction summary | `message.content`, `isMeta`, `isVisibleInTranscriptOnly`, `isCompactSummary`, `tool_use_id`, `is_error` |
| `assistant` | **one row per content block**, sharing `message.id`, chained by `parentUuid`; blocks `thinking` (with `signature`), `redacted_thinking` (with `data`), `text`, `tool_use` | `message.id`, `message.model`, `message.stop_reason` |
| `system` | notably `subtype: "compact_boundary"` | `subtype`, `compactMetadata` |
| `attachment` | files re-injected after a compaction | kept, not conversation |
| `custom-title`, `last-prompt` | the `/rename` name; the last prompt and its leaf | `customTitle`; `lastPrompt`, `leafUuid` |
| others | request/cost/telemetry rows (`api-request` among them) | kept, not conversation |

Common fields: `uuid`, `parentUuid`, `logicalParentUuid`, `sessionId`, `cwd`, `gitBranch`, `version`, `slug`, `timestamp`. Subagent rows carry `isSidechain: true`.

**Ordering**:
- Parallel tool calls: the `tool_use` rows are chained; each `tool_result` row's `parentUuid` is its own `tool_use` row, in completion order. Pair by `tool_use_id`, not by walking parents [1].
- A rewind or an edited prompt starts a new branch from an earlier row; the abandoned rows stay in the file. Claude Code resumes the branch of the last row [4]. Release 2.1.311 fixed transcript ordering and "taken back" messages [24].
- A killed process leaves the user row. On the next load Claude Code writes a synthetic assistant `"No response requested."` with `model: "<synthetic>"` [1].
- A turn the user interrupts ends with a user row `[Request interrupted by user]` (`[Request interrupted by user for tool use]` beside a tool result).
- A compaction restarts the chain: the boundary has `parentUuid: null` and `logicalParentUuid: <old leaf>` (see Compaction).

**Lineage.** `--fork-session` gives a new id and writes **no lineage field** [1]: a fork is a session with no parent (catalog `lineage.recorded: false`). Subagents live in `subagents/` and as `isSidechain` rows.

**Beside the transcript**:
- `history.jsonl`: one line per prompt, `{display, pastedContents, project, sessionId, timestamp (ms)}`. It stays when a session's file is deleted. 2026-11-08: 261 sessions in it, 214 transcripts, 88 sessions without one, in 19 projects no transcript names [40].
- `sessions/<pid>.json`: exists while a process runs a session. `{pid, sessionId, cwd, name, status, startedAt, updatedAt, procStart, pidDomain, kind, entrypoint, version, …}` (2.1.319), with the `<pid>.<hash>.key` peer-messaging key beside it. A clean exit removes it; a killed process leaves it behind, so it counts only once its process is looked at.
- `procStart`: the process's start time in the platform's unit. Windows: a FILETIME, equal to the creation time to the second [40]. Linux: field 22 of `/proc/<pid>/stat`, clock ticks since boot, equal exactly [41].
- `pidDomain`: where the pid is valid. `win32:<computer name>` on Windows; `linux:<machine id>:pid:[<pid namespace>]` on Linux, from `/etc/machine-id` and `/proc/self/ns/pid` (seen with 2.1.322 on 2026-11-11) [41].
- Retention: files older than `cleanupPeriodDays` are deleted by a sweep (default 30, minimum 1). Transcripts are not encrypted: a credential a tool printed is in the file [18].

## Features

### Compaction

**Triggers and controls** [1][9][16][23][24]:

| Control | Effect |
|---|---|
| `/compact`, or `claude -p --resume <id> "/compact"` | manual |
| automatic | when the conversation approaches the context limit; the default threshold of 2.1.311 was not read **[U]** |
| `DISABLE_AUTO_COMPACT=1` | automatic off, `/compact` stays |
| `DISABLE_COMPACT=1` | both off; since v2.1.311, in settings files it can only turn compaction off, not back on [24] |
| `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE` (1-100), `--autocompact <auto\|tokens>` (100k to 1M), `autoCompactWindow` | the percentage can only lower the threshold |

- **Targeted.** `/rewind` offers "Summarize from here" and "Summarize up to here": part of the conversation is compressed, the original messages stay in the transcript [19].
- **Record on disk** [1]: a `system` row `subtype: "compact_boundary"` with `parentUuid: null`, `logicalParentUuid: <old leaf>`, `compactMetadata{trigger: "manual"|"auto", preTokens, postTokens, preservedSegment{headUuid, anchorUuid, tailUuid}, preservedMessages}`. Then a `user` row with `isCompactSummary: true`, `isVisibleInTranscriptOnly: true` holding "This session is being continued from a previous conversation … Summary: …". Then the `/compact` command rows and `attachment` rows for files read earlier. The last assistant reply stays in place, hung under the summary by `preservedSegment`.
- **On resume** the model gets the summary, the preserved tail and later turns; earlier tools and thinking are not sent, and files read earlier return as a system message. Before a compaction it gets everything, thinking with its signature [1].
- **Headless.** `claude -p --resume <id> "/compact"` compacts the session it resumes: the rows land in that session's own transcript [1]. To compact without touching it, fork first (`--fork-session`) **[U]**.

### Ephemeral runs

- **No transcript.** `--no-session-persistence` (print mode only) writes no session file [9]. With `--resume <id>` the resumed transcript stays byte-identical and no file is added to the project folder. Hooks still run: the user's SessionStart hooks rewrote `session-env/<id>/` for the resumed id [27].
- **Under a new id.** `--fork-session` with `--resume` or `--continue` writes the continuation to a new file; the original stays as it was [9][19].
- **No tools.** `--tools ""` offers none; `EndConversation` is the one no `--tools` list removes, but `-p` runs do not offer it [29]. Put `--tools` before another option: after `--resume` it swallows the prompt and Claude Code answers `Error: No deferred tool marker found in the resumed session …`. A history holding tool calls is accepted with no tools offered (first-party API, `claude-sonnet-5`) [27].
- **`--bare` is not for subscription logins.** It skips hooks, but authenticates only with `ANTHROPIC_API_KEY` or an `apiKeyHelper`, never OAuth [9].
- **Output.** `--output-format json` prints one object: `result`, `is_error`, `subtype` (`success`), `session_id`, `total_cost_usd`, `usage`, `modelUsage`. With stdin left open, print mode waits 3 s for it (`Warning: no stdin data received in 3s`) [27].

### Tools

Tool names are the exact strings used in permission rules, subagent tool lists and hook matchers [12]. Documented set:

| Group | Tools |
|---|---|
| Files | `Read`, `Write`, `Edit`, `NotebookEdit` (Jupyter cells) |
| Shell | `Bash`; `PowerShell` (default on native Windows without Git Bash; elsewhere opt-in with `CLAUDE_CODE_USE_POWERSHELL_TOOL=1`); `Monitor` (a background command streamed line by line) |
| Search | `Glob`, `Grep`; `LSP` (code intelligence through language servers); `ToolSearch` (loads deferred tools) |
| Web | `WebFetch`, `WebSearch` (below) |
| Agents | `Agent` (subagent), `SendMessage`, `ListAgents`, `TaskStop`, `Workflow` (a script that orchestrates subagents), `Skill` (runs a skill) |
| Tasks and scheduling | `TaskCreate`, `TaskGet`, `TaskList`, `TaskUpdate` (they replace `TodoWrite`, off by default); `CronCreate`, `CronDelete`, `CronList`, `ScheduleWakeup`, `RemoteTrigger` |
| Planning and worktrees | `EnterPlanMode`, `ExitPlanMode`, `EnterWorktree`, `ExitWorktree`, `AskUserQuestion` |
| MCP | `ListMcpResourcesTool`, `ReadMcpResourceTool`, `WaitForMcpServers`; an MCP server's tools are `mcp__<server>__<tool>` [3][12] |
| Other | `Artifact`, `ReportFindings`, `PushNotification`, `SendFeedback`, `SendUserFile`, `ShareOnboardingGuide`, `SubagentHandback` (auto mode), `EndConversation` |

- **`Glob` and `Grep`** are on by default on Windows. On macOS, Linux and WSL the shell runs embedded `bfs` and `ugrep` instead, and the two tools are absent unless `--tools` or `--allowedTools` names them, or Bash is removed. Naming either in `--allowedTools` restores both; an allow rule in a settings file does not [29].
- **Not in the table.** `TaskOutput` was removed in v2.1.277 [12]. The SDK reference also names `ReadMcpResourceDir`, `Projects`, `RefreshMcpTools` (needs `CLAUDE_CODE_ENABLE_REFRESH_MCP_TOOLS=1`), `ShowOnboardingRolePicker` and `StructuredOutput`, and marks `REPL` as removed [32]. The 2.1.316 binary holds the names `ArtifactComments`, `ArtifactData`, `ArtifactCheck`, `TeamCreate` and `TeamDelete`; a name there does not show the tool is offered, which a `system/init` line's `tools` list would [37].
- **What a run is offered** depends on version, platform, model and settings. The mukade bench (2.1.311, default settings, no plugins) was offered 23: `Agent`, `Bash`, `CronCreate`, `CronDelete`, `CronList`, `Edit`, `EnterWorktree`, `ExitWorktree`, `Glob`, `Grep`, `ListAgents`, `Monitor`, `NotebookEdit`, `Read`, `ReportFindings`, `ScheduleWakeup`, `SendMessage`, `Skill`, `TaskStop`, `WebFetch`, `WebSearch`, `Workflow`, `Write` [3] (`src/tools.rs` `offered()`).
- **Mapping to Codex, OpenCode and Pi**: [../tool-conversion-tables.md](../tool-conversion-tables.md); not repeated here.

**Permissions and switches for one run.** Tools ask first unless a permission rule or mode allows them. Modes: `acceptEdits`, `auto`, `bypassPermissions`, `manual`, `dontAsk`, `plan` [9].

| Switch | Effect |
|---|---|
| `--tools "<list>"` | offers only these built-in tools; `--tools ""` offers none [35] |
| `--allowedTools "<rules>"` / `--disallowedTools "<rules>"` | allow / deny the named tools or rules; a bare name in `--disallowedTools` removes the tool from the model's context [35][31][32] |
| `--restricted` | removes the code-running tools and `WebFetch` unless `--tools` names them; ignores the user, project and local settings files (`--settings` still applies); confines file tools to the working directories; refuses `bypassPermissions` [35][9] |
| `--strict-mcp-config`, `--safe-mode` | with no `--mcp-config`, no MCP server loads; `--safe-mode` disables every customization [35][9] |
| `--permission-prompts host\|none` | `none` needs v2.1.259+ [31]; what `host` does was not read **[U]** (the flag's `--help` text would settle it) |
| `--include-hook-events` | listed by `--help` [35]; what it adds to the stream **[U]** (a `-p --output-format stream-json` run with a `PreToolUse` hook would show it) |

- `--tools` and `--allowedTools` take several values and swallow a prompt that follows them [35][31][32].
- claude.ai connectors and plugins are separate from `.mcp.json` servers [39]. Whether `--strict-mcp-config` or `--disable-slash-commands` excludes them is **[U]**; empty `mcp_servers` and `plugins` in a run's `system/init` line would settle it.
- **Read tools.** `Read`, `Glob` and `Grep` are read-only (a read of a network path asks first). For one run: name them in `--tools`; `Read(<path>)` rules scope or deny paths for all three; `--disallowedTools "Read"` or `"Glob Grep"` removes them [29][30][32].

**Web tools: `WebSearch` runs at the provider**:
- Input `{query, allowed_domains?, blocked_domains?}`; the two lists cannot be combined in one call [29][32].
- It "runs a query against Anthropic's web search backend and returns result titles and URLs" without fetching the pages; not exposed on Amazon Bedrock [29].
- In the 2.1.316 binary a call makes a nested Messages request with the `web_search_20250305` server tool (`max_uses: 8`): a second model call, up to eight backend searches per call [37].
- Whether every login type (a subscription login) may run it is **[U]**; a `-p` run on such a login would settle it. The mukade bench (2.1.311) was offered the tool [3].
- The per-turn limit resets on each `claude -p` run and is raised with `CLAUDE_CODE_MAX_WEB_SEARCHES_PER_SESSION`; its `-p` default, and whether it can be lowered, are **[U]** [29].

**Web tools: `WebFetch` runs locally**:
- Input `{url, prompt, offset?}` [32]. It fetches the page, converts HTML to Markdown and "runs the prompt against the content in a separate model call"; the model receives that result, not the raw page [29].
- The hostname first goes to `api.anthropic.com` for a blocklist check; `skipWebFetchPreflight: true` skips it [34].
- It refuses `localhost` and hosts without a dot. It does not follow a redirect to another host: the result begins `REDIRECT DETECTED: The URL redirects to a location that was not fetched automatically` and the model must call again [29][37].
- Rules match the host only: `WebFetch(domain:example.com)` that host, `WebFetch(domain:*.example.com)` its subdomains at any depth but not the apex, `WebFetch(domain:*)` every host. Case-insensitive; deny beats ask beats allow; an explicit rule beats the preapproved set; elsewhere `*` matches one label [30][29].
- No rule on the `url` field or a path [30]: an exact-URL allowlist needs a `PreToolUse` hook (see Hooks).

**Web tools: allow or deny for one run**:
- `WebSearch`: allow `--allowedTools "WebSearch"` (no specifier form) [29][31]; deny `--disallowedTools "WebSearch"` (removed from the model's context) [32] or leave it out of `--tools` [35]. In `-p`, a call that needs approval and is not allowed is denied [31]. No environment variable that disables search was found **[U]**; a search of the installed binary's variable names would settle it.
- `WebFetch`: asks in Manual mode except for built-in preapproved documentation domains [29]. Allow `--allowedTools "WebFetch(domain:docs.example.com)"`. Deny: a bare `WebFetch` in `--disallowedTools` removes the tool; denying `WebFetch(domain:*)` keeps it and refuses every fetch; `CLAUDE_CODE_DISABLE_WEB_FETCH=1` (v2.1.285+) turns it off; `--restricted` removes it unless `--tools` names it [29][30][35].

**Web tools: stream and transcript lines**:
- Search call: an `assistant` line with a `tool_use` block `name:"WebSearch"`, `input.query`, `input.allowed_domains`, `input.blocked_domains` [32][33].
- Search answer: a `user` line with a `tool_result` block and a top-level `tool_use_result`: `query`; `results[]`, one `{tool_use_id, content:[{title, url}]}` per backend search or a plain string (the nested model's summary); `durationSeconds`; `searchCount`. An error is the string `Web search error: <error_code>` [32][37]. Refined queries are counted by `searchCount`, not listed; the nested `server_tool_use` / `web_search_tool_result` blocks do not reach the outer stream [37]; the API result's `encrypted_content` and `page_age` are not in the output type [32]. Whether the binary's progress records (`query_update`, `search_results_received`) reach stream-json or a transcript is **[U]**; `grep -c query_update` on a saved run would settle it.
- Fetch: `tool_use` `name:"WebFetch"` with `input.url`, `input.prompt`, `input.offset?` [32][33]; the `tool_use_result` has `url`, `code`, `codeText`, `bytes`, `durationMs` and `result` (the extracted text); a background call returns `{detachedToolCall: true}` (v2.1.287+) [32]. In the binary `url` is assigned from the request variable, not a redirect target; where that variable comes from was not traced **[U]** [37].
- Refusals: a refused call also arrives as a `system/permission_denied` line (`tool_name`, `tool_use_id`, `decision_reason`) and in the final `result`'s `permission_denials` [31][32].
- What is established: the field lists are the SDK's message types [32]. Whether the CLI's stdout lines carry exactly these objects is **[U]** (consistent with the headless page, stated in no single sentence), and so is whether `--verbose` is required with `--output-format stream-json` in `-p` (the page's example uses it) [31]. One run with `WebSearch` allowed, with and without `--verbose`, would settle both.

**Web tools: no sample in the repository.** The one Claude fixture that mentions `WebSearch` lists it among its tool schemas and holds no web `tool_use` or `tool_result` block; its `usage.server_tool_use` counters read `web_search_requests: 0` and `web_fetch_requests: 0` [38]. The field lists above come from the SDK reference and the binary, not from a mukade fixture.

### Branches, forks and subagents

- **Checkpoints and rewind.** Each prompt that starts a turn checkpoints the files Claude edits; `/rewind` (or `Esc` twice on an empty prompt) restores code, conversation or both. Changes made only by Bash, and edits by subagents, are not tracked. Checkpoints are saved with the conversation, so `/rewind` works after a resume [19].
- **Rewound branches stay in the file**, and Claude Code resumes the branch of the last row (see Ordering) [4].
- **Branching.** `/branch`, or `claude --continue --fork-session`, keeps the original intact [19]; the fork records no parent [1].
- **Subagents.** The `Agent` tool spawns a subagent with its own context. Its transcript lives in `<session>/subagents/` in the same format [18]; in the main file its rows are `isSidechain: true` [4]. Definitions: `agents/*.md` (project or global) or `--agents <json-or-file>` [9][18].
- **Not exercised by the mukade bench:** auto-compaction by threshold, a second compaction, a rewind or fork in a real source, subagent sessions **[U]** [1].

### Models and providers

- **Wire protocol.** Claude models over the Anthropic Messages protocol; mukade treats Claude Code as an `anthropic-messages` destination [7]. The agent loop (the model plans, calls tools: read, edit, shell, search, web, subagents; observes the results; repeats until done), its tools and context management are also a library, the Agent SDK [21].
- **Models.** Aliases `default`, `best`, `fable`, `sonnet`, `opus`, `haiku`, `sonnet[1m]`, `opus[1m]`, `opusplan`, or a full model name. Precedence: `/model` in a session, `--model`, `ANTHROPIC_MODEL`, the `model` setting, `ANTHROPIC_DEFAULT_MODEL` [23]. Also `--effort low|medium|high|xhigh|max` and `--fallback-model <model>` [9]. Which models 2.1.311 can use was not verified **[U]**.
- **Authentication.** `claude auth login|logout|status`, `claude setup-token` [9].

| Provider | Switch [16][22][23] |
|---|---|
| Anthropic API | `ANTHROPIC_API_KEY`, or a Claude subscription login |
| Amazon Bedrock | `CLAUDE_CODE_USE_BEDROCK` |
| Google Cloud's Agent Platform | `CLAUDE_CODE_USE_VERTEX` |
| Microsoft Foundry | `CLAUDE_CODE_USE_FOUNDRY` |
| Claude Platform on AWS | `CLAUDE_CODE_USE_ANTHROPIC_AWS=1` |
| A proxy or gateway | `ANTHROPIC_BASE_URL`: changes where requests go, not which model answers [16][23] |

### Plugins and extensions

Skills, slash commands, subagents, hooks, MCP servers and plugins, all configured in files under `.claude/` and `~/.claude/` [18].

**Plugins**:
- **Layout.** `.claude-plugin/plugin.json` (only `name` is required), components at the plugin root: `skills/<name>/SKILL.md`, `commands/` (flat Markdown; skills are preferred for new plugins), `agents/`, `hooks/hooks.json`, `.mcp.json`, `.lsp.json`, `output-styles/`, `monitors/monitors.json`, `bin/` (on the Bash tool's `PATH` while the plugin is enabled) [13].
- **Install.** In a session `/plugin install <plugin>@<marketplace>` (opens the `/plugin` panel); from a shell `claude plugin install <plugin>[@<marketplace>] [--scope project|local]` (user scope by default). Other verbs: `list`, `enable`, `disable`, `uninstall`, `update`, `validate`, `details`, `marketplace` [9][14].
- **Marketplaces.** `/plugin marketplace add <owner/repo | git URL | path | https URL to marketplace.json>`, or `claude plugin marketplace add <source>`. Anthropic's official one is `claude-plugins-official`; interactive sessions register it on first start [14].
- **Scope and record.** `enabledPlugins` goes to `~/.claude/settings.json` (user), `.claude/settings.json` (project) or `.claude/settings.local.json` (local) [14]. Plugin files, cloned marketplaces and the `installed_plugins.json` record are under `~/.claude/plugins` [18]. One session only: `--plugin-dir <path>` or `--plugin-url <url>` [9].

**Other extensions**:
- **MCP servers.** `claude mcp add --transport http <name> <url>`; stdio: `claude mcp add [options] <name> -- <command> [args...]`; SSE is deprecated; WebSocket only through `claude mcp add-json`. Scopes: local (default) and user in `~/.claude.json`, project in `.mcp.json` at the project root [15]. Also `claude mcp list|get|remove|login|logout|serve` [9]. Plugin-bundled servers are named `mcp__plugin_<plugin>_<server>__<tool>` [15].
- **Skills, commands, agents.** Markdown with YAML frontmatter in `skills/`, `commands/` and `agents/` under a project's `.claude/` or under `~/.claude/` [18]. A skill runs through the `Skill` tool and adds no tool entry [12]. `--disable-slash-commands` turns skills and commands off; `--safe-mode` disables every customization [9].
- **Importing configuration.** `claude import [codex|gemini|cursor]` imports another agent's configuration (`--dry-run` previews) [9]. Configuration only, not sessions: mukade does the sessions.

### Hooks

**Where.** `~/.claude/settings.json`, `.claude/settings.json`, `.claude/settings.local.json`, managed policy settings, a plugin's `hooks/hooks.json`, skill frontmatter (while the skill is active) and subagent frontmatter (a `Stop` hook there becomes `SubagentStop`). Entries merge across levels. Structure: event → matcher group (`matcher`) → a `hooks` array of handlers [11].

**Events (33)** [11]:

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

**Handlers** [11]:
- Types: `command` (shell command), `http` (POSTs the JSON), `mcp_tool`, `prompt` (single-turn model check), `agent` (subagent; experimental). Common fields: `type`, `if`, `timeout`, `statusMessage`, `once`.
- Input: command hooks get the event JSON on stdin, HTTP hooks as the POST body. Common fields: `session_id`, `prompt_id`, `transcript_path`, `cwd`, `scratchpad_dir`, `permission_mode`, `effort`, `hook_event_name`; tool events add `tool_name`, `tool_input`, `tool_use_id`. Per-event fields beyond these were not read **[U]** (the page was fetched only in part).
- Exit codes: `0` success (JSON on stdout is applied); `2` a blocking error on events that can block, stderr fed back (ignored on `PermissionRequest`); any other code a non-blocking error. `onFailure: "block"` (v2.1.295+) makes a failure or timeout block.
- JSON output: `continue: false` stops Claude; `decision: "block"` with `reason` on prompt, post-tool, `Stop`, `PreCompact` and similar events; `hookSpecificOutput` carries `permissionDecision` (`allow`, `deny`, `ask`, `defer`) for `PreToolUse`, `additionalContext`, `updatedInput` and others.

**Web tools, skips, mukade**:
- A `PreToolUse` hook sees `tool_input.url` for `WebFetch` and can answer `permissionDecision: "deny"`. For `WebSearch` it sees `query`, `allowed_domains`, `blocked_domains` and an optional `mode` (`"standard"` or `"extended"`, v2.1.282+) [33].
- Hooks run in `-p` runs [31]. Whether a hook runs under `--restricted` is **[U]** (a `-p --restricted` run with a hook that logs its stdin would settle it). The `PostToolUse` `tool_response` shape for `WebSearch` is **[U]** (the same logging hook on a search would show it).
- `--bare` and `--safe-mode` skip hooks [9].
- mukade neither reads nor writes hook configuration.

## How mukade reads it

Code: `src/reader/claude_code.rs`, `src/reader/claude_code/order.rs`, `src/reader/claude_history.rs`, `src/reader/claude_live.rs`, `src/process.rs` [4].

**Limits, costliest first**:
- **Only the branch Claude Code resumes travels.** Rows off it are kept as `Meta(OffBranch)` and counted as `dropped_off_branch`; `--all-branches` is refused for Claude Code (`src/hop.rs` `all_branches_supported()`).
- **Subagent work does not travel.** `isSidechain` rows are kept, not conversation; `subagents/` folders are not listed.
- **A process killed while the model was answering leaves no sign**: that reply reads as an ordinary one, and `--interrupted drop` keeps it ([usage](../usage.md)).
- **Not exercised by a bench run:** auto-compaction by threshold, a second compaction, rewind or fork in a real source (branch selection is covered by synthetic tests on hand-made records), subagent sessions [1][4].
- **A title can be missed.** In a file bigger than the two peek windows, a `custom-title` written between them is not seen; the name falls back to the last prompt or the first one.

**Listing** (`src/reader/claude_code.rs` `list_cached()`, `peek()`):
- Reads `projects/*/*.jsonl`. Of each file it reads the **head and tail**, 128 KiB then 384 KiB (a file no bigger than the two is read whole; `src/reader/json.rs` `peek_ends()`), the way Claude Code's own picker does [9]: `sessionId`, `cwd`, timestamp, title.
- Title = the picker's: the last `custom-title`, else the `lastPrompt` of the last `last-prompt` row, else the first user prompt (never a sidechain's, a command's or a summary row), else the `slug` of a compact_boundary row. Each is folded to one line of at most 200 characters (`src/reader/json.rs` `title_line()`). A session of bookkeeping rows only (`/help`, `/exit`, their `<local-command-*>` output) has no prompt and stays untitled; Claude Code drops it from its own list.
- Cache: `mukade-data/cache/claude-list-<hash>.json`, keyed by path, size and modification time; a file modified in the last 2 s is never kept (`SETTLED_AFTER`). Changed files are read side by side.
- Speed (release build, 2026-11-08): before the cache, 11 s cold and 0.5 s warm for 214 files. With it, 285 ms for the first listing with the disk's own cache warm, then 65–72 ms (303 rows) [40]. A cold disk with an empty mukade cache was not measured again **[U]**.
- No archived flag in the store: every row is `"archived": false`, and `list --archived false` leaves nothing out (the same flag hides archived Codex and OpenCode sessions).

**Sessions without a transcript** (`src/reader/claude_code.rs` `add_what_the_transcripts_do_not_say()`, `src/reader/claude_history.rs` `sessions()`):
- A session `history.jsonl` names and no transcript holds is listed as `unreadable`, with the history file as its path, its `project` as `cwd`, its first prompt as title (a later one when the first is a slash command; at most 120 characters, `claude_history.rs` `TITLE_CHARS`) and its last prompt's time.
- A prompt without a `sessionId` (an older Claude Code) names a project, not a session: skipped.
- A running session with no transcript yet is listed, `unreadable` and `live`.
- `dump` of such an id says why it fails: known only from `history.jsonl`, or written no transcript yet (`why_not_found()`).

**`live`** (`src/reader/claude_live.rs` `registrations()`, `liveness()`; `src/process.rs` `inspect()`):
- `true`: a registration's process runs and its start time matches `procStart`, so a reused pid is not the session. Windows: within 2 s (`SAME_START`), by two Win32 calls; Linux: equal, from `/proc/<pid>/stat`, and a zombie counts as gone.
- `false`: the store has a `sessions/` folder and no registration for the session runs.
- `null`: no `sessions/` folder, or the registration's `pidDomain` is another platform, machine or pid namespace (a container).
- Verified live on Windows and on Linux (Claude Code 2.1.322, 2026-11-11: running → `true`, a changed `procStart` → `false`, another pid namespace → `null`) [41].
- macOS: `ps -p`, and `procStart` is not compared **[U]**: a reused pid reads as live.

**Reading one session** (`src/reader/claude_code.rs` `read()`, `parse_owned()`; `src/reader/claude_code/order.rs` `reconstruct()`):
- `dump` and `hop` match the file name by prefix of the id; an ambiguous prefix is refused.
- **Branch.** `last-prompt.leafUuid` is the checkpoint; a later continuation or a new user branch can supersede it. A new branch overrides only a checkpoint whose leaf was recorded before it; a checkpoint flushed before its own leaf is not a rewind ordering barrier. Mixed result/text user rows can start a new branch too. Without a checkpoint, the last non-sidechain user or assistant row with a UUID selects the branch. The reader walks `parentUuid`, and `logicalParentUuid` across a compaction.
- **Order and refusals.** Rows sharing `message.id` merge into one `Step`, including a response interleaved with its tool results; results pair by `tool_use_id` (L5). Recovered siblings and UUID-less blocks or results are placed by their dependencies, file position only breaking ties. Refused: unanchored conversation records, missing parents, cycles, duplicate UUIDs, ambiguous recovered results.
- **Compaction.** `compact_boundary` plus the `isCompactSummary` row become one `Compaction` entry (`summary`, `trigger`, `tokens_before`, `kept_from`). Preserved message lists and segments are checked; a retained UUID inside a merged response resolves to that response; a uniquely linked summary without a UUID can be recovered. Malformed, missing or contradictory preserved records are refused.
- A last line cut off mid-write is kept as an unfinished record.

**What rows become** (`src/reader/claude_code.rs` `class_of()`, `step_end()`):
- **Kept, never conversation:** `<synthetic>` assistant replies (`Meta(Synthetic)`); `isMeta`, `isVisibleInTranscriptOnly`, `<local-command-*>`, `<command-name>`, `<command-message>` rows (`Meta(Command)`); `attachment` rows; other `system` rows; unknown row types; subagent rows. Exception: the `isMeta` row after a prompt-type slash command is the expanded prompt, and becomes a user turn.
- **Interruptions.** `[Request interrupted by user]` (and `… for tool use]`) is read as a marker, not a prompt, and the reply it cut as `aborted`; `--interrupted drop` leaves that turn's work out.
- **Reasoning.** `thinking` keeps its `signature`; `redacted_thinking` keeps its `data` as the opaque payload; both are tagged `anthropic-messages` with the step's model.
- **Step end** from `stop_reason`: `tool_use`; `end_turn`, `stop_sequence`, `max_tokens`, `length` → completed; `refusal` → error.
- **Metadata** from the rows: `sessionId`, `cwd`, `gitBranch`, `version`, models; the title as in the listing; `parent` is always empty, as `--fork-session` leaves no lineage.

## How mukade writes it

Code: `src/writer/claude_code.rs` (`write_planned()`, `rows()`), `src/writer/plan.rs` [5].

**Limits, costliest first**:
- **Resume from the session's `cwd`.** From elsewhere Claude Code appends rows with the new `cwd` [1].
- **A `PowerShell` call travels as text form.** It is translated to Claude's `PowerShell` tool, but that tool is not in the offered set of 2.1.311 (`src/tools.rs` `offered()`), so the safety net in `src/tools.rs` `translate_with()` turns it to text, unless `--tool-map` binds it.
- **Signed thinking is resumed, not proven valid.** The payloads arrive byte for byte, but a plain `claude -p --resume` does not have the provider check earlier turns' thinking: a tampered copy resumes too [43].
- **Interrupted last turn.** On resume Claude Code inserts its synthetic `No response requested.`; the manifest records `interrupted_last_turn` [1].
- **Not exercised:** images and attachments toward Claude Code [1]. mukade does not create Claude Code's authentication or onboarding configuration [8].

**Where and how it is named**:
- File: `<CLAUDE_CONFIG_DIR>/projects/<project folder>/<new uuid>.jsonl`, the folder named by `src/roots.rs` `claude_project_dir_name()` (the `wL` rule). `--cwd` or the source's `cwd` is required. `--dest-id` reserves the id, `--dest-path` the file. No index file is written; the source is never touched.
- Version stamp: `--dest-version`, else the first version the resolved executable prints for `--version` [7].
- Resume command in the receipt: `claude -p --resume <id>`, the resolved executable in place of `claude` [5].
- **How `/resume` lists it.** The session ends as a native one does: `{"type":"custom-title","customTitle":…,"sessionId":…}` when it has a name (the source's, e.g. a Codex thread's name, or the caller's `WriteOpts.title`), then `{"type":"last-prompt","lastPrompt":…,"leafUuid":…,"sessionId":…}` naming the last user prompt (one line, at most 200 characters) and the last row. Claude Code 2.1.312's picker titles a session by its custom title, else that `lastPrompt`, else its first user prompt (read from its own code, 2026-10-28) [9]. Without those rows a session written from Codex was listed under the AGENTS.md text Codex injects as the first user message, and could not be told apart.

**Rows**:
- One row per content block; all rows of a step share a fresh `msg_<uuid>` `message.id`, chained by `parentUuid` in file order, **except** each `tool_result` row's parent is its own `tool_use` row. Otherwise Claude Code substitutes `[Tool result missing due to internal error]` (spec §6.2).
- Conversation rows carry `isSidechain: false`, `sessionId`, `cwd`, a monotone timestamp, and `version` when the destination version is known.
- Orphan and duplicate results, empty steps and unanswered calls are dropped or moved to text, and counted in the manifest [5].

**Content**:
- **Reasoning.** A block with only an encrypted payload is written `{"type":"redacted_thinking","data":…}`; one with a signature as `thinking` with its `signature`. Under the default `DropForeign` policy a payload is carried only when it came from the same protocol and is unmodified; otherwise it is dropped or written as labelled text, and the manifest says which [5][7].
- **Tool ids** are rewritten to `^[a-zA-Z0-9_-]+$`, as Anthropic destinations require: each other character becomes `_`, a collision gets `_2`, `_3`…, an empty id becomes `toolu_mukade`. The call and its result get the same id; counted as `rewritten_call_ids` [5] (`src/writer/plan.rs` `CallIds`).
- **Tools.** Calls become Claude's own `Bash`, `Read`, `Write`, `Edit`, `Grep`, `Glob`, `WebFetch`, `WebSearch` when equivalent. Subagent, skill, question, MCP and harness-internal calls travel as text form, following the tables [3][7]. Result content is flattened to text; non-text blocks are dropped and counted (`dropped_result_blocks`). `--without-tools` leaves out every historical call and result.

**Compaction**:
- `--compaction portable` writes the source's own summary as a user message (`[mukade handoff v1] …`), then the tail [1][7].
- `--compaction native` (hop only; needs `--context-window`) keeps every generated row and, when the session's estimate exceeds the budget, appends Claude's own pair before the picker rows: `system/compact_boundary` with `preservedSegment`/`preservedMessages`, then an `isCompactSummary` user row holding a deterministic summary (`src/native_compaction.rs` `plan_native_items()`).
- Verified live on Claude Code 2.1.318 (2026-11-04): an oversized session resumed, answered from the summary, and added no second `compact_boundary` (spec §4.4) [42].

## How mukade summarizes it

With `--compaction summary` and Claude Code as the source, mukade runs (`src/summary.rs` `summary_command()`):

```
claude -p --tools "" --resume <id> --no-session-persistence --output-format json [--model <id>] <prompt>
```

- `--tools ""` comes before `--resume`, so it cannot swallow the prompt.
- It runs in the session's directory (mukade's own when that is gone), with `CLAUDE_CONFIG_DIR` set to the source store.
- The model: `--summary-model`, else the session's own last model (never `<synthetic>`). Deadline: `--summary-timeout` (default 300 s).
- The summary is the JSON `result`; `is_error: true` or a `subtype` other than `success` is a failed summary (`src/summary.rs` `parse_reply()`). `total_cost_usd`, `usage` and `modelUsage` go to the manifest's `options.summary.usage` [5][27].
- The transcript is left as it was; the user's SessionStart hooks still run [27]. If the executable is a Windows `.cmd` launcher, the prompt must be one line of ASCII letters, digits, spaces and `.,;:()`, or mukade refuses before running.

## Open points

1. **Real-life shapes** **[U]**: auto-compaction by threshold, a second compaction, a rewind or fork in a real source, subagent sessions, an image going to Claude Code. Settled by hopping one when it happens.
2. **Signed thinking really validated** **[U]**: needs a source whose last assistant turn is a tool call with thinking; then the tampered copy must be refused [43].
3. **`PowerShell` in Claude's offered set**: take the destination's shell and tools from its settings and platform, and add `PowerShell` where it is enabled (to do, not [U]).
4. **macOS `live`** **[U]**: `procStart` is not compared there. Needs a macOS machine.
5. **Listing on a cold disk with an empty cache** **[U]**: the 11 s cold figure was not reproduced.

The rest, with the Windows checklist steps: [../../next.md](../../next.md). Every other **[U]** above names the run that would settle it.

## Sources

All accessed 2026-10-28 unless a source says otherwise.

Local (this repository and this machine):

1. `docs/session-interchange-spec.md` §0, §2.1, §3, §4, §5, §6, §7, §8, §10 (Claude Code 2.1.311 verified records and commands).
2. `docs/reference/harness-session-persistence-reference.md` (Claude Code dossier, secondary research report).
3. `docs/tool-conversion-tables.md` §2 to §4 (tools offered and used, name and argument tables).
4. `src/reader/claude_code.rs`, `src/reader/claude_code/order.rs`, `src/reader/claude_history.rs`, `src/reader/claude_live.rs`, `src/process.rs`.
5. `src/writer/claude_code.rs`, `src/writer/plan.rs`.
6. `src/exe.rs`, `src/roots.rs` (`claude_project_dir_name`), `catalog/harnesses.json`.
7. `src/policy.rs`, `src/detect.rs`, `src/detect/claude_code.rs`, `src/hop.rs`, `src/cli.rs`, `src/summary.rs`, `src/tools.rs`.
8. `README.md`, `docs/usage.md`.
9. Installed binary `C:\Users\alfio\.local\bin\claude.exe`: `claude --version` (2.1.312), `claude --help`, `claude plugin --help`, `claude mcp --help`, `claude import --help`, `claude install --help`, `claude agents --help`, `claude auth --help`. Also its own JavaScript, embedded in `claude.exe`, read on 2026-10-28: the `/resume` list is built from each session file's head and tail (first prompt, `lastPrompt`, `customTitle`, `aiTitle`), and drops sessions that are sidechains, team or daemon sessions, `/loop` sessions, of some entrypoints, superseded ("continued in"), recorded under another directory with the same project slug, or made of bookkeeping rows only (no `"parentUuid":`).

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
40. `docs/verification/listing-coverage-2026-11-08.md`: listing coverage and speed, `history.jsonl` and `sessions/<pid>.json` on Windows with Claude Code 2.1.319.
41. `docs/verification/linux-2026-11-11.md`: `live` on Linux, Claude Code 2.1.322.
42. `docs/verification/native-compaction-live.md`: native compaction records resumed live, Claude Code 2.1.318 (2026-11-04).
43. `docs/verification/live-hops-and-provider-reasoning.md`: live hops into Claude Code 2.1.312 with signed and redacted thinking, and the tampered-signature control (2026-10-28).
