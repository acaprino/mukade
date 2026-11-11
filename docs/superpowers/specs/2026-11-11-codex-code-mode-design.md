# Reading Codex Desktop code mode (`exec`) and `apply_patch`

Requirement L10 of the interchange spec (`docs/session-interchange-spec.md` §8.1, tables §5),
widened by what a census of real rollouts showed.

## Why

A Codex Desktop rollout (`originator: codex_work_desktop`) holds almost every tool call as one
custom tool `exec` whose input is a JavaScript script. mukade reads it as
`CanonicalTool::Other("exec")`, so it travels as text form in every direction, Codex → Codex
included: a Desktop session arrives at any destination with nearly no real tool calls. Separately,
Codex `apply_patch` calls read as canonical `Edit` without a path, so they are text form too.

The user's goal (2026-11-11): every session of every supported harness leaves and enters without
loss. This is the first of three loss-closing projects (then subagents, then forks and branches).

## What the census showed (846 `exec` calls, every rollout on the development machine)

| script shape | calls | share |
|---|---|---|
| exactly one statement `text(await tools.X({...}));` | 403 | 48% |
| several such statements, one per line, nothing else | 261 | 31% |
| anything else (variables, `Promise.allSettled`, `ALL_TOOLS`, `load()`, loops) | 182 | 21% |

- Inner tools of the one-statement scripts: `exec_command` 301, `web__run` 38, `apply_patch` 36,
  `write_stdin` 21, MCP tools (`mcp__*`) and `clock__curr_time` the rest.
- The output is a list of content items: item 0 is a header, then **one item per `text()` call, in
  order** (matched in 659 of 662 sequential scripts; the rest had fewer items).
- Headers: `Script completed\nWall time N seconds\nOutput:\n` (829), `Script failed\n…` (12),
  `Script running with cell ID N\n…` (3, plus 2 given as a plain string instead of a list).
- 12 outputs carry an `input_image` item beside the text.
- Patch operations inside `apply_patch`: `Update File` 267, `Add File` 62, `Delete File` 3.

## Goals

1. A one-statement script becomes the call it wraps, with its result (spec L10).
2. **Widening of L10:** a script made only of `text(await tools.X({...}));` statements becomes
   that many calls, each paired with its own output item.
3. Codex `apply_patch` input (plain Codex CLI calls and those inside `exec`) is parsed into
   canonical `Write` / `Edit` / multi-edit with a real path, so it translates instead of becoming
   text.
4. Nothing is lost: what is not split stays one text-form call with the script verbatim, and the
   original records stay in `Entry::raw` as today.

## Non-goals

- Executing or evaluating JavaScript. Only a fixed literal grammar is recognised.
- Splitting complex scripts (`Promise.allSettled`, variables, `ALL_TOOLS`, `load()`): they stay
  one text-form call, as the spec says.
- Writing `apply_patch` or `exec` toward Codex. Toward Codex an `Edit`/`Write` stays text form
  ("Codex's apply_patch form is not written yet", `src/tools.rs`); a split `exec_command` becomes
  Codex's own `exec_command` function call through the existing shell translation.
- Translating MCP tools, `web__run`, `write_stdin`, `view_image`: once split, they follow the
  existing rules (L14 and the current Codex names), which make them text form.

## Anti-goals (a failure even if it works)

- A call whose arguments were guessed. If the parser is not certain, the script stays whole.
- A result paired with the wrong call. If the item count does not match, the script stays whole.
- A new dependency: the list in `Cargo.toml` is closed on purpose. The parsers are hand-written.

## Decisions

### When a script is split

A script is split only when **all** of these hold, otherwise it stays one text-form call:

- After trimming, it is a sequence of one or more statements of exactly the form
  `text(await tools.<name>(<args>));` (the trailing `;` optional on the last), separated by
  whitespace and newlines, with nothing else (no comments, no other statements).
- `<args>` is empty, or one literal: an object literal (keys bare identifiers or quoted strings),
  a string, a number, `true`/`false`/`null`, or an array of literals; nested to any depth. Strings
  use `"` or `'`, with JavaScript escapes (`\n`, `\t`, `\\`, `\"`, `\'`, `\uXXXX`, `\u{…}`).
  Template literals (backticks), identifiers as values, spreads, expressions and trailing
  computation make the script whole.
- The output is a list whose item 0 is a header starting with `Script completed` and which holds
  exactly one `input_text` item per call after it. Images: all 12 real outputs with an
  `input_image` come from an `image(await tools.view_image(...))` statement, which is not of the
  accepted form, so those scripts stay whole; any output holding an `input_image` keeps its script
  whole (settled on the census, 2026-11-11).
- `Script failed` → split only a one-statement script, with its result marked `is_error`;
  a failed multi-statement script stays whole (which call failed is not recorded).
- `Script running with cell ID …`, a string output, or no output → whole.

### What a split call looks like

- Ids: the first call keeps the `exec` call's `call_id`; the others get `<call_id>_<n>` (n from 2).
  `_` is inside Anthropic's id pattern, so no writer rewrites them and the manifest counts no
  `rewritten_call_ids` loss for ids mukade made up (a `#` would). Results carry the same ids.
- `source_name` is the inner tool's name (`exec_command`, `apply_patch`, …); `input` the parsed
  literal, which is what plain Codex records for that tool (an `exec_command` object; for
  `apply_patch` the patch string); `canonical` from the same `canonical_call` the reader already
  uses, so shell dialect detection (L13) applies.
- The header item and the script text are not given to any call; they remain in `Entry::raw`.
  The wire document (`src/wire.rs`) has no `raw`, so a `dump` shows the split calls, not the
  script: this is a reading of the script, not a loss of what the model did (decision 2026-11-11).
- Where this happens: in the Codex reader (`src/reader/codex.rs`), when the step's calls and
  results are assembled, since only there are the call and its output both known. The script
  parser lives in its own module (builder: e.g. `src/reader/codex_exec.rs`).

### `apply_patch` parsing

Format: `*** Begin Patch` … `*** End Patch`, with file sections:

- `*** Add File: <path>` followed by `+` lines → canonical `Write { path, content }` (the lines
  without their `+`, joined with `\n`).
- `*** Update File: <path>` followed by hunks starting with `@@` (optionally `@@ <context>`), lines
  ` ` context, `-` removed, `+` added → per hunk `old = context + removed lines`, `new = context +
  added lines`, in line order. One hunk → `Edit { path, old_string, new_string }`; several hunks →
  a multi-edit (`edits: [{old_string, new_string}, …]`), which only Pi accepts today
  (`multi_edit` in `src/tools.rs`), text form elsewhere.
- `*** Delete File`, `*** Move to:`, `*** End of File` markers, several file sections in one
  patch, or anything not matching → the call stays as today (canonical `Edit` without a path,
  which `edit()` turns into text form "no path"). Builder: decide whether such calls should carry a
  clearer `why`.
- **Where it runs: at translate time, in `src/tools.rs`** (parser in its own module). The call's
  `input` stays the patch string exactly as recorded (the `ToolCall.input` contract), so the wire
  document, hop policies and a text-form call toward Codex keep the patch. Accept both recorded
  shapes: a bare string and an object `{"input": "<patch>"}`.
- A patch with **several file sections** stays text form: its single result cannot be split
  between files (census: 92 of 199 patches inside `exec`).

### What "translated" means on real rollouts (decision 2026-11-11)

Every Desktop rollout on the development machine declares `<shell>powershell</shell>`, so a split
`exec_command` reads as canonical `PowerShell`. The dialect gate (L13) is kept: a command string
is never rewritten, so it translates only toward a destination whose shell is PowerShell
(`--dest-shell powershell`, or Claude's `PowerShell` tool where it is offered). This project is the
**reading**; the destination-side default shell and Claude's `PowerShell` in the offered set are
the next project (L11), listed in `next.md`. The verification record counts calls split, and calls
written natively both with and without `--dest-shell powershell`, so the remaining gap is stated
in numbers.

## Constraints

- Rust edition and toolchain as in the repo; no new crates.
- Codex → Codex must not get worse than today (today it is text form).
- The working tree is shared with a session developing `src/bus/` (sessions bus / MCP chat), which
  also edits `src/lib.rs`, `src/reader/mod.rs`, `docs/usage.md`, `README.md`. Touch those only if
  unavoidable, and commit only this project's files (on `master`, per the repository's practice).
- Fixtures must not contain real session content: build them from the observed shapes with masked
  strings.

## Tests and verification

- Unit tests for the literal parser (every accepted form, every refused form) and the patch
  parser (add, one hunk, several hunks, delete, move, several files, malformed).
- Reader tests on a hand-made Desktop rollout: one-statement shell call, sequence of three calls,
  `Promise.allSettled` kept whole, failed script, running script, mismatched item count.
- A hop test Codex Desktop → Claude Code and → Pi showing `Bash` and `Edit` calls written natively.
- A read-only check over every rollout on this machine: count calls split vs kept whole, assert no
  panic and that every split call has exactly one result. Recorded in `docs/verification/`.
- Docs updated: spec §8.1 L10 row and §10, tables §5 (and the wrong "existing `apply_patch`
  parser" sentence), `docs/harnesses/codex.md` known limits, `next.md` Codex item 2.

## Left to the builder

Module layout, parser technique, error types, test names, the exact `why` strings.
