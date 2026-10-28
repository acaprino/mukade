# Tool conversion tables

| | |
|---|---|
| **Last updated** | 2026-10-28 |
| **Verified against** | Claude Code 2.1.311 · Codex CLI 0.164.0 (rollouts of Codex Desktop up to 0.164.0-alpha.11.1) · OpenCode 2.1.4 · Pi 1.2.0 |
| **Status** | draft; the policy (§1) is decided, the tables are the behaviour of Shinatama's own writers (spec L11–L14) |
| **Owner document** | [`session-interchange-spec.md`](session-interchange-spec.md) §3 and §8 (requirements L10–L14) |
| **Per-harness pages** | [Claude Code](harnesses/claude-code.md) · [Codex CLI](harnesses/codex.md) · [OpenCode](harnesses/opencode.md) · [Pi](harnesses/pi.md) — each lists its own tools and links back here; this file is the one place that says how a call of one harness becomes a call of another |
| **Review when** | any of the four harnesses changes version (register item E1 records drift), or a destination's offered tool list changes (§6) |

Evidence labels as in the spec: **[V]** seen on this machine (file given), **[S]** read in source,
**[U]** not established. Paths without a prefix are under `verify/H27/out/2026-10-27/`.

Sources of these tables:
- the tool lists each harness **offered to the model** in the H27 bench (`<h>/source/capture-gen.jsonl`, the request with the most tools) [V];
- a **census of the user's real stores**, structure only (tool name, argument key set, count; no values): `tool-census.json`, produced by `python -I verify/H27/tool-census.py` [V]. Read: Claude 400 newest transcripts (8,506 calls), Codex 400 newest rollouts (4,908 calls), OpenCode 2 databases (10,198 calls), Pi 113 sessions (10,488 calls);

## 1. Policy (decided 2026-10-27)

A tool call from the source is **translated into the destination's equivalent tool where one
exists and the call can be expressed without changing what it did**; otherwise it is carried in
**text form**. In detail:

1. **Canonical first.** The source reader maps every call to its canonical (Claude-named) form,
   keeping the source name and input; the destination writer maps canonical → destination with
   the tables below.
2. **Equivalent** means: the destination **offers** a tool with the same effect (for the
   destination version and model, §6), and every argument the call used maps to that tool's
   arguments with the same meaning (keys renamed, units converted). A call that used an argument
   with no counterpart (for example Claude `Edit` with `replace_all: true` into Pi, Claude
   `Grep` with `output_mode` into OpenCode, Codex `exec_command` with `workdir` into Claude or Pi)
   is **not** equivalent. The library checks this with an explicit list of the keys each
   translation maps or may drop (`accepted_keys` in `src/tools.rs`): any other key, including one
   it has never seen, makes the call text form. Every harness's spelling of a shared argument is
   accepted (`file_path`/`path`, `command`/`cmd`, `old_string`/`oldString`/`oldText`,
   `replace_all`/`replaceAll`, `run_in_background`/`background`, `glob`/`include`,
   `-i`/`ignoreCase`/`caseSensitive`, `head_limit`/`limit`, `context`/`-C`).
3. **Never invent a call.** A file read is not rewritten as a shell command (`cat`, `Get-Content`)
   that never ran; a missing tool is not emulated with another one.
4. **Text form** when there is no equivalent: the call becomes an assistant text block and its
   result a text block in the user message that held the result, at the same place:

   ```text
   [tool call · <source tool name> · id <call id> · not available in <dest>]
   <key>: <value>            (one line per argument; long values cut as in technical design 6.4)
   ```
   ```text
   [tool result · id <call id>]<error flag if is_error>
   <result text, verbatim; long output cut 15+15 lines as in 6.4>
   ```
   This is the same idea as Codex's own importer (`[external_agent_tool_call: …]`, CX4) and keeps
   the request valid for every provider (no `tool_use` without its tool, no orphan result).
5. **Results are never rewritten.** The result text the model saw in the source (Claude's
   `1\t` line numbers, OpenCode's `Read file …, lines 1-1`, Codex's `Chunk ID … Output:` header)
   travels verbatim; only the wrapper changes.
6. **Ids are kept**; parallel calls stay in one assistant message (spec T2, T3, T8). Toward an
   Anthropic-protocol destination an id outside `^[a-zA-Z0-9_-]+$` is rewritten, the same way for
   the call and its result, and counted (`rewritten_call_ids`).
7. The manifest counts `tools_translated`, `tools_text_form` and lists each text-form tool name. A
   translated call the source never answered is moved to text after all (`unanswered_calls`, label
   `no result in the source`) and is not counted as translated.

## 2. Tools each harness offers and really uses

**Offered to the model** in the bench (default settings, no plugins, mock model) [V]:

| harness | tools offered |
|---|---|
| Claude Code 2.1.311 (23) | `Agent`, `Bash`, `CronCreate`, `CronDelete`, `CronList`, `Edit`, `EnterWorktree`, `ExitWorktree`, `Glob`, `Grep`, `ListAgents`, `Monitor`, `NotebookEdit`, `Read`, `ReportFindings`, `ScheduleWakeup`, `SendMessage`, `Skill`, `TaskStop`, `WebFetch`, `WebSearch`, `Workflow`, `Write` |
| Codex 0.164.0 `exec` (9) | `exec_command`, `write_stdin`, `request_user_input`, `view_image`, `multi_agent_v1` (namespace), `get_goal`, `create_goal`, `update_goal`, built-in `web_search` — **no read, edit or apply_patch tool** with the bench's unknown model (fallback metadata) |
| OpenCode 2.1.4 (12) | `edit`, `glob`, `grep`, `question`, `read`, `shell`, `skill`, `subagent`, `webfetch`, `websearch`, `write`, `execute` |
| Pi 1.2.0 (4) | `read`, `bash`, `edit`, `write` (default set; `grep`, `find`, `ls` appear when enabled; a `powershell` tool exists in its source) |

**Really used** (census of the real stores, top tools) [V: `tool-census.json`]:

| harness | calls by tool |
|---|---|
| Claude Code | `Bash` 4314 · `Edit` 1220 · `StructuredOutput` 836 · `Read` 610 · `Grep` 365 · `Write` 333 · `PowerShell` 260 · `mcp__*` 190 · `Monitor` 91 · `TaskStop` 58 · `Agent` 45 · others ≤ 38 |
| Codex (Desktop) | **`exec` (custom, code mode) 4844**, inside which: `exec_command` 3230 · `apply_patch` 781 · `view_image` 652 · `write_stdin` 103 · `web__run` 53; then `send_message` 17, `wait_agent` 12, `spawn_agent` 10 |
| OpenCode | `shell` 4586 · `read` 2707 · `grep` 1007 · `edit` 974 · `write` 317 · `glob` 164 · `webfetch` 117 · `execute` 115 · `skill` 93 · `subagent` 56 |
| Pi | `bash` 6465 · `read` 2087 · `edit` 916 · `grep` 519 · `write` 252 · `structured_output` 70 · `subagent` 44 · `find` 38 · `ls` 34 · extension tools |

So five canonical tools carry almost all the traffic: shell, read, edit, write, grep/glob.

## 3. Name table (canonical → destination)

Cells: `name` = translated (argument rules in §4); **T** = text form; ≈ = translated only under
the condition in §4. The canonical form is Claude's, so the Claude Code column is the destination
for calls from Codex, OpenCode and Pi; a Claude source keeps its own `Grep`, `Glob`, `WebSearch` and
`WebFetch` calls unchanged.

| canonical (Claude) | Claude Code | Codex | OpenCode 2.1 | Pi |
|---|---|---|---|---|
| `Bash` | `Bash` | ≈ `exec_command` | `shell` | `bash` |
| `PowerShell` | `PowerShell` | ≈ `exec_command` (`shell: "pwsh"`) | ≈ `shell` (when the destination's shell is PowerShell) | T |
| `Read` | `Read` | **T** (no read tool) | `read` | `read` |
| `Write` | `Write` | ≈ `apply_patch` (`*** Add File`), when offered — **T** today | `write` | `write` |
| `Edit` | `Edit` | ≈ `apply_patch` (`*** Update File`, one hunk) — **T** today | ≈ `edit` | ≈ `edit` (`edits:[{oldText,newText}]`) |
| `MultiEdit` / Pi `edit` with several `edits` (not offered by 2.1.311) | **T** | **T** | **T** | `edit` (`edits[]`) |
| `Grep` | `Grep` | **T** | ≈ `grep` | ≈ `grep` (when enabled) |
| `Glob` | `Glob` | **T** | `glob` | ≈ `find` (when enabled) |
| `LS` (none in 2.1.311) | T | T | T | `ls` (when enabled) |
| `WebFetch` | ≈ `WebFetch` (needs its `prompt`) | T | ≈ `webfetch` (no `prompt`) | T |
| `WebSearch` | `WebSearch` | T (built-in `web_search` is server-side) | ≈ `websearch` (query only) | T |
| `Agent` / `subagent` / `spawn_agent` | T | T | T | T |
| `Skill` / `skill` | T | T | T | T |
| `AskUserQuestion` / `question` / `request_user_input` | T | T | T | T |
| `mcp__<server>__<tool>` | T | T | T | T |
| everything else (harness-internal: `StructuredOutput`, `Monitor`, `Cron*`, `Workflow`, goals, todo, `execute`, Pi extensions) | T | T | T | T |

Subagents, skills, questions and MCP tools are text form on purpose: the agent types, skill sets
and MCP servers of the destination are not the source's, so a translated call would claim a
capability the destination does not have.

**Encrypted arguments.** In the text form of a call, a value the source's provider encrypted (an
OpenAI Fernet token: `gAAAAA` and at least 100 base64url characters, alone or inside a longer
string) is replaced by `<encrypted by the source's provider, N chars>`: only that provider can read
it. Codex Desktop 0.165's `send_message`, `followup_task` and `spawn_agent` carry their whole
`message` that way. Decided with the user on 2026-10-28 (encrypted content is excluded); not
counted in the manifest.

## 4. Argument tables

### 4.1 Shell

| | Claude `Bash` | Codex `exec_command` | OpenCode `shell` | Pi `bash` |
|---|---|---|---|---|
| command | `command` | `cmd` | `command` | `command` |
| working dir | — | `workdir` | `workdir` | — |
| timeout | `timeout` **ms** | — (`yield_time_ms` is not a timeout) | `timeout` **ms** | `timeout` **seconds** |
| background | `run_in_background` | — (sessions via `write_stdin`) | `background` | — |
| sandbox flags | `dangerouslyDisableSandbox` (kept into Claude) | `sandbox_permissions`, `with_escalated_permissions` | — | — |
| description | `description` | `justification` (≈) | — | — |
| shell binary | Bash tool = POSIX shell; `PowerShell` tool separate | `shell` (default: the user's shell; **pwsh** on this machine [V]) | [U] on Windows | bash |

Rules: the command string is **never rewritten**; a call is translated only when the destination
runs the same shell dialect (Codex records it in `<environment_context><shell>…</shell>` [V];
Claude splits `Bash`/`PowerShell`), otherwise text form. Timeout ms ↔ s conversion for Pi (L12),
fractions included. A dropped argument with no effect on the recorded result (`description`,
`justification`, `yield_time_ms`, `max_output_tokens`, `shell`, `login`, `tty`, the sandbox flags)
does not block translation; a working directory into Claude or Pi does (they run in the session's
directory), and so does background execution (`run_in_background` or `background: true`) into a
destination without it (Codex, Pi). Into Claude it becomes `run_in_background: true`.

### 4.2 Read

| | Claude `Read` | OpenCode `read` | Pi `read` |
|---|---|---|---|
| path | `file_path` | `path` | `path` |
| first line | `offset` (line number) | `offset` (1-based) | `offset` (1-indexed) |
| line count | `limit` | `limit` (cap 2000) | `limit` |
| other | `pages` (PDF), `allow_large` | directory entries | images as attachments |

`pages` or `allow_large` used → text form outside Claude; into Claude they are kept. Into Codex
always text form.

### 4.3 Edit and write

| | Claude `Edit` | OpenCode `edit` | Pi `edit` | Codex `apply_patch` |
|---|---|---|---|---|
| path | `file_path` | `path` | `path` | `*** Update File: <path>` |
| change | `old_string` → `new_string` | `oldString` → `newString` | `edits:[{oldText,newText}]` | one `@@` hunk, `-old` / `+new` lines |
| all occurrences | `replace_all` | `replaceAll` | **none** (unique match required) | **none** |

`replace_all: true` (or OpenCode's `replaceAll`, or a MultiEdit entry's `replace_all`) → text form
into Pi and Codex. A Pi `edit` with **one** entry is an ordinary edit everywhere; with several
entries → Claude has no `MultiEdit` in 2.1.311 → text form (not split: one result cannot be split).
A MultiEdit into Pi with no entries, or an entry missing its old or new text, is text form.
Write: `file_path`/`path` + `content` everywhere; Codex `*** Add File` only when the destination
offers `apply_patch` (§6), else text form.

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

Translated only when the call used the shared subset (pattern, path, filter, case, literal, limit,
and context where the destination has it); `output_mode` other than the destination's default,
`type`, `-n`, `multiline`, one-sided context (`-A`, `-B`) → text form. Into Claude, a `literal`
search is text form (Claude's Grep is regex only), and a call from OpenCode or Pi gets
`output_mode: "content"` with `-n`, which is what those tools print.
Glob: `pattern`, `path` everywhere (OpenCode adds `hidden`, `limit`; Pi `find` and `ls` add `limit`);
an argument the destination's tool lacks is text form.

## 5. Reading Codex Desktop "code mode" (`exec`)

[V: census] Codex Desktop rollouts (`originator: codex_work_desktop`) hold almost every tool call
as **one custom tool `exec`** whose input is a script, for example (strings and numbers masked):
`text(await tools.exec_command({cmd:S, yield_time_ms:N}));`,
`text(await tools.apply_patch(S));`, or several calls with `Promise.allSettled([...])`.
The output is a list of content items. Read as an opaque tool, every Codex Desktop call would
become text form at best.

Rule (spec L10): a script that is **exactly one** `tools.exec_command({...})` or
`tools.apply_patch("...")` call wrapped in `text(await …)` with literal arguments is read as that
call (`Bash` / `Edit`/`Write` via the existing `apply_patch` parser); any other script (several
calls, variables, `ALL_TOOLS`, `web__run`, `view_image`) stays one call in **text form**, its
script verbatim.

## 6. The destination's tool set is per version and model

What a destination offers depends on its version, its settings and the model: the bench's Codex
with an unknown model offered no `apply_patch` [V], Codex Desktop uses it inside `exec` [V],
Pi offers `grep`/`find`/`ls` only when enabled, Claude offers `PowerShell` only where it is
enabled. The library therefore takes the destination's offered set from the destination itself
before translating (spec L11):
- Claude Code, OpenCode, Pi: the tool list of a dry request is not available without a model
  call; use this file's §2 list for the pinned version, plus the settings file of the
  destination home (Pi `settings.json`, Claude `settings.json` permissions/tools) [U: exact keys].
- Codex: `codex app-server` `model/list` exposes the model's metadata [U: whether it lists tools].
Until T11 exists, the library uses the §2 lists for the pinned versions and treats any tool not
listed there as not offered (text form).

## 7. Open questions

- [U] Does the Anthropic API accept a `tool_use` in history whose name is not in the request's
  `tools`? Does the Responses API accept a `function_call` item whose name is not offered? The
  mock accepts both; the text-form rule avoids the question.
- [U] OpenCode `shell` on Windows: which shell runs the command.
- [U] Pi `grep` arguments beyond `pattern`, `path`, `context`, `limit` (census shows only these).

## 8. Changelog

| date | change |
|---|---|
| 2026-10-28 | Claude Code added as a destination column; argument policy made explicit (any key outside the translation's list is text form) and implemented; every harness's spelling accepted; Pi `grep`/`find`/`ls` schemas read from source; OpenCode `background`/`replaceAll`, single-entry Pi edits, unanswered calls, id rewriting toward Anthropic. |
| 2026-10-27 | Rewritten for Shinatama's own library: the column on the external converter and its references removed. |
| 2026-10-27 | First version. Policy decided ("translate to the equivalent where possible, else text form"). Tables from the H27 bench captures and the real-store census. |

## 9. How to update this file

1. Rerun the bench legs for the new version: `verify/H27/<h>/gen-source.ps1` (the capture holds the offered tools).
2. Rerun the census: `python -I verify/H27/tool-census.py > verify/H27/out/<date>/tool-census.json`.
3. Diff the offered lists and argument schemas against §2 and §4; update the tables, the
   "Verified against" row, **Last updated**, and add a changelog line.
4. Update the writers' tables in the crate and their fixtures in the same change.
