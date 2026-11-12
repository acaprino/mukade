# DeepSeek Harness writer, resumed by dsh — 2026-11-12

A Claude Code fixture hopped into a real `dsh` store, then resumed by `dsh` itself. Machine: Windows 11,
`dsh` 0.2.2-rc.1 (npm global), model `cohere/north-mini-code:free` through OpenRouter's free route
(`docs/superpowers/specs/2026-11-12-dsh-openrouter.patch.yml`). One session was written to the real
`~/.dsh`, under a scratch project folder.

## What was run

```text
DSH_HOME=~/.dsh mukade hop claude_code c45db7bc-542c-4201-89de-b590c6fdb797 deepseek_harness \
  --from-file fixtures/claude_code/source/c45db7bc-542c-4201-89de-b590c6fdb797.jsonl --cwd <scratch>/live-cwd

cd <scratch>/live-cwd
dsh --profile headless --patch docs/superpowers/specs/2026-11-12-dsh-openrouter.patch.yml \
  --session-id session-b77ac875-d62d-454f-a556-1890e6ed1654 "Reply with OK"
```

| step | result |
|---|---|
| hop | exit 0; `messages_in` 10, `messages_out` 10, `tool_calls` 2, `tools_text_form` 0; strict gate checked, no violations; resume `dsh --profile headless --session-id session-b77ac875-d62d-454f-a556-1890e6ed1654` |
| file | `sessions/--D-Projects-...-scratchpad-dsh-live-cwd--/session-b77ac875-.../session.v4.jsonl.zstd`, 2 frames (header alone, then 38 events), content checksum on |
| Node's libzstd (`zlib.zstdDecompressSync`) on each frame | decodes both, checksums verified: 351 bytes, 7640 bytes |
| `dsh` resume | exit 0; printed `H27-T4 on compaction → H27-T3. OK.` (an answer to the converted history: the fixture's own prompts) |

## What dsh did with the file

The log after the resume, from event 37 (our last event was 36):

| seq | event | meaning |
|---|---|---|
| 37 | `session/end-seed {}` | dsh's marker when it opens a log |
| 38-41 | `model/selection`, `permission/preset`, `sandbox/mode`, `approval/policy` | the profile's settings |
| 43-45 | `turn/start` 6, `step/start` | a new turn after our five |
| 46 | `system/message`, `surfaceOp: {op: replace, startSeq: 2, endSeq: 2}` | **dsh replaced our empty node 0 with its own system prompt** |
| 47-49 | `user/message` (the prompt), runtime context, skill catalog | |
| 50 | `request/header` | |
| 52 | `assistant/message` | the answer |
| 54 | `turn/end` completed | |

So the file was found by its id and project folder, read without a format or checksum error, its
surface rebuilt (five turns of history, 2 tool calls with results), and extended. The foreign
assistant messages (no `replayState`) went to the model as another provider's history.

`mukade list deepseek_harness` and `mukade dump deepseek_harness b77ac875` read the resumed log back
(title `H27-T3 after compaction`, models `claude-opus-5-5` and the dsh model).

## What the writer relies on, and where it was established

| fact | evidence |
|---|---|
| zstd with the checksum flag, frame 1 the header alone | dsh refuses otherwise (`assertZstdHeaderFrame`); the frames above decode in libzstd |
| `ruzstd` has no checksum without a hashing crate | frames are compressed by `ruzstd` (`Fastest`), then the descriptor's checksum bit is set and the XXH64 low 32 bits appended by `reader::zstd_frames::encode_frame` |
| folder = `projectKey(cwd)` / `encodeSegment(id)`; a mismatch is "corrupt session log" | `dsh-session-persistence-jsonl` `index.js:3485`; ported, tested against its own examples |
| a root is opened in one encoding only | `ensureRootEncoding`; the writer follows the root (plain when it holds only plain logs) |
| header cwd must be absolute | `isHeaderLine`; a relative cwd is refused by the writer |
| node 0 may be an empty system message | dsh replaced it on the resume, above |
| a `tool/call` event per call | `openTurnClosers` treats a call with no `tool/call` as never started |
| no `replayState` | `toPiAssistant`: `replayState === undefined` is a foreign assistant |

Not exercised live: a resume that makes a tool call, `compression: none` roots, a hop from a dsh
session (covered by the writer's tests).
