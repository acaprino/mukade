# Pi

Pi is a minimal, extensible terminal coding agent from Earendil Inc., published on npm as `@earendil-works/pi-coding-agent` and run as `pi` [1][2][24].
mukade reads and writes its sessions: list, dump, hop in and out, resume, a summary by Pi's own model, and Pi's own compaction record.

## Status in mukade

| | |
|---|---|
| **Read** (`list`, `dump`) | ✅ every session file under `<PI_CODING_AGENT_DIR>/sessions`; the branch Pi resumes is carried, other branches are kept but not carried |
| **Write** | ✅ a new session file: a header, then `message` rows |
| **Hop as source** | ✅ `mukade hop pi <id> <dst> …`; `--all-branches` carries the other branches of the `parentId` tree as branches |
| **Hop as destination** | ⚠️ works; a shell call stays a call only in the dialect Pi runs (`shellPath`, else POSIX), and without `--dest-protocol` every reasoning block is dropped |
| **Resume** | ✅ `pi -p --session <file>`, run from the session's working directory with `PI_CODING_AGENT_DIR` set |
| **Summary by its own model** | ✅ `--compaction summary`: Pi forks the session into a scratch folder; the source file stays byte-identical [34] |
| **Native compaction** | ✅ `--compaction native` appends Pi's own `compaction` entry; live Pi 1.4.0 resumed one and answered from the summary (2026-11-04) [53]. `Compaction::DestinationNative` ❌ |
| **Subagents** | ⚠️ none of Pi's own; `hop --children` writes a source's subagents as session files of their own, unlinked: `parentSession` means fork |
| **Notable gaps** | ⚠️ sessions moved with `--session-dir`, `PI_CODING_AGENT_SESSION_DIR` or `sessionDir` are not found; written `usage` counters are zero; no system message is written |
| **Not supported** | ❌ Oh My Pi (`omp`), a different project (see Install and locate) |
| **mukade name** | `pi` (`mukade list pi`, `mukade dump pi <id>`, `mukade hop <src> <id> pi …`) [32] |
| **Versions** | readers and writers built against 1.2.0; mukade-written sessions also run live against 1.4.0, npm's `latest` on 2026-10-28 ([verification](../verification/pi-session-and-reasoning-verification.md)). Page updated 2026-11-11; Pi facts read 2026-10-28 and 2026-11-01 |

## Quick start

```powershell
# 1. Point mukade at the stores (Pi's is required, even for a standard install)
$env:PI_CODING_AGENT_DIR = Join-Path $HOME '.pi\agent'    # macOS/Linux: export PI_CODING_AGENT_DIR=~/.pi/agent
$env:CLAUDE_CONFIG_DIR   = Join-Path $HOME '.claude'      # the other end of the hops below

# 2. Find and inspect a session
mukade list pi
mukade dump pi YOUR_PI_SESSION_ID --output C:\work\session.json

# 3. Hop into Pi. --dest-protocol keeps reasoning
#    (name the protocol Pi's configured provider really uses)
mukade hop claude_code YOUR_CLAUDE_SESSION_ID pi --cwd 'C:\work\my-project' --dest-protocol anthropic-messages

# 4. Hop out of Pi
mukade hop pi YOUR_PI_SESSION_ID claude_code --cwd 'C:\work\my-project'

# 5. Resume: the receipt's `resume` command, from the project folder
Set-Location 'C:\work\my-project'
pi -p --session '<the receipt location>' 'where were we?'
pi --session '<the receipt location>'                    # the same, interactive
```

`--dest-protocol` takes `anthropic-messages`, `openai-responses` or `openai-chat` [32]; `--root <dir>` and more examples: [usage](../usage.md).

## Install and locate

**Install**

| OS | method | command |
|---|---|---|
| macOS, Linux | installer (pins every dependency, updates with `pi update`) | `curl -fsSL https://pi.dev/install.sh \| sh` [2][20] |
| Windows | installer | `powershell -c "irm https://pi.dev/install.ps1 \| iex"` [2][24] |
| any, Node.js ≥ 22.19 | npm (does not pin transitive dependencies) | `npm install -g --ignore-scripts @earendil-works/pi-coding-agent` [2][20] |
| any | pnpm / Bun | `pnpm add -g --ignore-scripts @earendil-works/pi-coding-agent` · `bun add -g --ignore-scripts @earendil-works/pi-coding-agent` [24] |
| macOS, Linux | Nix (built from source) | `nix profile add github:earendil-works/pi/stable` (older: `nix profile install`); `nix profile upgrade pi`; pin by tag, e.g. `github:earendil-works/pi/v1.0.0` [20] |
| Android | Termux | `pkg update && pkg upgrade`, `pkg install nodejs git`, then the npm command [20b] |

- Pi requires Node.js `>=22.19.0` (`engines`) [1]; the installers can install it [2]. The version mukade was built against: npm's suffix (`…/pi-coding-agent@1.2.0`); check with `pi --version` [20].
- The `bash` tool runs `shellPath` from the settings (with `-c`) when set; else on native Windows Git Bash under `Program Files`, then `bash.exe` on `PATH`, else an error; elsewhere `/bin/bash`, then `bash`, then `sh` [12][54]. Sessions do not record the shell [54].

**Where it lives** ([harness catalog](../reference/harness-catalog.md#finding-existing-installations); by hand `command -v pi` or `Get-Command pi -All`, `npm prefix -g`, `pi --version` [35])

| what | Windows | macOS / Linux |
|---|---|---|
| Managed-install launcher | `%USERPROFILE%\.pi\agent\bin\pi.cmd` | `~/.pi/agent/bin/pi` |
| Managed releases | `%USERPROFILE%\.pi\agent\install\releases\`, selected by `current-version` | `~/.pi/agent/install/releases/`, selected by `current-version` |
| npm launcher | `%APPDATA%\npm\pi.cmd` | `$(npm prefix -g)/bin/pi` |
| Default agent/session store | `%USERPROFILE%\.pi\agent` | `~/.pi/agent` |

**What moves the store**

| setting | moves | mukade follows it |
|---|---|---|
| `PI_CODING_AGENT_DIR` | the whole agent directory, on every OS [6][7][16] | ✅ |
| `--session-dir`, then `PI_CODING_AGENT_SESSION_DIR`, then the `sessionDir` setting (first one set wins) | the session files only [6][7][16] | ❌ (see How mukade reads it) |

**How mukade finds it** [31][32] ([harness catalog](../reference/harness-catalog.md#finding-existing-installations) for non-default installs)

| | |
|---|---|
| Store, executable | `PI_CODING_AGENT_DIR` or `--root` / `--dest-root`, required by every command; `--pi-exe` → `MUKADE_PI_EXE` → `pi` on `PATH` (with `PATHEXT`) → managed launcher → npm's launcher on Windows → bare `pi` (`src/exe.rs` `resolve()`, [usage §7](../usage.md#7-stores-and-executables)). Only `mukade harnesses` falls back to `~/.pi/agent`, read-only (`src/cli.rs` `inventory_roots()`) |
| Beside a portable store | `bin/pi.cmd` or `pi.cmd` (Windows), `bin/pi` or `pi` (macOS, Linux): what `scan` reports and `--dest-root` uses |
| Store detection (`detect`, `scan`, `--root`) | `sessions/--*--/*.jsonl`, and a `models.json` holding a `providers` object (`src/detect/pi.rs` `PiLayout`) |
| Inventory | `mukade harnesses`: the one executable it resolves (not every copy on disk), its version, the session count and the fork count (headers that name a `parentSession`) |

**Project, vendor, licence**

| | |
|---|---|
| Package | open-source coding-agent CLI with a TypeScript SDK and an RPC subprocess interface [1]. `@earendil-works/pi-coding-agent`; `package.json` names the repository `git+https://github.com/earendil-works/pi.git` (directory `packages/coding-agent`), the binary `pi` and the config directory `.pi` [1]. `pi --version` printed `1.2.0` on this machine [5], later `1.4.0` [37] |
| Vendor | Earendil Inc. (the pi.dev footer credits "Earendil Inc. & Contributors") [24]; package author Mario Zechner [1]; npm maintainers `mitsuhiko`, `badlogic`, `rwachtler` [23] |
| Licence, source | MIT (`package.json`, README, repository <https://github.com/earendil-works/pi>, a monorepo that also holds `pi-ai`, `pi-agent-core`, `pi-tui`) [1][2][21] |
| Site, docs | <https://pi.dev>, docs <https://pi.dev/docs/latest>, packages <https://pi.dev/packages> [15][24]. The `/latest` docs follow the newest release and were not compared with 1.2.0 **[U]**; this page uses the docs shipped in the installed 1.2.0 package (`docs/*.md`) [3] |
| Releases | <https://github.com/earendil-works/pi/releases>: tag `v1.2.0`, 12 Oct, commit `06a4669` (published 2026-10-12); npm's `latest` on 2026-10-28 was 1.4.0 [22][23] |
| Not this project | *Oh My Pi*: a fork at `github.com/can1357/oh-my-pi`, `@oh-my-pi/pi-coding-agent`, command `omp`, state under `~/.omp` [25][27]; mukade does not read or write it |

**Run it** (every mode runs the same agent and session code [3])

| mode | command | notes |
|---|---|---|
| interactive | `pi`, `pi "List all .ts files in src/"` | full-screen TUI by default (`--tui-mode fullscreen\|regular`) [5] |
| print | `pi -p "<prompt>"`, `git diff \| pi --print "Review this change"` | prints the final assistant text and exits; also chosen when stdin or stdout is redirected [4] |
| JSON events | `pi --mode json "<prompt>" > events.jsonl` | JSONL events of one run [4] |
| RPC | `pi --mode rpc --no-session`, `pi --mode rpc --session <id>` | JSONL commands on stdin, e.g. `{"id":"c1","type":"compact"}`, `{"type":"abort"}`; records split only on LF; stdin UTF-8 **without** a BOM [17][26] |
| SDK | `import { createAgentSession } from "@earendil-works/pi-coding-agent";` | `const { session } = await createAgentSession(); await session.prompt("…")` [18] |

**Session flags** [4][7]

| flag | effect |
|---|---|
| `--continue` (`-c`), `--resume` (`-r`) | the current project's most recent session, or a picker |
| `--session <path\|id>` | path, exact or partial ID; searches the current project first and offers to fork a match from another project |
| `--fork <path\|id>` | a new session copied from that one; not combinable with `--session`, `--continue`, `--resume` or `--no-session` |
| `--session-id <id>`, `--name <name>` | the new session's id; its display name |
| `--session-dir <dir>`, `--no-session` | another session folder; no session file (see Ephemeral runs) |

Tool selection flags are under Tools.

## Session store

**Layout**

| path | holds |
|---|---|
| `<agent-dir>` | the store; `~/.pi/agent` by default, `PI_CODING_AGENT_DIR` moves it [6][7][16] |
| `<agent-dir>/sessions/--<path>--/<timestamp>_<session-id>.jsonl` | one session: a JSON Lines tree of entries, abandoned branches and compactions included [6][7] |
| `<agent-dir>/models.json` | provider endpoints and models [19] |
| `<agent-dir>/mcp.json`, `.pi/mcp.json` | MCP servers [4][5] |
| `extensions/`, `skills/`, `prompts/`, `themes/` under `<agent-dir>` and `.pi` | resources (see Plugins and extensions) [5][13][14][16] |

**Names** [6][10]

- `<path>`: `getDefaultSessionDirPath` takes the resolved working directory, drops one leading `/` or `\`, and turns every `/`, `\` and `:` into `-`: `C:\work\my-project` becomes `--C--work-my-project--`.
- `<timestamp>`: the ISO instant with `:` and `.` turned into `-`, e.g. `2026-10-27T16-00-39-689Z`.
- `<session-id>`: a UUID, unless the SDK or `--session-id` supplies one.

**Header and entries** [6][10]

- Header (line 1, not part of the tree): `{"type":"session","version":3,"id","timestamp","cwd"}`, plus `"parentSession"` after `/fork` or `/clone`.
- Versions: 1 linear, 2 added `id`/`parentId`, 3 renamed the `hookMessage` role to `custom`; older files are migrated on load (`CURRENT_SESSION_VERSION = 3`).
- Every entry has `type`, `id` (usually 8 hex characters), `parentId` (`null` at a root) and an ISO `timestamp`; a `message`'s own `timestamp` is in milliseconds.

**Record types** (Pi: [6][9]; mukade: [29])

| `type` (role) | fields | reaches the model | mukade reads it as |
|---|---|---|---|
| `message` (`user`) | an `AgentMessage`: `content` | yes | `User` |
| `message` (`assistant`) | `content` of `text`/`thinking`/`toolCall`, plus `api`, `provider`, `model`, `usage`, `stopReason` | yes | `Step`: thinking (signature kept, protocol from `api`), text, all `toolCall`s. `stopReason` `toolUse` → `ToolUse`, `aborted` → `Aborted`, `error` → `Error`, any other → `Completed` |
| `message` (`toolResult`) | `toolCallId`, `content`, `isError` | yes | consecutive results gathered into one `Results` |
| `message` (`bashExecution`) | a `!cmd` the user typed: `command`, `output`, `exitCode`, `cancelled`, `truncated`, `fullOutputPath`, `excludeFromContext` | yes, as user text starting ``Ran `<command>` ``; not when `excludeFromContext` is set (the `!!` prefix) | `User` with Pi's own text (`bashExecutionToText`); `Meta` when `excludeFromContext` |
| `message` (`system`) | prompt `sections`, plus `toolsAdded`/`toolsRemoved` | yes | `Meta(SystemPrompt)` |
| `message` (`custom`) | an extension's message | yes | `Meta(Internal)`: not carried |
| `compaction` | `summary`, `firstKeptEntryId`, `tokensBefore`, `systemMessage?`, `usage?`, `details?`, `fromHook?` | yes, as the summary | `Compaction{summary, kept_from: firstKeptEntryId, tokens_before}` |
| `branch_summary` | `summary`, `fromId`, `usage?`, `details?`, `fromHook?` | yes, as a user message | `User` with `BRANCH_SUMMARY_PREFIX` + summary + `</summary>` (`Meta` if empty) |
| `custom_message` | `customType`, `content`, `display`, `details?` | yes, as a user message | `User` from its `content` |
| `context_edit` | `targetId`, `replacement` (`null` drops the target) | changes what the target contributes | applied (see How mukade reads it) |
| `model_change`, `thinking_level_change` | `provider`/`modelId`, `thinkingLevel` | settings only | `Meta(ModelChange)` |
| `custom`, `label`, `session_info`, `usage` | extension state, bookmarks, the display name, usage not tied to a message | no | `Meta(Internal)`; the last `session_info.name` titles the session, in a list and in `meta.title` |

The coding agent adds the roles `bashExecution`, `custom`, `branchSummary`, `compactionSummary` [9].

**Order and lineage**

- The resumed context is the path ending at the file's last entry: on load `_buildIndex` makes the last non-header entry the leaf and `buildSessionPath(entries, leafId)` walks `parentId` back from it (falling back to the last entry) and reverses it [10][26]. Entries off that path stay in the file [7].
- A forked session's header names its source in `parentSession`: `/fork`, `/clone` and mukade's own summary fork write it [6][7]. Nothing else lives beside a session: one file holds its whole tree.

## Features

### Compaction

- **Trigger.** Automatic when `contextTokens > contextWindow - reserveTokens` (default 16384); manual with `/compact [instructions]` or the RPC `{"type":"compact"}` (optional `customInstructions`) [8][17].
- **Settings.** `compaction.enabled`, `reserveTokens`, `keepRecentTokens` (default 20000), with overrides per `provider/modelId` [8].
- **Entry.** Pi walks back until `keepRecentTokens` is reached, has the model summarize what lies before, and appends a `compaction` entry whose `firstKeptEntryId` names the first entry kept (a compaction that keeps nothing stores its own id). A cut never falls on a tool result; the original entries stay [6][7][8]. The summary carries a Pi-added `<read-files>` block; `details` holds `readFiles` and `modifiedFiles` [26].
- **Rebuilt context.** `buildContextEntries()` walks leaf to root and uses the latest compaction on the path: the compaction, then the non-system entries from `firstKeptEntryId` up to it, then everything after [6]. The model receives the system checkpoint, a user message `The conversation history before this point was compacted into the following summary:\n\n<summary>\n…\n</summary>`, the kept tail and the new prompt [9][26].
- **Headless.** `pi --mode rpc --session <id>` with stdin `{"type":"compact"}` appends the entry to that session's file [17][26]. In print mode `/compact` is sent to the model as text [26].

### Ephemeral runs

- **`--no-session`** writes no session; not combinable with `--session`, `--continue`, `--resume` or `--fork` [4].
- **`--fork <path|id> --session-dir <dir>`** copies the session into a new file under `<dir>`; the source file and its folder stay byte-identical [34]. Delete `<dir>` afterwards and nothing is left.
- **`--no-tools`** offers no tools; a history holding tool calls is still accepted (`openrouter`) [34].
- **`--offline`** disables only startup network activity (the model catalog refresh) [39]. **`-p`** prints the final assistant text on stdout [4].

### Tools

Tool names, the default set and the tool flags were re-checked on 2026-11-01 against `pi --help` of the installed 1.4.0, npm's `latest` [36][37]: no tool name changed. The parameter schemas below were read in 1.2.0 (`dist/core/tools/*.js`) [11]; only `read`'s `path`, `offset` and `limit` were seen again at 1.4.0 [43]. Whether the others changed is **[U]** (Open points).

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

**Choosing the set**

- Default: `DEFAULT_TOOL_NAMES = ["read", "bash", "edit", "write"]`, in `dist\core\tools\index.js` at 1.4.0 [11][43].
- `--tools` (e.g. `pi --tools read,grep,find,ls -p "…"`, or `+name`/`-name` entries) or the `defaultTools` setting (e.g. `{"defaultTools": ["read", "powershell", "edit", "write"]}`) [4][12].
- At 1.4.0 a list of plain names in `--tools` replaces the whole default selection, extension tools included, but keeps MCP tools unless an entry starts with `mcp__`; with one, `--tools` filters MCP tools too: `--tools read,grep,find,ls,'mcp__<server>__<tool>'` offers that one MCP tool, and an MCP tool no entry names is not declared directly (only `codemode` or `tool_search` could still reach it) [39][40].
- `--exclude-tools` (`-xt`) removes tools by name or `*` pattern, MCP tools included; `--no-builtin-tools` (`-nbt`) removes the built-in tools, keeping extension tools; `--no-tools` (`-nt`); `--no-mcp`; `--no-extensions` (`-ne`), with which an explicit `-e <path>` still loads [37][39].
- Read-only for one run: `--tools read,grep,find,ls`, which cannot write files, run commands or use the network [39][43].

**Beyond the built-in set**

- `codemode` (`--tools +codemode`), `tool_search`, MCP tools `mcp__<server>__<tool>` and extension tools [4][5]. `codemode` and `tool_search` are two of the four built-in extensions in `dist\extensions` (`codemode`, `llama`, `mcp`, `tool-search`), registered inactive [43][44].
- A connected MCP server that offers resources adds `list_mcp_resources`, `list_mcp_resource_templates` and `read_mcp_resource` [40][45].
- mukade's translation: [tool-conversion-tables](../tool-conversion-tables.md); its `WebFetch` and `WebSearch` rows map to Pi as text (**T**), which agrees with Pi having no web tool [28][44].

#### Web tools

**What Pi has**

- Pi 1.4.0 has **no built-in web search or page-reading tool**: no web tool registration was found in the installed package, and `read` takes a file `path`, not a URL [43][44].
- Web access comes only from an extension, package, skill or MCP server; none below is called official in the pages read [46]-[49].
- No sandbox, no network switch (`--offline` only skips startup network activity [39]). The security page puts prompt injection and the missing sandbox "generally outside the security boundary" of a vulnerability report and advises restricting network access at the OS or container level [41].

**Third-party options**

| option | where it runs | tools and notes |
|---|---|---|
| extension `pi-web-access` | in the Pi process | `web_search`, `fetch_content`, `get_search_content`, `source_check`; default `provider: "auto"` needs no key (via Exa MCP); `fetchContent.domainPolicy` takes `allow` and `deny` host lists, off by default [47] |
| extension `pi-web-search` | at the model provider (Gemini grounding, Responses or Messages API web search, DeepSeek and Ollama Cloud endpoints) | `web_search`, `url_context` [48] |
| `@onkernel/browser-loop` | listed in the gallery | tools not read **[U]** [46] |
| skill `brave-search` in `badlogic/pi-skills` | through the `bash` tool | `badlogic` is one of Pi's npm maintainers [23], but the page does not call the skill official [49] |
| a web-search MCP server | `pi mcp add <server> -- <cmd>` or `--url <url>` | offers `mcp__<server>__<tool>`, the name fixed by the server **[U]**; `pi mcp list --json` shows tool names and exposure without a model turn [37][40] |

**What a JSON run shows**

- `pi --mode json "<prompt>"` writes a session header, then JSONL events [38]. A tool call is `tool_execution_start{toolCallId,toolName,args}`, optionally `tool_execution_update{toolCallId,toolName,args,partialResult}`, then `tool_execution_end{toolCallId,toolName,result:{content:[{type:"text",text}],details},isError,durationMs}`. The query is in `args`; result URLs in `result.content[].text` [38]. Every session event except `message_update` passes through unchanged [42].
- Through `codemode` the model's call is `codemode` with `args.code` as the script, and only the script's output reaches the model; each nested tool call emits its own `tool_execution_*` events with a `parentToolCallId` [50][51][52]. That nothing hides or encrypts `args` or `result` is **[U]**: no statement was found either way in the docs read.

**Not established**

- **[U]** The field names the two extensions use in these events, and whether provider-side search puts its query or result URLs in the output; one run settles it (a model turn, not run): `pi --no-session --mode json -ne -e <extension> --tools read,<tool> "<one search>"`, filtered to `tool_execution_*` [38][39][48].
- **[U]** The `pi-web-access` README was read to character 100,000 of 155,660: any prompt-injection or "URLs seen in results" restriction in the rest is unknown (offset 100000 settles it) [47].

### Branches, forks and subagents

- **`/tree`** moves within the current file. Selecting a user message puts its text back in the editor and submitting it starts a new branch; selecting any other entry continues after it. The abandoned branch stays in the file [7].
- **`branch_summary`.** On leaving a branch Pi can append a model-written summary: `parentId` is the entry the new branch continues from, `fromId` the old leaf [6][8]. The model sees the user message `The following is a summary of a branch that this conversation came back from:\n\n<summary>\n…</summary>` [9].
- **`/fork`** creates a new session from an earlier user message; **`/clone`** copies the active branch into a new session. Both set `parentSession` in the new header [6][7]. The branch that resumes: see Session store, "Order and lineage".
- **Subagents:** none built in; sub-agents and plan mode are left to extensions and packages [2]. mukade reads a `subagent` call as `Agent` [33].

### Models and providers

- **Agent loop.** A submitted message joins the *active branch*; Pi builds a request from the system prompt, that branch, the tools and the model settings, streams and records the reply, runs each tool call and records the results: one turn. Tool results or queued messages start another; an abort stops the run [3].
- **Provider layer.** Models are called through `@earendil-works/pi-ai` ("Unified LLM API with automatic model discovery and provider configuration"); the tool-calling runtime is `pi-agent-core`, beside `pi-tui`, `pi-mcp` and `pi-codemode`, all `^1.2.0` [1][19][21].
- **Protocols.** pi-ai's `KnownApi`: `openai-completions`, `mistral-conversations`, `openai-responses`, `azure-openai-responses`, `openai-codex-responses`, `anthropic-messages`, `bedrock-converse-stream`, `google-generative-ai`, `google-vertex`, `pi-messages`; `Api = KnownApi | (string & {})` leaves room for others. Known providers include `anthropic`, `openai`, `google`, `amazon-bedrock`, `openrouter` [19].
- **Endpoints and credentials.** `<agent-dir>/models.json` declares compatible endpoints: `{"providers": {"<name>": {"baseUrl", "api", "apiKey", "models": [{"id"}]}}}`; `apiKey` accepts `$NAME`, `${NAME}`, a literal or `!command`. Credentials: `--api-key`, then `auth.json`, then `models.json`, then environment variables [19]. `/login` signs in to a built-in provider [2].
- **Choosing.** `--provider <p> --model <m>` or `--model provider/id:<thinking>`; thinking levels `off`, `minimal`, `low`, `medium`, `high`, `xhigh`, `max` [5]. Assistant messages record `api`, `provider`, `model` [6]; thinking blocks keep their `thinkingSignature` [26].

### Plugins and extensions

| mechanism | where it lives | how it is added |
|---|---|---|
| extensions (TypeScript/JavaScript, loaded with `jiti`) | `<agent-dir>/extensions/`, `.pi/extensions/` (a file, or a folder with `index.ts`/`index.js`) | drop it in the folder; one run `pi --extension ./hello.ts` (`-e`); off with `--no-extensions` [5][13][16] |
| custom tools | inside an extension: `pi.registerTool()` with name, description, TypeBox schema, `execute()` | through the extension [13] |
| skills ([Agent Skills](https://agentskills.io/specification) folders with `SKILL.md`) | `<agent-dir>/skills/`, `.pi/skills/`, `~/.agents/skills/`, `.agents/skills/` | the folder or `--skill <path>`; run with `/skill:name`; off with `--no-skills` [14] |
| prompt templates, themes | `prompts/` and `themes/` under `<agent-dir>` and `.pi` | the folder, `--prompt-template`, `--theme` [5][16] |
| Pi packages (bundles of the above) | npm, git or a local path, declared in `settings.json` | `pi install npm:@example/pi-tools@1.0.0`, `pi install git:github.com/example/pi-tools@v1`, `pi install ./local-package`; `-l` writes `.pi/settings.json`; `pi list`, `pi remove <source>`, `pi update --extensions`, `pi config` [15] |
| MCP servers | `<agent-dir>/mcp.json`, `.pi/mcp.json` | `pi mcp add <server> [options] -- <command> [args...]`; `pi mcp list`; off with `--no-mcp` [4][5] |

- Project resources (`.pi/…`) load only after project trust; `--approve` / `--no-approve` decide it for one run [5][16]. Extensions run inside the Pi process, with its permissions [13].

### Hooks

Pi's hooks are extension event handlers, `pi.on(event, handler)`, run in load order; some notify, others transform data, replace results or cancel an operation [13]. `fromHook` and the v3 rename of `hookMessage` to `custom` show extensions took over an earlier hook API [6][8]. No setting that runs shell commands as hooks (like Claude Code's `hooks`) appears in the 1.2.0 docs **[U]**.

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

What handlers can do [8][13]:

- `tool_call` can change a call's input or block it; `tool_result` handlers compose. `message_end` can replace a finished message.
- `turn_end` and `agent_before_settle` can append `custom`, `custom_message`, `context_edit` or `compaction` entries and request one more model call with `continue: true`.
- `session_before_compact` and `session_before_tree` let an extension write the summary itself.

## How mukade reads it

Sources: [29], tool names [33]. The record-by-record mapping is the last column of "Record types".

**What you may miss**

- **Only `<PI_CODING_AGENT_DIR>/sessions`.** `StoreRoots::pi_sessions()` (`src/roots.rs`) is always that folder: mukade ignores `PI_CODING_AGENT_SESSION_DIR`, the `sessionDir` setting and `--session-dir`, so sessions Pi was told to store elsewhere are not found [31].
- **One branch.** The reader follows `parentId` from the last record that has an `id` and is not the header, as Pi does [10]; records off that path are kept verbatim as `Meta(OffBranch)`, not resumed, counted as `dropped_off_branch`. `--all-branches` carries them as branches instead (`src/reader/branches.rs`: the user and assistant records off the path, grouped by `parentId`).

**Finding sessions**

- **Listing.** Every `*.jsonl` under `sessions/`, in any project folder; the id is the file name after the last `_`. `read <id>` accepts the file stem, a stem ending in `_<id>`, or a UUID prefix (several matches: refused). `--from-file <path>` reads a file outside the store.
- **What a row shows.** Each file is read at its two ends only (128 KB, then 384 KB; a smaller file whole). The header gives `cwd` and `parentSession`, a file path: the row's `parent` is the `id` in that file's own header (`kind: fork`; the same in `Meta.parent` when read), or the path as written when no such file is here. The title is the display name of the last `session_info` row (what `--name` and a rename write), else the first `user` message, folded to one line of at most 200 characters; a session with neither stays untitled, listed under its id. `updated` is the newest `timestamp` in those two ends.

**How records become entries**

- **`context_edit`** on the resumed path is applied as Pi's context builder does: the latest edit of a target wins; the target keeps its role and takes the replacement's content, or is left out (`Meta(Internal)`) when the replacement is `null`. Its record stays verbatim in `raw`.
- **Interrupted turn.** A turn holding an `Aborted` step travels by default; `--interrupted drop` ([usage](../usage.md#a-session-with-interrupted-turns)) leaves its steps and results out, its prompt stays. A killed turn leaves only the user entry [26].
- **Thinking.** The signature is kept, with the protocol taken from `api`. A `redacted: true` block (Anthropic's `redacted_thinking`, as Pi keeps it) is read as an encrypted payload with no text.
- **Bash timeout.** Seconds to canonical milliseconds (×1000, rounded); the raw record keeps the original.
- **Tool names.** `CanonicalTool::from_pi` (`src/model/tool.rs`) maps `read`, `write`, `edit`, `bash`, `grep`, `find` (→ `Glob`), `ls`, `subagent` (→ `Agent`); any other name, `powershell` included, stays as it is.

## How mukade writes it

Sources: [30], tool translation [28] ([tool-conversion-tables](../tool-conversion-tables.md)).

**What you may lose**

- **Shell calls take the shell Pi runs.** A call is written as `bash` only when its dialect is the one `--dest-shell` names, else the one Pi runs (`src/harnesses/pi.rs` `default_shell()`): the `shellPath` of `<cwd>/.pi/settings.json`, else of `<PI_CODING_AGENT_DIR>/settings.json` (the project's wins, as Pi merges them), read by its name, so `pwsh` is PowerShell and a shell of no known dialect is none; unset, POSIX. Otherwise it travels as text, as does one with a working directory or background execution. The timeout goes back to seconds, rounded up (`src/harnesses/pi.rs` `translate()`).
- **Reasoning needs `--dest-protocol`.** Pi speaks whatever its provider speaks, so by default every reasoning block toward Pi is dropped [32]. With it, thinking is written with its `thinkingSignature` only when the reasoning policy carries it natively; an Anthropic encrypted payload becomes `{"type":"thinking","thinking":"[Reasoning redacted]","thinkingSignature":<data>,"redacted":true}` (`src/policy.rs` `payload_fits()`).
- **Offered tools.** Pi's offered set is `read`, `bash`, `edit`, `write` (`src/tools.rs` `offered()`): `grep`, `find` and `ls` calls travel as text unless `--tool-map <path>` binds them. An edit with `replace_all` travels as text; several edits become one `edit` with `edits[]`.

**The file**

- **File.** `<PI_CODING_AGENT_DIR>/sessions/--<dir>--/<stamp>_<uuid>.jsonl`, named as in Session store, Names (UTC stamp). `--cwd` sets the project, else the source's; `--dest-id` must be a UUID; `--dest-path` absolute, inside `PI_CODING_AGENT_DIR`, no `..`.
- **Rows.** The header `{"type":"session","version":3,"id","timestamp","cwd"}`, then `message` rows only, each with a new 8-hex `id` chained by `parentId` (`null` on the first). No system message is written; Pi adds one on the first request of a session that has none [6].
- **Assistant messages.** Thinking, then text, then all the step's calls in this one message (L3). `api` comes from `--dest-protocol` (none given: no `api`); `provider` and `model` from `--model-provider` / `--model`, else the source (`src/writer/plan.rs` `provider_id()`, `model_id()`). `stopReason` keeps `aborted` and `error`, else `stop` (no calls) or `toolUse`. `usage` has zero counters: Pi refuses a session without it (`Cannot read properties of undefined (reading 'totalTokens')`).
- **Results.** One `toolResult` per result, in call order, with the call's `toolName`; a call carried as text is answered by a `user` text message in the same place.

**Compaction and resume**

- **Compaction** ([usage §5](../usage.md#5-choose-what-travels)). `--compaction portable` (`Portable`) writes the summary as a user message. `--compaction native` (needs `--context-window <n>`) keeps every generated entry and appends Pi's own `compaction` entry with `firstKeptEntryId` and `tokensBefore`; live Pi 1.4.0 resumed such a session and answered from the summary (2026-11-04) [53]. `Compaction::DestinationNative` is refused with `Unsupported` (`src/hop.rs` `select()`): Pi's own compaction is the caller's to run, as `pi --mode rpc --session <id>` with stdin `{"id":"c1","type":"compact"}` [26].
- **Resume.** The receipt's command is `[<--pi-exe or "pi">, "-p", "--session", "<file path>"]`: the file path, because `--session <id>` searches the current project first. Run it from the session's working directory with `PI_CODING_AGENT_DIR` set [26][30][32]. Interactive: `pi --session <file>` (`interactive_resume` in `catalog/harnesses.json`).
- **Isolated runs.** The bench runs Pi with `--offline --no-skills` and no provider API-key variables; Pi reads `~\.agents\skills` whatever `PI_CODING_AGENT_DIR` says [26].

## How mukade summarizes it

Source: [34] ([usage §5](../usage.md#5-choose-what-travels)).

- **Command.** With `--compaction summary` and Pi as the source, mukade runs `pi -p --fork <session file> --session-dir <folder> --no-tools [--provider <p>] [--model <id>] <prompt>` and reads the summary from stdout. `<folder>` is mukade's own under `mukade-data/tmp`, removed afterwards; `--no-session` cannot be combined with `--fork`, hence the fork (`src/summary.rs` `summary_command()`).
- **Model.** The session's own last model, provider and model id passed apart because a model id can hold a slash (`moonshotai/kimi-k2.6`); `--summary-model <id>` goes as `--model <id>` alone (`src/summary.rs` `model_args()`).
- **From a file.** A Pi session read with `--from-file` can be summarized too; Pi is the only source that can, because it forks from a file.

## Open points

Ranked by what they cost a user. The rest: [`../../next.md`](../../next.md).

1. **Pi's `powershell` tool as a destination.** It is off unless `defaultTools` or `--tools` names it; mukade writes a PowerShell call as `bash`, and only toward a `shellPath` of PowerShell. Reading `defaultTools` and translating to `powershell` settles it in code (next.md).
2. **1.4.0 tool schemas [U].** Only `read`'s parameters were re-checked at 1.4.0; reading `<pkg>\dist\core\tools\*.js` of 1.4.0 settles the others.
3. **Shell-command hooks [U].** No such setting appears in the 1.2.0 docs; reading the 1.4.0 docs (`docs/extensions.md`, `docs/configuration.md`) settles it.
4. **Web extension events [U].** The field names `pi-web-access` and `pi-web-search` use; settled by the one JSON run under Tools, "Not established".
5. **`/latest` docs vs 1.2.0 [U].** Settled by comparing <https://pi.dev/docs/latest> with the packaged `docs/*.md`.

Further **[U]** under Tools: the rest of the `pi-web-access` README, `@onkernel/browser-loop`'s tools, MCP web-search tool names, and whether `codemode` hides anything.

## Sources

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
28. `docs/tool-conversion-tables.md`; `src/harnesses/pi.rs` (`offered`, `translate`, `multi_edit`); `src/tools.rs`.
29. `src/reader/pi.rs`; `src/reader/json.rs` (`peek_ends`, `PEEK_HEAD_BYTES`, `PEEK_TAIL_BYTES`, `title_line`); `src/hop.rs` (`export_all_branches`). Checked 2026-11-11.
30. `src/writer/pi.rs`; `src/writer/plan.rs` (`provider_id`, `model_id`, `protocol_name`); `src/policy.rs` (`payload_fits`, `destination_protocol`); `src/hop.rs` (`Compaction::DestinationNative`, `select`). Checked 2026-11-11.
31. `src/harnesses/pi.rs` (the adapter), `src/exe.rs`, `src/roots.rs`, `src/cli.rs` (usage text, `inventory_roots`), `src/detect/pi.rs`, `catalog/harnesses.json`. Checked 2026-11-11.
32. `README.md`, `docs/usage.md`.
33. `src/model/tool.rs` (`CanonicalTool::from_pi`).
34. `docs/verification/source-model-summary-live.md`: the live check of 2026-10-28 (throwaway sessions, source hashed before and after; Codex against a local mock); `src/summary.rs` (`summary_command`, `model_args`).
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
53. `docs/verification/native-compaction-live.md`: the live native-compaction check of 2026-11-04 (Pi 1.4.0, scratch home, 4,001 rows, `compaction` with `firstKeptEntryId`).
54. Pi 1.4.0 package, read only, 2026-11-12: `dist/utils/shell.js` (the shell the `bash` tool runs, `shellPath`), the `powershell` tool and `defaultTools`, the settings files and their merge. Decisions: [`docs/superpowers/specs/2026-11-12-destination-shell-design.md`](../superpowers/specs/2026-11-12-destination-shell-design.md).
