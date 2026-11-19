# Pi session and reasoning verification

Two separate runs on 2026-10-28 provide different evidence: session import through a mock
provider on Pi 1.2.0 and 1.4.0, and signed-reasoning replay through a real provider on Pi 1.4.0.
Their methods, results and limitations are kept separate below.

## Session import with a mock provider

| | |
|---|---|
| **Date** | 2026-10-28 |
| **Pi versions** | 1.2.0 (the globally installed one, the version mukade was built against) and 1.4.0 (npm `latest` that day, installed into a scratch folder with `--ignore-scripts`) |
| **mukade** | `master` at the commit that adds `context_edit` (`cbebc84`) |
| **Result** | 18 of 18 scenarios pass on both versions |

### Why

mukade writes Pi sessions and reads them back with its own reader, so a mistaken reading of Pi's
format passes every unit test. This run asks the real Pi instead: it resumes what mukade wrote, and
the request Pi then sends to the model is compared with what the source session said.

### Method

- **No real model.** Pi's home (`PI_CODING_AGENT_DIR`), `HOME` and `USERPROFILE` point into a scratch
  folder; `PI_OFFLINE=1`, `PI_SKIP_VERSION_CHECK=1`, `PI_TELEMETRY=0`; no provider API keys in the
  environment. Pi's only provider is a `models.json` entry of type `anthropic-messages` whose
  `baseUrl` is a mock server on `127.0.0.1`.
- **The mock checks the provider rules** of spec §7 on every request and answers HTTP 400 on a
  violation, so a session a real provider would refuse fails visibly: a first message that is not a
  user's, a `tool_use` not answered in the next message (A1), a `tool_result` that answers no
  `tool_use` before it (A2), a call answered twice (A3), a `thinking` block without a signature (A4),
  a tool id outside `^[a-zA-Z0-9_-]+$`. It logs every request body, and otherwise streams a short
  reply, `MOCK-REPLY`.
- **mukade resumes through the real `pi.cmd`** (`--resume-prompt`), from the session's directory,
  with the store variable set by mukade itself.
- **Ground truth.** For the Pi-format scenarios a session is built by hand, Pi is asked to resume a
  copy of it (Pi appends its own reply to the file it resumes), and the conversation Pi sends is
  compared with the one Pi sends for mukade's rewrite of the original.

### Scenarios

| # | scenario | what must hold |
|---|---|---|
| 1 | Claude Code fixture → Pi, resumed by Pi | accepted; the model receives the whole conversation; no rule violated |
| 2 | Codex fixture → Pi | the same |
| 3 | OpenCode export → Pi | the same |
| 4 | Pi fixture → Pi | the same |
| 5 | Claude Code → Pi with `--compaction portable` | the handoff line and the source's summary reach the model |
| 6 | project folder of a path with a space, `_`, `.` and `é` | `pi -c` run in that directory finds the session |
| 7 | a step with native and text-form calls | no `No result provided` insertion by Pi; every result arrives |
| 8 | an aborted step | Pi leaves it out of the next request |
| 9 | a failed call and an unanswered call | the request is valid; the failed result arrives |
| 10 | a duplicate result | only the first answer arrives |
| 11 | an image in a user turn and in a tool result | both reach the model as Anthropic image blocks (`image/png`, base64) |
| 12–18 | Pi-format ground truth: abandoned branch, `branch_summary`, `bashExecution` (text, `!!` exclusion, exit code), `custom_message`, `context_edit` that replaces, `context_edit` that removes, aborted turn | the conversation Pi sends for mukade's rewrite equals the one it sends for the original |

### Findings

- **`context_edit` was not applied (fixed).** Pi shows the model the edited content of a target, or
  nothing when the replacement is null. mukade's reader kept the entry as an internal record, so the
  destination received the unedited original. Both cases differed from what Pi sends on 1.2.0 and
  1.4.0 before the fix, and match after it (`src/reader/pi.rs`, `apply_context_edits`).
- **Everything else matched** once the harness itself was right (its first runs failed on its own
  mistakes: a working directory that did not exist, a model entry without image input, and a
  comparison made against a file Pi had already appended to). That includes the project-folder
  rule, the result order, aborted steps and image shapes that had so far been derived from Pi's
  source.
- **No difference between 1.2.0 and 1.4.0** in any scenario: the session format (header version 3,
  `id`/`parentId` entries) was accepted unchanged by 1.4.0.

### Notes for anyone driving Pi from a script

- In print mode Pi reads piped standard input: a process started with stdin left open waits forever.
  mukade closes stdin; a harness must too.
- On Windows an npm launcher is `pi.cmd`, with the real process (`node`) behind it: a timeout that
  stops only the child leaves Pi running and holding the output pipes open. mukade stops the whole
  process tree (`exe::run_within`).
- Pi appends the resume prompt and its reply to the session file it resumes.

### Not covered by the mock-provider run

- A real provider: refusals are those of the mock's rules, which are the rules of spec §7, not a
  recorded API behaviour.
- Reasoning blocks with a real signature: the mock does not sign anything, so replay of a signed
  `thinking` block is checked here only for the presence of a signature. A real model's signed
  block was checked later the same day in the [signed-reasoning run](#signed-reasoning-with-a-real-provider).
- Pi versions between 1.2.0 and 1.4.0, and 1.4.0's own new features.
- The scripts that drove this run are in [`pi-live/`](pi-live/).

## Signed reasoning with a real provider

**Question.** When a session holds reasoning a real model signed, does mukade carry the signature
into the destination intact, so that the destination's provider accepts it on resume? The earlier
[mock-provider run](#session-import-with-a-mock-provider) could not sign a real block.

**Answer.** Yes, for Pi → Pi with the source's protocol named. The signature arrives byte for byte,
Pi sends it back, and the provider accepts it; one character changed and the same provider refuses
the resume. With the default the block is dropped and counted; with `--reasoning as_text` it
becomes labelled text. Every resume answered correctly.

### Real-provider setup

| | |
|---|---|
| Pi | 1.4.0 (npm, global install), run with a scratch `PI_CODING_AGENT_DIR`, `HOME` and `USERPROFILE` |
| model | `anthropic/claude-haiku-5.5` through OpenRouter, `--thinking low`, no tools |
| credentials | `OPENROUTER_API_KEY` from the environment only; no credential file read or copied |
| mukade | `target/release/mukade.exe`, built from `c9aace6` (the code is unchanged since; later commits are documentation) |
| calls | 5 real model calls: the source session, three resumes, the negative control |

Pi records an OpenRouter call to an Anthropic model with `"api": "anthropic-messages"` and the
block's signature in `thinkingSignature` (1017 characters here, 58 characters of thinking text).
mukade reads it as an opaque payload of protocol `anthropic-messages` with that signature.

### Real-provider results

The source answered `17 × 23` with `391`. Each hop wrote a new Pi session from it; each was then
resumed once with *"What number did you answer before? Reply with only that number."*

| hop | thinking block written | signature | manifest | resume |
|---|---|---|---|---|
| default (`drop_foreign`, no `--dest-protocol`) | none | — | `reasoning_dropped: 1`, loss line `reasoning dropped: foreign provider or no opaque payload (1)` | exit 0, `391` |
| `--dest-protocol anthropic-messages` | 1 | **identical** to the source's | no loss, strict clean | exit 0, `391` |
| `--reasoning as_text` | none; the text, labelled `[earlier reasoning, not replayable]`, before the answer | — | no loss line, no count (see findings) | exit 0, `391` |

**Negative control.** A copy of the native hop's session with one character changed in the middle
of that signature, resumed the same way: exit 1, the provider refused it with

```text
400 invalid_request_error: messages.1.content.0: Invalid `signature` in `thinking` block
```

So Pi does send the carried block back, and the provider checks its signature: the native hop's
successful resume is evidence that the signature was carried intact, not that it was ignored.

### Real-provider findings

1. **Toward Pi the default drops every reasoning block**, because Pi speaks whatever provider it is
   configured with and mukade cannot know which (`src/policy.rs`, `destination_protocol`). Naming
   it with `--dest-protocol` keeps blocks of that protocol, as this run shows. The usage guide says
   so (section 4, "Tool calls and reasoning").
2. **The default's loss line named the wrong reason here.** It said "foreign provider or no opaque
   payload"; the block had its payload and the destination could hold it — what was missing is the
   destination's protocol. *Fixed the same day:* every dropped block now has a loss line with its
   own reason, here "the destination's provider protocol is not known; name it with
   --dest-protocol" (`ReasoningDrop` in `src/policy.rs`, test
   `reasoning_not_carried_natively_is_counted_with_its_reason`).
3. **`--reasoning as_text` left no trace in the manifest.** The block's signature is gone and its
   text is no longer reasoning to the provider, but `counts` had no number for it and `losses` no
   line; only `options.reasoning: "as_text"` said it happened. That was short of "which of the
   three happened is in the manifest" (README) and of "no loss is silent" (spec §1.3). *Fixed the
   same day:* a `reasoning_as_text` count and the line "reasoning written as labelled text, no
   longer replayable by a provider (n)".

### Not covered by the real-provider run

- Other destinations live: Claude Code (needs Anthropic credentials), Codex (needs an encrypted
  payload, which an Anthropic model does not produce), OpenCode.
- Encrypted reasoning (`redacted_thinking`, OpenAI `encrypted_content`).
- A source from another harness: the source here is a Pi session, so the reader is Pi's.
- Pi 1.2.0: this run used 1.4.0 only.

### How to run the real-provider check again

```sh
cargo build --release
python -I docs/verification/pi-reasoning-live/live_reason.py <empty scratch dir>          # 4 calls
python -I docs/verification/pi-reasoning-live/live_reason_control.py <same scratch dir>   # 1 call
```

`OPENROUTER_API_KEY` must be set; each script prints and saves a JSON report in the scratch dir.
The scripts in that folder are the ones that produced the results above, with their paths made
relative to the repository and the environment moved into `live_env.py`; they were not rerun after
that change, since every run costs real calls.
