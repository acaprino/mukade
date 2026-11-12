# Subagents phase 1 — implementation plan

Spec: `docs/superpowers/specs/2026-11-12-subagents-design.md` (Phase 1). Branch `subagents`,
worktree `D:\Projects\mukade-subagents`, base `74dd2f5`. TDD every task (the test fails first).
Global: no behaviour change beyond the spec; public API additions only (new struct fields are fine
where the structs are built with `..Default::default()` or inside the workspace — say so if not);
no new crates; tests/architecture.rs green; `cargo test -j 4 --workspace` and
`cargo clippy -j 4 --workspace --all-targets` (no new warnings) before each commit; repository
commit trailer.

### Task 1: `ParentKind` and the lineage fields
**Files:** `src/model/*`, `src/reader/mod.rs` (`SessionInfo`), `src/wire.rs`, the four readers'
existing parent assignments (Codex `parent_thread_id` preference, OpenCode `parentID`, Pi `Fork`),
tests in each reader's test file.
- [ ] `ParentKind`, `Meta.parent_kind`, `SessionInfo.parent_kind`; dump document carries it.
- [ ] OpenCode reads `parentID` (export form) as well as `parent_id`; `info.fork` only if the field
      is verified in OpenCode 2.2.5 source (else note it in the report).
- [ ] Codex prefers `parent_thread_id`; guardian review identification if the rollout says so.
- [ ] Commit `feat(model): a session says how it came from its parent`.

### Task 2: Claude children
**Files:** `src/reader/claude_code.rs` (+ helpers), `src/harnesses/claude_code.rs` if the adapter
lists, fixtures under `fixtures/claude_code/`, tests.
- [ ] `list_all` lists children with a transcript; id `<parent-id>.<agentId>`; title from meta.json.
- [ ] `read` of a qualified id and `read_file` of an `agent-*.jsonl` read the sidechain rows as
      content, with the qualified id and `parent`/`parent_kind`.
- [ ] Check every place a session id ends up in a file name or a path (manifest save, TUI, cache)
      still works with a `.` in it.
- [ ] Commit `feat(claude): subagent sessions are listed and read`.

### Task 3: The report the parent saw
**Files:** the three readers, `src/writer/plan.rs` / manifest counting, tests.
- [ ] Claude attachment task-notification, OpenCode synthetic subagent report, Codex plaintext
      `agent_message` → `User` turn in place; encrypted → Meta counted as
      `dropped_subagent_messages` (manifest `counts` + `losses`).
- [ ] Check the strict provider rules (`src/strict.rs`) still pass with a user turn between a call
      and later turns; add a test hop for each.
- [ ] Commit `feat(readers): a subagent's report stays in the parent that received it`.

### Task 4: CLI, catalog, docs
**Files:** `src/cli.rs` (`list --children`, `parent`/`kind` fields, usage text),
`catalog/harnesses.json` (Codex note), `tests/cli.rs`, docs: `docs/usage.md`, the four harness pages
(a "Subagents" row/section: how each stores them, what mukade reads), `docs/session-interchange-spec.md`
(lineage), `next.md` (phase 1 done; phase 2 item; open items from the spec). Docs style: clear,
complete, not wordy; tables; one home per fact.
- [ ] Commit `feat(cli): list --children; docs: subagent sessions`.
