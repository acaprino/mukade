# OpenCode

Open-source AI coding agent with a terminal UI, a desktop app and a web app, made by Anomaly [9][3][4].
mukade reads it through `opencode session export` and writes it through `opencode session import`;
it never writes OpenCode's SQLite database itself [1][10][27].

## Status in mukade

| | |
|---|---|
| Read (`list`, `dump`) | ✅ `list` reads the selected database read-only, no OpenCode process; `dump` runs `session export --standalone` |
| Write | ✅ `session import --standalone`; needs a project id, a provider and a model (found or given) |
| Hop as source | ✅ unset `OPENCODE_DB` means OpenCode's own configured store |
| Hop as destination | ✅ one assistant message per step, call and result in one tool part |
| Resume | ✅ `<exe> run --standalone --session <id>`; mukade's TUI: `<exe> mini --standalone --session <id>` |
| Summary by its own model | ✅ `--compaction summary`: a deny-all `run --fork`, the fork deleted afterwards [37] |
| Native compaction | ⚠️ reads the source's `compaction` record; `--compaction native` writes one (live on 2.2.5, 2026-11-04 [27 §4.4][56]); OpenCode's own HTTP compaction is never run by mukade |
| Gaps | ⚠️ child sessions not followed; sibling `opencode*.db` and legacy rows listed, not read; signed reasoning dropped unless `--dest-protocol` names the protocol |

**Versions** (harness facts as of 2026-11-01; mukade facts checked against the code on 2026-11-11):
- **2.1.4: verified** [1]: reader, writer, probes, on-disk facts.
- **2.2.5: installed now** [39], newest release, published 2026-10-30 [47]. Only the native-compaction
  hop (2026-11-04) [56] and the listing check (2026-11-08) [55] ran on it.
- ***2.2.5* labels** (Tools rows and notes, Web tools, extra `--format json` events): read in the V2
  docs of 2026-11-01 [41]–[46], the `v2.2.5` source [48]–[54] and its `--help` [40] through a
  summarising fetch (a model's extract, not raw bytes); not mukade behaviour. Whether each exists in
  2.1.4: **[U]**.
- **2.2.0** (npm `latest` on 2026-10-28) already changes media blobs, compaction start and
  instruction persistence [6][28].

## Quick start

```powershell
# 1. Select the store: the database file (a folder passed as --root means its opencode.db)
$env:OPENCODE_DB = "$HOME\.local\share\opencode\opencode.db"

# 2. Find a session and look at it
mukade list opencode
mukade list opencode --archived false              # without the archived sessions
mukade dump opencode YOUR_OPENCODE_SESSION_ID --output 'C:\work\session.json'

# 3. Out of OpenCode
$env:PI_CODING_AGENT_DIR = Join-Path $HOME '.pi\agent'
mukade hop opencode YOUR_OPENCODE_SESSION_ID pi --cwd 'C:\work\my-project'

# 4. Into OpenCode (preview the import document first; it writes nothing)
$env:CLAUDE_CONFIG_DIR = Join-Path $HOME '.claude'
mukade document opencode claude_code YOUR_CLAUDE_SESSION_ID --cwd 'C:\work\my-project' --project-id YOUR_OPENCODE_PROJECT_ID
mukade hop claude_code YOUR_CLAUDE_SESSION_ID opencode --cwd 'C:\work\my-project' --project-id YOUR_OPENCODE_PROJECT_ID --model-provider YOUR_PROVIDER --model YOUR_MODEL

# 5. Resume: the receipt's resume command, from the project folder
Set-Location 'C:\work\my-project'
opencode run --standalone --session ses_RETURNED_ID 'where were we?'
```

- `--project-id`, `--model-provider`, `--model` may be left out when mukade can find them
  ([How mukade writes it](#how-mukade-writes-it)); explicit values are faster. Set your installation's
  `XDG_*` variables in the same shell, for the hop and the resume. A custom **source** binary goes
  through `MUKADE_OPENCODE_EXE`; `--opencode-exe` applies to a destination only
  ([usage, OpenCode](../usage.md#opencode)).

## Install and locate

### Install

| OS | method | command |
|---|---|---|
| macOS, Linux | install script (binary in `$HOME/.opencode/bin`) | `curl -fsSL https://opencode.ai/v2/install \| bash` [3][9] |
| any (incl. Windows) | npm | `npm install -g @opencode/cli`; pinned: `npm install -g @opencode/cli@2.1.4` [6][9] |
| macOS, Linux | Homebrew tap (recommended in the README) | `brew install anomalyco/tap/opencode-v2` [3][9] |
| macOS, Linux | Homebrew core formula ("updated less") | `brew install opencode` [3]; which major version it installs: **[U]** |
| any | bun / pnpm / yarn / vite+ | `bun install -g --trust @opencode/cli` · `pnpm add -g --allow-build=@opencode/cli @opencode/cli` · `yarn global add @opencode/cli` · `vp install -g @opencode/cli` [9] |
| Arch Linux | AUR | `paru -S opencode-beta` [3][9]; the README also lists `sudo pacman -S opencode`, which line that package carries: **[U]** |
| any | mise ≥ 2026.10.9 (older mise installs OpenCode 1) | `mise use -g opencode` [9] |
| any | Nix | `nix run github:anomalyco/opencode` [3] |
| macOS, Windows, Linux (glibc, musl) | standalone archive | `https://opencode.ai/update/api/latest/cli/opencode/opencode-<os>-<arch>[-baseline][-musl].zip\|.tar.gz`, e.g. `…/opencode-windows-x64.zip`; always the *latest* release [9] |
| any | Docker image | `ghcr.io/anomalyco/opencode:<version>`, e.g. `:2.0.0` [9]; whether a `:2.1.4` tag exists: **[U]** |

- **npm postinstall** copies the matching optional dependency's native binary
  (`@opencode/cli-windows-x64`, …) into `bin/` [1][9]; here
  `%APPDATA%\npm\node_modules\@opencode\cli\bin\opencode.exe` [1].
- **winget:** "Windows package managers are not supported" [9], yet `opencode upgrade --method`
  offers `winget` [1]: whether it works **[U]**.
- **From V1:** remove a package-managed V1 install first; the V2 curl installer replaces the V1 binary [25].
- **Upgrade or pin:** `opencode upgrade [<target>] [--method curl|npm|pnpm|bun|yarn|vp|brew|winget]`.
- **Remove:** `opencode uninstall [--dry-run] [--force]`; keeps sessions, configuration, state [1][10].

### Locations

| what | Windows | macOS / Linux |
|---|---|---|
| install-script launcher (Git Bash/MSYS on Windows) | `$HOME/.opencode/bin/opencode.exe` | `~/.opencode/bin/opencode` [38] |
| npm package / launcher (defaults) | `%APPDATA%\npm\node_modules\@opencode\cli\bin\opencode.exe`, `%APPDATA%\npm\opencode.cmd` | `$(npm root -g)/@opencode/cli/bin/opencode`, `$(npm prefix -g)/bin/opencode` |
| database | `%USERPROFILE%\.local\share\opencode\opencode.db` | `~/.local/share/opencode/opencode.db` |

The binary derives the data folder from the home directory on every OS, Windows included [2].
Customized npm prefixes and Homebrew are found through `PATH`, `brew --prefix`, `npm prefix -g`,
`npm root -g`. `opencode --version` checks the install; `opencode debug paths db` prints the active
database without starting a server or opening it [1][10].

### Variables that move the store or change a run

| variable | effect |
|---|---|
| `OPENCODE_DB` | the database file. Relative values resolve beneath `$XDG_DATA_HOME/opencode`; `:memory:` accepted [14][2][10] |
| `XDG_DATA_HOME`, `XDG_CONFIG_HOME`, `XDG_STATE_HOME`, `XDG_CACHE_HOME` | replace `~/.local/share`, `~/.config`, `~/.local/state`, `~/.cache` [14][2] |
| `OPENCODE_DISABLE_CHANNEL_DB` | `1`/`true`: a build on a channel other than `latest\|dev\|beta\|next\|prod` keeps `opencode.db` instead of `opencode-<channel>.db` [10][2] |
| `OPENCODE_CONFIG`, `OPENCODE_CONFIG_CONTENT` | a configuration file [35]; inline configuration merged with the user's own [37] |
| `OPENCODE_DISABLE_CLAUDE_CODE`, `OPENCODE_DISABLE_CLAUDE_CODE_SKILLS` | read by the binary [2]; see Plugins and extensions |

A channel build's `opencode-<channel>.db` must be selected with `OPENCODE_DB` (the inventory's default is `opencode.db`). The bench isolated OpenCode with `OPENCODE_DB`, the four `XDG_*`, `OPENCODE_TEST_HOME` → scratch, `OPENCODE_DISABLE_MODELS_FETCH=1`, `OPENCODE_DISABLE_PROJECT_CONFIG=1`, `OPENCODE_DISABLE_CLAUDE_CODE=1`, `PWD` removed [27 §2.3].

### How mukade finds it

Resolution order, env vars and `--root`/`--dest-root`: [usage §7](../usage.md#7-stores-and-executables).

| what | OpenCode specifics |
|---|---|
| executable | `--opencode-exe` → the `--dest-root` installation's own (destination) → `MUKADE_OPENCODE_EXE` → `PATH` (`PATHEXT` included) → catalog locations (`%APPDATA%\npm\node_modules\@opencode\cli\bin\opencode.exe`, `%APPDATA%\npm\opencode.cmd`, `$HOME/.opencode/bin/opencode[.exe]`) → `opencode` ([`catalog/harnesses.json`](../../catalog/harnesses.json)). An OpenCode **source** ignores the flag: only `MUKADE_OPENCODE_EXE` applies [31][34][35] |
| store | `OPENCODE_DB` is passed to every subprocess when set, removed when unset (OpenCode then uses its own configured store). mukade never sets `XDG_*`; the caller sets them, and `OPENCODE_CONFIG`, when isolation matters (`src/reader/opencode.rs` `opencode_environment()`) [31][34][35] |
| `--root <path>` | a directory means `<path>/opencode.db`, a file is used as given; in a `hop` it names the source store (`src/roots.rs` `StoreRoots::set_root()`). `--dest-root <dir>` names a destination installation [35] |
| `mukade harnesses` | one resolved executable, not every copy. Store: `OPENCODE_DB`, else `{xdg_data_home}/opencode/opencode.db` (`XDG_DATA_HOME`, else `~/.local/share`); relative `OPENCODE_DB` resolves beneath `$XDG_DATA_HOME/opencode`. Session and fork counts from `session_v2` (`src/reader/opencode.rs` `store_counts()`) |
| `mukade detect` / `scan` | a folder holding `opencode.db`, or that file when it is SQLite (name compared without case; Codex's `state_*.sqlite` is not OpenCode) (`src/detect/opencode.rs` `OpenCodeLayout`). `scan` also takes the executable beside the store: `opencode.exe`, `bin/opencode.exe`, `resources/opencode-cli/opencode.exe`, `.opencode/bin/opencode.exe`, `opencode.cmd`, `bin/opencode.cmd` (no `.exe`/`.cmd` off Windows) [35]; portable example: [usage §2](../usage.md#mukade-scan). For a nonstandard database name, pass `opencode` and the file path |

Per-OS discovery, selecting a non-default installation:
[harness catalog](../reference/harness-catalog.md#finding-existing-installations).

### Vendor, licence, site

| | |
|---|---|
| vendor | Anomaly, GitHub organization `anomalyco`, <https://anoma.ly>; pinned repositories include `opencode` and `models.dev` [3][4]. npm maintainer of `@opencode/cli`: `thdxr` [6] |
| licence | MIT, in `@opencode/cli` 2.1.4's `package.json` and on the repository [1][3][6] |
| source | TypeScript, <https://github.com/anomalyco/opencode> [1][3][4] |
| site and docs | <https://opencode.ai>; V2 docs <https://opencode.ai/v2/docs/>; page index <https://opencode.ai/v2/llms.txt> [3][9][26] |
| desktop app | download from opencode.ai/download (beta) [3] |
| mukade name | `opencode` [34] |

**Which "opencode" this page covers:** the npm package `@opencode/cli` 2.1.4 (the name has been used by more than one project).

| check | result |
|---|---|
| installed `package.json` | `"name": "@opencode/cli"`, `"version": "2.1.4"`, `"license": "MIT"`, repository `git+https://github.com/anomalyco/opencode.git`, bins `opencode` and `opencode2` [1]; `opencode.exe --version` → `opencode v2.1.4` [1] |
| npm registry, 2.1.4 | same repository and bins, 12 platform packages `@opencode/cli-<os>-<arch>…` [6] |
| GitHub | tag `v2.1.4` in `anomalyco/opencode`, dated 23 Oct, commit `68bc26b7da1990ce10ffecfbc1e09eb13aa5d824` [5] |
| not this project | `github.com/opencode-ai/opencode`: a Go agent, archived 2025-09-18, continued as Charm's Crush [8]. npm `opencode-ai` (1.19.5 when checked) is the OpenCode 1 line; OpenCode 1 and 2 both install a command called `opencode` [7][25][28] |

### Commands (2.1.4)

All in `opencode --help` of 2.1.4 [1]; no `web` or `db` subcommand in this version.

| use | command | notes |
|---|---|---|
| TUI | `opencode [<directory>]` | full-screen interface [10] |
| minimal TUI | `opencode mini [--session <id>] [--continue] [--fork] [--replay-limit N]` | [1] |
| one-shot / scripts | `opencode run [--session <id>] [--continue] [--fork] [--model <provider/model#variant>] [--agent <a>] [--format default\|json] [--file <f>] [--title <t>] [--thinking] [--auto] [<message...>]` | `--session`: "Session ID to continue, or to create if it does not exist" [1] |
| sessions | `opencode session list [--max-count N] [--format table\|json]` · `session export [<session>] [--sanitize]` · `session import <file-or-URL> [--directory <D>]` · `session delete` | `list`: top-level sessions of the current project, newest first. `delete` also removes child sessions [1] |
| private server | append `--standalone` to `run`, `mini`, `session …`, `models`, `api`, `reload` | [1] |
| server / web | `opencode serve [--hostname H] [--port P] [--cors O] [--service] [--stdio]` | "Start the v2 API and web server" [1]; e.g. `opencode serve --hostname 0.0.0.0 --port 4096` [11] |
| browser or app | `opencode pair [--remote] [--url U]` | one-time links that expire after 5 minutes; needs the shared service [10][11] |
| remote client | `opencode --server http://127.0.0.1:4096` | [11] |
| HTTP API | `opencode api <operation \| method path...> [--data D] [--param k=v]` | e.g. `opencode api get /api/info` [1][14] |
| IDE (ACP) | `opencode acp` | for ACP clients such as Zed: `"agent_servers": {"OpenCode": {"command": "opencode", "args": ["acp"]}}` [12] |

**Client and server.**
- A CLI client "discovers or starts one shared background server for your user account", which owns
  sessions, configuration, integrations, permissions and tool execution [10][14].
- `--standalone` runs a private server instead; `--server <url>` connects to a named one [10].
- The server listens on localhost only: port 49374 on macOS, Linux and WSL, 28894 on Windows [11].
- **The shared service runs with the user's own environment:** a command without `--standalone`
  works on the real home and store, whatever variables the caller set [27 §1.3 rule 3].
- Manage it with `opencode service start|stop|restart|status|get|set|unset`;
  `opencode service set disabled true` makes private servers the default [1][10].

## Session store

| path | holds |
|---|---|
| `<data>/opencode.db` (`<data>` = `$XDG_DATA_HOME/opencode`, else `~/.local/share/opencode`) | every session, one SQLite database [14][2] |
| `<data>/opencode-<channel>.db` | a non-default channel build's database (see `OPENCODE_DISABLE_CHANNEL_DB`) [10][2] |
| `~/.config/opencode` (`$XDG_CONFIG_HOME`) | configuration [2][14] |
| `~/.local/state/opencode` | state, including the shared server's `service.json` [2][14] |
| `~/.cache/opencode`; `<data>/log/opencode.log` | cache; logs [2][14] |

**Tables** [28]:

| table | holds |
|---|---|
| `session_v2` | `id`, `project_id`, `parent_id`, `fork_session_id`, `fork_boundary`, `directory`, `title`, `model`, token counters, idle/compaction/suspension state |
| `session_message` | `id`, `session_id`, `type`, `seq`, timestamps, JSON `data`; unique `(session_id, seq)` |
| also | `session_inbox`, `instruction_*`, `blob`, `blob_ref`; `event_sequence` and `instruction_state` rows belong to a session too [37] |

**Records** (`session_message.type`, and the messages of an export) [27 §2.3]:

| type | meaning | fields mukade reads |
|---|---|---|
| `user` | a prompt | `text`, `files` (attachments) |
| `assistant` | **one model step**, parallel calls included | `content:[reasoning, text, tool…]`, `model{id, providerID}`, `finish`; each `tool` part holds the call and its result (`state{status, input, content}`); reasoning keeps `native.signature` |
| `compaction` | a summary boundary | `status`, `reason`, `summary`, `metadata.keepID`, `tokens.input` |
| `idle` | end of a turn (`{"type":"idle","outcome":"succeeded"}`) | none: not conversation |
| interrupted turn | an assistant row with empty text and no `finish`; the export omits it | none |

- **Order** is `seq`, never time [27 §2.3].
- **Export document** (`session export`): `{info, messages[]}` [27 §2.3][32]. `info` is a
  `Session.Info` (`id`, `projectID`, `location.directory`, `time`, `cost`, `tokens`, `title`,
  `model{id, providerID}`). `session import` takes the same document and answers 409 to a second
  import of the same id [27 §1.3].
- **Lineage:** a subagent's child session by `parent_id`; a fork (`run --fork`) by
  `fork_session_id`/`fork_boundary` [1][28]. `session list` shows top-level sessions only;
  `session delete` removes the children too [1].
- **Agent loop:** the server calls the model, runs the requested tools and continues until the turn
  ends; the `context` hook "runs before each agent-loop model call, including tool-driven
  continuations" [22]. One model step is stored as one assistant message and an `idle` record ends
  the turn [27 §2.3].

## Features

### Compaction

- **Automatic** near the context limit (`compaction.auto`, default `true`), keeping about
  `compaction.keep.tokens` (default 15000) of recent conversation beside the summary;
  `compaction.buffer` defaults to 15 % of the limit, at least 16k [15].
- **Settings:** `agents.compaction.model` picks the summary model;
  `providers.<name>.settings.compaction.type` is `"native"` (OpenAI Responses only) or `"summary"`
  [15]; `experimental.background_compaction` (new in 2.1.4) writes the summary early [15][28].
- **Manual, and headless:** only through the HTTP API:
  `opencode api post /api/session/<id>/compact --data '{}'`, then follow `session.compaction.*`
  events [15]; `run` cannot compact. The bench followed the compact request with
  `POST /api/experimental/session/<id>/wait` (204) [27 §2.3]. The record is written into the
  session it compacts [15].
- **Record:** `{type:"compaction", metadata{keepID:<first kept message>}, status:"completed"|"failed",
  reason:"manual", summary:<model reply verbatim>, …}`. A summary without the template headings
  (`## Objective` … `## Next Move`) fails with
  `Compaction summary did not match the required template` [15][27 §2.3][2].
- **On resume** the model receives one user message
  `<conversation-checkpoint>…<summary>…</summary><recent-context>…</recent-context></conversation-checkpoint>`,
  the kept tail flattened into it as text [27 §2.3]; earlier messages stay in the store [15].

### Ephemeral runs

- **No flag runs without saving:** every `run` writes to the store [1].
- **Fork:** `run --session <id> --fork` copies the session into a new one
  (`session_v2.fork_session_id` = the source, `fork_boundary` `{"type":"through","messageID":…}`)
  and leaves the source's rows unchanged [37].
- **Clean up:** `opencode session delete <id> --standalone` removes the fork with everything it added
  (its `session_v2`, `session_message`, `event_sequence` and `instruction_state` rows): every table's
  row count returns to what it was before the fork [37].
- **No switch removes the tools** of `run`; without `--auto`, permissions that ask are not approved
  [1]. `OPENCODE_CONFIG_CONTENT='{"permission":{"*":"deny"}}'` (merged with the user's own) denies
  every tool: asked to write a file, the model wrote none, and a fork of a session whose history
  holds a tool call was summarized normally [37].
- **Output:** `--format json` prints one event per line (`step_start`, `tool_use`, `text`,
  `step_finish`), each with the `sessionID` it belongs to: for a fork, the fork's id [37]. *2.2.5*:
  the source also writes `reasoning` events (only with `--thinking`) and `error` events [52].

### Tools

Built-in tools [16][27 §2.3]. The 2.1.4 bench offered the model all of them except `patch` [30 §2].
mukade's pinned OpenCode set is 12 tools (`src/tools.rs` `offered()`): `edit`, `glob`, `grep`,
`question`, `read`, `shell`, `skill`, `subagent`, `webfetch`, `websearch`, `write`, `execute`. How
mukade maps them: [`../tool-conversion-tables.md`](../tool-conversion-tables.md).

**Files**

| tool | what it does |
|---|---|
| `read` | file or directory listing (`path`, `offset`, `limit`) |
| `glob` | paths matching a pattern |
| `grep` | file contents search |
| `edit` | exact replacement (`path`, `oldString`, `newString`, `replaceAll`) |
| `write` | create or replace a file (`path`, `content`). *2.2.5*: permission key `edit` [42][54] |
| `patch` | one multi-file patch. *2.2.5*: offered when the model id contains `gpt-` and neither `oss` nor `gpt-4`; `edit` and `write` are then removed; permission key `edit` [42][54] |

**Shell, code, web**

| tool | what it does |
|---|---|
| `shell` | command in the user's shell (`command`, `workdir`, `timeout`, `background`) |
| `execute` | Code Mode JavaScript that calls and combines tools; MCP tools and the `browser`/`opencode` namespaces are reached through it. *2.2.5*: it has its own `fetch` global (see Web tools) [53] |
| `webfetch` | one URL as markdown, text or HTML. *2.2.5*: input `url`, `format` (default `markdown`), `timeout` (seconds, at most 120, default 30); textual responses only [42][50] |
| `websearch` | search through the selected integration. *2.2.5*: one of five providers or the Console-hosted one; input `query`; with none chosen, the first call raises a consent form that a headless run cancels (see Web tools) [41][45][49][51][52] |

**User and agents**

| tool | what it does |
|---|---|
| `question` | asks the user, if the client supports it |
| `skill` | loads one skill's instructions |
| `subagent` | starts a child session with a subagent |

**Gated or Code Mode only (*2.2.5*)** [54]

| tool | condition |
|---|---|
| `todowrite` | registered only when config `experimental.todo` is true. Direct tool or reached through `execute`: **[U]**; its `codemode` flag in `packages/core/src/tool/plugin/` at the tag settles it |
| `experimental_image_generate` | offered only while a connected provider has an image model. Direct or through `execute`: **[U]**, settled the same way |
| `list_mcp_resources`, `read_mcp_resource` | MCP resource tools for Code Mode, reached through `execute`; the registering condition was not read **[U]**. Permission keys `opencode_list_mcp_resources`, `opencode_read_mcp_resource` [42] |
| `artifact_*`, `schedule_*` | gated by config `experimental.artifacts` / `experimental.schedules`; Code Mode, reached through `execute` |
| `opencode` namespace | `session_rename`, `session_move`, `worktree_create\|list\|remove`, `models`; Code Mode, through `execute`; `worktree_remove` has permission key `edit`; the registering condition was not read **[U]** [42] |

**Registration, permissions, read-only (*2.2.5*)** [42][54]
- `codemode` defaults to true; `read`, `glob`, `grep`, `shell`, `subagent`, `skill`, `question`,
  `webfetch`, `websearch`, `edit`, `write` and `patch` register with `codemode: false`.
- No `lsp` tool is among the 19 files listed in `packages/core/src/tool/plugin`.
- Read-only: `read` (files and directories; PNG, JPEG, GIF, WebP and PDF up to 20 MiB reach the
  model), `glob`, `grep`. Switch: permission key = the tool name, resource = the normalised path
  (`read`) or the pattern, not the path (`glob`, `grep`), effect `allow|ask|deny`.

#### Web tools (*2.2.5*)

Read on 2.2.5 only (sources 40 to 54): not run through mukade, not checked on 2.1.4.

**`websearch` backend:** local in OpenCode, calling a third-party search service over HTTP; nothing
runs at the model provider [48][51]. Providers, in registration order [51][41]:

| provider | key |
|---|---|
| `exa` | `EXA_API_KEY` or a stored key |
| `firecrawl` | `FIRECRAWL_API_KEY` |
| `parallel` | `PARALLEL_API_KEY` |
| `tavily` | `TAVILY_API_KEY` |
| `tinyfish` | `TINYFISH_API_KEY` ("optional" in the docs) |

- **Console:** users connected to a Console workspace also get the hosted "OpenCode Web Search": no
  personal key, USD 0.01 per successful search, up to eight results, "zero data retention" [45].
- **No key set:** the request is still sent (Tavily and TinyFish add a keyless-mode header) [51].
  Whether each server accepts it **[U]**: one throwaway `run` turn per provider with no key (state
  `completed`, or `Web search authentication failed (HTTP 401)`) settles it [49].

**Switches**

| tool | default | permission action / resource | off |
|---|---|---|---|
| `websearch` | offered | `websearch` / the query text | config `{"websearch": false}` "removes the web search tool from model requests" [41][43] |
| `webfetch` | offered | `webfetch` / the requested URL | no enable flag; a `deny` rule [43][50] |

- `{"websearch": {"provider": "tavily"}}` fixes a provider and skips the consent form; `"random"`
  picks any available one, retrying another on HTTP 429 [41][43][48][49]. No environment variable
  enables or disables `websearch` [41].
- **Consent form:** with no stored choice the first search raises one (title "Web Search",
  `kind: "websearch.provider"`: allow, choose another, disable). `allow` stores `"random"`, `disable`
  `false`, in KV key `websearch:provider`, kept across sessions; a 1-minute timeout cancels it
  [41][48][49].
- **Headless `run`:** cancels forms (this one, questions); cancelling a non-question form sets exit
  code 1 [52]. Without `--auto` a request that would ask is rejected and the model told "This
  non-interactive run cannot ask the user for permission" (the run goes on); with `--auto` it is
  approved once [52]. A headless turn that should search needs `websearch.provider`, or `false` to
  remove the tool; how the key reaches a `run`: **[U]** (below).

**Events (`run --format json`):** one `{"type","timestamp","sessionID",...}` per line; none for
permission requests, questions or forms [52].

| call | fields |
|---|---|
| search: `tool_use`, `part.tool` = `"websearch"` | `part.state.input` = `{"query": ...}`; `.status` (`completed`/`error`); `.error` (`Web search rate limited (HTTP 429)`, `Web search authentication failed (HTTP 401)`, `Web search request failed (HTTP <status>)`); `.title`, `.time`, `.output`; `.metadata` = `{metadata, content, providerCall, providerResult:{executed,state}, rawInput}`, the tool's `metadata` being `{provider: <id>}` [49][52] |
| fetch: `tool_use`, `part.tool` = `"webfetch"` | `part.state.input` = `{url, format, timeout}` (requested URL; none after a redirect); `.output` = converted page text; tool metadata `{contentType}` only [50][52] |

Search result URLs are in the tool's model-facing content as `[title](url)` links and structured
`results[].url` [49]. Whether they reach `part.state.output` or `part.state.metadata.content` **[U]**
(`toolOutputText` not in the file read), and what `providerCall`/`providerResult` hold for a local
tool: one throwaway `run --standalone --format json` search turn, checking either field for `](http`,
then `opencode session delete <id> --standalone`, settles both [52].

**Limiting a turn**
- **Rules:** `{"action","resource","effect"}` objects in a `permissions` array (V2; V1: `permission`);
  `allow`, `ask`, `deny`; no match → `ask`; last match wins; base rule `* * allow` first, agent rules
  last [43]. Deny-all then allow is a documented pattern; an agent can carry its own `permissions`
  [44].
- **Examples:** search only: `{"action":"*","resource":"*","effect":"deny"}` then
  `{"action":"websearch","resource":"*","effect":"allow"}`. Fetch by URL:
  `{"action":"webfetch","resource":"https://docs.example.com/*","effect":"allow"}`: `*` matches any
  characters including `/`, `?` exactly one, the pattern matches the whole URL [43]. The docs give no
  `webfetch` example; this one is built from the documented syntax.
- **Deny-all removes** `shell`, `edit` (also the key of `write`, `patch`), `subagent`, `skill`,
  `execute` (Code Mode: MCP tools and the `opencode` namespace) and MCP tools (key
  `<server>_<tool>`) [42][43]. Whether MCP servers still start **[U]**: a local MCP server writing a
  marker file on start, under a deny-all `run --standalone` turn, settles it.
- **Redirects, network:** `webfetch` follows redirects by hand (at most 10), checking each target
  with a deny-only check on action `webfetch`, no prompt [50]. No OS sandbox or tool network
  allowlist is documented, only `HTTP_PROXY`, `HTTPS_PROXY`, `NO_PROXY` [46].
- **`execute`'s own `fetch`:** asserts `webfetch` on the original URL only (no re-check after
  redirects), 30 s limit, no size cap, any HTTP method, refuses `file:` URLs [53]. A turn with
  restricted web access must deny `execute` too.

**Not established [U], and what settles it**
1. That the `websearch` key reaches a `run` through `OPENCODE_CONFIG_CONTENT` with `--standalone` and
   beats the stored `websearch:provider` (the source reads the in-memory selection, then the KV; that
   the config key fills it is not read [48]).
   `OPENCODE_CONFIG_CONTENT='{"websearch":false}' opencode debug config`
   ("List configuration sources") and `opencode debug agents` ("List all agents") show the inline source and the agent's rules without a model turn; one model turn shows whether
   the tool is absent [40].
2. That the V2 `permissions` array works through the same variable (mukade's V1
   `{"permission":{"*":"deny"}}` denied every tool at 2.1.4 [37]): the same `debug` commands [40].
3. Which stream (stdout or stderr) carries `permission requested: <action> (<resources>); auto-rejecting`
   [52]: a `run` turn with the streams captured apart.
4. Whether `webfetch` can be limited natively to the URLs this turn's search found (patterns are
   static; workable: a second turn whose config holds exact allow rules for them): a config denying
   `*` and allowing one URL that redirects to another host would show the redirect refused in
   `state.error` [43][50].

### Branches, forks and subagents

- **Built-in agents** [17]: primary `build` (default) and `plan`; subagents `general` and `explore`;
  hidden `compaction`, `title`, `summary`.
- **Custom agents** [17]: `~/.config/opencode/agents/<name>.md` or `.opencode/agents/<name>.md`
  (body = system prompt), or the `agents` config key; `mode` is `primary` (default), `subagent` or `all`.
- **Subagents** run in a foreground or background child session [17]; **forks** (`run --fork`):
  Ephemeral runs. Storage of both: Session store, Lineage [1][28].
- Navigating the session tree in the TUI: **[U]**.

### Models and providers

- **Models:** `provider/model[#variant]` in the `model` config key, `/models` in the TUI and
  `--model` on `run`; `opencode models` lists them [18][1].
- **Catalog** from models.dev; providers configured under `providers` [19].
- **Providers:** `providers.<id>` with `settings.baseURL`, `settings.apiKey` (accepts `{env:VAR}`),
  `models`, `env` [19]. The bench's shape:
  `{"providers":{"anthropic":{"settings":{"baseURL":…,"apiKey":…},"models":{…}}}}` [27 §2.3].
- **Credentials:** `/connect` in the TUI [19] or `opencode auth login` [1].
- **Reasoning replay:** a stored signature is sent back only when the stored provider and model
  equal the current ones [27 §2.3].

### Plugins and extensions

- **Plugins** [20][1]. Listed in the `plugins` array of `opencode.json(c)` (global, project and
  `.opencode/` arrays are merged) as an npm name (`@version` optional), a local or `file://` path, or
  `{ "package": …, "options": {…} }`; `.opencode/plugins/` and `~/.config/opencode/plugins/` load
  without configuration. CLI: `opencode plugin add <package>`, `plugin list [--builtin]`,
  `plugin check`, `plugin update`, `plugin remove`. TUI-only plugins go in `cli.json`.
- **Writing a plugin** [21][2]. Import `@opencode/plugin` and call
  `Plugin.define({ id, async setup(ctx) {…} })`; a package exports `./server` (tools, hooks,
  integrations, commands, agents), `./tui`, `./gui-experimental` and `./rpc`.
- **MCP servers** [23][16]. Under `mcp.servers.<name>`: local
  `{type:"local", command, cwd, environment, disabled, codemode, timeout, protocol}` or remote
  `{type:"remote", url, headers, oauth, …}`. CLI: `opencode mcp add context7 --url
  https://mcp.context7.com/mcp`, `opencode mcp add everything -- npx -y
  @modelcontextprotocol/server-everything`, `mcp list`, `mcp auth <name>`, `mcp logout <name>`.
  MCP tools are reached through `execute` (Code Mode) unless the server sets `"codemode": false`.
- **Custom commands** [24]. `~/.config/opencode/commands/*.md` or `.opencode/commands/*.md` (nested
  folders become `/team/review`), or the `commands` key (each entry needs a `template`). Templates
  take `$ARGUMENTS`, `$1`, `$2`… and `!`-shell blocks.
- **Skills and Claude Code compatibility.** Skills are documented at
  <https://opencode.ai/v2/docs/skills/> [26]; details **[U]** (not read). The binary reads
  `OPENCODE_DISABLE_CLAUDE_CODE` and `OPENCODE_DISABLE_CLAUDE_CODE_SKILLS` [2]; the bench set the
  first to `1` [27 §2.3]; what each disables: **[U]**.

### Hooks

No hooks section in the configuration file: the 2.1.4 config page lists no `hooks` key [13]. Hooks
are registered by server plugins in `setup(ctx)` [22]:

| registration | hook names |
|---|---|
| `ctx.session.hook(name, cb, options?)` | `prompt`, `context`, `compaction` (setting `result` skips the model call), `generate`, `title`, `model.request` (can be scoped by `providerID`), `http.request`, `http.response`, `retry`, and the experimental `experimental.ws.handshake`, `experimental.ws.send`, `experimental.ws.receive` |
| `ctx.permission.hook` | `evaluate` (not called for a configured `deny`) |
| `ctx.shell.hook` | `create.before` |
| `ctx.tool.hook` | `execute.before`, `execute.after` (`status` is `"completed"` or `"error"`) |
| `ctx.event.subscribe({ types })` | events such as `config.updated` and `worktree.updated`; `server.connected` is excluded. No full event catalogue on that page [22]; compaction emits `session.compaction.*` [15] |

The V1 plugin "object form" is supported from OpenCode `1.18.29` [22]. The V1-style hook name
`tool.execute.before` does not occur in the 2.1.4 binary [2].

## How mukade reads it

Code: `src/reader/opencode.rs` [31].

- **`list`** needs the database selected (`OPENCODE_DB` or `--root`), else
  `OPENCODE_DB is not set: no OpenCode store to list`. It opens the file read-only (2 s busy timeout)
  without starting OpenCode; returns root sessions (`parent_id IS NULL`), newest `time_updated`
  first, archived included (`archived` = `time_archived` set; `--archived false` leaves them out).
- **`read`** (`dump`, a hop source) runs `<exe> session export <id> --standalone` (stdin closed, 120 s
  `EXPORT_TIMEOUT`) and parses `{info, messages}`. Stderr containing `not found` is a not-found
  error. The SHA-256 of the printed document is the source fingerprint (`--expect-sha256`).
- **`--from-file`** takes an export document, a `{session_v2, session_message}` row dump or a bare
  message array; when every row has a `seq`, rows are put in `seq` order.

**Sessions it lists but does not read** (each carries `unreadable` and `location` = its file;
[usage](../usage.md#the-list-output)):
- **Other database files.** When the selected file is named like OpenCode's own (`opencode.db`,
  `opencode-<channel>.db`), the other such files in its folder are listed too: a channel build starts
  a file of its own and leaves the old one with its sessions. On the machine of 2026-11-08,
  `opencode-.db` held 62 sessions in 2 folders that `opencode.db` does not have [55]. Another name
  (`--root sessions.db`, a test store) stands alone. An unreadable sibling is skipped.
- **Legacy rows.** A session only the pre-2.0 `session` table has (not yet moved to `session_v2`) is
  listed with the reason `legacy-schema row: OpenCode has not moved it to session_v2 and cannot
  export it`. OpenCode 2.2.5 did not export the newest of them (`Session not found`), and opening
  that database with it (on a copy) created `session_v2` and moved 37 of the 62 rows: it migrates
  what it opens [55].
- **`read` refuses them** (exit 2, naming the file and `--root`) **without starting OpenCode**: a
  stale channel file is not for mukade to change. Select that file (`--root` or `OPENCODE_DB`) to read it.

**Mapping**

| OpenCode record | mukade entry |
|---|---|
| `user` | User, `files` as attachments |
| `assistant` | one Step: reasoning, text, one call per `tool` part plus its result. `finish`: `stop`/`end_turn` completed, `tool-calls`/`tool_use` tool use, `aborted` Aborted, `error`/`refusal` Error, none Killed |
| reasoning part | `native.signature` / `native.encrypted`, or the complete `native.reasoningDetails` array kept in `Opaque.chat` with the source provider id (below) |
| `tool` part | `pending`/`running`: no result. `error`: result = `state.error.message` when there is no content, marked error. `state.metadata.truncated` kept |
| interrupted turn | a step with no `finish` (`Killed`) or `finish:"aborted"` (`Aborted`). Travels by default; `--interrupted drop` leaves its steps and results out, its prompt stays |
| `compaction`, `status:"completed"` | Compaction: `kept_from` = `metadata.keepID`, trigger from `reason` (`manual`, `auto`), tokens before from `tokens.input` |
| failed `compaction`, `idle`, unknown types | Meta (not conversation) |

**Reasoning kept whole** (rules: [spec §3.3](../session-interchange-spec.md#33-reasoning)): the
whole `native.reasoningDetails` array is kept in `Opaque.chat` with the source provider id (summary-first
and multiple encrypted entries, IDs, indices, extension fields; native OpenAI Gateway and OpenRouter
formats). `format` gives the protocol: `anthropic-claude-v1` → Anthropic Messages;
`openai-responses-v1` and `openai-native-responses-v1` → OpenAI Responses; mixed formats are
`mixed-reasoning-details` and do not borrow the first detail's protocol (`src/reader/opencode.rs`
`detail_protocol()`). Without details, the protocol comes from the message's `model.providerID`.

**Limits, most costly first**
- **Child sessions are not followed:** one session is read, its `parent_id` kept; subagent calls
  travel as text form [30 §3][31].
- **Sessions in a sibling file or the legacy table** are listed, not read (above).
- **No SQLite read of a transcript:** `list` reads the selected database's session table and its
  siblings read-only; `read` goes through `opencode session export` [31].

## How mukade writes it

Code: `src/writer/opencode.rs` [32].

**Refused before anything is written** [35] (exit 2): `--dest-path` (the import allocates the file);
a `cwd` starting with `-`; a `--dest-id` other than `ses_` plus letters, digits, `_`, `-` (at most
128 characters); a `--dest-agent` empty, over 256 characters or with control characters; a
`--metadata-file` that is not a JSON object file of at most 1 MiB.

**Steps**
1. Build `{info, messages}`: id = `--dest-id` or a new `ses_<uuid>`; message ids `msg_<uuid>`.
2. Write it as `mukade-import-<id>-<uuid>.json` (new, 0600 on Unix) in `mukade-data/tmp` (or
   `$MUKADE_DATA_DIR/tmp`); leftovers of a killed run older than an hour are swept. If the folder
   cannot be made, the hop stops with an error naming it, before anything is written [35].
3. Run `<exe> session import <file> --directory <cwd> --standalone` (120 s `IMPORT_TIMEOUT`), with
   `OPENCODE_DB` passed when set, removed otherwise. The document is removed on success and failure.
4. Require `Imported session: <id>` on stdout, not just a zero exit. With `--dest-id` or
   `--metadata-file`, the reported id must match and the session (and its `metadata`) is checked
   read-only in `session_v2`, so `OPENCODE_DB` must name the destination. No such line, a failed or
   timed-out import, or a check that cannot confirm: exit 5 with the id; inspect the destination
   before retrying [35].
5. Receipt `location`: the `OPENCODE_DB` path, else `opencode's configured store, session <id>`.
   Resume: `<exe> run --standalone --session <id>`, a resume prompt appended last [34 `hop.rs`].

`mukade document opencode <src-harness> <session-id>` prints the same document without writing (`ses_preview` unless `--dest-id`) [35]; fields: [usage §3](../usage.md#mukade-document).

**Lookups** (`src/reader/opencode.rs` `project_id_for()`, `src/dest.rs` `configured_model_in()`) [31][34]
- **Project id:** `--project-id`, else `session list --standalone` run inside `--cwd` against the
  destination store; up to 5 `ses_` ids are exported, the first whose `info.location.directory`
  equals the cwd (case, `\`/`/`, trailing slash ignored) gives `info.projectID`. Cached in
  `mukade-data/cache/probes.json` per binary (path, size, modification time), database and
  directory. With no session there yet the write fails:
  "OpenCode needs the destination's project id (`--project-id`)".
- **Provider and model:** `--model-provider`/`--model`, else the `model` key (`provider/model`) of the
  configuration beside the store (`<root>/config/opencode/opencode.json` for
  `<root>/share/opencode/opencode.db`; when that file exists it alone answers), else
  `OPENCODE_CONFIG`, else `$XDG_CONFIG_HOME/opencode/opencode.json`, else the source's first model. A
  provider is required: "OpenCode needs the destination's provider id (`--model-provider`)".
- **Version:** `--dest-version`, else the npm `package.json` the binary belongs to, else a cached
  answer, else the first `x.y.z` of `<exe> --version`.
- Only `OPENCODE_DB` is set for the import; the engine runs with the `XDG_*` configuration of mukade's shell ([How mukade finds it](#how-mukade-finds-it)) [35].

**Import document shape.** Each item was probed against 2.1.4: without it, `session import` answers
`Bad Request` [32][27 §2.3].

| part | shape mukade writes |
|---|---|
| `info` | `{id, projectID, location:{directory}, time:{created, updated}, cost:0, tokens:{input, output, reasoning, cache:{read, write}}, title?, metadata?, model:{id, providerID}, agent?}`. `projectID`, `cost`, `tokens` are required together; `model` needs both fields. `metadata` from `--metadata-file`, `agent` from `--dest-agent` |
| user message | `{id, type:"user", time:{created}, text, files:[…]}` |
| assistant message | `{id, type:"assistant", agent, model:{id, providerID}, content:[…], finish:"stop"\|"tool-calls", rawFinish:"end_turn"\|"tool_use", time:{created, completed}, cost:0}`, one per step (L8); `agent` = `--dest-agent`, else `"build"` |
| reasoning part | `{type:"reasoning", text, native:{…}}`, only when the reasoning policy keeps it native (below) |
| tool part | `{type:"tool", id, name, executed:false, state:{input, status:"completed"\|"error", content:[…], error?, metadata:{truncated:false}}, time:{created, completed}}` |
| failed call | `state.error` = `Session.StructuredError` `{"type":"tool.execution", "message":<result text>}`, the type OpenCode records for a failed tool [32][2] |
| user image | `Prompt.FileAttachment` `{data, mime, source:{type:"inline"}}` in `files` [32 `media.rs`][2] |
| tool-result image | `Tool.FileContent` `{type:"file", uri:"data:<mime>;base64,<data>", mime}` [32 `media.rs`][2] |
| call without an OpenCode equivalent | the call goes into the assistant text, its result follows as text ([tool conversion §1](../tool-conversion-tables.md)) |
| source compaction | one user message carrying the summary |
| `--compaction native` | the whole history, then `{"type":"compaction","status":"completed","reason":"manual","summary":…,"metadata":{"keepID":<first kept message>},"time":{"created":…}}` with a deterministic summary of the earlier user requests; live OpenCode 2.2.5 resumed such a session and answered from the summary (2026-11-04) [27 §4.4][56] |

**Reasoning into OpenCode** (rules: [spec §3.3](../session-interchange-spec.md#33-reasoning); policy:
[usage §5](../usage.md#tool-calls-and-reasoning-what-is-converted-and-when)) [27 §3.3]
- **Name the protocol.** OpenCode speaks whatever its provider speaks, so with the default
  `drop_foreign` signed reasoning is dropped with "the destination's provider protocol is not known;
  name it with --dest-protocol". `--dest-protocol openai-responses` names the reasoning protocol even
  when the Gateway-facing envelope is Chat Completions; it does not override an incompatible provider
  format. `as_text` writes foreign reasoning labelled `[earlier reasoning, not replayable]`.
- **A kept `Opaque.chat`** is written back as the original complete array when the destination
  provider is the source provider. A single ordinary Anthropic detail is also accepted toward
  `openrouter` (same array) and `anthropic` (scalar `native.signature` / `native.encrypted`); a native
  `openai-native-responses-v1` array toward `openrouter` is refused (`src/policy.rs`
  `chat_destination_fits()`).
- **Scalar input** (`native.signature` / `native.encrypted` only): toward `openrouter`, OpenRouter's
  shape, which OpenCode keeps and replays there:
  `native:{reasoningDetails:[{type:"reasoning.text", text, signature, format, index}]}` for signed
  text, `{type:"reasoning.encrypted", data, format, index}` for an encrypted payload; `format`
  `anthropic-claude-v1` or `openai-responses-v1` from the block's protocol (seen in a live 2.1.4
  export; verified live 2026-10-28: one changed signature character is refused by the provider,
  `Invalid signature in thinking block`). Toward any other provider: `native:{signature?, encrypted?}`.
- **An unknown protocol, an incompatible provider or an unrepresentable envelope:** a counted loss in the
  manifest, never an unsupported field. Regression:
  [`tests/reasoning_replay.rs`](../../tests/reasoning_replay.rs). The 2026-11-02 check imported
  converted arrays through a real OpenCode 2.2.3 importer and found them unchanged on their owners:
  import and storage proven, not an outgoing request or provider acceptance [57].

**The npm launcher.** `%APPDATA%\npm\opencode.cmd` only forwards `%*` [36]; it adds no `--standalone`. Any `.cmd` launcher: a multi-line resume prompt is refused before anything is written (Rust cannot pass a line break to a batch file; `src/hop.rs` `check_resume_prompt()`, `src/exe.rs` `is_batch()`) [34][33]; on timeout the child is `cmd.exe`, so the tree is killed with `taskkill` (`src/exe.rs` `run_within()`, `kill_tree()`) [33].

**`--standalone`** is on every command mukade spawns that touches the store (export, import, `session list`, `session delete`, `run`, the resume command), so no work reaches the user's background service (L1); only the version probe `opencode --version` runs without it [27 §1.3][31][32][34]. **Deadlines:** export and import 120 s; `--version`, `session list` and project-id probes 30 s each (`PROBE_TIMEOUT`); resume prompt 300 s by default; past one, the process is killed (Windows: the whole tree) [33].

**Limits, most costly first**
- `--compaction` toward OpenCode: `none`, `portable`, `summary`, `native`; the library's
  `DestinationNative` (OpenCode's own HTTP compaction) returns `Unsupported`, so the caller runs it
  [34 `hop.rs`][27 §4.3].
- `opencode run --session <unknown id>` silently creates a new session [27 §5][1]; use the id the
  receipt returns.
- No direct SQLite write: everything goes through `session import` [32].

## How mukade summarizes it

With `--compaction summary` (or `--intent NAME`) and OpenCode as the source, mukade runs
`<exe> run --standalone --session <id> --fork --format json [-m <provider>/<model>] <prompt>` with
`OPENCODE_CONFIG_CONTENT='{"permission":{"*":"deny"}}'`, from the session's directory when it still
exists, within `--summary-timeout` (300 s by default) (`src/summary.rs`) [37].

- **Reply:** the `text` events joined; a `tool_use` or `error` event fails the summary, as does an
  empty reply.
- **Fork:** the `sessionID` of the first event; deleted (`opencode session delete <fork id>
  --standalone`, 30 s) whatever came of the run. If that fails, a successful hop says
  `the summarizing run left a fork in the source store: run <command>` in its manifest, and a failed
  summary in its error.
- **Model:** `--summary-model` as given (`provider/model`); else the session's last model with its
  provider, or none when the provider is unknown; then the manifest still names the recorded model
  though OpenCode ran with its default (a review point in next.md).
- **`opencode.cmd`:** the prompt must be one line of ASCII letters, digits, spaces and `.,;:()`, or
  it is refused; an edited intent prompt can break this.
- **Refused** (exit 2): `--from-file` for an OpenCode source, and `--all-branches` [27 §4.5].

## Open points

Ranked by cost to a user. The rest: [../../next.md](../../next.md) and the **[U]** marks above.

1. **Only 2.1.4 is verified end to end [U].** 2.2.0 already changes media blobs, compaction start and
   instruction persistence [6][28]. Re-running reader, writer and probes against 2.2.5 in a scratch
   store settles it.
2. **OpenCode's own `anthropic` provider [U]:** `native.signature` / `native.encrypted` written, only `openrouter` verified live; needs an API key (next.md item 14).
3. **OpenAI encrypted reasoning through OpenRouter [U]:** written as `reasoning.encrypted`, never exercised; needs an API key (next.md item 15).
4. **Replay of a kept `reasoningDetails` array [U]:** import and storage are proven, the outgoing
   request and provider acceptance are not. Observing at least two sequential tool steps (three model
   requests) and the outgoing proof settles it [27 §3.3].
5. **Web access of a headless turn [U]:** whether `OPENCODE_CONFIG_CONTENT` carries the `websearch`
   key and the V2 `permissions` array, and whether MCP servers start under deny-all. The `debug
   config` / `debug agents` checks and throwaway turns under Web tools settle it.

## Sources

All accessed 2026-10-28 unless a source says otherwise.

1. Installed package `C:\Users\alfio\AppData\Roaming\npm\node_modules\@opencode\cli`: `package.json`, `postinstall.mjs`, and the output of `bin\opencode.exe --version` and `--help` for these subcommands: `session`, `session list|export|import`, `run`, `serve`, `service`, `plugin`, `plugin add|list`, `mcp`, `mcp add`, `acp`, `upgrade`, `uninstall`, `mini`, `pair`, `debug`, `debug paths|config`, `auth`, `models`, `api`, `reload`.
2. Text of the binary `…\@opencode\cli\bin\opencode.exe` 2.1.4 (grep for the database path code, XDG paths, `OPENCODE_*` names, `@opencode/plugin/*` modules and schema identifiers).
3. GitHub, anomalyco/opencode, <https://github.com/anomalyco/opencode>.
4. GitHub, Anomaly organization, <https://github.com/anomalyco>.
5. GitHub release v2.1.4, <https://github.com/anomalyco/opencode/releases/tag/v2.1.4>.
6. npm registry, `@opencode/cli` 2.1.4, <https://registry.npmjs.org/@opencode/cli/2.1.4>, and package document (dist-tags, maintainers), <https://registry.npmjs.org/@opencode/cli>.
7. npm registry, `opencode-ai` latest, <https://registry.npmjs.org/opencode-ai/latest>.
8. GitHub, opencode-ai/opencode (archived), <https://github.com/opencode-ai/opencode>.
9. OpenCode docs, Intro, <https://opencode.ai/v2/docs/>.
10. OpenCode docs, CLI, <https://opencode.ai/v2/docs/cli/>.
11. OpenCode docs, CLI › Web, <https://opencode.ai/v2/docs/cli/web/>.
12. OpenCode docs, CLI › ACP, <https://opencode.ai/v2/docs/cli/acp/>.
13. OpenCode docs, Config, <https://opencode.ai/v2/docs/config/>.
14. OpenCode docs, Troubleshooting, <https://opencode.ai/v2/docs/troubleshooting/>.
15. OpenCode docs, Compaction, <https://opencode.ai/v2/docs/compaction/>.
16. OpenCode docs, Tools, <https://opencode.ai/v2/docs/tools/>.
17. OpenCode docs, Agents, <https://opencode.ai/v2/docs/agents/>.
18. OpenCode docs, Models, <https://opencode.ai/v2/docs/models/>.
19. OpenCode docs, Providers, <https://opencode.ai/v2/docs/providers/>.
20. OpenCode docs, Plugins, <https://opencode.ai/v2/docs/plugins/>.
21. OpenCode docs, Build › Plugins overview, <https://opencode.ai/v2/docs/build/plugins/>.
22. OpenCode docs, Build › Plugins › Server, <https://opencode.ai/v2/docs/build/plugins/server/>.
23. OpenCode docs, MCP servers, <https://opencode.ai/v2/docs/mcp-servers/>.
24. OpenCode docs, Commands, <https://opencode.ai/v2/docs/commands/>.
25. OpenCode docs, Migrate from V1, <https://opencode.ai/v2/docs/migrate-v1/>.
26. OpenCode docs index, <https://opencode.ai/v2/llms.txt>.
27. Repository: [`docs/session-interchange-spec.md`](../session-interchange-spec.md) §1.3, §2.3, §3.3, §4, §5.
28. Repository: [`docs/reference/harness-session-persistence-reference.md`](../reference/harness-session-persistence-reference.md), OpenCode dossier and version table (secondary source).
30. Repository: [`docs/tool-conversion-tables.md`](../tool-conversion-tables.md) §2, §3.
31. Repository: [`src/reader/opencode.rs`](../../src/reader/opencode.rs).
32. Repository: [`src/writer/opencode.rs`](../../src/writer/opencode.rs), [`src/writer/media.rs`](../../src/writer/media.rs).
33. Repository: [`src/exe.rs`](../../src/exe.rs).
34. Repository: [`src/cli.rs`](../../src/cli.rs) (`opencode_project_id`), [`src/roots.rs`](../../src/roots.rs) (`StoreRoots::set_root`), [`src/dest.rs`](../../src/dest.rs) (`configured_model_in`), [`src/hop.rs`](../../src/hop.rs).
35. Repository: [`README.md`](../../README.md), [`docs/usage.md`](../usage.md), [`src/detect/opencode.rs`](../../src/detect/opencode.rs).
36. Local npm launcher `C:\Users\alfio\AppData\Roaming\npm\opencode.cmd` (150 bytes, generated by npm for `@opencode/cli`).
37. [`docs/verification/source-model-summary-live.md`](../verification/source-model-summary-live.md): the live check of 2026-10-28 (throwaway sessions, source hashed before and after; Codex against a local mock).
38. Official install script, https://opencode.ai/v2/install, accessed 2026-10-30 (`INSTALL_DIR=$HOME/.opencode/bin`, Windows `.exe` handling); `opencode debug paths db` with `OPENCODE_DB=custom.db` and `:memory:` on this machine, 2026-10-30.
39. `opencode --version` of the installed binary, printing `opencode v2.2.5`, accessed 2026-11-01.
40. `opencode run --help` and `opencode debug --help` of the installed 2.2.5 binary, accessed 2026-11-01.
41. OpenCode docs, Web search, <https://opencode.ai/v2/docs/websearch/>, accessed 2026-11-01.
42. OpenCode docs, Tools, <https://opencode.ai/v2/docs/tools/>, accessed 2026-11-01.
43. OpenCode docs, Permissions, <https://opencode.ai/v2/docs/permissions/>, accessed 2026-11-01.
44. OpenCode docs, Agents, <https://opencode.ai/v2/docs/agents/>, accessed 2026-11-01.
45. OpenCode docs, Console › Web search, <https://opencode.ai/v2/docs/console/websearch/>, accessed 2026-11-01.
46. OpenCode docs, Network, <https://opencode.ai/v2/docs/network/>, accessed 2026-11-01.
47. GitHub releases of anomalyco/opencode (v2.2.5, published 2026-10-30), <https://api.github.com/repos/anomalyco/opencode/releases?per_page=8>, accessed 2026-11-01.
48. Source `packages/core/src/websearch.ts` at tag `v2.2.5` of <https://github.com/anomalyco/opencode>, accessed 2026-11-01 (summarising fetch).
49. Source `packages/core/src/tool/plugin/websearch.ts` at tag `v2.2.5`, accessed 2026-11-01 (summarising fetch).
50. Source `packages/core/src/tool/plugin/webfetch.ts` at tag `v2.2.5`, accessed 2026-11-01 (summarising fetch).
51. Source `packages/core/src/plugin/websearch/{index,exa,firecrawl,parallel,tavily,tinyfish,mcp}.ts` at tag `v2.2.5`, accessed 2026-11-01 (summarising fetch).
52. Source `packages/cli/src/run/noninteractive.ts` at tag `v2.2.5`, accessed 2026-11-01 (summarising fetch).
53. Source `packages/core/src/codemode/{tool,fetch}.ts` at tag `v2.2.5`, accessed 2026-11-01 (summarising fetch).
54. Source `packages/core/src/tool/plugin/*.ts` (file listing: read, glob, grep, edit, write, shell, subagent, skill, question, patch, todo, artifact, schedule, opencode, mcp-resource, image-generate) and `packages/core/src/tool/AGENTS.md` at tag `v2.2.5`, accessed 2026-11-01 (summarising fetch).
55. [`docs/verification/listing-coverage-2026-11-08.md`](../verification/listing-coverage-2026-11-08.md): the listing check of 2026-11-08 on the real stores (OpenCode 2.2.5).
56. [`docs/verification/native-compaction-live.md`](../verification/native-compaction-live.md): the live native-compaction check of 2026-11-04 (OpenCode 2.2.5).
57. [`docs/verification/2026-11-02-opaque-reasoning.md`](../verification/2026-11-02-opaque-reasoning.md): the opaque-reasoning check of 2026-11-02 (OpenCode 2.2.3 importer).
