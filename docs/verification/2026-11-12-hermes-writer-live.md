# Hermes Agent writer, through its own import — 2026-11-12

A Claude Code fixture hopped into the real Hermes store with `mukade hop`, then read back with
mukade's Hermes reader. Machine: Windows 11, Hermes Agent v0.23.1 (`%LOCALAPPDATA%\hermes`, schema
33). No model was called: the write is `hermes sessions import`, which starts the gateway and stores
the rows. The session stays in the store; that is accepted.

## What was run

```text
cargo test -j 4 --test hermes_live -- --ignored --nocapture the_fixture_hops_into_the_real_store
```

The test (`tests/hermes_live.rs`, `#[ignore]`) runs, with `HERMES_HOME=%LOCALAPPDATA%\hermes`:

```text
mukade hop claude_code c45db7bc hermes \
  --from-file fixtures/claude_code/source/c45db7bc-542c-4201-89de-b590c6fdb797.jsonl
```

then reads the receipt's `id` with `reader::hermes::Hermes.read` and compares texts with the fixture.

## Result

| step | result |
|---|---|
| hop | exit 0 in 74 s (the import starts the gateway); id `20261112_124543_56752d`; location `...\hermes\state.db` |
| counts | `messages_in` 10, `messages_out` 10, `tool_calls` 0, `tools_text_form` 2; strict gate checked, no violations |
| resume | `hermes.EXE chat -Q --resume 20261112_124543_56752d -q` |
| losses named | reasoning dropped (provider unknown, 1); two `Read` calls written as text; interrupted last turn; compaction summary as one user message |
| `hermes sessions list` | the session is listed, "just now", with no title (Hermes had not titled it) and no workspace |
| `mukade list hermes` | lists it first; title taken from its first user text; `cwd` null |
| `mukade dump hermes 20261112_124543_56752d` | meta `schema 33`; messages in order from id 41; the assistant turn carries the call as `[tool call · Read · id … · not available in hermes]` followed by its arguments |
| text check | 8 texts of the fixture (user prompts that are not context tags, and every assistant text part) found verbatim in the read-back user and assistant text: 1530 user chars, 517 assistant chars |

## What this establishes

| fact | evidence |
|---|---|
| Hermes accepts the Claude file mukade writes and allocates the id | `Imported Claude Code session as <id>` parsed from stdout |
| user and assistant text survive the import | the 8 texts above |
| tool calls survive as text only | `tools_text_form` 2, the call text in the dump; Hermes's import keeps `[ran tool: X]` and drops results, so mukade writes call and result into the text itself |
| the session reads as a Hermes session | `harness_version` `schema 33`, rows `user`/`assistant` ordered by id |

Not exercised: resuming the imported session with a model (`hermes --resume`; needs a provider), a
session whose cwd is a real folder (the fixture's folder is not on this machine, so none is kept),
and the 1-in-N case where Hermes did not title the session in time (this run had no title; the
earlier import `20261112_094615_fb3768` has `Imported from Claude Code: ...`).
