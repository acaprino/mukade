# Next: open points

What is still open after 2026-10-28, with how each one gets settled. Unverified facts are marked
**[U]** in the docs as well.

## Codex

1. **Chain of pages (paginated threads, 0.165) [U].** A thread redone more than once is read by
   following each `history_base` back to the first file, the latest version winning. Tested on
   hand-made files only: of the 982 rollouts on this machine one is a page, and its base is a plain
   file.
   *To settle:* in the Codex app, open a test chat, send two messages, edit the second, then edit
   it again. Read the rollout files (read only) and compare with what the app shows; then
   `mukade dump codex <thread id>` must give the same turns.
2. **Code mode (L10).** Codex Desktop wraps almost every call in one `exec` script
   (`tools.exec_command`, `tools.apply_patch`); mukade carries it as text, not as the call it
   wraps.
   *To settle:* parse the one-call `exec` script into its call (spec §8.1, tables §5).
3. **Subagent threads and forks are not linked.** A subagent's rollout reads as its own session
   (fixed id), but its parent thread is not recorded (`meta.parent` empty), and subagent threads
   are not carried with their root thread.

Settled 2026-11-04: the live Codex → Codex hop with real encrypted reasoning passed, including the
tampered-copy refusal, and uncovered (and fixed) the message-id shape that made written threads
unresumable (`docs/verification/codex-encrypted-reasoning-live.md`); the two leftover threads of
2026-10-28 were removed with `codex delete --force`, no index residue.

## Claude Code

1. **Signed and redacted thinking accepted by the provider [U].** mukade's copies resume and the
   payloads are byte-identical, but a plain `claude -p --resume` does not have the provider check
   earlier turns' thinking: the tampered copy resumed too.
   *To settle:* a source whose last assistant turn is a tool call with thinking, which the
   provider does validate on the next request; then the tampered copy must be refused. A few
   Haiku calls on the Claude plan.
2. **Leftovers in the Claude store.** The live-test sessions, all under the project folder
   `…-scratchpad-live-claude-work`, none in real projects. Remove that folder when no longer
   needed.

## OpenCode

1. **OpenCode's own Anthropic provider [U].** Toward `openrouter` the reasoning shape is verified
   live (`native.reasoningDetails`); toward OpenCode's own `anthropic` provider mukade writes
   `native.signature` / `native.encrypted`, not verified live.
   *To settle:* the same live test with an Anthropic API key and `--model-provider anthropic`.
2. **OpenAI encrypted reasoning through OpenRouter [U]:** written as `reasoning.encrypted` with
   `format: openai-responses-v1`, never exercised.

## Listing (added 2026-11-08, `docs/verification/listing-coverage-2026-11-08.md`)

1. **`live` off Windows [U].** Claude Code's `sessions/<pid>.json` is checked with `/proc/<pid>`
   (or `ps -p` where there is no `/proc`) and `pidDomain` against the platform and `HOSTNAME` or
   `/etc/hostname`; `procStart` is not compared there (its format on Linux and macOS is not known),
   so a reused process id reads as live (the safe side for a cleaner).
   *To settle:* run `mukade list claude_code` on Linux and macOS with a session open, and read
   `sessions/<pid>.json` there.
2. **The listing cache on a cold disk [U].** Warm-disk numbers are measured (285 ms the first time,
   65–72 ms after, 303 rows); the 11 s cold-disk first run is not reproduced with an empty mukade cache.
3. **Codex `threads` layout.** The reader asks only for columns it finds in `threads`
   (`PRAGMA table_info`), and takes the newest `state_<n>.sqlite`; a Codex that renames them lists
   rollouts as before and no thread from the database. Nothing watches for that.
4. **Claude Code prompts without a `sessionId`** (an older Claude Code) name a project, not a
   session, and are skipped: a project known only through them is not listed.
5. **OpenCode legacy rows.** OpenCode migrates a database it opens (37 of 62 rows moved in the
   copy it was tried on), so a legacy row may become readable once the user runs that database;
   `list` then shows it as a plain `session_v2` row. mukade never starts OpenCode to do this.

## Across harnesses

1. **Native compaction records: built and live-verified for all four destinations.**
   `--compaction native` keeps the whole generated history and appends the destination's own
   deterministic record: OpenCode `compaction` (`metadata.keepID`), Pi `compaction`
   (`firstKeptEntryId`), Codex `compacted` (`replacement_history`), and Claude Code
   `compact_boundary` + `isCompactSummary` (`preservedSegment`/`preservedMessages`). On 2026-11-04 an
   oversized synthetic session was written and resumed once by each real binary: Claude Code 2.1.318,
   Codex 0.166.0, OpenCode 2.2.5 and Pi 1.4.0 all answered from the summary and added no second native
   compaction (`docs/verification/native-compaction-live.md`). Still open:
   - *Destination-specific budget thresholds [U].* The common budget is conservative and uses the
     verified OpenCode ceiling; Claude Code, Codex and Pi's own trigger values are not all verified.
2. **Summary written by the source's model: built** (`--compaction summary`, spec §4.5, live
   record `docs/verification/source-model-summary-live.md`). Still open:
   - *Codex's real model [U].* Checked against a mock only (quota). *To settle:* after the reset,
     `mukade hop codex <thread id> pi --compaction summary --resume-prompt "What were we doing"` on
     a throwaway thread: the rollout's SHA-256 unchanged, the summary a summary (not a work log:
     Codex's reply is not checked for tool calls), the destination answering from it.
   - *A session larger than the source model's window [U].* It must end in `summary failed`, never
     half a summary.
   - *Smaller points from the review:* the manifest names the recorded model when OpenCode ran with
     its default; the summary is paid for before `--keep-turns` is checked against the source's
     turns; `write` ignores the `--summary-*` flags without a word; the source's own
     compaction lines are hidden when a kept tail holds it;
     a summary run's folder name is pid and time (two threads of one library caller could share it),
     and a run longer than an hour can have it swept by another mukade; the tests sharing
     `mukade-data/tmp` rely on `cargo test` running binaries one at a time (not nextest); no test
     covers the fork leftover `dump` prints.
3. **Not exercised on real sessions:** auto-compaction by threshold, a second compaction, rewind
   or tree navigation in the source, images and attachments toward Claude Code (spec §10).
4. **Speed of very large sessions.** The 100 MB case was not re-measured after the speed work
   (1, 10 and 30 MB were).
5. **Review leftovers:** a test that the strict gate writes nothing on refusal needs a hook in the
   production code; the cases in `tests/regressions.rs` could move to the files of the parts they
   test.
6. **A timeout stops only the process outside Windows [U].** `exe::run_within` ends the whole
   process tree on Windows (`taskkill /T`); elsewhere `kill_tree` is a plain `child.kill()`
   (`src/exe.rs`), so subprocesses a harness started can outlive a timeout. The docs and the
   comment on `run_within` now say so; the behaviour is unchanged. A process group would fix it,
   but a child in its own group no longer gets the terminal's Ctrl-C, so interrupting mukade
   would leave the harness (and a paid model call) running, and catching the signal needs a new
   dependency (or unsafe FFI) where the list in `Cargo.toml` is closed on purpose. Neither the
   leak nor that side effect has been run: it needs a Unix host (WSL or CI), which the Windows
   development host lacks.
