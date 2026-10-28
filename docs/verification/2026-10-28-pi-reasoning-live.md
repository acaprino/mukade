# Live check: signed reasoning through a hop into Pi 1.4.0 (2026-10-28)

**Question.** When a session holds reasoning a real model signed, does mukade carry the signature
into the destination intact, so that the destination's provider accepts it on resume? Until this
run the only evidence was the mock of [2026-10-28-pi-live.md](2026-10-28-pi-live.md), which signs
nothing.

**Answer.** Yes, for Pi → Pi with the source's protocol named. The signature arrives byte for byte,
Pi sends it back, and the provider accepts it; one character changed and the same provider refuses
the resume. With the default the block is dropped and counted; with `--reasoning as_text` it
becomes labelled text. Every resume answered correctly.

## Setup

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

## Results

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

## Findings

1. **Toward Pi the default drops every reasoning block**, because Pi speaks whatever provider it is
   configured with and mukade cannot know which (`src/policy.rs`, `destination_protocol`). Naming
   it with `--dest-protocol` keeps blocks of that protocol, as this run shows. The usage guide says
   so (section 4, "Tool calls and reasoning").
2. **The default's loss line names the wrong reason here.** It says "foreign provider or no opaque
   payload"; the block had its payload and the destination could hold it — what was missing is the
   destination's protocol.
3. **`--reasoning as_text` leaves no trace in the manifest.** The block's signature is gone and its
   text is no longer reasoning to the provider, but `counts` has no number for it and `losses` no
   line; only `options.reasoning: "as_text"` says it happened. That is short of "which of the three
   happened is in the manifest" (README) and of "no loss is silent" (spec §1.3).

## Not covered

- Other destinations live: Claude Code (needs Anthropic credentials), Codex (needs an encrypted
  payload, which an Anthropic model does not produce), OpenCode.
- Encrypted reasoning (`redacted_thinking`, OpenAI `encrypted_content`).
- A source from another harness: the source here is a Pi session, so the reader is Pi's.
- Pi 1.2.0: this run used 1.4.0 only.

## How to run it again

```sh
cargo build --release
python -I docs/verification/pi-reasoning-live/live_reason.py <empty scratch dir>          # 4 calls
python -I docs/verification/pi-reasoning-live/live_reason_control.py <same scratch dir>   # 1 call
```

`OPENROUTER_API_KEY` must be set; each script prints and saves a JSON report in the scratch dir.
The scripts in that folder are the ones that produced the results above, with their paths made
relative to the repository and the environment moved into `live_env.py`; they were not rerun after
that change, since every run costs real calls.
