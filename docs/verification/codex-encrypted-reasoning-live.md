# Live Codex → Codex hop with OpenAI encrypted reasoning (2026-11-04)

**Question.** Does Codex 0.166 take back a session mukade wrote, with real OpenAI encrypted
reasoning carried byte for byte — and does the provider check that payload when the copy is
resumed? This was the last hop of the [2026-10-28 live run](live-hops-and-provider-reasoning.md),
then blocked by the ChatGPT plan's usage limit.

**Answer.** Yes, after one writer fix. The clean copy resumes and answers from the carried
reasoning; the copy with one character of the encrypted payload changed is refused by the
provider (`400 invalid_encrypted_content`). The negative control discriminates: OpenAI checks
encrypted reasoning on resume, like Anthropic checks signatures through Pi.

## Setup

| | |
|---|---|
| mukade | release build of `d586801` plus the message-id fix described below |
| Codex | `codex-cli 0.166.0`, the user's real ChatGPT login and real `CODEX_HOME` (tokens refresh in place; the store is not copied) |
| model | `gpt-6.1-sol`, reasoning effort `xhigh` (see finding 1) |
| script | [`live-hops/live_codex.py`](live-hops/live_codex.py) with `MUKADE_CODEX_MODEL=gpt-6.1-sol`, `MUKADE_CODEX_EFFORT=xhigh` |
| source | one real `codex exec` call in a scratch working directory: *"What is 19 times 21? Think it through, then reply with only the number."* → `399`, one `reasoning` item, `encrypted_content` 1636 chars |

Model calls: 1 source, 1 resume of the clean copy, 1 resume of the tampered copy, plus the probe
and discrimination runs listed under findings (≤6 small calls in total).

## Results

| step | outcome |
|---|---|
| `mukade dump codex <source>` → `mukade write codex` | exit 0; `losses: []`; strict gate checked, no violations; the written `encrypted_content` is byte-identical to the source's (1636 chars) |
| `codex exec resume <copy>` | exit 0, answer `399` — the resumed thread answers from the carried conversation |
| same, one character of the encrypted payload flipped | **refused**: `400 … The encrypted content gAAA...uA== could not be verified. Reason: Encrypted content could not be decrypted or parsed.` (`invalid_encrypted_content`) |

## Findings

1. **Not every model emits encrypted reasoning.** The account's default `gpt-6-astra` produced
   zero reasoning items (`turn_context` effort `none`, `reasoning_output_tokens: 0`) whatever
   `model_reasoning_effort` said — the user's own config had `xhigh`. `gpt-6.1-sol` at effort
   `low` also emitted none; at `xhigh` it did. `gpt-5.1`, `gpt-5.1-codex` and `gpt-5.1-codex-max`
   are "not supported … with a ChatGPT account". The script now takes
   `MUKADE_CODEX_MODEL`/`MUKADE_CODEX_EFFORT` and refuses a source thread without an encrypted
   reasoning item, so it cannot silently test nothing.
2. **A rollout id in a foreign shape makes the thread unresumable.** The first full run failed on
   *both* copies with `400 … This request does not support user-supplied input item IDs.`
   (`param: input`, `unsupported_parameter`) — the same refusal the native-compaction run hit for
   `replacement_history`, now on the plain write path. The source thread itself resumed fine, so
   the difference was in mukade's copy: its `message` items carried synthesized ids of the shape
   `msg_<uuid v4 simple>` (32 hex, no dashes), while Codex's own local ids are
   `msg_<uuid v7, dashed>`. Two in-place probes on fresh copies settled it: with the ids
   **removed** the resume succeeded (`399`), with the ids **rewritten to the dashed uuid v7
   shape** it also succeeded. Codex evidently recognises its own id shape and strips it before
   replaying history, and forwards anything else to the Responses API, which refuses it.
   **Fixed**: `src/writer/codex.rs` now generates every local message id as `msg_<uuid v7>`
   (`local_message_id()`), keeping the rollout faithful to Codex's own files; the regression test
   `message_ids_are_in_codex_own_local_shape` (`tests/writer_codex.rs`) pins the shape. The rows
   keep their ids — unlike the `compacted` record's `replacement_history`, where they are
   stripped (see [native-compaction-live.md](native-compaction-live.md)).

## Cleanup

All ten threads this run made in the real store (source, both failed copies, both probes, the
final clean and tampered copies, and three model probes) were removed with `codex delete --force`
(exit 0 each); no rollout file and no `session_index.jsonl` entry remained. The scratch directory
was deleted.

## How to run it again

```sh
cargo build --release
set MUKADE_CODEX_MODEL=gpt-6.1-sol   # a model that emits encrypted reasoning on this account
set MUKADE_CODEX_EFFORT=xhigh
python -I docs/verification/live-hops/live_codex.py <scratch>
```

`<scratch>` must be empty (the script caches its source call in `<scratch>/source.json`).
