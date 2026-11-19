# Forks and branches in mukade: research (2026-11-12)

Read-only. Code read at `D:\Projects\mukade-subagents` **HEAD `edab9ce`** (two commits past origin/master
`74dd2f5`: `001997a` subagent spec, `edab9ce` "a session says how it came from its parent", which adds
`ParentKind`). Files with uncommitted changes (`src/reader/claude_code.rs`, `claude_code/order.rs`,
`src/reader/mod.rs`) were read with `git show HEAD:`. Paths below are relative to that repo unless they
name a harness. `[V]` verified here (code, binary, source or store); `[U]` not verified.

Installed: Claude Code 2.1.322 (`%USERPROFILE%\.local\bin\claude.exe`), Codex 0.167.1
(`%LOCALAPPDATA%\Programs\OpenAI\Codex\bin\codex.exe`), OpenCode 2.2.5 (`@opencode/cli`), Pi 1.4.0
(`@earendil-works/pi-coding-agent`). Codex public source: `openai/codex@c0cc9fc820f6` (main, fetched today).
Scan scripts: `scratchpad\scripts\{claude_scan,claude_branch_detail,codex_scan,codex_threads,opencode_scan,pi_scan}.py`.

---

## 0. Headline findings

1. **Every harness now records fork lineage natively; mukade's catalog says two do not.**
   Claude `/branch` writes `forkedFrom:{sessionId,messageUuid}` on every copied row [V binary]; Codex writes
   `session_meta.forked_from_id` (+ `forked_from_ordinal_exclusive`, `attached_thread`) [V source, 13 on disk].
   `catalog/harnesses.json` says `recorded:false` for both (lines 35-38, 67-70), and the subagent spec item 6
   (`docs/superpowers/specs/2026-11-12-subagents-design.md:55`) repeats "Codex: forks are not recorded".
2. **Pi's `parentSession` is a file path, not an id** (`session-manager.js:1248,1404`) [V]. mukade puts it in
   `parent` as is (`src/reader/pi.rs:297,701`), so "parent" is an id for Codex/OpenCode and a path for Pi.
3. **Codex has two fork shapes.** Legacy threads fork by **copy** (`ForkPersistence::Copied`); paginated threads
   (8487 of 8633 rows here) fork by **reference**: `history_base` → the origin's rollout prefix
   (`ForkPersistence::Referenced`, `thread_manager.rs:1862-1888`) [V source]. mukade's page follower would read a
   referenced fork, but count the origin's later turns as the fork's `dropped_off_branch`, and `--all-branches`
   would splice them into the fork [V code, U on a real file].
4. **Codex `thread_rolled_back` is ignored by mukade** [V]: Codex drops the last N user turns on replay
   (`rollout_reconstruction.rs:576-577`); mukade carries them as live history. 0 such events on disk here.
5. **`dump` never shows off-branch records**: the document form drops every `Meta` (`src/wire.rs:119`). Only
   `hop`/`document` count them (`dropped_off_branch`); `dump` → `write` loses even the count [V].
6. **OpenCode cannot receive a fork link through `session import`**: `SessionTransfer.import` reads
   `info.parentID` (child link) but never `info.fork` [V binary]. OpenCode keeps no branches at all.
7. **Real data here is thin**: 2 of 288 Claude sessions have a branch (1 off-branch conversation record each),
   0 Pi branches, 0 Pi/OpenCode/Claude forks, 13 Codex forked *subagents*, 0 top-level Codex forks among the
   34 surviving rollouts, 12 Codex DB rows whose rollout was a revert page (files gone). Design must rest on
   source + fixtures, not on this store.

---

## 1. Per harness: how forks and branches are recorded, resumed, and counted here

### 1.1 Fork recording

| harness | how a fork is made | what is written | history in the fork | evidence |
|---|---|---|---|---|
| Claude Code | `/branch` (interactive) | new `<new-id>.jsonl`; only the rows of the **current path** (`A=new Set(current messages)`), relinked linearly (`parentUuid` = previous kept row, `progress` rows never parents), **same `uuid`s**, `sessionId`=new, `isSidechain:false`, `forkedFrom:{sessionId:<origin>,messageUuid:<row uuid>}` on each row; `custom-title` "`<title> (Branch)`" | **copy** | [V] claude.exe 2.1.322 strings: `P={...s,...I,sessionId:o,parentUuid:S,isSidechain:!1,sessionKind:void 0,forkedFrom:{sessionId:r,messageUuid:s.uuid}}`, "No messages to branch", `\(Branch(?: (\d+))?\)$`, `tengu_conversation_forked` |
| Claude Code | Agent SDK `forkSession(id,{upToMessageId})` | same, but **new uuids** (remapped), `forkedFrom` on each row, title "`… (fork)`" | copy | [V] SDK bundled in opencode.exe 2.2.5: `uuid:v,parentUuid:m,…,sessionId:c,…,forkedFrom:{sessionId:t,messageUuid:_.uuid}` |
| Claude Code | `--resume X --fork-session` | new session id; whether rows get `forkedFrom` | copy (expected) | [U] code path not isolated; no fork of any kind exists in this store (0 `forkedFrom` rows, 0 uuids shared across 288 files) |
| Codex | `codex fork [ID]`, `codex exec fork`, app-server `thread/fork`, legacy thread | new rollout; `session_meta.forked_from_id`; history copied (`ForkPersistence::Copied`, `ForkSnapshot::TruncateBeforeNthUserMessage(n)` or `Interrupted`) | copy | [V] `protocol.rs:3519` `forked_from_id`, `thread_manager.rs:1807-1858`, `:218-236` |
| Codex | same, **paginated** thread (Desktop default) | new rollout whose `session_meta.history_base` = `{thread_id:<origin>, end_ordinal_exclusive, end_byte_offset}` + `forked_from_id`; `forked_from_ordinal_exclusive` keeps the fork boundary when a later revert replaces the base; `attached_thread:true` "marks a persisted fork as attached" | **reference** to origin's file prefix | [V] `protocol.rs:3519-3526,3573`; `thread_manager.rs:1862-1888` (`ForkPersistence::Referenced{history_base,inherited_item_count}`); `:1805` "Paginated sources are rejected. Use prepare_fork + fork_prepared_thread" |
| Codex | subagent with `fork_turns:"all"` (`spawn_agent`) | child rollout: line 0 child `session_meta` with `forked_from_id`=`parent_thread_id`=parent, `subagent_history_start_ordinal`; line 1 = **the parent's own `session_meta`**; records 2…start-1 = parent's history copy; then "You are now a newly spawned child agent" | copy | [V] 13 rollouts (3 parents), e.g. `rollout-2026-11-10T09-58-56-01a1c4ae…jsonl` start 25 |
| OpenCode | `run --session X --fork`, `mini --fork`, HTTP `POST /api/session/:id/fork {before?}`, ACP `session/fork` | event `session.forked{sessionID,parentID,boundary:{type:"before"\|"through",messageID}}`; projector inserts `session_v2` with `parent_id:null`, `fork_session_id`, `fork_boundary`, then **copies** source `session_message` rows with seq ≤ boundary (new ids `<event>_<seq>`, same `seq`; skips unfinished assistant, running shell/compaction; remaps compaction `keepID`) | copy | [V] opencode.exe strings `Session.fork`, projector `insert(We).values({id:r.data.sessionID,parent_id:null,fork_session_id:r.data.parentID,fork_boundary:…})` |
| Pi | `/fork` (pick a user message), `/clone` | `createBranchedSession(leafId)`: new file with only the **root→leaf path** (labels stripped, re-chained), same entry ids; header `parentSession: <source file path>` | copy (one branch) | [V] `dist/core/session-manager.js:1205-1260`; `agent-session-runtime.js:195-245` |
| Pi | `/fork` with no target | empty new session, header `parentSession` | none | [V] `agent-session-runtime.js:203-205` |
| Pi | CLI `--fork <path\|id>` | `forkFrom`: new header `parentSession: <resolved source path>`, **all** entries copied (whole tree, all branches) | copy (whole tree) | [V] `session-manager.js:1374-1410`; `main.js:277` |

### 1.2 Branch recording (inside one session)

| harness | branch mechanism | on disk | which branch the harness resumes / shows | evidence |
|---|---|---|---|---|
| Claude Code | rewind (`Esc Esc`, `/rewind`), edited prompt | new row whose `parentUuid` is an earlier row; abandoned rows stay; compaction boundary `parentUuid:null` + `logicalParentUuid` | `last-prompt.leafUuid` checkpoint; 2.1.322 adds `explicit:true`, `rewound:true`+`cutUuid`, and withdrawal `leafUuid:null`; else newest row | [V] claude.exe: `if(Bo.leafUuid)Pn=Bo.explicit===!0…,Fo=Bo.rewound===!0…,Eo=Bo.rewound===!0?Bo.cutUuid:void 0`; `queueLeafCheckpointWithdrawal({type:"last-prompt",leafUuid:null,explicit:!0})`. Store: 22,058 `last-prompt` rows, none with `explicit`/`rewound` |
| Codex (legacy) | TUI backtrack / rollback | event `thread_rolled_back{num_turns}` appended; earlier records stay | replay drops the last `num_turns` user turns (`drop_last_n_user_turns`) | [V] `protocol.rs:4118-4120`; `rollout_reconstruction.rs:345-347,576-577` |
| Codex (paginated) | `thread/revert` (Desktop "redo") | page `rollout-<ts>-<thread>_<page>.jsonl` with `history_base` (same thread id); base's tail stays | `state_5.threads.rollout_path` names the page | [V] codex.exe "thread/revert only supports paginated threads"; docs/harnesses/codex.md:160-169 |
| OpenCode | revert | `session_v2.revert {messageID, partID?, snapshot, files}` staged; `session.revert.commit` → `session.revert.committed{to}`; `clear` undoes | no tree: committed revert removes the later messages; while staged they are hidden | [V] opencode.exe `SessionRevert.plan/commit`, routes `session.revert.stage/commit/clear`; [U] the delete in the committed projector not traced |
| Pi | `/tree` navigation, `branch_summary` | every entry `id`/`parentId`; new branch from earlier entry; optional `branch_summary{fromId}`; `label` entries are tree nodes | leaf = **last non-header entry in the file** on load; `buildSessionPath` walks `parentId` back | [V] `session-manager.js:124-145,732-741,1161-1199` |

### 1.3 Counts on this machine

| harness | sessions | forks | sessions with >1 branch | deepest | notes |
|---|---|---|---|---|---|
| Claude Code | 288 main `.jsonl` (mukade says 400: history prompts included) | 0 (`forkedFrom` rows 0; uuids shared across files 0) | 2 (prompt branch points); 5 with >1 user/assistant leaf | 2 branches, 1 branch point | `eaf8a335…` re-submitted `[Image #1]` 6 s later (26 rows off path, 1 conversational); `fd9d923d…` task-notification vs prompt (7 rows off path). mukade counts 1 `dropped_off_branch` each (`document` run) [V] |
| Codex | 34 rollouts on disk; 8633 DB rows (8612 rollouts gone) | 13 forked subagents (`forked_from_id`), 0 top-level among 34 files; DB has no fork column; `attached_thread_parent_id` set 0 | 0 on disk (0 `history_base`, 0 `thread_rolled_back`) | — | 12 DB rows whose `rollout_path` is a page (reverted threads, files gone); 3507 spawn edges; history_mode: 8487 paginated / 146 legacy |
| OpenCode | 127 `session_v2` | 0 (`fork_session_id` 0) | n/a (no tree; `revert` set on 0) | — | 51 rows with `parent_id` are subagent children; `mukade harnesses` prints **"forks: 51"** (counts `parent_id OR fork_session_id`, `src/reader/opencode.rs:674`) [V] |
| Pi | 117 (67 at depth 1, 50 nested `session.jsonl`) | 0 (`parentSession` 0) | 0 | 1 | 0 `branch_summary`; all headers v3 |

---

## 2. What mukade does today (HEAD `edab9ce`)

### 2.1 Listing

| surface | Claude | Codex | OpenCode | Pi |
|---|---|---|---|---|
| `list` (CLI JSON rows) | every main file; `parent` never set | rollouts + DB rows; DB `subagent` rows removed (incl. forked subagents) | `WHERE parent_id IS NULL` → **forks listed as roots** | every file, forks included |
| `list_all` (TUI) | = `list` (default, `src/reader/mod.rs:53-55`) | adds subagents, `parent` from `thread_spawn_edges` (`codex_state.rs:185-204`), `ParentKind::Spawn` (`codex.rs:1026-1035`) | all rows; `parent` = `parent_id` only → `Spawn` (`opencode.rs:582,599-602`); query never selects `fork_session_id` | same as `list` |
| fork marked? | no (`claude_code.rs:1001-1002`) | top-level fork: no (`forked_from_id` never read; grep finds no `forked_from` in `src/`) | no in listing; yes in `dump` (export `info.fork` → `Fork`, `opencode.rs:348-363`) | yes: `parent` = **path**, `Fork` (`pi.rs:614-616,700-702`) |
| JSON row fields | `id,title,cwd,timestamp,location,archived,unreadable,live` — **no `parent`/`kind`** (`src/cli.rs:467-476`) | same | same | same |
| `harnesses` fork count | "unknown (not recorded)" (catalog) | "unknown (not recorded)" (catalog) | counts children too (wrong label) | headers naming `parentSession` (`pi.rs:766-806`) |
| TUI | flat list (`list_all`, `mukade-tui/src/main.rs:1937`); `↳` after the title when `parent` is set, fork and spawn alike (`:1698`); detail pane `parent: <id or Pi path>` (`:1770-1771`); no grouping under the origin | | | |

### 2.2 Dump (read)

| | Claude | Codex | OpenCode | Pi |
|---|---|---|---|---|
| default branch | `last-prompt.leafUuid`, superseded by a later continuation or later prompt; else last conversational row; null leaf clears (`order.rs:110-150`). `explicit`/`rewound`/`cutUuid` not read [V] | latest page; earlier files' tails → `Meta(OffBranch)` (`codex.rs:137-168`); `thread_rolled_back` **not applied** | what `session export` prints (OpenCode's own view); staged `revert` not read (grep: no `revert` in `src/`) [U what export prints while staged] | path from last row with an `id` (`pi.rs:181-215`); off-path → `Meta(OffBranch)` (`pi.rs:340-350`) |
| off-branch in output | `Session` keeps them as `Meta(OffBranch)` in file order (`claude_code.rs:578-590`), but the **dump document drops all `Meta`** (`src/wire.rs:119`) | same | none exist | same |
| `--all-branches` | refused exit 2 (`src/hop.rs:965-975`; `harnesses/mod.rs:81-92` default `None`) | `Branches::All`: every file of the thread whole, base first, then the page → **one linear history**: abandoned turns, then the redo, no marker (`codex.rs:315-327`; `harnesses/codex.rs:52-67`) | refused | refused |
| fork history | `/branch` copy read as an ordinary session | copy forks and forked subagents: inherited records read as the session's own (dump of `01a1c4ae…`: message 1 is the parent's prompt; `parent_kind: spawn`) [V]; referenced fork: origin prefix read via `base_prefix` (lookup `rollout_id(file)==base.thread`, `codex.rs:233-262`), origin's later turns become `OffBranch` [V code] | export returns the fork's own copied rows; `parent`+`Fork` | whole copied history; `parent` = source path |

### 2.3 Hop / write / document

| question | answer | evidence |
|---|---|---|
| Destination gets | the active branch only (conversation entries; `Meta` never planned) | `src/writer/plan.rs:270-292` |
| Off-branch counted? | yes in `hop`/`document`: `dropped_off_branch` = number of `Meta(OffBranch)` entries (conversation rows only; off-branch attachments/system rows keep their own Meta kind and are not counted) + loss line "records of an abandoned branch (a rewind or an edited prompt) left behind (n)" | `plan.rs:289-290`; `manifest.rs:203-206`; `claude_code.rs:584-587` |
| `dump` → `write` | off-branch invisible → count 0, no loss line | `wire.rs:119` |
| `--all-branches` hop | Codex only; abandoned + active turns written as one linear conversation; nothing counted as dropped | `codex.rs:324-327` |
| Fork link | **never carried**: `Meta.parent` "Deliberately *not* carried into a hop"; writers write no `parentSession`, `forkedFrom`, `forked_from_id` or `info.fork`/`parentID` | `src/model/mod.rs:49-52`; grep of `src/writer/` |
| Fork history | duplicated: the full inherited history is written as the new session's own | readers above |
| Writers' shape | linear chains: Claude `parentUuid` = previous row (tool_result → its tool_use), ends `custom-title`, `last-prompt{leafUuid:last row}` (`writer/claude_code.rs:95,126,313-318`); Pi header v3 without `parentSession`, `parentId` chain (`writer/pi.rs:114-118,352-358`); Codex legacy rollout, no `history_mode`, no DB row (`writer/codex.rs:10-12`); OpenCode `session import` (`writer/opencode.rs:3-5`) | |
| TUI | `e` export = dump document (`main.rs:840-843`); hop builds `HopRequest`, which has no all-branches field (`src/hop.rs:172-180`) → active branch only | |

---

## 3. Native shapes a writer would need, per destination

### 3.1 (a) Several branches in one session

| destination | can hold a tree? | shape to write | resume target | risks |
|---|---|---|---|---|
| Claude Code | yes | all branches in one file, rows chained by `parentUuid` (abandoned rows point at their real parent; tool_result rows at their own tool_use row); **active branch last in file**; final `last-prompt{leafUuid:<active leaf>}` (optionally `explicit:true`) | `last-prompt.leafUuid`, else newest row [V reader logic] | [U] that 2.1.322 resumes the intended leaf of a mukade-written multi-leaf file (needs a load without a prompt); compaction boundaries per branch; the resume picker's message count |
| Pi | yes | entries with `id`/`parentId` tree; abandoned branches first, **active leaf the last entry**; optional `branch_summary{fromId}` (only if the source had one) | last non-header entry [V `session-manager.js:732-741`] | ids must be unique 8-hex; `label` entries are tree nodes |
| Codex | partly | (i) legacy rollout: write branch A, then `event_msg thread_rolled_back{num_turns:k}`, then branch B (DFS with one rollback per return); works only where the branch point is a **user-turn boundary** (`drop_last_n_user_turns`) [V source]; (ii) paginated: base file + page with `history_base{thread_id,end_ordinal_exclusive,end_byte_offset}` (exact byte offset, `history_mode:"paginated"`) | (i) replay [V]; (ii) `threads.rollout_path` — mukade writes no DB row [U whether Codex picks the page when it rebuilds] | mid-turn branch points (Pi `/tree` onto an assistant/tool entry) cannot be expressed; [U] how TUI/Desktop display rolled-back turns |
| OpenCode | **no** | active branch only; every other record counted (`dropped_off_branch`) | — | a staged `revert` written by import is not a branch |

### 3.2 (b) A fork linked to its origin

| destination | native link | shape | constraints |
|---|---|---|---|
| Claude Code | `forkedFrom:{sessionId,messageUuid}` on each copied row (as `/branch`); title "… (Branch)" optional | copy the inherited rows (whole), `forkedFrom.sessionId` = destination id of the hopped origin, `messageUuid` = the origin row's **destination** uuid (mukade mints fresh uuids, so the origin hop's source-entry → dest-uuid map must be known, e.g. from its manifest) | [U] whether the UI uses it beyond file-history copy (`forkedFromSessionId`) and compaction `namesSource` |
| Codex | `session_meta.forked_from_id` (+ optional `forked_from_ordinal_exclusive`, `attached_thread`) | simplest: copied fork (full history in the rollout) + `forked_from_id` = dest origin id; referenced fork needs a paginated origin and `history_base` into its file | [U] how Codex lists/attaches forks; `forked_from_id` must be a valid ThreadId ("forked_from_id is invalid" string) |
| OpenCode | `session_v2.fork_session_id`/`fork_boundary` | **not writable through `session import`** (import reads `info.parentID` only, which makes a *child*: hidden from `session list`, deleted with its parent) [V]. HTTP `POST /api/session/:id/fork` forks without a model call, but the fork's own later messages cannot then be imported (import refuses an existing id) | self-contained copy; at most a soft link in `info.metadata` (`--metadata-file`; import passes `metadata` through) |
| Pi | header `parentSession` = **absolute path of the origin's file** | write the copied entries (whole or active path) under a v3 header with `parentSession` = the hopped origin's new file | Pi's `/resume` selector nests forks under the origin by canonical path (`session-selector.js:164-183`) [V]; a path that does not exist → listed as a root |

---

## 4. Gaps and risks

### 4.1 Listing forks under their origin

| gap | detail |
|---|---|
| Claude | read `forkedFrom.sessionId` from the first conversational row (one peek per file); `--fork-session` rows [U] |
| Codex | read `forked_from_id` from line 0 of each rollout (listing already peeks `cwd`, `codex.rs` `peek_cwd`); DB has no fork column, so DB-only rows (8612 here) cannot be classed. Forked subagent = Spawn **and** Fork: `ParentKind` can say only one |
| OpenCode | `session_rows` must select `fork_session_id`/`fork_boundary` (`opencode.rs:582`); fix the `harnesses` fork count (`:674`) |
| Pi | map `parentSession` path → id through the listing's own `path` (canonicalize, case-insensitive on Windows); origin outside the store → unresolved |
| model | `parent` holds an id for 3 harnesses and a path for Pi; a `parent_path` or resolution step is needed. CLI rows carry no `parent`/`kind` yet (subagent phase 1 adds them with `--children`) |
| TUI | flat; `↳` for both kinds; nesting is deferred by the subagent spec ("TUI phase") — forks and children should share one tree design |
| docs | catalog lineage (Claude, Codex), `docs/harnesses/claude-code.md:17,172,280,382`, codex.md:176-180, subagent spec :55 all say no fork lineage |

### 4.2 Exporting all branches for every harness

| gap | detail |
|---|---|
| model has no tree | `Entry` = `{id, kind, raw}` (`src/model/mod.rs:110-117`): no parent pointer, no branch id. Off-branch records are `Meta` with only `raw`; a tree must be rebuilt from raw `parentUuid`/`parentId` |
| dump drops Meta | `wire.rs:119`: the document form needs a branch representation (e.g. `branches:[{from:<entry id>, messages:[…]}]` or a per-message `parent`) |
| Claude/Pi off-branch rows are not parsed | they are `Meta(OffBranch)` raw rows: steps not merged by `message.id`, results not paired; an all-branches read must run the full conversation parser per branch |
| Codex `All` is linear | concatenation is not a tree; a redo page's branch point is `end_ordinal_exclusive` — enough to express it as a branch |
| Codex rollback | `thread_rolled_back` must be applied first (today a correctness bug), then the dropped turns are a branch |
| OpenCode | nothing to export (no branches) — refuse or no-op |

### 4.3 Hopping a session with branches

| gap | detail |
|---|---|
| writers are linear | Claude and Pi writers need a tree mode (parent per row, active leaf last, Claude `last-prompt` = active leaf) |
| plan/strict per branch | the strict gate (each call answered once) and L5 pairing must hold per branch; a call answered on one branch and unanswered on another is legal in the source |
| Codex | only turn-boundary branch points via rollback; others → active + counted loss |
| OpenCode | active + counted loss (as today) |
| counting | `dropped_off_branch` counts conversation rows only; with "keep all" it must become per-branch counts (kept vs dropped) |
| Claude 2.1.322 leaf flags | `explicit`, `rewound`, `cutUuid` not modelled; a rewound checkpoint may pick a different leaf than mukade [U] |

### 4.4 Hopping a fork

| gap | detail |
|---|---|
| origin identity | linking needs "was the origin hopped, and to which destination id/path/uuids": a hop registry or the origin's manifest (source id → dest id, entry → dest uuid for Claude `messageUuid`) |
| shared prefix | forks copy (Claude, OpenCode, Pi, Codex legacy) or reference (Codex paginated). The fork boundary: Claude `forkedFrom.messageUuid` per row; Codex `forked_from_ordinal_exclusive` / `history_base.end_ordinal_exclusive` / `subagent_history_start_ordinal`; OpenCode `fork_boundary{type,messageID}` (message ids differ in the copy: `<event>_<seq>`, `seq` equal); Pi: same entry ids as the origin |
| Codex referenced fork today | wrong `dropped_off_branch` (origin's later turns), and `--all-branches` splices them in; the fork needs its own path in `read_rollout` (stop at `history_base`, no off-branch tail) |
| Codex forked subagents | hopped today with the parent's history duplicated as the child's (13 here); subagent phase 2 (`hop --children`) will duplicate it again unless `subagent_history_start_ordinal` is honored |
| destinations | link natively: Pi (path), Claude (`forkedFrom`), Codex (`forked_from_id`); OpenCode: copy only (+ metadata) |
| Pi path | the destination path of the origin must be known before writing the fork's header; Pi CLI `--fork` copies the whole tree, `/fork` only one path: the source shape tells which |
