# Tool conversion tables

How a tool call recorded by one harness is written for another: **translated** into the
destination's equivalent tool, or carried as **text**. Predict what a hop does to tool calls (§3,
§4); update with the code when a harness changes (§9). Code: `src/tools.rs` `translate_with()`;
readers: `src/model/tool.rs` `CanonicalTool`.

| | |
|---|---|
| **Last updated** | 2026-11-12 |
| **Verified against** | Claude Code 2.1.311 · Codex CLI 0.164.0 (rollouts of Codex Desktop up to 0.164.0-alpha.11.1; code mode §5 on Codex Desktop 0.167.0 / 0.167.1 rollouts) · OpenCode 2.1.4 · Pi 1.2.0; the destination's shell (§4.1) on Claude Code 2.1.322, Codex CLI 0.167.1, OpenCode 2.2.5, Pi 1.4.0 |
| **Status** | draft; policy (§1) decided; tables are what mukade's writers do (spec L10–L14) |
| **Owner document** | [`session-interchange-spec.md`](session-interchange-spec.md) §3 and §8 (requirements L10–L14) |
| **Per-harness pages** | [Claude Code](harnesses/claude-code.md) · [Codex CLI](harnesses/codex.md) · [OpenCode](harnesses/opencode.md) · [Pi](harnesses/pi.md): each lists its own tools and links here |
| **Review when** | a harness changes version (register item E1), or a destination's offered tools change (§6) |

Evidence labels as in the spec: **[V]** reported observation, **[S]** read in source, **[U]** not
established. Verify tool availability against the harness version and configuration in use.

## 1. Policy (decided 2026-10-27)

A call is **translated** into the destination's equivalent tool where one exists and the call can
be expressed without changing what it did; otherwise it travels in **text form**.

**Deciding**

1. **Canonical first.** The source reader maps every call to its canonical (Claude-named) form,
   keeping the source name and input; the destination writer maps canonical → destination with the
   tables below.
2. **Equivalent** means both: the destination **offers** a tool with the same effect, for its
   version and model (§6); and every argument the call used maps to that tool's arguments with the
   same meaning (keys renamed, units converted).

   Not equivalent, for example: Claude `Edit` with `replace_all: true` into Pi; Claude `Grep` with
   `output_mode` into OpenCode; Codex `exec_command` with `workdir` into Claude or Pi.
   An explicit list of the keys each translation maps or may drop decides
   (`Destination::accepted_keys()` in the destination's adapter `src/harnesses/<harness>.rs`, over
   `common_keys()` in `src/tools.rs`): any other key, even one never seen, makes the call text form.
   Every harness's spelling of a shared argument is accepted: `file_path`/`path`,
   `command`/`cmd`, `workdir`/`cwd`, `description`/`justification`,
   `old_string`/`oldString`/`oldText` (and the `new` forms), `replace_all`/`replaceAll`,
   `run_in_background`/`background`, `glob`/`include`, `-i`/`ignoreCase`/`caseSensitive`,
   `head_limit`/`limit`, `context`/`-C`.
3. **Never invent a call.** A read is not rewritten as a `cat` or `Get-Content` that never ran; a
   missing tool is not emulated with another.

**Writing**

4. **Text form** when there is no equivalent: the call becomes an assistant text block, its result
   a text block in the user message that held it, at the same place:

   ```text
   [tool call · <source tool name> · id <call id> · not available in <dest>]
   <key>: <value>
   ```
   ```text
   [tool result · id <call id>] (error)
   <result text>
   ```
   - `<dest>` is the harness id (`claude_code`, `codex`, `opencode`, `pi`).
   - One `<key>: <value>` line per argument: a string as recorded (line breaks kept), any other
     value as JSON; a non-object input (an `apply_patch` string, an `exec` script) follows the
     header whole.
   - ` (error)` only for an error result. Result text is verbatim; of structured blocks only text
     travels. Nothing is cut (`src/tools.rs` `text_form()`, `text_form_result()`).

   Like Codex's own importer (`[external_agent_tool_call: …]`, CX4); no `tool_use` without its tool,
   no orphan result: valid for every provider.
5. **Results are never rewritten.** The text the model saw (Claude's `1\t` line numbers,
   OpenCode's `Read file …, lines 1-1`, Codex's `Chunk ID … Output:` header) travels verbatim; only
   the wrapper changes. Non-text result blocks a destination cannot hold are counted
   `dropped_result_blocks`.
6. **Ids are kept**; parallel calls stay in one assistant message (spec T2, T3, T8). Toward an
   `anthropic-messages` destination (Claude Code by default) an id outside `^[a-zA-Z0-9_-]+$` is
   rewritten the same way for call and result (each other character becomes `_`; a clash gets
   `_<n>`) and counted `rewritten_call_ids`.

**Counting and overrides**

7. **Manifest** (fields: spec §6.3): `counts.tool_calls` (translated), `counts.tools_text_form`, a
   `losses` line per text-form tool. A translated call the source never answered moves to text
   (label `no result in the source`), counted `unanswered_calls`, not as translated. `mukade
   document` previews `tools_translated`, `tools_text_form`, `tools_omitted`.
8. **`--tool-map <path>`** (`hop`, `write`, `document`; library `WriteDetails::tool_map`). A call
   whose source name and whole input equal an entry's `source` and `original` is written as its
   `destination` with its `mapped` input: before the tables, without the offered-set check, counted
   as translated. File format: [usage §6](usage.md#6-host-and-automation-options). `mukade document`
   from an OpenCode source maps each call whose name OpenCode offers to itself.
9. **`--without-tools`** (`hop`, `write`): no call or result is written; counted `tools_omitted`.

## 2. Tools each harness offers and really uses

Representative names for the versions above; availability depends on model, configuration, extensions.

| harness | tools offered |
|---|---|
| Claude Code 2.1.311 (23) | `Agent`, `Bash`, `CronCreate`, `CronDelete`, `CronList`, `Edit`, `EnterWorktree`, `ExitWorktree`, `Glob`, `Grep`, `ListAgents`, `Monitor`, `NotebookEdit`, `Read`, `ReportFindings`, `ScheduleWakeup`, `SendMessage`, `Skill`, `TaskStop`, `WebFetch`, `WebSearch`, `Workflow`, `Write` |
| Codex 0.164.0 `exec` (9) | `exec_command`, `write_stdin`, `request_user_input`, `view_image`, `multi_agent_v1` (namespace), `get_goal`, `create_goal`, `update_goal`, built-in `web_search` |
| OpenCode 2.1.4 (12) | `edit`, `glob`, `grep`, `question`, `read`, `shell`, `skill`, `subagent`, `webfetch`, `websearch`, `write`, `execute` |
| Pi 1.2.0 (4) | `read`, `bash`, `edit`, `write` (default set; `grep`, `find`, `ls` appear when enabled; a `powershell` tool exists in its source) |

The library's copy, `src/tools.rs` `offered()`, is held to this table by
`tests/tools_translation.rs` `the_offered_sets_are_the_tables_lists` (its Codex set has 8 names:
`multi_agent_v1` is a namespace).

**Really used** (census of the real stores, top tools) [V: `tool-census.json`]:

| harness | calls by tool |
|---|---|
| Claude Code | `Bash` 4314 · `Edit` 1220 · `StructuredOutput` 836 · `Read` 610 · `Grep` 365 · `Write` 333 · `PowerShell` 260 · `mcp__*` 190 · `Monitor` 91 · `TaskStop` 58 · `Agent` 45 · others ≤ 38 |
| Codex (Desktop) | **`exec` (custom, code mode) 4844**, inside which: `exec_command` 3230 · `apply_patch` 781 · `view_image` 652 · `write_stdin` 103 · `web__run` 53; then `send_message` 17, `wait_agent` 12, `spawn_agent` 10 |
| OpenCode | `shell` 4586 · `read` 2707 · `grep` 1007 · `edit` 974 · `write` 317 · `glob` 164 · `webfetch` 117 · `execute` 115 · `skill` 93 · `subagent` 56 |
| Pi | `bash` 6465 · `read` 2087 · `edit` 916 · `grep` 519 · `write` 252 · `structured_output` 70 · `subagent` 44 · `find` 38 · `ls` 34 · extension tools |

Five canonical tools carry almost all traffic: shell, read, edit, write, grep/glob.

## 3. Name table (canonical → destination)

Cells: `name` = translated (argument rules in §4); **T** = text form; ≈ = translated only under
the condition in §4. The canonical form is Claude's, so the Claude Code column is the destination
for Codex, OpenCode and Pi calls; a Claude source keeps its own `Grep`, `Glob`, `WebSearch` and
`WebFetch` calls.

| canonical (Claude) | Claude Code | Codex | OpenCode 2.1 | Pi |
|---|---|---|---|---|
| `Bash` | `Bash` | ≈ `exec_command` | ≈ `shell` | ≈ `bash` |
| `PowerShell` | ≈ `PowerShell` (where the installation runs it, §4.1; else **T**) | ≈ `exec_command` | ≈ `shell` | ≈ `bash` |
| `Read` | `Read` | **T** (no read tool) | `read` | `read` |
| `Write` | `Write` | **T** (`apply_patch` `*** Add File` not written) | `write` | `write` |
| `Edit` | `Edit` | **T** (`apply_patch` `*** Update File` not written) | ≈ `edit` | ≈ `edit` (`edits:[{oldText,newText}]`) |
| `MultiEdit` / Pi `edit` with several `edits` / Codex `apply_patch` with several hunks (Claude 2.1.311 offers no `MultiEdit`) | **T** | **T** | **T** | ≈ `edit` (`edits[]`) |
| `Grep` | `Grep` | **T** | ≈ `grep` | ≈ `grep` (when offered) |
| `Glob` | `Glob` | **T** | `glob` | ≈ `find` (when offered) |
| `LS` (none in 2.1.311) | T | T | T | ≈ `ls` (when offered) |
| `WebFetch` | ≈ `WebFetch` (needs its `prompt`) | T | ≈ `webfetch` (no `prompt`) | T |
| `WebSearch` | `WebSearch` | T (built-in `web_search` is server-side) | ≈ `websearch` (query only) | T |
| `Agent` / `Task` / `subagent` / `spawn_agent`; `Skill` / `skill`; `AskUserQuestion` / `question` / `request_user_input`; `mcp__<server>__<tool>` and Codex MCP tools; everything else (harness-internal: `StructuredOutput`, `Monitor`, `Cron*`, `Workflow`, goals, todo, `execute`, `view_image`, `write_stdin`, `web__run`, Pi extensions) | T | T | T | T |

- **Shell rows:** translated only into the dialect the destination runs; Claude has no dialect check
  (§4.1).
- **"When offered":** Pi's `grep`, `find`, `ls` are outside the pinned set, so the CLI writes them as
  text (§6).
- **Toward Codex** only `exec_command` is ever translated.
- Subagents, skills, questions and MCP tools are text form on purpose (L14): a translated call would
  claim a capability the destination lacks.

**Reading: source name → canonical** (`src/model/tool.rs` `from_claude()`, `from_codex()`,
`from_opencode()`, `from_pi()`); the table's names, except:

| harness | names read differently |
|---|---|
| Claude Code | `Task` → `Agent`; `LS`, `List` → `LS` |
| Codex | `exec_command`, `local_shell` → `Bash` or `PowerShell` (§4.1); `apply_patch` → `Edit`, then read per §5; `web_search` → `WebSearch`; `request_user_input` → `AskUserQuestion` |
| OpenCode | `shell` → `Bash`; `subagent` → `Agent`; `question` → `AskUserQuestion` |
| Pi | `find` → `Glob`; `ls` → `LS`; `subagent` → `Agent` |

Any other name stays itself (`CanonicalTool::Other`) and travels as text.

**Encrypted arguments.** In a call's text form, a value the source's provider encrypted becomes
`<encrypted by the source's provider, N chars>` (only that provider can read it). Detected: an
OpenAI Fernet token (a word starting `gAAAAA`, base64url characters and `=`, ≥ 100 characters),
alone or inside a longer string; Codex Desktop 0.165's `send_message`, `followup_task` and
`spawn_agent` carry their whole `message` so. Decided with the user on 2026-10-28 (encrypted content
excluded); not counted in the manifest.

## 4. Argument tables

### 4.1 Shell

| | Claude `Bash` | Codex `exec_command` | OpenCode `shell` | Pi `bash` |
|---|---|---|---|---|
| command | `command` | `cmd` | `command` | `command` |
| working dir | — | `workdir` | `workdir` | — |
| timeout | `timeout` **ms** | — (`yield_time_ms` is not a timeout) | `timeout` **ms** | `timeout` **seconds** |
| background | `run_in_background` | — (sessions via `write_stdin`) | `background` | — |
| sandbox flags | `dangerouslyDisableSandbox` (kept into Claude) | `sandbox_permissions`, `with_escalated_permissions`, `prefix_rule` | — | — |
| description | `description` | `justification` (≈) | — | — |
| shell binary | POSIX (Git Bash on Windows); `PowerShell` is a separate tool | the call's `shell`, else PowerShell on Windows and the login shell elsewhere | config `shell`, `$SHELL`, then on Windows `pwsh`, `powershell`, Git Bash, `cmd` | `shellPath`, else Git Bash on Windows, `bash` elsewhere |

**Dialect.** The command string is **never rewritten**, so a call is translated only when the
destination runs the same dialect.

- Source dialect: Claude splits `Bash`/`PowerShell`. A Codex call takes its own `shell` argument
  (`pwsh`, `powershell`; `bash`, `sh`, `zsh`, `posix`), else the
  `<environment_context><shell>…</shell>` declared before it [V] (`bash`, `sh`, `zsh`, `posix`,
  `git-bash`; `powershell`, `pwsh`), else `Bash` (`src/reader/codex.rs` `canonical_call()`).
- Destination dialect, first answer wins: `--dest-shell` (the names below, or `posix`; `none`: no
  dialect, text form into Codex, OpenCode and Pi); else what the destination runs for the project
  the session is filed under (`src/dest.rs` `shell_in()`, the adapter's `default_shell()`; library
  `WriteDetails::dest_shell`). Where each reads it: the harness pages.

  | destination | dialect without `--dest-shell` |
  |---|---|
  | Codex | PowerShell on Windows, POSIX elsewhere (no configuration key) |
  | OpenCode | its configuration's `shell`, else `$SHELL`, else PowerShell on Windows, else none |
  | Pi | its settings' `shellPath`, else POSIX |
  | Claude Code | none needed: the call's own dialect picks the tool (its `PowerShell` tool is the offered set's, below) |

  A shell is read by its base name, `.exe` and case ignored: `bash`, `sh`, `zsh`, `dash`, `ksh` are
  POSIX; `pwsh`, `powershell` PowerShell; any other (`cmd`, `fish`, `nu`) gives no answer from that
  source (`src/tools.rs` `shell_named()`).
- Into Claude: no dialect check; `Bash` → `Bash`, `PowerShell` → `PowerShell` where the
  installation runs that tool (`src/tools.rs` `offered_in()`; [Claude Code](harnesses/claude-code.md#the-powershell-tool)).
- The Codex writer records the destination's shell in the new rollout's
  `<environment_context><shell>` (`bash` or `powershell`), so a later read knows the dialect.

**Arguments.**

- **Timeout (L12):** Pi reader: seconds → milliseconds, fractions kept (1.5 s → 1500 ms); Pi writer:
  milliseconds → whole seconds, rounded up (2500 ms → 3 s). Claude and OpenCode keep milliseconds;
  into Codex it is dropped.
- **Dropped, not blocking** (no effect on the recorded result): `description`/`justification`
  (becomes Claude's `description` or Codex's `justification`), `yield_time_ms`,
  `max_output_tokens`, `shell`, `login`, `tty`, and the table's sandbox flags
  (`dangerouslyDisableSandbox` is kept into Claude).
- **Blocking:** a working directory (`workdir`, `cwd`) into Claude or Pi (they run in the session's
  directory); background execution (`run_in_background`, `background: true`) into Codex or Pi.
  Background into Claude is `run_in_background: true`, into OpenCode `background: true`.

### 4.2 Read

| | Claude `Read` | OpenCode `read` | Pi `read` |
|---|---|---|---|
| path | `file_path` | `path` | `path` |
| first line | `offset` (line number) | `offset` (1-based) | `offset` (1-indexed) |
| line count | `limit` | `limit` (cap 2000) | `limit` |
| other | `pages` (PDF), `allow_large` | directory entries | images as attachments |

- `pages` or `allow_large` → text form outside Claude (kept into Claude); `offset`, `limit` copied
  unchanged.

### 4.3 Edit and write

| | Claude `Edit` | OpenCode `edit` | Pi `edit` | Codex `apply_patch` (not written, §5) |
|---|---|---|---|---|
| path | `file_path` | `path` | `path` | `*** Update File: <path>` |
| change | `old_string` → `new_string` | `oldString` → `newString` | `edits:[{oldText,newText}]` | one `@@` hunk, `-old` / `+new` lines |
| all occurrences | `replace_all` | `replaceAll` | **none** (unique match required) | **none** |

**Edit**
- `replace_all: true` (or `replaceAll`, or a multi-edit entry's `replace_all`) → text form into Pi
  and Codex; into Claude `replace_all: true`, into OpenCode `replaceAll: true`.
- An edit without new text deletes its old text (new text `""`).
- A Pi `edit` with **one** entry is an ordinary edit everywhere.

**Multi-edit:** several entries → only Pi takes them; Claude (no `MultiEdit` in 2.1.311) and
OpenCode get text form, not split (one result cannot be split). Into Pi, no entries or an entry
missing its old or new text → text form.

**Write:** `file_path`/`path` + `content` everywhere (a missing `content` is written as `""`).

**Toward Codex** every write and edit is text form: `apply_patch` is not in the pinned offered set
and its patch form is not written (`src/tools.rs` `edit()`).

### 4.4 Search

| | Claude `Grep` | OpenCode `grep` | Pi `grep` [S: pi-coding-agent 1.2.0 `dist/core/tools/grep.js`] |
|---|---|---|---|
| pattern / path | `pattern`, `path` | `pattern`, `path` | `pattern`, `path` |
| file filter | `glob`, `type` | `include` | `glob` |
| case | `-i` | `caseSensitive` (inverted) | `ignoreCase` |
| literal | — | `literal` | `literal` |
| limit | `head_limit` | `limit` | `limit` |
| context | `-A`,`-B`,`-C`,`context` | — | `context` (both sides, = `-C`) |
| mode | `output_mode`, `-n`, `multiline` | — | — |

**Grep**
- Translated only over the shared subset: pattern, path, filter, case, literal, limit, and context
  where the destination has it (Pi yes, OpenCode no).
- Text form (a Claude source into Claude excepted): `output_mode` other than `files_with_matches`
  (Claude's default); `type`; `-n`, `multiline`, `offset`, one-sided context (`-A`, `-B`); context
  into OpenCode.
- Into Claude from OpenCode or Pi: `literal` is text form (Claude's Grep is regex only); otherwise
  the call gets `output_mode: "content"` with `-n: true`, which is what those tools print. So
  `-n: true` and `multiline: false` are kept there; `-n: false` and `multiline: true` are text form.

**Glob**
- `pattern`, `path` everywhere. OpenCode adds `hidden`, `limit`; Pi `find` and `ls` add `limit`.
- An argument the destination's tool lacks → text form.

## 5. Reading Codex Desktop "code mode" (`exec`)

[V: census] Codex Desktop rollouts (`originator: codex_work_desktop`) hold almost every call as
**one custom tool `exec`** whose input is a script, e.g. (strings and numbers masked)
`text(await tools.exec_command({cmd:S, yield_time_ms:N}));`, `text(await tools.apply_patch(S));`,
or several calls in `Promise.allSettled([...])`; the output is a list of content items. Read as an
opaque tool, every such call would be text form.

**Rule** (spec L10, built 2026-11-11; `src/reader/codex/exec.rs` `split_code_mode()`,
`parse_script()`): a script is read as its calls, in order, only when all of this holds:

1. **Shape:** only statements `text(await tools.<name>(<literal>));`, one or several; each `;`
   optional; `await` followed by whitespace.
2. **Arguments:** literals only: objects (bare or quoted keys), arrays, `"`/`'` strings with
   JavaScript's escapes (no raw line break), decimal numbers, `true`, `false`, `null`; nesting ≤ 128
   deep. A call without arguments is `{}`.
3. **Output:** a list. Item 0 is a text header beginning `Script completed` (or `Script failed` for
   a one-call script, whose result is then an error); then exactly one `input_text` item per
   statement, that call's result.
4. **Ids:** the first call keeps the `exec` call's id, the n-th is `<id>_<n>` (inside every
   provider's id alphabet, so no writer rewrites it).

Anything else is one call in **text form**, script verbatim: variables, `Promise.allSettled`,
`ALL_TOOLS`, `image(...)`, comments, template literals, a running script, a string output, an image
item, or a mismatched count.

Census of 981 real scripts: 725 split, 256 kept whole
([record](verification/2026-11-11-codex-code-mode.md), which also counts how many split calls reach
each destination natively). Split calls follow the ordinary rules: `exec_command` is a shell call in
the source's dialect (L13); MCP tools, `web__run`, `write_stdin` are text form (L14).

**`apply_patch`** (`src/tools/patch.rs` `view()`), inside `exec` or as Codex's own call; input a
bare patch string or `{"input": …}`. It is read when the call is translated; the recorded input
stays the patch.

| patch | read as |
|---|---|
| exactly one `*** Add File:` section | `Write` (the `+` lines, each ending in a line break) |
| exactly one `*** Update File:` section with one `@@` hunk | `Edit`: old text = context and `-` lines, new text = context and `+` lines; the `@@` anchor is in neither |
| the same with several hunks | multi-edit (Pi only, §4.3) |
| anything else | text form |

- "Anything else": a delete, a move, `*** End of File`, several file sections (one result cannot be
  split between files), a hunk whose old text is blank (an insertion with no anchor), a hunk of
  context lines only (in Codex it only places the next hunk), no `*** Begin Patch` /
  `*** End Patch` frame, a line before the first `@@` or with another prefix.
- An empty line inside a hunk is an empty context line. The `@@ <anchor>` text (it picks among
  repeated occurrences in Codex) is not carried: the edit names its old text only.
- Toward Codex a patch is always text form (§4.3).

## 6. The destination's tool set is per version and model

A destination's offered set depends on version, settings and model: Codex Desktop uses
`apply_patch` inside `exec` [V]; Pi offers `grep`/`find`/`ls` only when enabled; Claude offers
`PowerShell` only where enabled. The library should take it from the destination before
translating (spec L11):

- Claude Code, OpenCode, Pi: no tool list without a model call. Use §2 for the pinned version, plus
  the destination home's settings (Pi `settings.json`, Claude `settings.json` permissions/tools)
  [U: exact keys].
- Codex: `codex app-server` `model/list` exposes the model's metadata [U: whether it lists tools].

**Today.** Until T11 exists, the library uses the §2 lists (`src/tools.rs` `offered()`) unless the
caller passes a set (`WriteDetails::offered`, `src/tools.rs` `translate_with()`). The CLI and the TUI
pass `src/tools.rs` `offered_in()`: the §2 list plus what the destination's configuration turns on,
so far Claude's `PowerShell` (§4.1). An unlisted tool is text form; the CLI has no flag for a wider
set.

**Planned:** Pi's `powershell` tool where its settings turn it on ([`next.md`](../next.md)).

## 7. Open questions

- [U] Does the Anthropic API accept a history `tool_use` whose name is not in the request's
  `tools`? Does the Responses API accept a `function_call` whose name is not offered? The mock
  accepts both; the text-form rule avoids the question.
- [U] Pi `grep` arguments beyond `pattern`, `path`, `context`, `limit` (census shows only these).

## 8. Changelog

| date | change |
|---|---|
| 2026-11-12 | The destination's shell (§4.1): Codex by platform, OpenCode's `shell` or `$SHELL`, Pi's `shellPath`; Claude's `PowerShell` offered where the installation runs it; `--dest-shell none`. §7: OpenCode's shell on Windows settled. |
| 2026-11-12 | Grep into Claude from another harness: `-A`, `-B`, `offset`, `multiline: true` and `-n: false` are text form, as §4.4 says (they were dropped without notice). |
| 2026-11-11 | L10 built (§5). Tables checked against the code: shell calls need `--dest-shell` outside Claude; `PowerShell` into Claude is text form, into Codex writes no `shell` argument, into Pi is `bash`; text form not cut; Grep `output_mode`; manifest key `tool_calls`; `--tool-map`, `--without-tools`, reading table. |
| 2026-10-28 | Claude Code column; argument policy explicit and implemented; every spelling accepted; Pi `grep`/`find`/`ls` schemas from source; OpenCode `background`/`replaceAll`, single-entry Pi edits, unanswered calls, id rewriting toward Anthropic. |
| 2026-10-27 | First version. Policy decided ("translate to the equivalent where possible, else text form"). |

## 9. How to update this file

1. Check the offered tool lists and argument schemas for the new version.
2. Diff against §2 and §4; update the tables, "Verified against", **Last updated** and the changelog.
3. In the same change update the writers' tables (in the destination's adapter `src/harnesses/<harness>.rs`:
   `offered()`, `accepted_keys()`, `translate()`) and their fixtures (`tests/tools_translation.rs`).
