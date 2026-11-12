# Forks and branches — implementation plan

Spec: `docs/superpowers/specs/2026-11-12-forks-branches-design.md`. Research with evidence:
`docs/superpowers/specs/2026-11-12-forks-branches-research.md` (copied from the scratchpad).
TDD every task (the test fails first). Global: no behaviour change beyond the spec's; public API
additions only, except `ParentKind::Fork` gaining a field (pre-release; every workspace use updated);
no new crates; tests/architecture.rs green; `cargo test -j 4 --workspace` and
`cargo clippy -j 4 --workspace --all-targets` (no new warnings) before each commit; repository
commit trailer. Tests never read real stores except `#[ignore]` live census tests that say so.

## Part A — fork lineage and Codex correctness

### Task 1: `ParentKind::Fork { at }`
- [ ] Struct variant with `#[serde(default)] at: Option<String>`; `ParentKind::fork()`; every
      construction updated (Pi, OpenCode, Hermes, DeepSeek, tests); `{"kind":"fork"}` still parses.
- [ ] Commit `feat(model): a fork says where it left its parent`.

### Task 2: Fork lineage in every reader
- [ ] Claude: `forkedFrom.sessionId` (+ `messageUuid` of the last copied row as `at`) on read and in
      the listing peek. Codex: top-level `session_meta.forked_from_id` → `Fork` (a forked subagent
      stays `Spawn` with `inherits_context`). OpenCode: listing selects `fork_session_id` /
      `fork_boundary` (column-guarded); `store_stats` forks count `fork_session_id` only. Pi:
      `parentSession` path → the id in that file's header (listing and read).
- [ ] Fixtures per harness (synthetic); listing and read agree.
- [ ] Commit `feat(readers): forks name their origin in every harness`.

### Task 3: Codex correctness
- [ ] `thread_rolled_back{num_turns}`: the last N user turns before it (and what followed them)
      become `Meta(OffBranch)`; counted as today.
- [ ] Referenced fork (`history_base`): the origin's prefix up to `end_ordinal_exclusive` only.
- [ ] Forked subagent: line-1 copied parent `session_meta` is Meta; the copied history is kept.
- [ ] Commit `fix(codex): rolled-back turns and referenced forks read as Codex replays them`.

### Task 4: Counts, catalog, manifest, docs (A)
- [ ] `store_stats().forks` for Claude and Codex from the peeks; catalog `lineage.recorded: true`
      for both with notes; the agreement test green. Manifest `source_parent`, `source_parent_kind`.
- [ ] Docs: each harness page's Branches/forks section, usage (lineage), catalog reference, next.md.
- [ ] Commit `feat(lineage): fork counts and the catalog record forks; docs`.

## Part B — every branch through dump and hop

### Task 5: Model and dump document
- [ ] `Session.branches: Vec<Branch>`, `Branch { from, entries }`; `entries` stays the active path;
      dump document `branches: [{from, messages}]` (absent when empty); `from_document` reads it.
- [ ] Commit `feat(model): a session can carry the branches it left`.

### Task 6: Readers fill branches with `--all-branches`
- [ ] Claude `parentUuid` tree, Pi `parentId` tree, Codex redo pages + rollbacks; each branch parsed
      with the full conversation parser (steps merged, results paired). OpenCode refuses as today.
      Codex `--all-branches` becomes branches instead of a linear splice (visible change).
- [ ] Commit `feat(readers): --all-branches reads every branch as a branch`.

### Task 7: Writers keep branches where the destination has a tree
- [ ] Claude: rows chained by `parentUuid`, branches first, active path last, `last-prompt.leafUuid`
      = active leaf. Pi: `id`/`parentId` tree, active leaf last. Codex: `thread_rolled_back` between
      branches when the branch point is a user-turn boundary, else dropped and counted. OpenCode:
      dropped and counted. Strict gate per branch (prefix + branch); a failing branch is dropped and
      counted. Manifest `branches_kept`, `branches_dropped` + loss lines.
- [ ] Round trips: write into temp stores, read back with mukade's readers: same branches, same
      active leaf.
- [ ] Commit `feat(writers): branches travel where the destination keeps a tree`.

### Task 8: CLI and docs (B)
- [ ] `--all-branches` accepted for Claude, Pi, Codex in dump/hop/document; usage, tables, harness
      pages, interchange spec (branches in the document and manifest), next.md.
- [ ] Commit `docs: every branch through dump and hop`.
