# Hop registry, fork links and `write --children` — design

Status: **proposed** 2026-11-12, not decided. Two choices are the user's (§5). Everything below
marked [V] was read in this repository's code on 2026-11-12; [U] is not verified against a harness.

## 1. Problem

Three items of `next.md` share one missing piece: mukade forgets what it wrote.

| item | today | effect |
|---|---|---|
| A fork hopped is a copy | the manifest names the origin (`source.parent`, `parent_kind`); the written fork has no link to anything [V] | in the destination the fork is an unrelated session, even when its origin was hopped there before |
| A repeated `hop --children` | every hop writes the parent and each child under new ids (`src/children.rs` `import_with_children()`) [V] | hopping the same session twice leaves two full copies of the tree |
| `write` with children | `dump --children` nests each child's document in `children`; `write` imports the session alone and says so (`src/cli.rs`, `write`) [V] | a dump made to move a tree to another machine loses the subagents on import |

The third needs no memory and is independent; the first two need a record of what was written
where, and of which destination record each source entry became.

## 2. What exists

- `Written { id, location, resume }` is all a writer returns (`src/writer/mod.rs`) [V]. The ids
  of the records it wrote (Claude row `uuid`s, Codex ordinals, Pi entry ids, OpenCode message ids)
  are minted inside the writer and lost.
- `PlannedBlock` carries no source entry id (`src/writer/plan.rs`) [V]: the plan cannot say which
  source entry a written record came from.
- `manifest.children` already maps each child's `source_id` to `dest_id` and `dest_path` [V].
- The native fork link of each harness, as its reader reads it [V]:

  | harness | where a fork names its origin | its boundary |
  |---|---|---|
  | Claude Code | `forkedFrom: {sessionId, messageUuid}` on every row `/branch` copied | `messageUuid` |
  | Codex | `session_meta.forked_from_id` | `forked_from_ordinal_exclusive` |
  | OpenCode | `fork.sessionID` (export), `fork_session_id` (row) | `fork_boundary` |
  | Pi | the header's `parentSession` (the origin's file path) | none |
  | DeepSeek Harness | none for a web-UI fork (0.2.2-rc.1 writes no `parentSession`) | `session/end-seed` |
  | Hermes | `model_config._branched_from` | none recorded |

## 3. Proposal A — `write --children` (no registry; do first)

1. `mukade write <dest> <doc> --children` imports the nested `children` after the session, as
   `hop --children` does: same ids chosen up front, same native links, same `manifest.children`.
   Without the flag, `write` keeps today's behaviour and message.
2. `import_with_children` takes its children as `Vec<ChildRead>` already; a second constructor
   builds that list from the document tree instead of the store: `info` from each child's `meta`,
   `depth` and `parent` from the nesting, `call_id` from `meta.parent_kind.call_id`, `session` from
   `wire::from_document`. A child document that does not parse is a loss line, as an unreadable
   child is in a hop.
3. Each child is written with what its document holds (`dump` already chose its compaction);
   `children_lost` of the document is copied into `losses`.
4. Tests: `dump --children` of the Claude subagent fixture, then `write --children` into Claude
   Code, Codex, OpenCode (scratch store) and Pi; the manifest's `children` matches the hop's.

Size: small, plumbing only.

## 4. Proposal B — the hop registry, fork links, repeated hops

### 4.1 The registry

- **Where:** `mukade-data/registry/`, beside the cache (`src/portable.rs`), private to the user like
  the rest of `mukade-data`. One JSON file per source session:
  `registry/<source-harness>/<fnv64 of source id>.json`, holding a list of the times it was written.
- **A record:** `{time, source: {harness, id, content_sha256}, dest: {harness, store, id,
  location}, as: "session" | "child", entries: [[source_entry_id, dest_record_id], …]}`. `store` is
  the destination root the hop used, so two installations of one harness are told apart.
- **Written by** `import_inner` after a write succeeded, for the session and for each child; best
  effort, like the cache: a registry that cannot be written is a warning, never a failed hop.
- **Trusted only after a check:** a record is used only if its destination session is still there
  (the file exists; OpenCode: the id is in the database) — a deleted copy is simply not found.
- **Cost:** one pair per conversation entry (blocks, not raw rows): ~60 bytes each, ~60 KB for a
  thousand-entry session.

### 4.2 What the writers must return

- `PlannedBlock` gains the source entry id it came from (`Option<String>`; a block mukade adds,
  such as a handoff line, has none).
- `Written` gains `records: Vec<(String, String)>`: for each block with a source id, the id of the
  destination record that holds it. File writers (Claude Code, Codex, Pi) know it as they mint it;
  OpenCode's ids are chosen by mukade in the document it imports [V for sessions, U for messages];
  Hermes and DeepSeek Harness return none at first.

### 4.3 Fork links through a hop

When the source is a fork (`parent_kind: fork`) and the registry holds its origin written to the
same destination store, the fork is written with the destination's own link:

| destination | written link | boundary |
|---|---|---|
| Claude Code | `forkedFrom: {sessionId: <origin's dest id>, messageUuid: <dest uuid of `at`>}` on the copied prefix's rows | needs `at` mapped |
| Codex | `forked_from_id` = origin's dest id, `forked_from_ordinal_exclusive` = dest ordinal after `at` | needs `at` mapped |
| OpenCode | `fork: {sessionID, boundary}` in the imported document [U: whether `session import` keeps it] | needs `at` mapped |
| Pi | `parentSession` = origin's dest file | none needed |
| DeepSeek Harness, Hermes | none (no writable field, or not verified) | — |

- `at` unknown, or not in the origin's `entries`: Pi still links; the others write no link and say
  why (`losses`: "fork of <origin>: its boundary is not known in the destination").
- Origin not yet hopped there: no link, a loss line saying so. A later hop of the origin does not
  rewrite the fork: written sessions are never edited.
- The manifest gains `dest.parent` (the destination id linked to) when a link was written.

### 4.4 Repeated hops

The registry also tells when the same source content (`content_sha256`) was already written to
the same destination store and is still there. What to do then is §5, question 1. A child is never
reused on its own: in Claude Code and OpenCode a child belongs to the parent it was written under,
so a new parent copy takes new child copies.

### 4.5 Tests

- Registry: a hop writes its record; a deleted destination session is not found; an unwritable
  folder warns and the hop succeeds.
- Writers: `records` maps every planned block that has a source id (Claude, Codex, Pi).
- Forks: hop a Claude Code origin into Claude Code, then its fork: the fork's prefix rows carry
  `forkedFrom` with the origin copy's id and the mapped uuid; Pi: `parentSession`; fork first, then
  origin: no link, a loss line.
- Live [U]: Claude Code shows the written fork under its origin; Codex resumes a fork written with
  `forked_from_id`.

Size: medium. Order: §4.1–4.2, then 4.3 per destination (Pi, Claude Code, Codex, OpenCode), then 4.4.

## 5. Choices for the user

1. **The same unchanged session hopped again into the same destination**, still there:
   - (a) write a new copy, as today, and print a notice naming the earlier one — *recommended*:
     no behaviour changes, the user learns about the duplicate;
   - (b) refuse (exit 2) unless `--again`;
   - (c) write nothing and print the earlier receipt, unless `--again`.
2. **The registry itself:** it keeps, in `mukade-data`, the ids and paths of every session mukade
   wrote and where it came from. On by default with a variable to turn it off
   (`MUKADE_REGISTRY=off`) — *recommended* — or off unless asked (`--register`), which makes fork
   links depend on the user remembering the flag.

## 6. Out of scope

Rewriting a written session to add a link later; links into Hermes and DeepSeek Harness; carrying
the registry between machines (a `dump` names its origin already); the Claude Code leaf flags
(`next.md`).
