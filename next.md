# Next: open points

**Status on 2026-11-11.** Read top to bottom: what is done, what needs your Windows machine, what
needs a key, what needs a decision. Unverified facts are marked **[U]** here and in the docs.

**The step-by-step for every open check:** [docs/verification/windows-checklist.md](docs/verification/windows-checklist.md).

---

## ✅ Done on 2026-11-11 (Linux, [record](docs/verification/linux-2026-11-11.md))

| what | before | now |
|---|---|---|
| Claude Code `live` on Linux | `[U]`, and in fact **broken** (`null` for every live session) | fixed, verified on 2.1.322: `true` / `false` / `null` all correct |
| Timeout off Windows | only the child was stopped; its subprocesses kept running | the whole process tree is stopped; Ctrl-C still reaches the harness |
| Test suite on Linux | 6 tests failed (test-harness bugs) | **519 / 519 pass** |
| TUI export / import | missing | `e` exports, `i` imports into any installation |
| Codex code mode (L10) | every Desktop `exec` call travelled as text | literal scripts read as their calls, each with its own result: 725 of 981 real scripts ([census](docs/verification/2026-11-11-codex-code-mode.md)) |
| Codex `apply_patch` | an edit with no path, always text | one added file → write, one hunk → edit, several hunks → multi-edit (Pi) |
| Codex subagent parent | `meta.parent` always empty | read from `session_meta.session_id` |
| `updated` for Codex and Pi | always empty | Pi: newest record time; Codex: database, else file time |
| `--keep-turns` with `--compaction summary` | the summary was paid for before the turn check | the check comes first |

---

## 🟢 Needs your Windows machine (free)

Checklist step in brackets.

1. **Setup + whole suite on Windows** [0]: `cargo test --workspace --no-fail-fast` → 0 failed.
2. **TUI export / import by hand** [1].
3. **Windows timeout still kills the tree** [2]: unchanged code, but no automated test covers it.
4. **Codex code mode on a real Desktop thread** [3]: the read-only census over 34 real rollouts passed
   (every split call has one result); a live hop of a real Desktop thread into a destination is not done yet.
5. **Codex subagent parent on a real thread** [4].
6. **Codex chain of pages** [5] [U]: one page in 982 rollouts, its base a plain file; never a chain.
7. **Remove the old Claude Code test sessions** [9]: folder `…-scratchpad-live-claude-work`.
8. **Listing cache on a cold disk** [12] [U]: warm 65–72 ms measured; cold 11 s not reproduced.
9. **100 MB session speed** [13]: 1, 10, 30 MB were re-measured, 100 MB not.
10. **Real-life shapes** [14] [U]: auto-compaction by threshold, a second compaction, rewind or
    `/tree`, an image going to Claude Code. Hop one whenever it happens.

## 🔵 Needs code (next)

- **Destination shell (L11/L13).** A hop with no `--dest-shell` knows no destination dialect, so
  every shell call is text form, and Claude's `PowerShell` tool is not in its offered set even
  where it is enabled. On the real Desktop rollouts: of 696 split `exec_command` calls, 0 translate
  toward Codex without the flag and 696 with `--dest-shell powershell`; toward Claude none do.
  *To do:* take the destination's shell from its own configuration (Codex `config.toml`, Claude
  settings and platform, Pi settings), and add `PowerShell` to Claude's offered set where enabled.

## 🟡 Needs a few model calls

11. **Codex summary with its real model** [6] [U]: only a mock so far (quota).
12. **A session bigger than the source model's window** [7] [U]: must end `summary failed`, never half a summary.
13. **Claude Code signed thinking really validated** [8] [U]: needs a source whose last assistant
    turn is a tool call with thinking; then the tampered copy must be refused.

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

- **Codex subagent threads are not carried** with their root thread in a hop: each is its own
  session. The parent is now known; carrying children needs a destination format for them.
- **Codex `apply_patch`** with several files, a delete or a move travels as text, and the patch
  form toward Codex is not written.
- **Codex code-mode scripts with variables, `Promise.allSettled` or `ALL_TOOLS`** (256 of 981
  real ones) stay one `exec` call in text form: nothing is evaluated.
- **Codex `threads` layout**: the reader asks only for columns it finds (`PRAGMA table_info`). A
  Codex that renames them lists rollouts as before, without the database rows; nothing warns.
- **Claude Code prompts without a `sessionId`** (old Claude Code) name a project, not a session: skipped.
- **OpenCode legacy rows** become readable only after OpenCode itself migrates that database;
  mukade never starts OpenCode to do it.
- **Native-compaction budgets**: one conservative budget for all destinations, based on the
  verified OpenCode ceiling. The other harnesses' own trigger values are not all verified [U].
- **Small review points still open** (harmless, low priority):
  - the manifest names the recorded model when OpenCode ran with its default;
  - a summary run's folder name is pid and time: two threads of one library caller could share it,
    and a run longer than an hour can be swept by another mukade;
  - the tests sharing `mukade-data/tmp` assume `cargo test` runs one test binary at a time (not nextest);
  - no test covers the fork leftover `dump` prints;
  - a test that the strict gate writes nothing on refusal would need a hook in production code;
  - the cases in `tests/regressions.rs` could move next to the code they test.
- **Not supported at all yet:** Hermes Agent and DeepSeek Harness ([ADR-0002](docs/adr/ADR-0002-hermes-and-deepseek-harness-read-first.md), Proposed).

---

## History

- 2026-11-04: live Codex → Codex hop with real encrypted reasoning passed, including the
  tampered-copy refusal; it found the message-id shape that made written threads unresumable
  (fixed). Native compaction records accepted by all four real binaries
  (`docs/verification/native-compaction-live.md`).
- 2026-11-08: listing coverage (`docs/verification/listing-coverage-2026-11-08.md`).
- 2026-11-11: Linux verification and the fixes above.
