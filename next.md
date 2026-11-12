# Next: open points

**Status on 2026-11-12.** What is done, then what is open, by what it needs (Windows, model calls,
code, a key, macOS), then the known limits. Unverified facts are marked **[U]** here and in the docs.

**The step-by-step for every open check:** [docs/verification/windows-checklist.md](docs/verification/windows-checklist.md).
Its step numbers are in brackets below.

---

## ✅ Done on 2026-11-12

| what | before | now |
|---|---|---|
| Hermes Agent and DeepSeek Harness ([design](docs/superpowers/specs/2026-11-12-hermes-deepseek-design.md), [ADR-0002](docs/adr/ADR-0002-hermes-and-deepseek-harness-read-first.md) amended) | neither was supported | both read and written: Hermes `state.db` (schema 32+) and, as a destination, through `hermes sessions import` (tools as text) ([live](docs/verification/2026-11-12-hermes-writer-live.md)); dsh zstd logs, resumed by `dsh` 0.2.2-rc.1 ([live](docs/verification/2026-11-12-dsh-writer-live.md)); the TUI lists, hops and resumes Hermes, and says dsh has no terminal UI; `--compaction summary` from either is refused with its reason (a run on either appends to the session) |
| Forks, part A ([design](docs/superpowers/specs/2026-11-12-forks-branches-design.md)) | a Claude Code or Codex fork looked like an unrelated session; Pi's `parent` was a path; OpenCode's fork count counted subagents; a Codex `thread_rolled_back` turn travelled as live history; a referenced fork counted its origin's later turns as its own dropped branch | every reader names a fork's origin (`parent`, `parent_kind` `fork` with `at`), Pi's `parent` is an id, `harnesses` counts forks for all six, a rolled-back turn is `Meta(OffBranch)`, a referenced fork reads its origin's prefix only, the manifest states `source.parent` and `source.parent_kind` |
| Forks and branches, part B ([design](docs/superpowers/specs/2026-11-12-forks-branches-design.md)) | `--all-branches` was Codex only and one linear splice; `dump` dropped every abandoned record | `Session.branches` and `branches` in the dump document; `--all-branches` for Claude Code, Codex and Pi sources; Claude Code and Pi destinations write the tree, Codex writes a branch and rolls it back, OpenCode, Hermes and DeepSeek Harness leave branches out and count them (`counts.branches_kept`, `branches_dropped`) |
| Subagent sessions, phase 1 ([spec](docs/superpowers/specs/2026-11-12-subagents-design.md)) | Claude Code children unreadable and never listed; OpenCode's `parentID` missed; `parent` meant fork for Pi, spawner for Codex and OpenCode, unsaid; what a child reported to its parent read as `Meta` and lost silently | `parent_kind` (fork / spawn with call and agent) on every parent; Claude children `<parent-id>.<agentId>` read and listed (`list --children`, the TUI); reports the parent received are user turns, an encrypted Codex one counted (`dropped_subagent_messages`) |
| Subagent sessions, phase 2 ([plan](docs/superpowers/plans/2026-11-12-subagents-phase2.md)) | a hop left a session's subagents behind; `dump` had no way to carry them | `dump --children` nests each child's document; `hop --children` writes each child as a session of its own after the parent, linked where the destination can (OpenCode `info.parentID`, checked with the real `opencode` 2.2.5 in a scratch store; Claude Code `subagents/` folder), unlinked for Codex and Pi, and lists them in `manifest.children` |
| Chat launch for OpenCode and Pi ([record](docs/verification/2026-11-12-chat-live.md#run-9-the-opencode-and-pi-launch-kits-on-linux-without-the-harnesses)) | `chat launch opencode\|pi` refused (exit 2); a listener stopped by the user held the member's stream, so the next prompt's news was lost to `stream_busy` | OpenCode 2: a server plugin and the MCP entry in `OPENCODE_CONFIG_CONTENT`, a private server (`--standalone`); Pi: one extension (`pi -e`) that registers the MCP entry; both join, get the news with each prompt and go on when listening, a typed prompt (and Escape in Pi) ends the wait; a stopped hook cancels its call (`notifications/cancelled`), waits run in 5 s slices. Run under Node against a real service; no real OpenCode or Pi yet (below) |
| `next.md` code items, 2026-11-12 (afternoon) | the items below | Claude Code `queued_command` prompts typed mid-turn are user turns in place (once when a plain row repeats them); the Codex reader reads `<shell>cmd</shell>` and a call's shell given as a path (`cmd`, `fish`: no dialect, text form); Pi's `powershell` tool where `defaultTools` turns it on; a seeded DeepSeek fork names `at` (dsh's `buildForkSeed`: the event before `session/end-seed`); `list --children` keeps each child's `meta.json` in the listing cache; the summary's run folder is unique per call and spared while its process runs; the manifest names no model when the summarizing run named none; a test covers the fork leftover `dump` prints |
| Destination shell (L11/L13) | without `--dest-shell` every shell call into Codex, OpenCode and Pi was text (0 of 696 real Desktop calls toward Codex); Claude's `PowerShell` never offered; TUI hops passed no shell | the destination's own: Codex by platform, OpenCode's `shell` or `$SHELL`, Pi's `shellPath`; Claude's `PowerShell` where the installation runs it; CLI and TUI; `--dest-shell none` names no dialect, so shell calls into Codex, OpenCode and Pi stay text as before, while Claude's offered set is unchanged ([tables §4.1](docs/tool-conversion-tables.md#41-shell)) |

## ✅ Done on 2026-11-11

Records: [Linux](docs/verification/linux-2026-11-11.md) ·
[Codex code mode](docs/verification/2026-11-11-codex-code-mode.md).

| what | before | now |
|---|---|---|
| Claude Code `live` on Linux | `[U]`, and in fact **broken** (`null` for every live session) | fixed, verified on 2.1.322: `true` / `false` / `null` all correct |
| Timeout off Windows | only the child was stopped; its subprocesses kept running | the whole process tree is stopped; Ctrl-C still reaches the harness |
| Test suite on Linux | 6 tests failed (test-harness bugs) | **519 / 519 pass** |
| TUI export / import | missing | `e` exports, `i` imports into any installation |
| Codex code mode (L10) | every Desktop `exec` call travelled as text | literal scripts read as their calls, each with its own result: 725 of 981 real scripts |
| Codex `apply_patch` | an edit with no path, always text | one added file → write, one hunk → edit, several hunks → multi-edit (Pi) |
| Codex subagent parent | `meta.parent` always empty | read from `session_meta.session_id` |
| `updated` for Codex and Pi | always empty | Pi: newest record time; Codex: database, else file time |
| `--keep-turns` with `--compaction summary` | the summary was paid for before the turn check | the check comes first |

---

## 🟢 Needs your Windows machine (free)

**Setup and TUI**

1. **Setup + whole suite on Windows** [0]: `cargo test --workspace --no-fail-fast` → 0 failed.
2. **TUI export / import by hand** [1].
3. **Windows timeout still kills the tree** [2]: unchanged code, but no automated test covers it.

**Codex**

4. **Code mode on a real Desktop thread** [3]: the read-only census over 34 real rollouts passed
   (every split call has one result); a live hop of a real Desktop thread is not done yet.
5. **Subagent parent on a real thread** [4]: read-only on 2026-11-12, all 25 child rollouts on the
   author's store (19 subagent, 6 guardian) read with `parent` = `parent_thread_id`; a hand check is left.
6. **Chain of pages** [5] [U]: one page in 982 rollouts, its base a plain file; never a chain.

**Cleanup, speed, real life**

7. **Remove the old Claude Code test sessions** [9]: folder `…-scratchpad-live-claude-work`.
8. **Listing cache on a cold disk** [12] [U]: warm 65–72 ms measured; cold 11 s not reproduced.
9. **100 MB session speed** [13]: 1, 10, 30 MB were re-measured, 100 MB not.
10. **Real-life shapes** [14] [U]: auto-compaction by threshold, a second compaction, rewind or
    `/tree`, an image going to Claude Code. Hop one whenever it happens.
11. **Claude Code shows a written subagent** [15] [U]: `hop --children` writes a subagent into the
    parent's `subagents/` folder in the shape Claude Code writes; mukade reads it back, whether Claude
    Code lists or resumes it is not checked.
12. **Claude Code resumes the right leaf of a hopped tree** [18] [U]: `hop --all-branches` writes a session with several leaves; mukade reads it back, Claude Code opening the active path is not checked.

## 🟡 Needs a few model calls

13. **Codex summary with its real model** [6] [U]: only a mock so far (quota).
14. **A session bigger than the source model's window** [7] [U]: must end `summary failed`, never half a summary.
15. **Claude Code signed thinking really validated** [8] [U]: needs a source whose last assistant
    turn is a tool call with thinking; then the tampered copy must be refused.
16. **Hermes resumes a written session with a model** [16] [U]: `hermes --resume <id>` on a session `hop`
    imported, with the free OpenRouter route; the import and the read-back are verified, the answer is not.
17. **dsh resumes a written session and the model calls a tool** [17] [U]: the live resume answered a plain
    prompt; a task that makes it read a file settles the converted tool history.

- **OpenCode and Pi launched into a room** [U]: `mukade chat launch opencode --room r --listen 120`
  and `mukade chat launch pi --room r --listen 120` next to a launched Claude Code, as in run 7 of the
  [chat record](docs/verification/2026-11-12-chat-live.md). Settles what Node could not: OpenCode
  loading the plugin folder with `--standalone` and expanding `{env:…}` in `OPENCODE_CONFIG_CONTENT`,
  the synthetic message coming ahead of the prompt; Pi loading the `.ts` extension and connecting the
  server it registers; both models answering through `chat_send`.

## 🔵 Needs code (next)

**Forks, left open** ([design](docs/superpowers/specs/2026-11-12-forks-branches-design.md)).

- *Branches, written:* whether Claude Code 2.1.322 resumes the intended leaf of a mukade-written file with several leaves, and whether Codex counts the user-role messages mukade writes inside a turn (a tool result written as text) when it rolls back, are **[U]** (a live load without a prompt would settle them); a paginated Codex fork or redo page is read, never written; OpenCode keeps no branches (its `revert` is not read).
- *A fork hopped is a copy:* the manifest names the origin, nothing links the written fork to a written origin. Decided: a hop registry ([design](docs/superpowers/specs/2026-11-12-hop-registry-design.md)), not implemented yet.
- *Claude Code `--fork-session`* [U]: whether its rows carry `forkedFrom`. The leaf flags of 2.1.322 are not read: they sit on `last-prompt` rows (`leafUuid`, `explicit`, `rewound`, `cutUuid`); in the binary, an explicit rewound checkpoint marks `cutUuid` and its ancestors hidden until a later row descends from them (`noteCheckpoint`/`noteRow`), and a `leafUuid: null` with `explicit` clears the leaf. A real rewound transcript is needed to implement it against.
- *Codex:* a thread that survives only as a database row cannot be told as a fork (no fork column); a paginated fork is read, never written.
- *DeepSeek Harness:* a fork made in dsh's web UI (0.2.2-rc.1, `forkSession`) writes `isSeeded: true` and **no `parentSession`**, so its parent is not recorded and it lists as a root; its `at` is set only where a header names the parent. Hermes `/branch` has no `at` (the copy boundary is not recorded).

**Subagents, left open** ([spec](docs/superpowers/specs/2026-11-12-subagents-design.md); phases 1 and 2 are done above):

- *Writing:* `write` imports a document's session alone and says so when it holds `children` (decided: `write --children`, [design](docs/superpowers/specs/2026-11-12-hop-registry-design.md) §3); a
  `write` that carries them would need a manifest of its own. A fork or inherited-context child is
  written with what its own file holds (`inherits_context` in its manifest entry); Codex and Pi
  children are unlinked sessions (neither records a spawner a writer can set). A repeated
  `hop --children` writes the children again under new ids. `--compaction portable|summary` covers
  the session alone.
- *Claude Code, written children:* **[U]** whether Claude Code lists or resumes one ([11]).

- *Claude Code:* a fork child's context (its `fork-context-ref` into the parent) is not read; a
  child whose chain continues in a sibling agent's transcript is refused (7 of 825 on the author's
  store); `forked-skill` files and `workflows/` are not listed.
- *Codex:* the encrypted prompts a parent sends its children stay encrypted; threads whose rollout
  is gone are listed by `--children` as before.
- *TUI:* `R` on a Claude Code child names the parent to resume instead.
- *Claude Code queued prompts* [U]: the 25 on the author's store that are also a plain row are read
  once when the two are adjacent; whether Claude Code ever writes the plain row further away is not
  checked.

**Shell dialects, left open** ([spec](docs/superpowers/specs/2026-11-12-destination-shell-design.md)).

- *Pi with `defaultTools` naming plain tools* (e.g. `["read","powershell","edit","write"]`): Pi then
  drops `bash`, mukade still offers it.
- *Codex Desktop with its agent in WSL* [U]: which shell its rollouts declare.

## 🔴 Needs an API key

18. **OpenCode's own `anthropic` provider** [10] [U]: mukade writes `native.signature` /
    `native.encrypted`; only `openrouter` is verified live.
19. **OpenAI encrypted reasoning through OpenRouter** [11] [U]: written as `reasoning.encrypted`,
    never exercised.

## ⚪ Needs macOS (no machine yet)

17. `live` uses `ps -p` and does **not** compare `procStart` there [U]: a reused pid reads as live.
18. The process-tree kill through `ps -A` [U] (Linux uses `/proc`; same logic).

---

## 🧭 Known limits, by decision or waiting on upstream

Not bugs. Each is stated in the docs where it applies.

**Codex**

- **Subagent threads are not carried** with their root thread in a hop (phase 2 above): each is its
  own session, `list --children` lists it.
- **`apply_patch`** with several files, a delete, a move or an end-of-file anchor travels as text,
  and the patch form toward Codex is not written.
- **Code-mode scripts with variables, `Promise.allSettled` or `ALL_TOOLS`** (256 of 981 real ones)
  stay one `exec` call in text form: nothing is evaluated.
- **`threads` layout:** the reader asks only for columns it finds (`PRAGMA table_info`). A Codex
  that renames them lists rollouts as before, without the database rows; nothing warns.

**Other harnesses**

- **Claude Code prompts without a `sessionId`** (old Claude Code) name a project, not a session: skipped.
- **OpenCode legacy rows** become readable only after OpenCode itself migrates that database;
  mukade never starts OpenCode to do it.
- **Native-compaction budgets:** one conservative budget for all destinations, based on the
  verified OpenCode ceiling. The other harnesses' own trigger values are not all verified [U].
- **Hermes Agent** ([page](docs/harnesses/hermes-agent.md)): a written session has user and assistant text
  only (tools as text, no model, no reasoning); the import takes about a minute; `--compaction summary`
  and `native` are refused. One live import was left untitled by Hermes, the cause is not known.
- **DeepSeek Harness** ([page](docs/harnesses/deepseek-harness.md)): no terminal UI, so the TUI cannot resume
  it; no image is written; `--compaction summary` and `native` are refused; format 4 only.

**Small review points still open** (harmless, low priority)

- The tests sharing `mukade-data/tmp` assume `cargo test` runs one test binary at a time (not nextest).
- A test that the strict gate writes nothing on refusal would need a hook in production code.

---

## History

- 2026-10-30: the `tests/regressions.rs` cases moved into the test files of the parts they test
  (`f81b182`).
- 2026-11-04: live Codex → Codex hop with real encrypted reasoning passed, including the
  tampered-copy refusal; it found the message-id shape that made written threads unresumable
  (fixed). `--compaction native`: native compaction records accepted by all four real binaries
  (`docs/verification/native-compaction-live.md`).
- 2026-11-08: listing coverage (`docs/verification/listing-coverage-2026-11-08.md`).
- 2026-11-10: `list --archived true|false`, the same for every harness; `--interrupted keep|drop`
  (hop, write and document), an interrupted turn read as one.
- 2026-11-11: Linux verification, Codex code mode and the fixes above.
