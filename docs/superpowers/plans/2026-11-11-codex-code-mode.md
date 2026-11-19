# Codex code mode (`exec`) and `apply_patch` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Read Codex Desktop `exec` scripts made of literal `text(await tools.X(...))` statements as the calls they wrap, and read Codex `apply_patch` input as `Write`/`Edit`/multi-edit with a real path.

**Architecture:** A hand-written literal parser turns a script into `(tool name, arguments)` pairs; the Codex reader, once a session's entries are assembled, replaces a splittable `exec` call and its result with N calls and N results (`finish()`). A patch parser runs at translate time in `src/tools.rs`, so `ToolCall.input` stays the patch string as recorded.

**Tech Stack:** Rust (edition as in `Cargo.toml`), `serde_json`; no new crates.

**Spec:** `docs/superpowers/specs/2026-11-11-codex-code-mode-design.md`

## Global Constraints

- No new dependency; the list in `Cargo.toml` is closed on purpose.
- Work in the worktree `D:\Projects\mukade-l10` (branch `l10-code-mode`). The main tree is shared with a session developing `src/bus/`; do not touch `src/lib.rs`, `src/reader/mod.rs`, `docs/usage.md`, `README.md`, `docs/README.md`, `docs/adr/README.md`, `src/portable.rs`. New modules are declared inside the file that owns them (`src/reader/codex.rs`, `src/tools.rs`).
- Fixtures hold no real session content: masked strings only (`CMD-1`, `D:/w/a.txt`, …).
- Split-call ids: first call keeps the `exec` `call_id`; the n-th (n ≥ 2) is `format!("{call_id}_{n}")`.
- A script is split only when every rule of spec "When a script is split" holds; otherwise the `exec` call and its result are left exactly as today.
- Cargo on this machine: transient `os error 1455` / `LNK1171` → rerun with `-j 4`.

## Review Focus

1. A string argument containing `));\ntext(await tools.` (a script inside a command) must not be read as two statements → test `a_statement_boundary_inside_a_string_is_not_a_boundary` (Task 1).
2. An output with more or fewer items than statements must leave the script whole → test `an_item_count_mismatch_keeps_the_script_whole` (Task 2).
3. A script whose output is missing (killed turn) must leave the script whole and not panic → test `an_exec_without_output_stays_whole` (Task 2).
4. A patch hunk that only adds lines (empty old text) must not become an `Edit` with an empty `old_string` → test `a_pure_insertion_hunk_is_not_an_edit` (Task 3).
5. Non-ASCII and `\u{...}` escapes in script strings decode to the same characters the model saw → test `escapes_decode_like_javascript` (Task 1).

---

### Task 1: Literal script parser

**Files:**
- Create: `src/reader/codex/exec.rs`
- Modify: `src/reader/codex.rs` (add `mod exec;` near the top)

**Interfaces:**
- Produces: `pub(super) fn parse_script(script: &str) -> Option<Vec<(String, serde_json::Value)>>` — `Some` only when the whole script is one or more `text(await tools.<ident>(<literal>?));` statements; an empty argument list gives `json!({})` (Codex records a no-argument call as `{}`).

- [ ] **Step 1: Write the failing unit tests** in `#[cfg(test)] mod tests` of `exec.rs`:
  - `one_statement_is_one_call`: `text(await tools.exec_command({cmd:"CMD-1", yield_time_ms:1000}));` → `[("exec_command", {"cmd":"CMD-1","yield_time_ms":1000})]`.
  - `statements_in_sequence_are_calls_in_order`: three statements on three lines (exec_command, apply_patch with a string argument, `clock__curr_time({})`) → three pairs in order; the last without a trailing `;` is accepted.
  - `literals_of_every_kind`: `{a:'s', "b":-1.5e3, c:[1,true,null,{d:false}], e:[],}` (trailing comma) → `{"a":"s","b":-1500.0,"c":[1,true,null,{"d":false}],"e":[]}`; integers stay integers (`1` is `json!(1)`, not `1.0`).
  - `escapes_decode_like_javascript`: `"a\nb\t\\\"q\" \u00e9 \u{1F600} \'"` → `"a\nb\t\\\"q\" é 😀 '"`.
  - `a_statement_boundary_inside_a_string_is_not_a_boundary`: one statement whose `cmd` string is `"x));\ntext(await tools.y({}));"` (escaped as the script would hold it) → exactly one call, `cmd` equal to that text.
  - `anything_else_is_refused` → `None` for each: `const r = await Promise.allSettled([tools.exec_command({cmd:"a"})]);`, `text(ALL_TOOLS.filter(x=>x));`, `image(await tools.view_image({path:"p"}));`, `text(await tools.exec_command({cmd:x}));`, a template literal argument `` {cmd:`a`} ``, `text(await tools.a({}), 1);`, two arguments `tools.a({}, {})`, a `// comment` line, an empty script, `text(await tools.a({}));  extra`.
- [ ] **Step 2: Run** `cargo test -j 4 --lib reader::codex::exec` → FAIL (module missing).
- [ ] **Step 3: Implement `parse_script`** as a small recursive-descent parser over `char_indices`: skip whitespace; expect `text(`, `await`, `tools.`, identifier `[A-Za-z_$][A-Za-z0-9_$]*`, `(`, optional literal, `)`, `)`, optional `;`; repeat to end. Literals: object (keys identifier or quoted string, trailing comma allowed), array (trailing comma allowed), string (`"`/`'`, escapes `\n \r \t \b \f \v \0 \\ \" \' \/ \xHH \uHHHH \u{H…}`, surrogate pairs from two `\uHHHH` combined; a raw newline inside a string refuses), number (`-`? digits, optional fraction, optional exponent; no fraction/exponent → `i64`/`u64`, else `f64`; a non-finite value refuses), `true`, `false`, `null`. Anything else → `None`.
- [ ] **Step 4: Run** `cargo test -j 4 --lib reader::codex::exec` → PASS.
- [ ] **Step 5: Commit** `feat(codex): parse a code-mode script made of literal tool calls`.

### Task 2: Split `exec` calls and their results in the Codex reader

**Files:**
- Modify: `src/reader/codex.rs` (`finish()`, around `:796`)
- Modify: `src/reader/codex/exec.rs`
- Test: `tests/codex_reader.rs`

**Interfaces:**
- Consumes: `parse_script` (Task 1); `canonical_call(name, input, env_shell)` and `output_item(item)` in `codex.rs` (private, reachable from the child module as `super::`); `crate::reader::json::result_content`.
- Produces: `pub(super) fn split_code_mode(entries: &mut [Entry], env_shell: Option<CanonicalTool>)`, called in `finish()` after `self.flush()` and before the compaction `kept_from` loop.

- [ ] **Step 1: Write the failing tests** in `tests/codex_reader.rs`, each built with `rows_with(...)` from `custom_tool_call` `{name:"exec", call_id:"call_x", input:<script>}` + `custom_tool_call_output` `{call_id:"call_x", output:[{type:"input_text", text:"Script completed\nWall time 0.1 seconds\nOutput:\n"}, {type:"input_text", text:"OUT-1"}, …]}`:
  - `a_one_call_script_reads_as_that_call`: `exec_command({cmd:"CMD-1"})` → `calls_of` has id `call_x`, `source_name` `exec_command`, `input` `{"cmd":"CMD-1"}`, canonical `Bash` (fixture declares no shell); its result `ResultContent::Text("OUT-1")`, `is_error` false; no call named `exec` remains.
  - `a_sequence_reads_as_calls_with_their_own_results`: three statements → ids `call_x`, `call_x_2`, `call_x_3`, results `OUT-1..3` paired by id, in one `Step`, results in one `Results` entry in the same order.
  - `a_failed_one_call_script_is_an_error_result`: header `Script failed\n…` → split, `is_error` true. Same header with two statements → stays one `exec` call.
  - `an_item_count_mismatch_keeps_the_script_whole`: two statements, one output item → one call `exec` with canonical `Other("exec")`, result unchanged (equal to what today's reader produces).
  - `an_exec_without_output_stays_whole`, `a_running_script_stays_whole` (`Script running with cell ID 3\n…`), `a_string_output_stays_whole`, `an_image_output_stays_whole` (an `input_image` item among them), `a_complex_script_stays_whole` (`Promise.allSettled`).
  - `the_split_keeps_every_source_record`: the existing `every_source_record_is_kept_exactly_once` invariant still holds on a session with a split (raw rows unchanged).
- [ ] **Step 2: Run** `cargo test -j 4 --test codex_reader` → the new tests FAIL.
- [ ] **Step 3: Implement `split_code_mode`.** For each `Kind::Step` call with `source_name == "exec"` and a string `input` (the script), find the `Kind::Results` entry holding `call_id`, and its raw row whose `payload.call_id` matches; read `payload.output`. Split only if: `parse_script` gives `n` calls; `output` is an array; item 0 is `input_text` beginning `Script completed` (or `Script failed` with `n == 1`); every other item is `input_text`; there are exactly `n` of them. Then replace the call in place with `n` `ToolCall`s (`canonical: Some(super::canonical_call(&name, &input, env_shell.clone()))`) and the result in place with `n` `ToolResult`s (`content: result_content(&[super::output_item(item)])`, `is_error` from the header, `truncated_by_source` false). `raw` is never touched.
- [ ] **Step 4: Run** `cargo test -j 4 --test codex_reader` → PASS; `cargo test -j 4` → all PASS.
- [ ] **Step 5: Commit** `feat(codex): read a code-mode exec script as the calls it wraps (L10)`.

### Task 3: `apply_patch` read as Write / Edit / multi-edit at translate time

**Files:**
- Create: `src/tools/patch.rs`
- Modify: `src/tools.rs` (`mod patch;`; hook at the top of `translate_with`, `:145`)
- Test: `tests/tools_translation.rs`

**Interfaces:**
- Produces: `pub(crate) fn view(input: &Value) -> Option<(CanonicalTool, Value)>` — accepts `input` as a bare patch string or `{"input": "<patch>"}`; returns `(Write, {"file_path", "content"})`, `(Edit, {"file_path", "old_string", "new_string"})` or `(MultiEdit, {"file_path", "edits": [{"old_string","new_string"}, …]})`; `None` for anything else.
- In `translate_with`: when `call.source_name == "apply_patch"` and `patch::view(&call.input)` is `Some((canonical, input))`, translate that canonical/input pair instead; otherwise unchanged.

- [ ] **Step 1: Write the failing tests** (unit tests in `patch.rs` for the parser, integration in `tests/tools_translation.rs` through `translate`):
  - `an_added_file_is_a_write`: `*** Begin Patch\n*** Add File: D:/w/a.txt\n+L1\n+L2\n*** End Patch` → toward Claude `Write {file_path:"D:/w/a.txt", content:"L1\nL2\n"}`; toward Pi `write {path, content}`.
  - `one_hunk_is_an_edit`: `*** Update File: D:/w/a.txt\n@@\n ctx\n-old\n+new\n ctx2` → Claude `Edit {file_path, old_string:"ctx\nold\nctx2", new_string:"ctx\nnew\nctx2"}`; an `@@ fn anchor()` header line is not part of the texts.
  - `several_hunks_are_a_multi_edit`: two `@@` hunks → Pi `edit` with two `edits`; Claude text form ("no multi-edit equivalent").
  - `the_object_shape_reads_too`: `{"input": "<patch>"}` → same as the bare string.
  - `a_pure_insertion_hunk_is_not_an_edit`: a hunk of `+` lines only → `view` is `None` → text form.
  - `what_is_not_one_plain_file_stays_text`: `*** Delete File`, `*** Move to:`, `*** End of File`, two file sections, no `*** Begin Patch`, a line in a hunk starting with another character → `None`.
  - `the_patch_stays_the_recorded_input`: after `translate`, the `ToolCall.input` passed in is unchanged (it is borrowed; assert the text form toward Codex is still text form).
- [ ] **Step 2: Run** `cargo test -j 4 --test tools_translation` and `cargo test -j 4 --lib tools::patch` → FAIL.
- [ ] **Step 3: Implement `view`.** Split on `\n`; first line `*** Begin Patch`, last non-empty `*** End Patch`; exactly one section header (`*** Add File: ` or `*** Update File: `, path = rest of line, trimmed, non-empty). Add: every body line starts with `+` → content = lines without `+` joined by `\n`, plus a final `\n`. Update: body is hunks, each starting with a line beginning `@@`; hunk lines start with ` `, `-`, `+`, or are empty (an empty context line); old = context+removed, new = context+added, each joined by `\n`; a hunk with empty old → `None`; zero hunks → `None`. One hunk → Edit, several → MultiEdit.
- [ ] **Step 4: Run** the two commands of Step 2 → PASS; `cargo test -j 4` → all PASS (the L11 test `every_translated_name_is_offered_by_its_destination` included).
- [ ] **Step 5: Commit** `feat(tools): read a Codex apply_patch as the write or edit it makes`.

### Task 4: A Desktop session hops with native calls

**Files:**
- Test: `tests/hop.rs` (follow its existing `request`/`read_destination` helpers; use a hand-made rollout written to a temp dir and `mukade::hop::export_path` + `mukade::hop::import`, or the closest existing pattern there)

**Interfaces:**
- Consumes: Tasks 2 and 3 through the public pipeline only.

- [ ] **Step 1: Write the failing test** `a_code_mode_rollout_reaches_claude_and_pi_with_native_calls`: a masked rollout with `<environment_context><shell>bash</shell>`, one step holding a two-statement `exec` (`exec_command({cmd:"ls CMD-1"})`, `apply_patch("*** Begin Patch\n*** Update File: D:/w/a.txt\n@@\n-a\n+b\n*** End Patch")`) and its 3-item output. Toward Claude Code the written transcript holds `tool_use` `Bash` and `Edit` with ids `call_x` and `call_x_2` and two `tool_result`s; the manifest counts no text-form calls and no `rewritten_call_ids`. Toward Pi: `bash` and `edit`.
- [ ] **Step 2: Run** `cargo test -j 4 --test hop a_code_mode` → PASS if Tasks 2–3 are right (this is a pipeline test; if it fails, the failure names the broken layer). Fix the layer, not the test.
- [ ] **Step 3: Commit** `test(hop): a Codex Desktop session hops with native shell and edit calls`.

### Task 5: Census on real rollouts, docs, merge

**Files:**
- Create: `tests/codex_code_mode_census.rs` (one `#[test] #[ignore]`)
- Create: `docs/verification/2026-11-11-codex-code-mode.md`
- Modify: `docs/session-interchange-spec.md` (§8.1 L10 row: built, with the sequence widening; §10: remove the "Not implemented: L10" item), `docs/tool-conversion-tables.md` (§5: the widening, the ids, and replace "via the existing `apply_patch` parser" with the real rule), `docs/harnesses/codex.md` (Known limits: code mode and `apply_patch` items), `next.md` (Codex item 2 → settled, plus a new "Across harnesses" item: destination default shell and Claude `PowerShell` in the offered set, L11), `src/model/tool.rs:97` (the `from_codex` doc comment).

- [ ] **Step 1: Write the census test**: reads every `rollout-*.jsonl` under `$CODEX_HOME/sessions` and `$CODEX_HOME/archived_sessions` (skip when `CODEX_HOME` is unset), parses each with `mukade::reader::codex::read_file`; asserts every call has exactly one result with its id unless the turn was killed, and no call id contains `#`; prints totals: `exec` calls seen, split, kept whole; split calls by inner tool; and, translating every split call with `tools::translate` toward Claude Code and Pi with `None` and with `Some(Shell::PowerShell)`, how many translate. It never writes.
- [ ] **Step 2: Run** `CODEX_HOME=$USERPROFILE/.codex cargo test -j 4 --test codex_code_mode_census -- --ignored --nocapture` → PASS; copy the printed totals.
- [ ] **Step 3: Write** `docs/verification/2026-11-11-codex-code-mode.md`: machine, date, Codex versions seen, the totals, and the honest gap (PowerShell-declared sessions translate only with `--dest-shell powershell`; multi-file patches stay text). Update the docs listed above.
- [ ] **Step 4: Run** `cargo test -j 4` and `cargo clippy -j 4 --all-targets` → PASS, no new warnings.
- [ ] **Step 5: Commit** `docs(codex): code mode is read; census of the real rollouts` and fast-forward master from the main tree: `git -C D:/Projects/mukade merge --ff-only l10-code-mode` (if master moved, `git rebase master` in the worktree first), then `git -C D:/Projects/mukade push`, `git worktree remove D:/Projects/mukade-l10`, `git branch -d l10-code-mode`.
