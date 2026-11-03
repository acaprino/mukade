# Pi

| | |
|---|---|
| **Documentation updated** | 2026-11-01 |
| **Version described** | 1.2.0 (the version mukade's readers and writers were built against); mukade-written sessions were also run live against 1.4.0, npm's `latest` on 2026-10-28 — see [`../verification/pi-session-and-reasoning-verification.md`](../verification/pi-session-and-reasoning-verification.md) |
| **Vendor** | Earendil Inc. ("Earendil Inc. & Contributors") [24]; package author Mario Zechner [1] |
| **Website** | <https://pi.dev>, documentation <https://pi.dev/docs/latest> [24] |
| **License / source** | MIT; <https://github.com/earendil-works/pi>, package directory `packages/coding-agent` [1][21] |
| **mukade name** | `pi` |

Version note: the tool names, the default tool set and the tool flags listed below were re-checked on 2026-11-01 against `pi --help` of the installed 1.4.0, which is npm's `latest` [36][37]; the default set is `DEFAULT_TOOL_NAMES` in `dist\core\tools\index.js:35` [43]. No tool name changed. The parameter schemas in the table of "Built-in tools" are still those read in 1.2.0 (only `read`'s `path`, `offset` and `limit` were seen again at 1.4.0 [43]); whether the others differ is **[U]**, and reading `<pkg>\dist\core\tools\*.js` of 1.4.0 settles it.

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
| tool selection | `pi --tools read,grep,find,ls -p "<prompt>"`; also `--exclude-tools <names or * patterns>` (`-xt`), `--no-builtin-tools` (`-nbt`), `--no-tools` (`-nt`), `--no-mcp`, `--no-extensions` (`-ne`) | narrow the tools of one run; the rules for `--tools` and MCP tools are under "Built-in tools" [37][39] |
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

### Canonical locations and finding an existing install

| what | Windows | macOS / Linux |
|---|---|---|
| Managed-install launcher | `%USERPROFILE%\.pi\agent\bin\pi.cmd` | `~/.pi/agent/bin/pi` |
| Managed releases | `%USERPROFILE%\.pi\agent\install\releases\`, selected by `current-version` | `~/.pi/agent/install/releases/`, selected by `current-version` |
| npm launcher | `%APPDATA%\npm\pi.cmd` | `$(npm prefix -g)/bin/pi` |
| Default agent/session store | `%USERPROFILE%\.pi\agent` | `~/.pi/agent` |

The current managed installer puts the launcher in `<PI_CODING_AGENT_DIR>/bin` (default
`~/.pi/agent/bin`) and selects a release under `<PI_CODING_AGENT_DIR>/install/releases/`; the npm
launcher remains a separate installation form [35]. `PI_CODING_AGENT_DIR` moves the store on every
OS [6][7][16]. Find the selected binary with `command -v pi` (macOS/Linux) or
`Get-Command pi -All` (PowerShell), the npm prefix with `npm prefix -g`, and the version with
`pi --version`. For session storage moved separately with `--session-dir`,
`PI_CODING_AGENT_SESSION_DIR` or `sessionDir`, see the limitation in §8: mukade does not follow it.

`mukade harnesses` reports the one executable it resolves, not every copy on disk. See the
[harness catalog](../reference/harness-catalog.md#finding-existing-installations) for the shared
per-OS discovery procedure and for selecting a non-default installation in mukade.

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
| `custom`, `label`, `session_info`, `usage` | extension state, bookmarks, the display name (the last `session_info.name` is what mukade titles the session by, in a list and in `meta.title`), usage not tied to a message | no |

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
- **`--exclude-tools` (`-xt`)** removes tools by name or `*` pattern, MCP tools included; **`--no-builtin-tools` (`-nbt`)** removes the built-in tools and keeps extension tools; `--no-tools` also has the short form `-nt`; with `--no-extensions` (`-ne`) an explicit `-e <path>` still loads [37][39].
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
[4][5]. How mukade translates them: [`../tool-conversion-tables.md`](../tool-conversion-tables.md); its
`WebFetch` and `WebSearch` rows map to Pi as text (**T**), which agrees with Pi having no web tool [28][44].

Checked at 1.4.0 [39][40]: a list of plain names in `--tools` replaces the whole default selection,
extension tools included, but does not remove MCP tools unless one entry starts with `mcp__`; with such
an entry `--tools` filters MCP tools too, so `--tools read,grep,find,ls,'mcp__<server>__<tool>'` offers
that one MCP tool, and an MCP tool no entry names is not declared directly (only `codemode` or
`tool_search` could still reach it). Two flags complete the set [37][39]: `--exclude-tools` (`-xt`)
removes tools by name or `*` pattern, MCP tools included, and `--no-builtin-tools` (`-nbt`) removes the
built-in tools but keeps extension tools.

When a connected MCP server offers resources, three more model-facing tools exist: `list_mcp_resources`,
`list_mcp_resource_templates` and `read_mcp_resource` [40][45]. `codemode` and `tool_search` are two of the
four built-in extensions in `dist\extensions` (`codemode`, `llama`, `mcp`, `tool-search`) and are registered
inactive [43][44].

Read-only tools: `read` (on by default), `grep`, `find`, `ls` (off by default) cannot write files, run commands or use the network; the switch for one run is `--tools read,grep,find,ls` [39][43].

#### Web tools

- Pi 1.4.0 has **no built-in web search or page-reading tool**: no web tool registration was found in the installed package, and `read` takes a file `path`, not a URL [43][44]. Web access comes only from an extension, a package, a skill or an MCP server, and none of the ones below is described as official in the vendor documentation or the pages read [46]-[49].
- Extension `pi-web-access` runs in the Pi process: tools `web_search`, `fetch_content`, `get_search_content`, `source_check`; its default `provider: "auto"` needs no key (it goes through Exa MCP); `fetchContent.domainPolicy` takes `allow` and `deny` host lists, off by default [47].
- Extension `pi-web-search` runs at the model provider's side (Gemini grounding, Responses or Messages API web search, DeepSeek and Ollama Cloud endpoints): tools `web_search`, `url_context` [48]. The gallery also lists `@onkernel/browser-loop`, whose tools were not read **[U]** [46].
- Skill `brave-search` in `badlogic/pi-skills` runs through the `bash` tool; `badlogic` is one of Pi's npm maintainers [23], but the page does not call the skill official [49]. A web-search MCP server added with `pi mcp add <server> -- <cmd>` or `--url <url>` offers `mcp__<server>__<tool>`, the name being fixed by the server **[U]**; `pi mcp list --json` shows the tool names and their exposure without a model turn [37][40].
- `pi --mode json "<prompt>"` writes a session header, then JSONL events, and exits [38]. A tool call is `tool_execution_start{toolCallId,toolName,args}`, optionally `tool_execution_update{toolCallId,toolName,args,partialResult}`, then `tool_execution_end{toolCallId,toolName,result:{content:[{type:"text",text}],details},isError,durationMs}`; the query is a field of `args` and result URLs are in `result.content[].text` [38]. The writer passes every session event except `message_update` through unchanged [42].
- Through `codemode` the model's call is `codemode` with `args.code` as the script and only the script's output reaches it, while each nested tool call emits its own `tool_execution_*` events with a `parentToolCallId` [50][51][52]. That nothing hides or encrypts `args` or `result` is **[U]**: no statement was found either way in the docs read.
- Not established **[U]**: the field names the two extensions use in these events, and whether provider-side search puts its query or result URLs in the output; one run settles it: `pi --no-session --mode json -ne -e <extension> --tools read,<tool> "<one search>"`, filtered to `tool_execution_*` events (it starts a model turn and was not run) [38][39][48].
- The `pi-web-access` README was read only up to character 100,000 of 155,660, so any prompt-injection or "URLs seen in results" restriction in the rest is **[U]**; reading it from offset 100000 settles it [47].
- Pi has no sandbox and no network switch: `--offline` only disables startup network activity (the model catalog refresh) [39]. The security page calls prompt injection from untrusted content and the lack of a built-in sandbox "generally outside the security boundary" of a vulnerability report, and advises restricting network access at the OS or container level [41].

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
| executable | `--pi-exe` → `MUKADE_PI_EXE` → `pi` on `PATH` (with `PATHEXT`) → the managed launcher `~/.pi/agent/bin/pi[.cmd]`, then npm's `%APPDATA%\npm\pi.cmd` or `pi` on Windows → bare `pi`; locations live in [`catalog/harnesses.json`](../../catalog/harnesses.json) [31] |
| store detection | `sessions/--*--/*.jsonl` and Pi's `models.json` [32] |

mukade ignores `PI_CODING_AGENT_SESSION_DIR`, the `sessionDir` setting and `--session-dir`:
`StoreRoots::pi_sessions()` is always `<PI_CODING_AGENT_DIR>/sessions`, so sessions Pi was told to store
elsewhere are not found [31].

**Reading** [29]:
- **Listing.** `list` returns every `*.jsonl` under `sessions/`, in any project folder; the id is the
  part of the file name after the last `_`. `read <id>` accepts the file stem, a stem ending in `_<id>`,
  or a UUID prefix; several matches are refused as ambiguous.
- **What a row is named.** Each file is read at its two ends only (128 KB then 384 KB, a smaller file
  whole): the header gives `cwd` and `parentSession`, and the title is the display name of the last
  `session_info` row — what `--name` and a rename write — else the first `user` message, folded to one
  line of at most 200 characters. A session with neither stays untitled and is listed under its id.
  `updated` stays empty: the header's stamp is the creation, and only a read finds the last one. A
  missing, damaged or oversized record never hides a session.
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
repository paths are relative to `D:\Projects\mukade`. All accessed 2026-10-28 unless a source says otherwise.
In sources 36 to 52, read on 2026-11-01, `<pkg>` is the same folder holding Pi 1.4.0.

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
34. `docs/verification/source-model-summary-live.md`: the live check of 2026-10-28 (throwaway sessions, source hashed before and after; Codex against a local mock)
35. Official managed installers, https://pi.dev/install.sh and https://pi.dev/install.ps1, accessed 2026-10-30 (`<PI_CODING_AGENT_DIR>/bin`, `<PI_CODING_AGENT_DIR>/install/releases/`, `current-version`).
36. https://registry.npmjs.org/@earendil-works/pi-coding-agent/latest (version 1.4.0, repository, author, licence, engines), accessed 2026-11-01.
37. Output of `pi --version` (`1.4.0`), `pi --help` ("Built-in Tool Names", tool flags) and `pi mcp --help` (Pi 1.4.0, this machine), accessed 2026-11-01.
38. "JSON event stream", `<pkg>\docs\json.md` lines 3-9, 101-115, 198-218, accessed 2026-11-01.
39. "Command Line", `<pkg>\docs\cli.md` lines 25-48, 98-106, 111-199, 237-242 (modes, sessions, tools, extensions, offline), accessed 2026-11-01.
40. "MCP", `<pkg>\docs\mcp.md` ("Control tool exposure", "Use resources", "Permissions"), accessed 2026-11-01.
41. "Run Pi safely", `<pkg>\docs\security.md` (whole page), accessed 2026-11-01.
42. `<pkg>\dist\modes\json-event.js:16-17` and `<pkg>\dist\modes\print-mode.js:84-86` (the JSON writer passes every session event except `message_update` through unchanged), accessed 2026-11-01.
43. `<pkg>\dist\core\tools\index.js` (`allToolNames`, `DEFAULT_TOOL_NAMES` line 35) and `<pkg>\dist\core\tools\{read,bash,powershell,edit,write,grep,find,ls}.js`; `<pkg>\dist\extensions\{codemode,tool-search,mcp}`, accessed 2026-11-01.
44. `<pkg>\README.md:19`; `<pkg>\package.json`; the listing of `<pkg>\dist\extensions` (`codemode`, `llama`, `mcp`, `tool-search`); a content search of `<pkg>` without `node_modules` for web search, fetch and browse terms and for the names Brave, Tavily, DuckDuckGo, Exa, Serper and SearXNG, accessed 2026-11-01.
45. `<pkg>\dist\extensions\mcp\resources.js:2-3,28-40,194-226` (the resource tool definitions), accessed 2026-11-01.
46. https://pi.dev/packages (package gallery, 5,372 packages; lists `pi-web-access`, `pi-web-search` and `@onkernel/browser-loop`, all third-party authors), accessed 2026-11-01.
47. https://registry.npmjs.org/pi-web-access/latest and https://raw.githubusercontent.com/nicobailon/pi-web-access/main/README.md (first 100,000 of 155,660 characters read), accessed 2026-11-01.
48. https://registry.npmjs.org/pi-web-search/latest and https://raw.githubusercontent.com/ttttmr/pi-web-search/main/README.md, accessed 2026-11-01.
49. https://github.com/badlogic/pi-skills (skill list, including `brave-search`), accessed 2026-11-01.
50. "Extensions", `<pkg>\docs\extensions.md` ("Tools", "Tool exposure", nested calls around line 261), accessed 2026-11-01.
51. "Codemode", `<pkg>\docs\codemode.md` lines 3-7, 24-44, accessed 2026-11-01.
52. `<pkg>\CHANGELOG.md:441` (nested tool calls emit events with `parentToolCallId`), accessed 2026-11-01.
