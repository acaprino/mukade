# Pi

| | |
|---|---|
| **Documentation updated** | 2026-10-28 |
| **Version described** | 1.2.0 (the version mukade's readers and writers were built against); mukade-written sessions were also run live against 1.4.0, npm's `latest` on 2026-10-28 — see [`../verification/2026-10-28-pi-live.md`](../verification/2026-10-28-pi-live.md) |
| **Vendor** | Earendil Inc. ("Earendil Inc. & Contributors") [24]; package author Mario Zechner [1] |
| **Website** | <https://pi.dev>, documentation <https://pi.dev/docs/latest> [24] |
| **License / source** | MIT; <https://github.com/earendil-works/pi>, package directory `packages/coding-agent` [1][21] |
| **mukade name** | `pi` |

## 1. Introduction

Pi is a minimal, extensible terminal coding agent, published on npm as `@earendil-works/pi-coding-agent`
and run as `pi` [1][2]. It ships four default tools, leaves sub-agents and plan mode to extensions and
packages [2], and stores each session as a JSONL tree of entries [6]. Version 1.2.0 was published on
2026-10-12; on 2026-10-28 npm's `latest` was 1.4.0, which mukade has not verified [22][23].

**Which project this is.** The installed package's `package.json` names the repository
`git+https://github.com/earendil-works/pi.git` (directory `packages/coding-agent`), the binary `pi` and
the config directory `.pi` [1], and `pi --version` prints `1.2.0` on this machine [5]. *Oh My Pi* is a
different project: a fork at `github.com/can1357/oh-my-pi`, published as `@oh-my-pi/pi-coding-agent`,
with the command `omp` and its state under `~/.omp` [25][27]. mukade does not read or write Oh My Pi.

## 2. Description

- **Agent loop.** A submitted message joins the *active branch*. Pi builds a request from the system
  prompt, that branch, the tools and the model settings, streams the reply (text and tool calls),
  records it, runs each tool call and records the results: one turn. It starts another turn when tool
  results or queued messages need one; an abort stops the run [3].
- **Provider layer.** Models are called through `@earendil-works/pi-ai` ("Unified LLM API with automatic
  model discovery and provider configuration"); the tool-calling runtime is `pi-agent-core`, beside
  `pi-tui`, `pi-mcp` and `pi-codemode`, all `^1.2.0` [1][19][21].
- **Minimal tool set.** `read`, `bash`, `edit`, `write` on by default; `grep`, `find`, `ls` built in,
  read-only and off by default [4][5][11].
- **Modes.** Interactive terminal UI, print, JSON event stream, RPC, and the in-process TypeScript SDK,
  all on the same agent and session code [3].

## 3. Entry points

| entry point | exact command | notes |
|---|---|---|
| interactive | `pi`, `pi "List all .ts files in src/"` | full-screen TUI by default (`--tui-mode fullscreen\|regular`) [5] |
| print | `pi -p "<prompt>"`, `git diff \| pi --print "Review this change"` | writes the final assistant text and exits; also chosen when stdin or stdout is redirected [4] |
| JSON events | `pi --mode json "<prompt>" > events.jsonl` | JSONL events of one run [4] |
| continue | `pi --continue` (`-c`), `pi --resume` (`-r`) | the current project's most recent session, or a picker [4][7] |
| a given session | `pi --session <path\|id>` | path, exact ID or partial ID; searches the current project first and offers to fork a match from another project [4] |
| other session flags | `--session-id <id>`, `--fork <path\|id>`, `--session-dir <dir>`, `--no-session`, `--name <name>` | `--fork` cannot be combined with `--session`, `--continue`, `--resume` or `--no-session` [4] |
| RPC | `pi --mode rpc --no-session`, `pi --mode rpc --session <id>` | JSONL commands on stdin, e.g. `{"id":"c1","type":"compact"}`, `{"type":"abort"}`; records split only on LF [17][26] |
| SDK | `import { createAgentSession } from "@earendil-works/pi-coding-agent";` | `const { session } = await createAgentSession(); await session.prompt("…")` [18] |

RPC stdin must be UTF-8 **without** a BOM. In print mode `/compact` is sent to the model as text, so
compaction from outside Pi goes through the RPC `compact` command [26].

## 4. Harness type

An open-source coding-agent CLI with a TypeScript SDK and an RPC subprocess interface; it requires
Node.js `>=22.19.0` (`engines`) [1].

### Vendor

The pi.dev footer credits "Earendil Inc. & Contributors" [24]. `package.json` names Mario Zechner as
author [1]; the npm maintainers are `mitsuhiko`, `badlogic` and `rwachtler` [23].

### Website and documentation

Site <https://pi.dev>, docs <https://pi.dev/docs/latest>, package gallery <https://pi.dev/packages>
[15][24]. The `/latest` docs follow the newest release and were not compared with 1.2.0 **[U]**; this
page uses the docs shipped in the installed 1.2.0 package (`docs/*.md`) [3]. Releases:
<https://github.com/earendil-works/pi/releases> (tag `v1.2.0`, 12 Oct, commit `06a4669`) [22].

### Installation

| OS | method | exact command |
|---|---|---|
| macOS, Linux | installer (pins every dependency, updates with `pi update`) | `curl -fsSL https://pi.dev/install.sh \| sh` [2][20] |
| Windows | installer | `powershell -c "irm https://pi.dev/install.ps1 \| iex"` [2][24] |
| any, Node.js ≥ 22.19 | npm (does not pin transitive dependencies) | `npm install -g --ignore-scripts @earendil-works/pi-coding-agent` [2][20] |
| any | pnpm / Bun | `pnpm add -g --ignore-scripts @earendil-works/pi-coding-agent` · `bun add -g --ignore-scripts @earendil-works/pi-coding-agent` [24] |
| macOS, Linux | Nix (built from source) | `nix profile add github:earendil-works/pi/stable` (older Nix: `nix profile install`); update with `nix profile upgrade pi`; pin with a tag such as `github:earendil-works/pi/v1.0.0` [20] |
| Android | Termux | `pkg update && pkg upgrade`, `pkg install nodejs git`, then the npm command [20b] |

The installers can install Node.js when needed [2]. For the version mukade verified, add npm's usual
version suffix (`…/pi-coding-agent@1.2.0`); check any install with `pi --version` [20]. On native
Windows the `bash` tool needs a Bash: Pi tries `shellPath` in `settings.json`, then Git Bash under
`Program Files`, then `bash.exe` on `PATH` [12].

### Licence

MIT, in `package.json`, the README and the repository, a monorepo that also holds `pi-ai`,
`pi-agent-core` and `pi-tui` [1][2][21].

## 5. Features

### Session store

- **Path.** `<agent-dir>/sessions/--<path>--/<timestamp>_<session-id>.jsonl`. The agent directory
  defaults to `~/.pi/agent`; `PI_CODING_AGENT_DIR` moves it. Session storage alone can move too:
  `--session-dir` beats `PI_CODING_AGENT_SESSION_DIR`, which beats the `sessionDir` setting [6][7][16].
- **Names.** `getDefaultSessionDirPath` takes the resolved working directory, drops one leading `/` or
  `\`, and turns every `/`, `\` and `:` into `-`; the file timestamp is the ISO instant with `:` and `.`
  turned into `-` [6][10]. The session id is a UUID unless the SDK or `--session-id` supplies one [6].
- **Header.** `{"type":"session","version":3,"id","timestamp","cwd"}`, plus `"parentSession"` after
  `/fork` or `/clone`; not part of the tree. Version 1 was linear, 2 added `id`/`parentId`, 3 renamed
  the `hookMessage` role to `custom`; older files are migrated on load (`CURRENT_SESSION_VERSION = 3`) [6][10].
- **Entries.** Each has `type`, `id` (usually 8 hex characters), `parentId` (`null` at a root) and an
  ISO `timestamp`; a `message`'s own `timestamp` is in milliseconds [6][10].

| entry `type` | content | reaches the model |
|---|---|---|
| `message` | an `AgentMessage` with role `system` (prompt `sections` plus `toolsAdded`/`toolsRemoved`), `user`, `assistant` (`content` of `text`/`thinking`/`toolCall`, plus `api`, `provider`, `model`, `usage`, `stopReason`), `toolResult`, `bashExecution` or `custom` | yes |
| `compaction` | `summary`, `firstKeptEntryId`, `tokensBefore`, `systemMessage?`, `usage?`, `details?`, `fromHook?` | yes, as the summary |
| `branch_summary` | `summary`, `fromId`, `usage?`, `details?`, `fromHook?` | yes, as a user message |
| `custom_message` | `customType`, `content`, `display`, `details?` | yes, as a user message |
| `context_edit` | `targetId`, `replacement` (`null` drops the target) | changes what the target contributes |
| `model_change`, `thinking_level_change` | `provider`/`modelId`, `thinkingLevel` | settings only |
| `custom`, `label`, `session_info`, `usage` | extension state, bookmarks, the display name, usage not tied to a message | no |

Sources for the table: [6][9]. A `bashExecution` message records a `!cmd` the user typed; Pi gives it
to the model as user text starting ``Ran `<command>` ``, unless `excludeFromContext` is set (the `!!`
prefix). The coding agent adds the roles `bashExecution`, `custom`, `branchSummary`, `compactionSummary` [9].

### Compaction

- **Trigger.** Automatic when `contextTokens > contextWindow - reserveTokens` (default 16384); manual
  with `/compact [instructions]` or the RPC `{"type":"compact"}` (optional `customInstructions`) [8][17].
  Settings: `compaction.enabled`, `reserveTokens`, `keepRecentTokens` (default 20000), with overrides
  per `provider/modelId` [8].
- **Entry.** Pi walks back until `keepRecentTokens` is reached, has the model summarize what lies before,
  and appends a `compaction` entry whose `firstKeptEntryId` names the first entry kept (a compaction
  that keeps nothing stores its own id). A cut never falls on a tool result; the original entries stay
  in the file [6][7][8].
- **Rebuilt context.** `buildContextEntries()` walks from the leaf to the root and uses the latest
  compaction on the path: the compaction first, then the non-system entries from `firstKeptEntryId` up
  to it, then everything after it [6]. The model receives the system checkpoint, a user message
  `The conversation history before this point was compacted into the following summary:\n\n<summary>\n…\n</summary>`,
  the kept tail and the new prompt [9][26].
- **Without a terminal** compaction is the RPC `{"type":"compact"}` (`pi --mode rpc --session <id>`); its entry is appended to that session's file [17][26].

### Ephemeral runs

- **`--no-session`** writes no session, but cannot be combined with `--session`, `--continue`,
  `--resume` or `--fork` [4].
- **`--fork <path|id> --session-dir <dir>`** copies the session into a new file under `<dir>`; the
  source file and its folder stay byte-identical [34]. Delete `<dir>` afterwards and nothing is left.
- **`--no-tools`** offers no tools; a history holding tool calls is accepted (`openrouter`) [34].
- **Output.** `-p` prints the final assistant text on stdout [4].

### Built-in tools

Parameter schemas from the installed 1.2.0 source (`dist/core/tools/*.js`) [11]:

| tool | enabled | parameters |
|---|---|---|
| `read` | default | `path`, `offset?` (first line, 1-indexed), `limit?` (lines) |
| `bash` | default | `command`, `timeout?` (**seconds**, no default) |
| `edit` | default | `path`, `edits: [{oldText, newText}]`; each `oldText` is unique in the original file and does not overlap another edit; all edits match the original, not each other's output |
| `write` | default | `path`, `content` |
| `grep` | off | `pattern`, `path?`, `glob?`, `ignoreCase?`, `literal?`, `context?` (default 0), `limit?` (default 100) |
| `find` | off | `pattern` (glob), `path?`, `limit?` (default 1000) |
| `ls` | off | `path?`, `limit?` (default 500) |
| `powershell` | off, native Windows only | the `bash` schema (`command`, `timeout?` in seconds); runs `pwsh.exe`, else Windows PowerShell, with `-NoProfile -NonInteractive -ExecutionPolicy Bypass` [11][12] |

The default set is `DEFAULT_TOOL_NAMES = ["read", "bash", "edit", "write"]` [11]. Change it with `--tools`
(e.g. `pi --tools read,grep,find,ls -p "…"`, or `+name`/`-name` entries) or `defaultTools` (e.g.
`{"defaultTools": ["read", "powershell", "edit", "write"]}`) [4][12]. Outside the built-in set there are
`codemode` (`--tools +codemode`), `tool_search`, MCP tools `mcp__<server>__<tool>` and extension tools
[4][5]. How mukade translates them: [`../tool-conversion-tables.md`](../tool-conversion-tables.md).

### Tree, branches and forks

- `/tree` moves within the current file. Selecting a user message puts its text back in the editor and
  submitting it starts a new branch; selecting any other entry continues after it. The abandoned branch
  stays in the file [7].
- On leaving a branch Pi can append a model-written `branch_summary`: `parentId` is the entry the new
  branch continues from, `fromId` the old leaf [6][8]. The model sees it as the user message
  `The following is a summary of a branch that this conversation came back from:\n\n<summary>\n…</summary>` [9].
- `/fork` creates a new session from an earlier user message; `/clone` copies the active branch into a
  new session. Both set `parentSession` in the new header [6][7].
- **Resume.** On load `_buildIndex` makes the file's last non-header entry the leaf;
  `buildSessionPath(entries, leafId)` walks `parentId` back from it (falling back to the last entry) and
  reverses the result. The context is the path that ends at the file's last entry [10][26].

### Models and providers

- pi-ai's `KnownApi` protocols: `openai-completions`, `mistral-conversations`, `openai-responses`,
  `azure-openai-responses`, `openai-codex-responses`, `anthropic-messages`, `bedrock-converse-stream`,
  `google-generative-ai`, `google-vertex`, `pi-messages`; `Api = KnownApi | (string & {})` leaves room
  for others. Known providers include `anthropic`, `openai`, `google`, `amazon-bedrock`, `openrouter` [19].
- `<agent-dir>/models.json` declares compatible endpoints:
  `{"providers": {"<name>": {"baseUrl", "api", "apiKey", "models": [{"id"}]}}}`; `apiKey` accepts `$NAME`,
  `${NAME}`, a literal or `!command`. Credentials: `--api-key`, then `auth.json`, then `models.json`,
  then environment variables [19]. `/login` signs in to a built-in provider [2].
- `--provider <p> --model <m>` or `--model provider/id:<thinking>`; thinking levels `off`, `minimal`,
  `low`, `medium`, `high`, `xhigh`, `max` [5]. Assistant messages record `api`, `provider`, `model` [6];
  thinking blocks keep their `thinkingSignature` [26].

## 6. Plugins and extensions

| mechanism | where it lives | how it is added |
|---|---|---|
| extensions (TypeScript/JavaScript, loaded with `jiti`) | `<agent-dir>/extensions/`, `.pi/extensions/` (a file, or a folder with `index.ts`/`index.js`) | drop it in the folder; for one run `pi --extension ./hello.ts` (`-e`); off with `--no-extensions` [5][13][16] |
| custom tools | inside an extension: `pi.registerTool()` with name, description, TypeBox schema, `execute()` | through the extension [13] |
| skills ([Agent Skills](https://agentskills.io/specification) folders with `SKILL.md`) | `<agent-dir>/skills/`, `.pi/skills/`, `~/.agents/skills/`, `.agents/skills/` | the folder or `--skill <path>`; run with `/skill:name`; off with `--no-skills` [14] |
| prompt templates, themes | `prompts/` and `themes/` under `<agent-dir>` and `.pi` | the folder, `--prompt-template`, `--theme` [5][16] |
| Pi packages (bundles of the above) | npm, git or a local path, declared in `settings.json` | `pi install npm:@example/pi-tools@1.0.0`, `pi install git:github.com/example/pi-tools@v1`, `pi install ./local-package`; `-l` writes `.pi/settings.json`; manage with `pi list`, `pi remove <source>`, `pi update --extensions`, `pi config` [15] |
| MCP servers | `<agent-dir>/mcp.json`, `.pi/mcp.json` | `pi mcp add <server> [options] -- <command> [args...]`; check with `pi mcp list`; off with `--no-mcp` [4][5] |

Project resources (`.pi/…`) load only after project trust is granted; `--approve` / `--no-approve`
decide it for one run [5][16]. Extensions run inside the Pi process with its permissions [13].

## 7. Hooks

Pi's hooks are extension event handlers, `pi.on(event, handler)`, run in load order; some only notify,
others transform data, replace results or cancel an operation [13]. The legacy field `fromHook` and the
v3 rename of `hookMessage` to `custom` show that extensions took over an earlier hook API [6][8]. No
setting that runs shell commands as hooks (like Claude Code's `hooks`) appears in the 1.2.0 docs **[U]**.

Events declared in `dist/core/extensions/types.d.ts` [13]:

| group | events |
|---|---|
| resources, trust | `project_trust`, `resources_discover`, `mcp_servers_change` |
| session | `session_start`, `session_shutdown`, `session_before_switch`, `session_before_fork`, `session_before_tree`, `session_tree`, `session_before_compact`, `session_compact`, `session_compact_failed`, `session_info_changed` |
| run and turn | `input`, `before_agent_start`, `agent_start`, `turn_start`, `turn_end`, `agent_end`, `agent_before_settle`, `agent_settled` |
| messages, context | `message_start`, `message_update`, `message_end`, `context`, `context_with_system` |
| provider, model | `before_provider_request`, `before_provider_headers`, `provider_stream_event`, `after_provider_response`, `model_select`, `thinking_level_select`, `cache_warming_decision` |
| tools, shell | `tool_call`, `tool_execution_start`, `tool_execution_update`, `tool_execution_end`, `tool_result`, `user_bash` |
| UI | `ui_prompt_start`, `ui_prompt_end` |

`tool_call` can change a call's input or block it; `tool_result` handlers compose; `message_end` can
replace a finished message; `turn_end` and `agent_before_settle` can append `custom`, `custom_message`,
`context_edit` or `compaction` entries and request one more model call with `continue: true` [13].
`session_before_compact` and `session_before_tree` let an extension write the summary itself [8].

## 8. How mukade reads and writes it

| | |
|---|---|
| CLI name | `pi` (`mukade list pi`, `mukade dump pi <id>`, `mukade hop <src> <id> pi …`; examples in [`../usage.md`](../usage.md)) [32] |
| store root | `PI_CODING_AGENT_DIR`, required even for a standard install; sessions are read and written under `<PI_CODING_AGENT_DIR>/sessions` [31][32] |
| executable | `--pi-exe` → `MUKADE_PI_EXE` → `pi` on `PATH` (with `PATHEXT`, so npm's `pi.cmd` is found) → `%APPDATA%\npm\pi.cmd`, `%APPDATA%\npm\pi` → bare `pi` [31] |
| store detection | `sessions/--*--/*.jsonl` and Pi's `models.json` [32] |

mukade ignores `PI_CODING_AGENT_SESSION_DIR`, the `sessionDir` setting and `--session-dir`:
`StoreRoots::pi_sessions()` is always `<PI_CODING_AGENT_DIR>/sessions`, so sessions Pi was told to store
elsewhere are not found [31].

**Reading** [29]:
- **Listing.** `list` returns every `*.jsonl` under `sessions/`, in any project folder; the id is the
  part of the file name after the last `_`. `read <id>` accepts the file stem, a stem ending in `_<id>`,
  or a UUID prefix; several matches are refused as ambiguous.
- **Active branch.** The reader follows `parentId` from the last record that has an `id` and is not the
  header, as Pi does [10]. Records off that path are kept verbatim as `Meta(OffBranch)`, not resumed.

| Pi record | interchange entry |
|---|---|
| `message` `user` | `User` |
| `message` `assistant` | `Step`: thinking (signature kept, protocol from `api`), text, all `toolCall`s; `stopReason` `toolUse` → `ToolUse`, `aborted` → `Aborted`, `error` → `Error`, any other → `Completed` |
| `message` `toolResult` | consecutive results gathered into one `Results` |
| `message` `bashExecution` | `User` with Pi's own text (`bashExecutionToText`); `Meta` when `excludeFromContext` |
| `branch_summary` | `User` with `BRANCH_SUMMARY_PREFIX` + summary + `</summary>` (`Meta` if empty) |
| `custom_message` | `User` from its `content` |
| `compaction` | `Compaction{summary, kept_from: firstKeptEntryId, tokens_before}` |
| `message` `system` · `model_change`, `thinking_level_change` | `Meta(SystemPrompt)` · `Meta(ModelChange)` |
| `context_edit` on the resumed path | applied, as Pi's context builder does: the latest edit of a target wins; the target keeps its role and takes the replacement's content, or is left out (`Meta(Internal)`) when the replacement is `null`; its record stays verbatim in `raw` |
| anything else (`custom`, `label`, `session_info`, `usage`) | `Meta(Internal)` |

- **Bash timeout and tool names.** `bash` `timeout` is converted from seconds to the canonical
  milliseconds (×1000, rounded); the raw record keeps the original. `from_pi` maps `read`, `write`,
  `edit`, `bash`, `grep`, `find` (→ `Glob`), `ls`, `subagent` (→ `Agent`); any other name, `powershell`
  included, stays as it is [33].

**Writing** [30]:
- **File.** `<PI_CODING_AGENT_DIR>/sessions/--<dir>--/<stamp>_<uuid>.jsonl`, `<dir>` as in
  `getDefaultSessionDirPath` (resolved path, one leading separator dropped, only `/`, `\`, `:` → `-`):
  `C:\work\my-project` becomes `--C--work-my-project--`. The stamp is UTC with milliseconds,
  e.g. `2026-10-27T16-00-39-689Z`.
- **Rows.** The header `{"type":"session","version":3,"id","timestamp","cwd"}`, then `message` rows only,
  each with a new 8-hex `id` chained by `parentId` (`null` on the first). No system message is written;
  Pi adds one on the first request of a session that has none [6].
- **Assistant messages.** Thinking with its `thinkingSignature` only when the reasoning policy carries it
  natively, then text, then all the step's calls in this one message (L3). `api` comes from
  `--dest-protocol`; `provider` and `model` from `--model-provider` / `--model`, else the source.
  `stopReason` keeps `aborted` and `error`, else `stop` (no calls) or `toolUse`. `usage` has zero
  counters: Pi refuses a session without it (`Cannot read properties of undefined (reading 'totalTokens')`).
- **Results.** One `toolResult` per result, in call order, with the call's `toolName`; a call carried as
  text is answered by a `user` text message in the same place; `bash` timeouts go back to seconds (L12) [28].
- **Compaction.** `Portable` writes the summary as a user message, not a native `compaction` entry;
  whether Pi accepts a hand-written `compaction` entry is **[U]**. `Compaction::DestinationNative` is
  refused with `Unsupported` (`src/hop.rs:353`): Pi's own compaction is the caller's to run, as
  `pi --mode rpc --session <id>` with stdin `{"id":"c1","type":"compact"}` [26].
- **Resume.** The receipt's command is `[<--pi-exe or "pi">, "-p", "--session", "<file path>"]`: the file
  path, because `--session <id>` searches the current project first. Run it from the session's working
  directory with `PI_CODING_AGENT_DIR` set [26][30][32].
- **Isolated runs.** mukade's bench runs Pi with `--offline --no-skills` and no provider API-key
  variables; `--no-skills` is needed because Pi reads `~\.agents\skills` whatever `PI_CODING_AGENT_DIR`
  says [26].

### How mukade summarizes it

With `--compaction summary` and Pi as the source, mukade runs
`pi -p --fork <session file> --session-dir <folder> --no-tools [--provider <p>] [--model <id>] <prompt>`,
where `<folder>` is its own under `mukade-data/tmp`, removed afterwards, and reads the summary from
stdout. The provider and the model id go apart, because a model id can hold a slash itself
(`moonshotai/kimi-k2.6`). A Pi session read from a file (`--from-file`) can be summarized too [34].

## 9. Sources

`<pkg>` = `C:\Users\alfio\AppData\Roaming\npm\node_modules\@earendil-works\pi-coding-agent` (1.2.0);
repository paths are relative to `D:\Projects\mukade`. All accessed 2026-10-28.

1. Pi `package.json`, `<pkg>\package.json`.
2. Pi README, `<pkg>\README.md`.
3. "How Pi Works" and the docs index, `<pkg>\docs\how-pi-works.md`, `<pkg>\docs\index.md`.
4. "Command Line", `<pkg>\docs\cli.md`.
5. Output of `pi --version` and `pi --help` (Pi 1.2.0, this machine).
6. "Session File Format", `<pkg>\docs\session-format.md`.
7. "Sessions and Context", `<pkg>\docs\sessions.md`.
8. "Compaction Reference", `<pkg>\docs\compaction.md`.
9. "Message Types", `<pkg>\docs\message-types.md`; `<pkg>\dist\core\messages.js` (`convertToLlm`, `bashExecutionToText`, `COMPACTION_SUMMARY_PREFIX`, `BRANCH_SUMMARY_PREFIX`).
10. `<pkg>\dist\core\session-manager.js` (`getDefaultSessionDirPath`, `buildSessionPath`, `_buildIndex`, `CURRENT_SESSION_VERSION`, `timestamp.replace(/[:.]/g, "-")`).
11. `<pkg>\dist\core\tools\{read,bash,edit,write,grep,find,ls,powershell}.js`; `<pkg>\dist\core\settings-manager.js` (`DEFAULT_TOOL_NAMES`).
12. "Run Pi on Windows", `<pkg>\docs\windows.md`.
13. "Extensions", `<pkg>\docs\extensions.md`; `<pkg>\dist\core\extensions\types.d.ts`.
14. "Skills", `<pkg>\docs\skills.md`.
15. "Pi Packages", `<pkg>\docs\packages.md`.
16. "Configuration" and "Environment Variables", `<pkg>\docs\configuration.md`, `<pkg>\docs\environment-variables.md`.
17. "RPC Mode" and "RPC Commands", `<pkg>\docs\rpc.md`, `<pkg>\docs\rpc-commands.md`.
18. "SDK", `<pkg>\docs\sdk.md`.
19. "Choose a Model", `<pkg>\docs\models.md`; `@earendil-works/pi-ai` 1.2.0, `<pkg>\node_modules\@earendil-works\pi-ai\package.json` and `…\dist\types.d.ts` (`KnownApi`, `KnownProvider`).
20. "Quickstart", `<pkg>\docs\quickstart.md`. (20b) "Termux Setup", `<pkg>\docs\termux.md`.
21. earendil-works/pi, <https://github.com/earendil-works/pi>, accessed 2026-10-28.
22. earendil-works/pi releases, <https://github.com/earendil-works/pi/releases>, accessed 2026-10-28.
23. npm registry metadata, <https://registry.npmjs.org/@earendil-works/pi-coding-agent> (`time["1.2.0"]` = 2026-10-12T23:03:25Z, `dist-tags.latest` = 1.4.0, maintainers), accessed 2026-10-28; the page <https://www.npmjs.com/package/@earendil-works/pi-coding-agent> refused the fetch (HTTP 403).
24. Pi website, <https://pi.dev>, accessed 2026-10-28.
25. can1357/oh-my-pi, <https://github.com/can1357/oh-my-pi>, accessed 2026-10-28.
26. `docs/session-interchange-spec.md` §2.4, §4, §5.
27. `docs/reference/harness-session-persistence-reference.md`, entries "pi" and "Oh My Pi".
28. `docs/tool-conversion-tables.md`.
29. `src/reader/pi.rs`.
30. `src/writer/pi.rs`; `src/writer/plan.rs` (`provider_id`, `model_id`, `protocol_name`).
31. `src/exe.rs`, `src/roots.rs`, `src/cli.rs`.
32. `README.md`, `docs/usage.md`.
33. `src/model/tool.rs` (`CanonicalTool::from_pi`).
34. `docs/verification/2026-10-28-summary-live.md`: the live check of 2026-10-28 (throwaway sessions, source hashed before and after; Codex against a local mock)
