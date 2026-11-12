# Forks and branches — design

Status: decided 2026-11-12 (design delegated). Starts after subagents phase 2 and the Hermes/DeepSeek
branch are on master (both touch the readers and writers this changes). Facts: scratchpad `forks-branches-research.md`
([V] unless marked). Builds on subagents phase 1 (`ParentKind`, `list --children`, CLI `parent`/`kind`).

## Problem (short)

| gap | effect |
|---|---|
| Claude `forkedFrom`, Codex `forked_from_id`, OpenCode `fork_session_id` are never read in listings; catalog says Claude/Codex record no forks | forks look like unrelated sessions |
| Pi `parent` is a file path, the others an id | callers cannot follow it |
| Codex `thread_rolled_back{num_turns}` ignored | rolled-back turns hop as live history (correctness bug) |
| Codex referenced fork (`history_base`) counts the origin's later turns as the fork's dropped branch; `--all-branches` splices them in | wrong counts and content |
| `--all-branches` is Codex-only and linear (no branch marker); dump drops every Meta | branches cannot be exported or carried, even where the destination has a tree |
| OpenCode `harnesses` "forks" counts subagent children | wrong number |

## Part A — fork lineage and Codex correctness (plan A)

1. **Readers set `parent` + `ParentKind::Fork`** (listing and read):
   Claude: first conversational row's `forkedFrom.sessionId` (one peek per file; `--fork-session` [U]);
   Codex: `session_meta.forked_from_id` (listing already peeks line 0); a forked subagent stays
   `Spawn` (spawn is the relation that matters; its inherited prefix is kept, see 3);
   OpenCode: listing selects `fork_session_id` → `Fork`; Pi: `parentSession` (a file path) resolved
   to the `id` in that file's header line, in listing and read alike; a path that does not resolve
   stays as given (no new field).
2. **`ParentKind::Fork { at: Option<String> }`** (was a unit variant; the enum is
   `#[non_exhaustive]`): the origin entry the fork starts after when the source says (Claude
   `forkedFrom.messageUuid` of the last copied row; Codex `forked_from_ordinal_exclusive` /
   `history_base.end_ordinal_exclusive`; OpenCode `fork_boundary.messageID`). `ParentKind::fork()`
   builds `Fork { at: None }`, as `spawn()` does. Serialized `{"kind":"fork"}` still reads (`at`
   defaults). Every construction is updated, the Hermes and DeepSeek readers included.
3. **Codex correctness**: apply `thread_rolled_back{num_turns}` (the last N user turns before it
   become `Meta(OffBranch)`, counted as today); a referenced fork reads the origin prefix up to
   `end_ordinal_exclusive` only (no off-branch tail from the origin). A forked subagent's copied
   parent history (records 2…`subagent_history_start_ordinal`-1) is kept — it is what the child
   model saw — and its line-1 parent `session_meta` is Meta.
4. **Counts**: `store_stats().forks` counted for Claude and Codex (from the listing peeks); catalog
   `lineage.recorded: true` for Claude (`forkedFrom`) and Codex (`forked_from_id`), notes updated;
   OpenCode counts `fork_session_id` only. The agreement test stays green.
5. **Hop of a fork**: unchanged — a self-contained copy. The manifest gains `source_parent` and
   `source_parent_kind`. Linking a fork to an already-hopped origin is out of scope (needs a hop
   registry; `next.md` item).

## Part B — all branches through dump and hop (plan B)

1. **Model**: `Session.branches: Vec<Branch>`, `Branch { from: String /* the entry id the branch
   leaves from */, entries: Vec<Entry> }`. `entries` stays the active path; every existing consumer
   is unchanged. A branch is parsed with the full conversation parser (steps merged, results paired).
2. **Readers** fill `branches` when asked (`Harness::read_all_branches` / `--all-branches` becomes
   available for Claude, Pi and Codex): Claude `parentUuid` tree (active = mukade's leaf rule), Pi
   `parentId` tree (active = last entry), Codex: redo pages (`end_ordinal_exclusive` is the branch
   point) and rolled-back turns. OpenCode: refused as today (no tree).
   Codex `--all-branches` changes from a linear splice to branches (visible change, documented).
   Without `--all-branches` nothing changes (off-branch rows stay `Meta(OffBranch)`, counted).
3. **Dump document**: `branches: [{from, messages}]` (absent when empty).
4. **Writers**, by destination:

   | destination | branches written as | else |
   |---|---|---|
   | Claude Code | one file, rows chained by `parentUuid` (branch rows point at their real parent), branches first, active path last, `last-prompt.leafUuid` = active leaf | — |
   | Pi | `id`/`parentId` tree, branches first, active leaf the last entry | — |
   | Codex | branch A, `event_msg thread_rolled_back{num_turns:k}`, branch B — only when the branch point is a user-turn boundary | dropped + counted |
   | OpenCode | — | dropped + counted |

   New manifest counts: `branches_kept`, `branches_dropped` (+ loss line per dropped branch).
5. **Strict gate per branch**: each branch with its shared prefix must pass `strict::check_plan`
   on its own; a branch that fails is dropped and counted, the active path must pass as today.

## Out of scope (next.md)
Fork links through a hop (hop registry); Claude 2.1.322 leaf flags `explicit`/`rewound`/`cutUuid`
[U]; Codex paginated writing (`history_base` pages); OpenCode staged `revert`; TUI tree (TUI phase).

## Tests
Synthetic fixtures per harness: Claude fork (`forkedFrom`) and a 2-leaf tree; Codex fork
(`forked_from_id`), a rollback, a redo page with `history_base`, a forked subagent; OpenCode fork
row; Pi fork with `parentSession` path and a 2-branch tree. Read-back tests: write branches into a
temp store and read them back with mukade's own readers (same branches, active leaf preserved).
