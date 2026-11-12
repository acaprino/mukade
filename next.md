# Next: open points

**Status on 2026-11-12.** What is done, then what is open, by what it needs (Windows, model calls,
code, a key, macOS), then the known limits. Unverified facts are marked **[U]** here and in the docs.

**The step-by-step for every open check:** [docs/verification/windows-checklist.md](docs/verification/windows-checklist.md).
Its step numbers are in brackets below.

---

## ✅ Done on 2026-11-12

| what | before | now |
|---|---|---|
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
5. **Subagent parent on a real thread** [4].
6. **Chain of pages** [5] [U]: one page in 982 rollouts, its base a plain file; never a chain.

**Cleanup, speed, real life**

7. **Remove the old Claude Code test sessions** [9]: folder `…-scratchpad-live-claude-work`.
8. **Listing cache on a cold disk** [12] [U]: warm 65–72 ms measured; cold 11 s not reproduced.
9. **100 MB session speed** [13]: 1, 10, 30 MB were re-measured, 100 MB not.
10. **Real-life shapes** [14] [U]: auto-compaction by threshold, a second compaction, rewind or
    `/tree`, an image going to Claude Code. Hop one whenever it happens.

## 🟡 Needs a few model calls

11. **Codex summary with its real model** [6] [U]: only a mock so far (quota).
12. **A session bigger than the source model's window** [7] [U]: must end `summary failed`, never half a summary.
13. **Claude Code signed thinking really validated** [8] [U]: needs a source whose last assistant
    turn is a tool call with thinking; then the tampered copy must be refused.

## 🔵 Needs code (next)

**Shell dialects, left open** ([spec](docs/superpowers/specs/2026-11-12-destination-shell-design.md)).

- *Pi's `powershell` tool* as a destination tool: off unless `defaultTools` names it; today a
  PowerShell call goes to `bash`, and only toward a `shellPath` of PowerShell.
- *Codex reader:* `<shell>cmd</shell>` is read as `Bash`, and a call's own `shell` given as a path
  is ignored (`src/reader/codex.rs` `shell_declared()`).
- *Codex Desktop with its agent in WSL* [U]: which shell its rollouts declare.

## 🔴 Needs an API key

14. **OpenCode's own `anthropic` provider** [10] [U]: mukade writes `native.signature` /
    `native.encrypted`; only `openrouter` is verified live.
15. **OpenAI encrypted reasoning through OpenRouter** [11] [U]: written as `reasoning.encrypted`,
    never exercised.

## ⚪ Needs macOS (no machine yet)

16. `live` uses `ps -p` and does **not** compare `procStart` there [U]: a reused pid reads as live.
17. The process-tree kill through `ps -A` [U] (Linux uses `/proc`; same logic).

---

## 🧭 Known limits, by decision or waiting on upstream

Not bugs. Each is stated in the docs where it applies.

**Codex**

- **Subagent threads are not carried** with their root thread in a hop: each is its own session.
  The parent is now known; carrying children needs a destination format for them.
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
- **Not supported at all yet:** Hermes Agent and DeepSeek Harness
  ([ADR-0002](docs/adr/ADR-0002-hermes-and-deepseek-harness-read-first.md), Proposed).

**Small review points still open** (harmless, low priority)

- The manifest names the recorded model when OpenCode ran with its default.
- A summary run's folder name is pid and time: two threads of one library caller could share it,
  and a run longer than an hour can be swept by another mukade.
- The tests sharing `mukade-data/tmp` assume `cargo test` runs one test binary at a time (not nextest).
- No test covers the fork leftover `dump` prints.
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
