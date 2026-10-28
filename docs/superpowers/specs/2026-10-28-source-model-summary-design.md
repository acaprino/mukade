# A summary written by the source's model, instead of the whole history

Status: built (plan `docs/superpowers/plans/2026-10-28-source-model-summary.md`; live record
`docs/verification/2026-10-28-summary-live.md`, which changed two commands: Codex forks instead of
resuming, Claude's `--tools ""` comes first). Date: 2026-10-28.

## Why

A hop today carries either the whole history (`Compaction::None`) or the summary the source
already wrote for itself (`Compaction::Portable` with `SummarySource::SourceRecord`). The second
needs a session the source has compacted, and Codex 0.165 keeps that summary encrypted, so it
often has nothing to carry. The user wants a third way: when a session moves, the **source
harness's own model** writes a compact summary of it, and the destination starts from that
summary instead of the whole history. The same summary must be exportable to a file and importable
later, not only usable inside one `hop`.

Half of it exists: `SummarySource::Supplied(String)` and `KeepFrom::LastTurns(n)` (`src/hop.rs`)
already write "handoff message + tail". What is missing is the part that asks the source's model
for the text, and the command-line surface.

## What the user gets

```text
mukade hop  <src> <id> <dst> --compaction summary [--keep-turns N] [--summary-model M] [--summary-timeout S]
mukade dump <src> <id>       --compaction summary [--keep-turns N] [...] --output doc.json   # export
mukade write <dst> doc.json                                                               # import, unchanged
```

- `--compaction summary`: mukade runs the source harness on the session, without saving
  anything, asks its model for a summary, and the destination gets one handoff message with that
  summary.
- `--keep-turns N` (default **0**): also keep the last `N` user turns verbatim after the summary
  (`KeepFrom::LastTurns(N)`). It is valid only with `summary`.
- `--summary-model M`: the model the summarizing run uses. Without it, mukade uses the model the
  session last used when the reader knows it, and otherwise the harness's configured default.
- `--summary-timeout S` (default 300 s): the deadline of the summarizing run (`exe::run_within`).
- `dump --compaction summary` writes the compacted document, so the summary can be exported, kept,
  read or edited, and later imported with the existing `write`. `wire` already carries a
  `Kind::Compaction` entry (`src/wire.rs:102`, `:304`), so `write` needs no change.

Library: a summarizing step that runs **before** `select`, so `select` stays a pure function of
the session it is given. A `HopRequest` asks for it through a new `SummarySource` variant (for
example `SourceModel { model, timeout }`). `hop()` resolves it into the text plus where the text
came from, then calls `select` with it. The function that runs the harness is public on its own
(for example `summarize(&SessionRef, &Session, &StoreRoots, &SummarizeOpts) -> Result<Summary, HopError>`),
like `export` and `import`. The exact names are the builder's call.

## Goals, non-goals, anti-goals

Goals
- The summary is written by the source harness's model, through the source harness's own binary,
  credentials and configuration. mukade holds no API key and calls no provider itself.
- One prompt, owned by mukade and versioned, the same for all four harnesses.
- Export/import: the compacted document is an ordinary interchange document.
- The manifest says the history travelled as a model-written summary, by which harness and model,
  and how many entries were left out.

Non-goals (v1)
- The harness's native `/compact`. It writes into the source's store, and Codex's result is
  encrypted.
- A `summarize` command that prints only the text, a `--summary-file` that supplies one, and
  user-supplied prompt instructions. They are easy later; v1 does not need them.
- Writing the summary as a destination-native compaction record (spec §4.4 stays open).
- Summarizing a `--from-file` source other than Pi, or an `--all-branches` selection. Both are
  refused with a reason (exit 2).

Anti-goals (each is a failure even if the hop technically works)
- The source session changes: a row appended, a file rewritten, a turn added.
- The summarizing run edits files or runs commands in the project.
- The model continues the task instead of summarizing, and that reply is carried as the summary.
- A summary that is empty or not a summary is written silently.

## How the summary is produced, per harness

Each run starts from the session's working directory, with the source store's variable set to the
root the session was read from, stdin closed and a deadline. The flags below were read from the
installed binaries' `--help` on 2026-10-28. **[U]** marks behaviour not yet run.

| source | command | how nothing is saved | output |
|---|---|---|---|
| Claude Code 2.1.312 | `claude -p --resume <id> --no-session-persistence --output-format json [--model M] "<prompt>"` | `--no-session-persistence` (print mode only). **[U]** that it also stops appending to the resumed file | `result` field of the JSON; its `modelUsage`/cost go to the manifest when present |
| Codex 0.164.0 | `codex exec resume <id> --ephemeral -o <tmp file> [-m M] "<prompt>"` | `--ephemeral` ("without persisting session files") | the file written by `--output-last-message`, in `mukade-data/tmp` |
| Pi 1.4.0 | `pi -p --fork <session file> --session-dir <mukade-data/tmp/…> [--model M] "<prompt>"` | the fork goes to a temporary session directory, removed afterwards (`--fork` cannot be combined with `--no-session`) | stdout |
| OpenCode 2.1.4 | `opencode run --standalone --session <id> --fork --format json [-m M] "<prompt>"`, then `opencode session delete <fork id> --standalone` | none of its own: the fork is created in the user's store and deleted afterwards. **[U]** what the fork leaves behind (lineage columns, project rows) | the assistant text from the JSON events |

OpenCode is the only source whose store is written, even briefly. If the delete fails, the hop
still goes on, and the receipt and manifest name the fork and the command that removes it. If a
live run shows that a fork leaves traces, the fallback is a temporary `OPENCODE_DB`: mukade
imports the session there with its own writer, runs the summary, then deletes that database.

**No tools.** The summarizing run must not run tools that change anything. Per-harness candidates:
Claude `--tools ""`, Pi `--no-tools`; Codex `exec` has no tool switch, so it relies on its
sandbox and approval defaults through `-c` overrides; OpenCode is run without `--auto`. **[U]**
whether a provider accepts a history with tool calls once tool definitions are removed (the
Anthropic API may refuse it). If it does not, the fallback keeps the tools defined and denies
their execution (Claude `--permission-mode dontAsk`, the equivalent elsewhere). Settling this per
harness is the first live check of the plan.

## The prompt (v1)

One English prompt, with a version stamp (`summary prompt v1`) recorded in the manifest. It asks
for:

1. the goal of the session and what the user asked for, in their words when it matters;
2. the current state: what is done, what is half done;
3. decisions taken and why, and approaches tried and dropped;
4. files read or changed, and how; commands and tests run, with their results;
5. open problems, unanswered questions, and the next step.

It also tells the model to reply **only** with the summary, in the language of the conversation,
without calling tools or continuing the task, and to give no length beyond "as short as it can
be while a new agent could continue the work from it alone".

Checks on the reply: it is empty, or the run ended in a tool call or an error → `SummaryFailed`,
nothing is written.

## Shape in the destination

The same as `Portable` with `Supplied` (spec §4.2): one `Kind::Compaction` entry, then the tail
from the `N`-th last user turn (none when `N = 0`). Its handoff line is new, so that the
destination's model knows where the text came from:

`[mukade handoff v1] Summary of the earlier conversation, written by the source agent's model when the session was moved:`

(`PORTABLE_HANDOFF`, `src/hop.rs:21`, stays for the source's own compaction.)

## Manifest, receipt and errors

- `options.compaction = "summary"`, and a `summary` object with `source: "source_model"`, the
  harness and its version, the model, `prompt: "v1"`, the summary's length, and the tokens or cost
  when the harness reports them.
- One loss line: `the history before <entry id> (<k> entries) travelled as a summary written by
  <harness>'s model`, plus the OpenCode leftover line when the delete failed.
- A new `HopError::SummaryFailed(reason)` → exit **4** (failed, nothing changed). Its reason names
  the command, the exit status and stderr, like `ResumeFailed`.

## Tests

- Unit and CLI tests with a fake harness executable (the existing `--<harness>-exe` override) that
  prints a canned summary. They check the arguments mukade passes per harness, the document shape
  for `--keep-turns 0` and `2`, the `dump` → `write` round trip, the manifest fields, and the
  failure paths (empty reply, non-zero exit, timeout). No real harness is reached.
- Live checks, recorded in `docs/verification/` like the Pi runs, one per source harness:
  (a) the source's files (or OpenCode's session rows) have the same SHA-256 before and after;
  (b) the no-tools mechanism is accepted with a history that holds tool calls;
  (c) the summary reaches the destination and the destination resumes from it.

## Calls made in this design (to check while reading)

1. A mukade prompt instead of the native `/compact`: the source stays untouched, and Codex's
   summary would be encrypted anyway.
2. `--keep-turns` defaults to 0 (summary only), because the request was "a summary instead of the
   whole history".
3. The summarizing run uses the session's own last model by default, not the harness default.
4. OpenCode forks and deletes; a temporary database is the fallback, not the first choice.
5. v1 leaves out `summarize` / `--summary-file` / custom instructions.
6. Summary failures exit 4, the same as other failures where nothing was written.

## Open items for the plan

- [U] Claude `--no-session-persistence` together with `--resume`: is the source file untouched?
- [U] Tool definitions removed vs. history holding tool calls, per provider.
- [U] Codex `exec resume --ephemeral` on a session whose last turn was interrupted.
- [U] OpenCode fork traces after `session delete`.
- [U] A session larger than the source model's window: does the harness compact on its own in
  memory, or fail? Either way, the failure must be a `SummaryFailed`, never a partial summary.
