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
2. **Live Codex → Codex, OpenAI encrypted reasoning.** Blocked on 2026-10-28 by the ChatGPT plan's
   usage limit (until 2026-11-04).
   *To settle:* `python -I docs/verification/live-hops/live_codex.py <empty folder>` after the
   limit resets; the tampered copy must be refused.
3. **Code mode (L10).** Codex Desktop wraps almost every call in one `exec` script
   (`tools.exec_command`, `tools.apply_patch`); mukade carries it as text, not as the call it
   wraps.
   *To settle:* parse the one-call `exec` script into its call (spec §8.1, tables §5).
4. **Subagent threads and forks are not linked.** A subagent's rollout reads as its own session
   (fixed id), but its parent thread is not recorded (`meta.parent` empty), and subagent threads
   are not carried with their root thread.
5. **Leftovers in the Codex store.** Two empty threads from the failed live calls:
   `01a18404-20d1-7891-acb4-4361bf8e2e5c`, `01a18407-acd4-73e1-8cf3-0a325a158a4f` (working directory
   in the scratchpad). Archive them in the app; removing the files alone breaks Codex's index.

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

## Across harnesses

1. **Native compaction records [U].** A portable hop writes the summary as a user message, never
   the destination's own record (Claude `compact_boundary`, Codex `compacted`, Pi `compaction`;
   spec §4.4).
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
     turns; `write` ignores the `--summary-*` flags without a word; `--summary-timeout` with an
     enormous value panics; the source's own compaction lines are hidden when a kept tail holds it;
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
