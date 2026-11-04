# Live acceptance of hand-written native compaction records (2026-11-04)

**Question.** Do the real destination binaries accept the native compaction records Mukade writes for
§4.4, resume the session, project the deterministic summary instead of the whole oversized history, and
avoid starting a second native compaction?

**Answer.** Yes, for all four destinations tested:

| destination | version | store used | history kept | native record | resume answer | extra compaction | cleanup |
|---|---:|---|---:|---|---|---:|---|
| Claude Code | 2.1.318 | the user's real configuration; sessions filed under a scratch project folder | 4,000 rows | `compact_boundary` + `isCompactSummary` | `ALPHA-7391` | no | session file removed |
| Codex CLI | 0.166.0 | the user's real `CODEX_HOME` | 10,002 rollout rows | `compacted` with 2 `replacement_history` items | `ALPHA-7391` | no | `codex delete --force` exit 0 |
| OpenCode | 2.2.5 | scratch database and XDG folders | 4,000 messages | `compaction` with backwards `metadata.keepID` | `ALPHA-7391` | no | `session delete` exit 0 |
| Pi | 1.4.0 | scratch home | 4,001 rows | `compaction` with `firstKeptEntryId` | `ALPHA-7391` | no | session file removed |

## Method

The script is [`native-compaction-live/live_native_compaction.py`](native-compaction-live/live_native_compaction.py).
It creates one synthetic Pi source locally, with no model call:

- 2,000 turns, 3,000 filler characters per early user message;
- source file: 7,517,432 bytes;
- first user message contains `ALPHA-7391`;
- final assistant reply is `DONE`.

For each destination it runs:

```sh
mukade hop pi <source-id> <destination> \
  --compaction native --context-window 40000 --max-output-tokens 32000 \
  --reasoning drop --cwd <scratch>/work ...
```

That budget is zero, so the retained tail is the newest generated item and the deterministic summary
carries the earlier user requests. The whole generated history remains in the destination store. Each
real binary then resumes the session once with:

> You are resuming a compacted session. What is the earliest secret code in the earlier conversation
> summary? Reply with only that code and nothing else.

The source is far larger than any destination model's window (over 2 million tokens by Mukade's
chars/3 estimate), so a destination that ignored the boundary could not have answered from the whole
history without failing or compacting again. After each resume the script re-inspects the store and
requires exactly one native compaction record.

## Findings

1. **Codex rejects user-supplied input item IDs in `replacement_history`.** The first live Codex resume
   failed with `400 … This request does not support user-supplied input item IDs.` Mukade now strips the
   top-level `id` from the items it places in `replacement_history`; the rollout rows keep their IDs.
2. **Codex replays `replacement_history`, not `compacted.message`.** After the ID fix the resume
   succeeded but answered that no secret code was available. The deterministic summary is now the first
   user `input_text` item in `replacement_history`; `compacted.message` remains for Mukade's reader and
   for human inspection.
3. **Claude Code, OpenCode and Pi accepted the hand-written records without changes.**
4. The Codex resume flag `--sandbox` is not accepted by `codex exec resume` in 0.166.0; the script uses
   `-c sandbox_mode="read-only"` instead. This was a script issue, not a Mukade record issue.

## Model calls

Five real model completions returned: Claude Code, OpenCode, Pi, one Codex run before finding 2, and the
final Codex run. One additional Codex request failed with HTTP 400 before an answer (finding 1). A
pre-resume dry run wrote and inspected all four destinations without any model call.

## Left behind

- Claude Code: the throwaway session file was removed; its scratch project folder was removed when empty.
- Codex: the throwaway thread was deleted with `codex delete --force`, exit 0.
- OpenCode: the throwaway session was deleted with `session delete`, exit 0; the database is scratch.
- Pi: the throwaway session file was removed; its home is scratch.
- The scratch directory still holds the synthetic source and `report-native-compaction.json`; it is under
  `target/tmp` and can be deleted.

## How to run it again

```sh
cargo build --release
python -I docs/verification/native-compaction-live/live_native_compaction.py <scratch> --no-resume
python -I docs/verification/native-compaction-live/live_native_compaction.py <scratch>
```

`OPENROUTER_API_KEY` is required for OpenCode and Pi. Claude Code must run with its default
configuration (`CLAUDE_CONFIG_DIR` unset for the resume); Codex uses the real `CODEX_HOME` login. No
credential is printed or copied.

## Limitations

- One synthetic session and one resume per destination.
- The source has no tools, attachments, reasoning or branches.
- It proves acceptance, summary projection and absence of a second native compaction for these versions;
  it does not verify each harness's own automatic-compaction threshold.
- Claude Code and Codex used the user's real logins; OpenCode and Pi used OpenRouter through scratch
  homes.
