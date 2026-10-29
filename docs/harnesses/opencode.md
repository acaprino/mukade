# OpenCode

| | |
|---|---|
| **Documentation updated** | 2026-10-28 |
| **Version described** | 2.1.4 (the version mukade's readers and writers were verified against) |
| **Vendor** | Anomaly (GitHub organization `anomalyco`, <https://anoma.ly>) [3][4] |
| **Website** | <https://opencode.ai>; V2 documentation at <https://opencode.ai/v2/docs/> [3][9] |
| **License / source** | MIT; TypeScript source at <https://github.com/anomalyco/opencode> [1][3][4] |
| **mukade name** | `opencode` |

## 1. Introduction

OpenCode is an open-source AI coding agent with a terminal interface, a desktop app and a web
app [9]. Version 2 (npm package `@opencode/cli`) keeps every session in one SQLite database and
offers its own `session export` and `session import` commands [1][10][27]. mukade reads OpenCode
through that export and writes through that import. It does not write OpenCode's database itself.

**Which "opencode" this page covers.** More than one project has used the name. This page covers
the npm package `@opencode/cli` 2.1.4. The way this was established:

| check | result |
|---|---|
| installed `package.json` | `"name": "@opencode/cli"`, `"version": "2.1.4"`, `"license": "MIT"`, repository `git+https://github.com/anomalyco/opencode.git`, bins `opencode` and `opencode2` [1] |
| `opencode.exe --version` | `opencode v2.1.4` [1] |
| npm registry, version 2.1.4 | same repository, same bins, 12 platform packages `@opencode/cli-<os>-<arch>…` [6] |
| GitHub | tag `v2.1.4` in `anomalyco/opencode`, dated 23 Oct, commit `68bc26b7da1990ce10ffecfbc1e09eb13aa5d824` [5] |
| not this project | `github.com/opencode-ai/opencode`: a Go agent, archived on 2025-09-18 and continued as Charm's Crush [8]. npm `opencode-ai` (1.19.5 when checked) is the OpenCode 1 line. OpenCode 1 and 2 both install a command called `opencode` [7][25][28] |

## 2. Description

- **Client/server.** A CLI client "discovers or starts one shared background server for your user
  account", which owns sessions, configuration, integrations, permissions and tool execution
  [10][14]. `--standalone` runs a private server instead; `--server <url>` connects to a named one
  [10]. The server listens on localhost only: port 49374 on macOS, Linux and WSL, 28894 on Windows [11].
- **The shared service runs with the user's own environment.** A command without `--standalone`
  works on the real home and store, whatever variables the caller set [27 §1.3 rule 3]. Manage
  it with `opencode service start|stop|restart|status|get|set|unset`; `opencode service set
  disabled true` makes private servers the default [1][10].
- **Agent loop.** The server calls the model, runs the requested tools and continues until the turn
  ends; the `context` hook "runs before each agent-loop model call, including tool-driven
  continuations" [22]. One model step is stored as one assistant message, parallel calls included,
  and an `idle` record ends the turn [27 §2.3].
- **Providers.** The catalog comes from models.dev; providers are configured under `providers`, and
  credentials are added with `/connect` in the TUI [19] or `opencode auth login` [1].

## 3. Entry points

All of the following appear in `opencode --help` of 2.1.4 [1]. There is no `web` or `db`
subcommand in this version.

| entry point | command | notes |
|---|---|---|
| TUI | `opencode [<directory>]` | full-screen interface [10] |
| minimal TUI | `opencode mini [--session <id>] [--continue] [--fork] [--replay-limit N]` | [1] |
| one-shot / scripts | `opencode run [--session <id>] [--continue] [--fork] [--model <provider/model#variant>] [--agent <a>] [--format default\|json] [--file <f>] [--title <t>] [--thinking] [--auto] [<message...>]` | `--session`: "Session ID to continue, or to create if it does not exist" [1] |
| sessions | `opencode session list [--max-count N] [--format table\|json]` · `session export [<session>] [--sanitize]` · `session import <file-or-URL> [--directory <D>]` · `session delete` | `list` shows top-level sessions of the current project, newest first. `delete` also removes child sessions [1] |
| private server | append `--standalone` to `run`, `mini`, `session …`, `models`, `api`, `reload` | [1] |
| server / web | `opencode serve [--hostname H] [--port P] [--cors O] [--service] [--stdio]` | "Start the v2 API and web server" [1]. Example: `opencode serve --hostname 0.0.0.0 --port 4096` [11] |
| browser or app | `opencode pair [--remote] [--url U]` | prints one-time links that expire after 5 minutes. Requires the shared service [10][11] |
| remote client | `opencode --server http://127.0.0.1:4096` | [11] |
| HTTP API | `opencode api <operation \| method path...> [--data D] [--param k=v]` | e.g. `opencode api get /api/info` [1][14] |
| IDE (ACP) | `opencode acp` | for ACP clients such as Zed: `"agent_servers": {"OpenCode": {"command": "opencode", "args": ["acp"]}}` [12] |
| desktop app | download from opencode.ai/download (beta) | [3] |

## 4. Harness type

### Vendor

Anomaly, the GitHub organization `anomalyco`. Its pinned repositories include `opencode` and
`models.dev` [4]. The npm maintainer of `@opencode/cli` is `thdxr` [6].

### Website and documentation

Product site: <https://opencode.ai>. V2 docs: <https://opencode.ai/v2/docs/>. The full page index is
at <https://opencode.ai/v2/llms.txt> [26].

### Installation (each official method with exact commands, per OS)

| OS | method | command |
|---|---|---|
| macOS, Linux | install script (puts the binary in `$HOME/.opencode/bin`) | `curl -fsSL https://opencode.ai/v2/install \| bash` [3][9] |
| macOS, Linux | Homebrew tap (recommended in the README) | `brew install anomalyco/tap/opencode-v2` [3][9] |
| macOS, Linux | Homebrew core formula ("updated less") | `brew install opencode` [3]. Which major version it installs: **[U]** |
| any (incl. Windows) | npm | `npm install -g @opencode/cli`. Pinned: `npm install -g @opencode/cli@2.1.4` [6][9] |
| any | bun / pnpm / yarn / vite+ | `bun install -g --trust @opencode/cli` · `pnpm add -g --allow-build=@opencode/cli @opencode/cli` · `yarn global add @opencode/cli` · `vp install -g @opencode/cli` [9] |
| Arch Linux | AUR | `paru -S opencode-beta` [3][9]. The README also lists `sudo pacman -S opencode`. Which line that package carries: **[U]** |
| any | mise ≥ 2026.10.9 (older mise installs OpenCode 1) | `mise use -g opencode` [9] |
| any | Nix | `nix run github:anomalyco/opencode` [3] |
| macOS, Windows, Linux (glibc, musl) | standalone archive | `https://opencode.ai/update/api/latest/cli/opencode/opencode-<os>-<arch>[-baseline][-musl].zip\|.tar.gz`, e.g. `…/opencode-windows-x64.zip`. These URLs always serve the *latest* release [9] |
| any | Docker image | `ghcr.io/anomalyco/opencode:<version>`, e.g. `:2.0.0` [9]. Whether a `:2.1.4` tag exists: **[U]** |

- The npm postinstall copies the native binary of the matching optional dependency
  (`@opencode/cli-windows-x64`, …) into `bin/` [1][9]. Here: `%APPDATA%\npm\node_modules\@opencode\cli\bin\opencode.exe` [1].
- "Windows package managers are not supported" [9], yet `opencode upgrade --method` offers
  `winget` [1]: **[U]** whether winget works.
- Remove a package-managed V1 install first; the V2 curl installer replaces the V1 binary [25].
- Upgrade or pin: `opencode upgrade [<target>] [--method curl|npm|pnpm|bun|yarn|vp|brew|winget]`.
  Remove: `opencode uninstall [--dry-run] [--force]` (keeps sessions, configuration, state) [1][10].

### Licence

MIT, declared in `@opencode/cli` 2.1.4's `package.json` and on the repository [1][3][6].

## 5. Features

### Session store

- **Database.** `opencode.db` (SQLite) in `$XDG_DATA_HOME/opencode`, else `~/.local/share/opencode`
  [14][2]; the binary derives these from the home directory on every OS, so on Windows it is
  `%USERPROFILE%\.local\share\opencode\opencode.db` [2]. **`OPENCODE_DB`** overrides it (relative
  values resolve under the data directory; `:memory:` accepted); a build on a channel other than
  `latest|dev|beta|next|prod` uses `opencode-<channel>.db` unless `OPENCODE_DISABLE_CHANNEL_DB` is
  `1`/`true` [10][2]. `opencode debug paths db` prints the active path without starting a server or
  opening the database [1][10].
- **Other paths** [2][14]: config `~/.config/opencode` (`$XDG_CONFIG_HOME`), state
  `~/.local/state/opencode` (shared server's `service.json`), cache `~/.cache/opencode`, logs
  `<data>/log/opencode.log`.
- **Tables** [28]. `session_v2` (`id`, `project_id`, `parent_id`, `fork_session_id`,
  `fork_boundary`, `directory`, `title`, `model`, token counters, idle/compaction/suspension
  state); `session_message` (`id`, `session_id`, `type`, `seq`, timestamps, JSON `data`; unique
  `(session_id, seq)`); also `session_inbox`, `instruction_*`, `blob`, `blob_ref`. Order is `seq`,
  never time [27 §2.3].
- **Export document** (`session export`): `{info, messages[]}` [27 §2.3][32]. `info` is a
  `Session.Info` (`id`, `projectID`, `location.directory`, `time`, `cost`, `tokens`, `title`,
  `model{id, providerID}`); messages are `user`, `assistant`, `compaction` or `idle`. An assistant
  message has `content:[reasoning, text, tool…]`; each `tool` part holds the call and its result
  (`state{status, input, content}`); reasoning keeps `native.signature`. `session import` takes the
  same document and answers 409 to a second import of the same id [27 §1.3].
- **Interrupted turn.** An assistant row with empty text and no `finish`; the export omits it [27 §2.3].

### Compaction

- **Automatic** near the context limit (`compaction.auto`, default `true`), keeping about
  `compaction.keep.tokens` (default 15000) of recent conversation beside the summary;
  `compaction.buffer` defaults to 15 % of the limit, at least 16k [15]. `agents.compaction.model`
  picks the summary model; `providers.<name>.settings.compaction.type` is `"native"` (OpenAI
  Responses only) or `"summary"` [15]; `experimental.background_compaction` (new in 2.1.4) writes
  the summary early [15][28].
- **Manual** only through the HTTP API: `opencode api post /api/session/<id>/compact --data '{}'`,
  then follow `session.compaction.*` events [15]; `run` cannot compact. The bench followed the
  compact request with `POST /api/experimental/session/<id>/wait` (204) [27 §2.3].
- **Record:** `{type:"compaction", metadata{keepID:<first kept message>}, status:"completed"|"failed",
  reason:"manual", summary:<model reply verbatim>, …}`. A summary without the template headings
  (`## Objective` … `## Next Move`) fails with `Compaction summary did not match the required
  template` [15][27 §2.3][2].
- **On resume** the model receives one user message
  `<conversation-checkpoint>…<summary>…</summary><recent-context>…</recent-context></conversation-checkpoint>`,
  the kept tail flattened into it as text [27 §2.3]; earlier messages stay in the store [15].
- **Without a terminal** compaction is the HTTP call above; its record is written into the session it compacts [15].

### Ephemeral runs

- **No flag runs without saving**: every `run` writes to the store [1]. `run --session <id> --fork`
  copies the session into a new one (`session_v2.fork_session_id` = the source, `fork_boundary`
  `{"type":"through","messageID":…}`) and leaves the source's rows unchanged [37].
- **`opencode session delete <id> --standalone`** removes the fork with everything it added (its
  `session_v2`, `session_message`, `event_sequence` and `instruction_state` rows): every table's row
  count returns to what it was before the fork [37].
- **No switch removes the tools** of `run`; without `--auto`, permissions that ask are not
  approved [1]. The inline configuration `OPENCODE_CONFIG_CONTENT='{"permission":{"*":"deny"}}'`
  (merged with the user's own) denies every tool: asked to write a file, the model wrote none, and a
  fork of a session whose history holds a tool call was summarized normally [37].
- **Output.** `--format json` prints one event per line (`step_start`, `tool_use`, `text`,
  `step_finish`), each with the `sessionID` it belongs to: for a fork, the fork's id [37].

### Built-in tools

| tool [16][27 §2.3] | what it does |
|---|---|
| `read` | file or directory listing (`path`, `offset`, `limit`) |
| `glob` | paths matching a pattern |
| `grep` | file contents search |
| `edit` | exact replacement (`path`, `oldString`, `newString`, `replaceAll`) |
| `write` | create or replace a file (`path`, `content`) |
| `patch` | one multi-file patch; offered only for supported GPT models, which get it instead of `edit`/`write` |
| `shell` | command in the user's shell (`command`, `workdir`, `timeout`, `background`) |
| `webfetch` | one URL as markdown, text or HTML |
| `websearch` | search through the selected integration |
| `question` | asks the user, if the client supports it |
| `skill` | loads one skill's instructions |
| `subagent` | starts a child session with a subagent |
| `execute` | Code Mode JavaScript that calls and combines tools; MCP tools and the `browser`/`opencode` namespaces are reached through it |

The 2.1.4 bench offered the model all of these except `patch` [30 §2]. How mukade maps them:
[`../tool-conversion-tables.md`](../tool-conversion-tables.md).

### Agents, subagents and sessions tree

- **Built-in agents** [17]: primary `build` (default) and `plan`; subagents `general` and
  `explore`; hidden `compaction`, `title`, `summary`.
- **Custom agents** [17]: `~/.config/opencode/agents/<name>.md` or `.opencode/agents/<name>.md`
  (body = system prompt), or the `agents` config key; `mode` is `primary` (default), `subagent` or `all`.
- **Session tree.** A subagent runs in a foreground or background child session [17], stored by
  `parent_id`; forks (`run --fork`) by `fork_session_id`/`fork_boundary` [1][28]. `session list`
  shows top-level sessions only; `session delete` removes the children too [1]. Navigating the
  tree in the TUI: **[U]**.

### Models and providers

- **Models**: `provider/model[#variant]` in the `model` config key, `/models` in the TUI and
  `--model` on `run`; `opencode models` lists them [18][1].
- **Providers**: `providers.<id>` with `settings.baseURL`, `settings.apiKey` (accepts `{env:VAR}`),
  `models`, `env` [19]; the bench's shape: `{"providers":{"anthropic":{"settings":{"baseURL":…,"apiKey":…},"models":{…}}}}` [27 §2.3].
- **Reasoning replay**: a stored signature is sent back only when the stored provider and model
  equal the current ones [27 §2.3].

## 6. Plugins and extensions

- **Plugins** [20][1]. Listed in the `plugins` array of `opencode.json(c)` (global, project and
  `.opencode/` arrays are merged) as an npm name (`@version` optional), a local or `file://` path,
  or `{ "package": …, "options": {…} }`; `.opencode/plugins/` and `~/.config/opencode/plugins/`
  load without configuration. CLI: `opencode plugin add <package>`, `plugin list [--builtin]`,
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
- **Custom commands** [24]. `~/.config/opencode/commands/*.md` or `.opencode/commands/*.md`
  (nested folders become `/team/review`), or the `commands` key (each entry needs a `template`).
  Templates take `$ARGUMENTS`, `$1`, `$2`… and `!`-shell blocks.
- **Skills**: documented at <https://opencode.ai/v2/docs/skills/> [26]; details **[U]** (not read).
  **Claude Code compatibility**: the binary reads `OPENCODE_DISABLE_CLAUDE_CODE` and
  `OPENCODE_DISABLE_CLAUDE_CODE_SKILLS` [2], the bench set the first to `1` [27 §2.3]; what each
  disables: **[U]**.

## 7. Hooks

OpenCode 2.1.4 has no hooks section in its configuration file: the config page lists no `hooks`
key [13]. Hooks are registered by server plugins in `setup(ctx)` [22]:

| registration | hook names |
|---|---|
| `ctx.session.hook(name, cb, options?)` | `prompt`, `context`, `compaction` (setting `result` skips the model call), `generate`, `title`, `model.request` (can be scoped by `providerID`), `http.request`, `http.response`, `retry`, and the experimental `experimental.ws.handshake`, `experimental.ws.send`, `experimental.ws.receive` |
| `ctx.permission.hook` | `evaluate` (not called for a configured `deny`) |
| `ctx.shell.hook` | `create.before` |
| `ctx.tool.hook` | `execute.before`, `execute.after` (`status` is `"completed"` or `"error"`) |
| `ctx.event.subscribe({ types })` | events such as `config.updated` and `worktree.updated`; `server.connected` is excluded. There is no full event catalogue on that page [22]; compaction emits `session.compaction.*` [15] |

The V1 plugin "object form" is supported from OpenCode `1.18.29` [22]. The V1-style hook name
`tool.execute.before` does not occur in the 2.1.4 binary [2].

## 8. How mukade reads and writes it

| item | behaviour [source] |
|---|---|
| CLI name | `opencode` (`claude_code \| codex \| opencode \| pi`) [34] |
| store | `OPENCODE_DB`, the database file. When set, it is passed to every `opencode` subprocess. When unset, it is removed from their environment, so OpenCode uses its own configured store. mukade never sets the `XDG_*` variables: the caller sets them, and `OPENCODE_CONFIG` too, when isolation matters [31 `opencode_environment`][34 `roots.rs`][35] |
| `--root <path>` | a directory means `<path>/opencode.db`; a file is used as given. In a `hop` it names the source store [34 `apply_root`]. `mukade detect` recognizes a SQLite file named `opencode.db` [35] |
| executable | `--opencode-exe <path>` → `MUKADE_OPENCODE_EXE` → `PATH` (`PATHEXT` included) → the well-known install paths (`%APPDATA%\npm\node_modules\@opencode\cli\bin\opencode.exe`, then `~/.opencode/bin/opencode`) → `opencode` [33 `resolve`, `default_candidates`]. For an OpenCode **source**, only `MUKADE_OPENCODE_EXE` applies, not the flag [31][34] |
| `--standalone` | on every command mukade spawns (export, import, `session list`, the resume command), so that no work reaches the user's background service (L1) [31][32][34] |

**The npm launcher.** `%APPDATA%\npm\opencode.cmd` only forwards `%*` to the real binary [36]; it does
not add `--standalone`. Two things hold for any `.cmd` launcher:
Rust cannot pass a line break to a batch file, so mukade refuses a multi-line resume prompt for one
before it writes anything [34 `check_resume_prompt`][33 `is_batch`]; and on a timeout the child is
`cmd.exe`, so the whole process tree has to be killed with `taskkill` [33 `run_within`, `kill_tree`].

**Reader** (`src/reader/opencode.rs`) [31]. `read` runs `<exe> session export <id> --standalone`
(stdin closed, 120 s `EXPORT_TIMEOUT`; stderr with `not found` → not-found error) and parses
`{info, messages}`; `--from-file` also takes a `{session_v2, session_message}` row dump or a bare
message array. `list` opens the explicitly selected `OPENCODE_DB` read-only, returning root
sessions including archived ones, without starting an OpenCode process. Mapping:

| OpenCode record | mukade entry |
|---|---|
| `user` | User, with `files` as attachments |
| `assistant` | one Step: reasoning with `native.signature` / `native.encrypted`, or through OpenRouter `native.reasoningDetails[0]` (`signature` or `data`, protocol from its `format`), text, one call per `tool` part plus its result. A part still `pending`/`running` has no result; an `error` part's result is `state.error.message`, marked as an error. No `finish` means killed |
| `compaction`, `status:"completed"` | Compaction: `kept_from` = `metadata.keepID`, trigger from `reason` |
| failed `compaction`, `idle`, unknown types | Meta (not conversation) |

**Writer** (`src/writer/opencode.rs`) [32]:
1. Build `{info, messages}` (new or reserved id `ses_<id>`, message ids `msg_<uuid>`) and write it
   as `mukade-import-<id>.json` (created new, 0600 on Unix) in the selected Mukade data folder.
2. Run `<exe> session import <file> --directory <cwd> --standalone` (120 s `IMPORT_TIMEOUT`); a
   `cwd` starting with `-` is refused. The document is removed on success and on failure.
3. Require `Imported session: <id>` rather than treating a zero exit as proof. A reserved ID is also
   checked in the selected database with the supplied metadata; an uncertain result reports code 5.
   Receipt `location`: the `OPENCODE_DB` path, else
   `opencode's configured store, session <id>`; resume: `<exe> run --standalone --session <id>`,
   a resume prompt appended as the last argument [34 `hop.rs`].

**Import document shape.** Each item was probed against 2.1.4: without it, `session import`
answers `Bad Request` [32][27 §2.3].

| part | shape mukade writes |
|---|---|
| `info` | `{id, projectID, location:{directory}, time:{created, updated}, cost:0, tokens:{input, output, reasoning, cache:{read, write}}, title?, model:{id, providerID}}`. `projectID`, `cost` and `tokens` are all required together, and `model` must have both fields |
| user message | `{id, type:"user", time:{created}, text, files:[…]}` |
| assistant message | `{id, type:"assistant", agent:"build", model:{id, providerID}, content:[…], finish:"stop"\|"tool-calls", rawFinish:"end_turn"\|"tool_use", time:{created, completed}, cost:0}`, with one message per step (L8) |
| reasoning part | `{type:"reasoning", text, native:{signature?, encrypted?}}`, only when the reasoning policy keeps it as native. Toward the `openrouter` provider, OpenRouter's own shape, which OpenCode keeps and replays there: `native:{reasoningDetails:[{type:"reasoning.text", text, signature, format, index}]}` for signed text, `{type:"reasoning.encrypted", data, format, index}` for an encrypted payload, `format` `anthropic-claude-v1` or `openai-responses-v1` from the block's protocol (seen in a live 2.1.4 export; verified live 2026-10-28: a copy with one character of the signature changed is refused by the provider, `Invalid signature in thinking block`) |
| tool part | `{type:"tool", id, name, executed:false, state:{input, status:"completed"\|"error", content:[…], error?, metadata:{truncated:false}}, time:{created, completed}}` |
| failed call | `state.error` = `Session.StructuredError` `{"type":"tool.execution", "message":<result text>}`. `tool.execution` is the type OpenCode itself records for a failed tool [32][2] |
| user image | `Prompt.FileAttachment` `{data, mime, source:{type:"inline"}}` in `files` [32 `media.rs`][2] |
| tool-result image | `Tool.FileContent` `{type:"file", uri:"data:<mime>;base64,<data>", mime}` [32 `media.rs`][2] |
| call without an OpenCode equivalent | the call goes into the assistant text, and its result follows as a user message, as described in [`../tool-conversion-tables.md`](../tool-conversion-tables.md) §1 |
| source compaction | one user message carrying the summary. No native `compaction` record is written yet [27 §4.4] |

**Lookups** [34 `cli.rs`]:
- **Project id**: `--project-id`, else `session list --standalone` run inside `--cwd` against the
  destination store; up to 5 `ses_` ids are exported and the first whose `info.location.directory`
  equals the cwd (case and `\`/`/` ignored) gives `info.projectID`. With no session there yet the
  write fails: "OpenCode needs the destination's project id (`--project-id`)".
- **Provider and model**: `--model-provider`/`--model`, else the `model` key (`provider/model`) of
  `OPENCODE_CONFIG` or `$XDG_CONFIG_HOME/opencode/opencode.json`, else the source's first model; a
  provider is required. **Version**: the first `x.y.z` of `<exe> --version`.

**Deadlines**: export and import 120 s; `--version`, `session list` and project-id probes 30 s
each (`PROBE_TIMEOUT`); resume prompt 300 s by default. Past one, the process tree is killed [33].

**Known limits:**
- No session listing and no direct SQLite read or write [31].
- `--compaction` is `none` or `portable`; the library's `DestinationNative` returns `Unsupported`,
  so the caller runs OpenCode's HTTP compaction itself [34 `hop.rs`][27 §4.3].
- `opencode run --session <unknown id>` silently creates a new session [27 §5][1].
- Reasoning from another provider would reach OpenCode's model as visible text; the default
  `drop_foreign` drops it [27 §3.3].
- Subagent calls travel as text form; child sessions are not followed (one session is read, its
  `parent_id` kept) [30 §3][31].
- Only 2.1.4 is verified. 2.2.0 (npm `latest` on 2026-10-28) already changes media blobs,
  compaction start and instruction persistence [6][28].

### How mukade summarizes it

With `--compaction summary` and OpenCode as the source, mukade runs
`opencode run --standalone --session <id> --fork --format json [-m <provider>/<id>] <prompt>`
with `OPENCODE_CONFIG_CONTENT='{"permission":{"*":"deny"}}'`, reads the summary from the `text`
events, and fails the summary on a `tool_use` or an `error` event. The fork's id is the `sessionID`
of the first event, so the fork is deleted (`opencode session delete <fork id> --standalone`)
whatever came of the run. When that delete fails, a hop that succeeded says
`the summarizing run left a fork in the source store: run <command>` in its manifest, and a failed
summary says the same in its error [37].

## 9. Sources

All accessed 2026-10-28.

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
34. Repository: [`src/cli.rs`](../../src/cli.rs) (`apply_root`, `opencode_project_id`, `opencode_config_model`), [`src/roots.rs`](../../src/roots.rs), [`src/hop.rs`](../../src/hop.rs).
35. Repository: [`README.md`](../../README.md), [`docs/usage.md`](../usage.md), `src/detect.rs`.
36. Local npm launcher `C:\Users\alfio\AppData\Roaming\npm\opencode.cmd` (150 bytes, generated by npm for `@opencode/cli`).
37. `docs/verification/source-model-summary-live.md`: the live check of 2026-10-28 (throwaway sessions, source hashed before and after; Codex against a local mock)
