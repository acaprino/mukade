# Subagents phase 2 — implementation plan

Spec: `docs/superpowers/specs/2026-11-12-subagents-design.md` (Phase 2). Branch `subagents`
(base `883b8d7`, phase 1 on master). `ParentKind::Spawn.inherits_context` marks a child that starts
from the parent's conversation (Claude `fork` agent: prefix not in its file; Codex `forked_from_id`:
prefix copied into its rollout) — such a child is written with what its file holds; the inherited
prefix is not re-added (stated in the manifest as `inherits_context`). TDD every task. Global constraints as in phase 1.

## Decisions (made on the user's behalf)
- Opt-in: `--children` on `dump` and `hop` (and `HopRequest.children: bool` in the library). Without
  it nothing changes.
- Which children: every readable child whose `parent` is the session being hopped, depth 1 and
  deeper (a child's own children follow it, parent before child). A child that cannot be read
  (Claude meta.json without transcript, Codex DB edge whose rollout is gone) is listed in the
  manifest `losses` as `child <id>: <reason>` and the hop goes on.
- Order: parent first, then children in spawn order (call order in the parent; unknown order last,
  by created time).
- Same pipeline per child: each child is read, planned and written as its own session with the same
  options as the parent (compaction, reasoning, tools, dest shell); `--dest-id`/`--dest-path` apply
  to the parent only. `--expect-sha256` applies to the parent only.
- Link per destination:

  | destination | child written as | link |
  |---|---|---|
  | OpenCode | its own imported session | `info.parentID` = the destination parent's id (parent imported first; child ids chosen before the parent is written so the parent's text can name them) |
  | Claude Code | `<project>/<parent-id>/subagents/agent-<agentId>.jsonl` rows with `isSidechain:true`, `agentId`, `sessionId` = parent id, + `agent-<agentId>.meta.json` {agentType, description, toolUseId (destination call id when the spawn call maps), spawnDepth} | the folder; agentId = `a` + 16 hex derived from the destination child |
  | Codex | its own rollout | none native (writer writes no session_id / DB rows); manifest only |
  | Pi | its own session file | none native (parentSession means fork); manifest only |

- The parent's text-form spawn call gets one more line naming the destination child id
  (`→ <dest-child-id>`), so a reader of the destination parent can find it.
- Manifest: `children: [{source_id, dest_id, dest_path?, link: "native"|"none"}]`; receipt prints
  one line per child. Each child also gets its own manifest entry file? → No: one manifest per hop,
  children listed inside (decided: one file, simpler for hosts).
- `dump --children`: the dump document gains `children: [<dump document>...]` (recursive), each a
  full document of its own.
- Failure: a child write failure after the parent is written does not undo the parent; exit code 4
  (write failed) with the receipt listing what was written. Re-run is safe with `--dest-id` for the
  parent? → children get new ids each time; documented.

## Tasks
1. Library: `children_of(session_ref)` per harness through the reader (uses phase 1 `list_all` +
   `parent`/`parent_kind`); `HopRequest.children`; dump document `children`. Tests per harness with
   phase 1 fixtures.
2. Writers: OpenCode `parentID` in the import document (and import order); Claude subagents folder +
   meta.json writer; Codex/Pi as plain sessions. Text-form spawn call names the destination child.
   Tests: write into temp stores and read back with mukade's own readers (the child lists under its
   parent for OpenCode and Claude).
3. CLI: `dump --children`, `hop --children`, receipt + manifest `children`, exit code on partial
   failure. CLI tests end to end for each destination.
4. Docs: usage (flags, manifest field), each harness page (subagents written), interchange spec
   (children in the dump document and manifest), next.md (phase 2 done; [U] items: whether Claude
   Code shows a hand-written subagent; whether OpenCode import accepts a child with parentID — verify
   with the real `opencode` binary in a scratch store if possible without model calls).

## Notes from the phase 1 implementer (2026-11-12, "Phase 2 fit")
- Child id `<parent-id>.<agentId>` → agent id for `subagents/agent-<id>.jsonl`; depth-2 parent is `<root>.<parentAgentId>`.
- Dump: `meta.parent_kind` does not clash with a top-level `children: [...]`.
- `list_all` yields every child with its parent.
- Must change: `--children` is refused outside `list` (src/cli.rs) and `tests/cli.rs` `children_is_a_list_option` asserts that for `dump` → widen both.
- `--compaction summary` of a Claude child is refused → `hop --children --compaction summary` summarizes the parent only (children carried whole, or refused if one is too big? → children carried with `--compaction none`, stated in the receipt).

## Phase 1 review leftovers to keep in mind
- A Claude child whose transcript continues another agent's (resumed agent) is refused by the reader: in `hop --children` it is a child loss line, not a failure.
- `list_all` reads every child's meta.json each call: phase 2 calls it once per hop, not per child.
