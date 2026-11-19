---
adr_id: ADR-0002
version: 1
title: Hermes and DeepSeek Harness enter read-first; Hermes stays source-only
status: Accepted
author: Alfio Caprino
scope:
  paths:
    - src/model/harness.rs
    - catalog/harnesses.json
    - src/reader/**
    - src/writer/**
    - src/hop.rs
    - src/cli.rs
    - src/dest.rs
  modules:
    - "mukade::model"
    - "mukade::reader"
    - "mukade::writer"
    - "mukade::hop"
  domains:
    - harness-support
governs_symbols:
depends_on:
created_from_task: brainstorm-hermes-deepseek-harness
proposed_on: 2026-10-31
proposed_at_sha: d2fb033fd90bdc654efe2b64081be64161fdeb19
accepted_on: 2026-11-12
accepted_at_sha: 879368a78a440c417fb047dbe17913bd1605a4ad
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
superseded_by: null
changes:
  - 2026-11-12: accepted with changes (see Amendment): Hermes is a destination through its own import; schema gate 32 and later; the DeepSeek Harness writer exists and was verified live.
---

# ADR-0002: Hermes and DeepSeek Harness enter read-first; Hermes stays source-only

<!-- adr-header:start -->
Version 1 · Proposed 2026-10-31 by Alfio Caprino · Accepted with changes 2026-11-12 (Amendment)
<!-- adr-header:end -->

## Amendment 2026-11-12: accepted with changes

Both harnesses were implemented, installed and exercised on a real machine (Hermes v0.23.1,
`dsh` 0.2.2-rc.1). What that showed replaces parts of the Decision below; the text of the original
decision stays as the record of what was proposed.

| original | now | why |
|---|---|---|
| Decision 1: Hermes is source-only | **Hermes is a destination, through its own `hermes sessions import --from claude`**; mukade still never opens `state.db` for writing (INV-001 holds) | the import is the supported way in that RA-001 and RA-002 lacked: the gateway stores the rows. It keeps only user and assistant text, so tools travel as text ([verification](../verification/2026-11-12-hermes-writer-live.md)) |
| Decision 2: the DeepSeek writer comes later, after a live resume | **The writer exists and the live resume is recorded** (INV-004 satisfied): `dsh` opened, resumed and extended a mukade-written log ([verification](../verification/2026-11-12-dsh-writer-live.md)) | the format facts were established from source and a real store |
| Decision 4 and INV-003: Hermes schema 31 only | **Schema 32 and later, gated by the columns the reader needs** (32 at v0.22.5, 33 at v0.23.1); DeepSeek format 4 only, as proposed | Hermes never refuses a newer schema and adds columns without a bump; 31 refused every real store |
| Decision 3 and INV-002: a source-only harness is refused as a destination | **Kept as the mechanism** (`Harness::destination()` is `None`), but no harness is source-only now: all six are destinations | |
| Consequences: a decoder for DeepSeek | `ruzstd` (pure Rust) decodes; its encoder writes the frames and mukade sets the checksum | `ruzstd` has no checksum without a hashing crate |
| EX-002, EX-004 (refused destinations) | no longer apply to Hermes and DeepSeek; the refusal is tested with a stand-in source-only adapter (`src/harnesses/mod.rs`, `a_destination_without_a_writer_is_refused_before_planning`) | |
| EX-001, EX-003, EX-005 | hold, with schema 32 in place of 31 in EX-001 and EX-003 | |
| RA-001, RA-002, RA-005 | stay rejected: no direct `state.db` write, no backup-zip import, no bypass flag | the import is none of these |

Also decided on 2026-11-12: `--compaction summary` is refused from both (no run that leaves the
session untouched: `hermes --resume` and `dsh --session-id` append to it); the TUI lists and hops
both, resumes Hermes with `hermes --resume`, and says dsh has no terminal UI. Design:
[2026-11-12-hermes-deepseek-design.md](../superpowers/specs/2026-11-12-hermes-deepseek-design.md).

## Context and Problem Statement

mukade reads and writes exactly four harnesses. `HarnessId` has four variants and
`HarnessId::ALL: [HarnessId; 4]` (`src/model/harness.rs:8`, `:17`), the unknown-harness error lists
the four names (`src/model/harness.rs:44`), and `catalog/harnesses.json` describes the same four.
Every harness in that list is both a hop source and a hop destination: the library has no notion of
a harness that can be read but not written.

The user asked to study two more harnesses and integrate their support: Hermes Agent (Nous
Research) and DeepSeek Harness (`deepseek-ai/deepseek-harness`, launcher `dsh`). The study found:

- **Hermes** (v0.22.5) keeps sessions in a SQLite `state.db` (schema version 31, WAL). Its gateway
  is documented as the only writer of canonical sessions; out-of-process writers go through that
  owner. There is no command to import a session (upstream issue #8650). `hermes import <zip>`
  restores a whole backup and overwrites files in the Hermes home. Reading is safe: `state.db`
  opens read-only, and `rusqlite` with `bundled` is already a dependency.
- **DeepSeek Harness** (session format v4, product tag `dsh-v0.2.0-rc.2`, pre-release) keeps one
  event-sourced log per session at `<root>/--<cwd>--/<session-id>/session.v4.jsonl[.zstd]`, as
  checksummed Zstandard frames by default. The root is configured per profile and has no default.
  Reading needs a Zstandard decoder, which mukade does not have; writing also needs synthetic
  turn/step events that must satisfy the published v4 schema.

The question this record answers is how these two harnesses enter mukade without breaking the
rules that hold everywhere: the destination is always new, no loss is silent, nothing is written
that a harness could refuse.

## Decision Drivers

- The user chose the scope: Hermes "Solo lettura prima"; DeepSeek Harness "Lettura prima,
  scrittura dopo".
- The user named the DeepSeek harness by repository: `https://github.com/deepseek-ai/deepseek-harness`.
- Hermes documents that writing `state.db` from outside its gateway is not supported; a mukade
  write would be a session Hermes itself never produced.
- Both stores are young and move fast (Hermes schema 31, DeepSeek Harness 17 schema changes since
  2026-09-11 under a "no compatibility before release" stance); reading an unknown version
  guesses at meaning.
- mukade's existing contract is that a write is held against what the destination would accept
  and refused otherwise (`README.md`, "check" step).

## Decision

Hermes and DeepSeek Harness become mukade harnesses, each with a reader first.

1. **Hermes is source-only.** mukade reads Hermes sessions and never writes one. It stays
   source-only until upstream offers a supported way to add a session that does not write
   `state.db` behind the owning gateway.
2. **DeepSeek Harness is read-first.** Its reader ships first. Its writer ships in a later phase,
   enabled only after a session written by mukade has been resumed by a live `dsh` and that run is
   recorded under `docs/verification/`.
3. **Whether a harness can be a destination is part of the harness's definition in the library.**
   A hop or write toward a source-only harness is refused with exit code 2 and the reason, before
   any plan or write, and nothing is written.
4. **Each new reader accepts only the store format version it was verified against** (Hermes
   `state.db` schema 31, DeepSeek Harness session format 4) and refuses any other, naming the
   version it found.

## Verified Interview Examples

- **EX-001** (nominal case): Input: `mukade hop hermes <session-id> claude_code`, with a Hermes
  `state.db` at schema 31 -> Expected: a new Claude Code session is written and the manifest lists
  every loss, as for the four existing sources.
- **EX-002** (boundary case): Input: `mukade hop claude_code <session-id> hermes` -> Expected: exit
  code 2, a reason that says Hermes is read-only in mukade, nothing written, and no `state.db`
  opened for writing.
- **EX-003** (rejection or error case): Input: `mukade list hermes` against a `state.db` whose
  `schema_version` is not 31 -> Expected: refused with a reason that names the version found; no
  session is listed from it.
- **EX-004** (boundary case): Input: `mukade hop claude_code <session-id> deepseek_harness` before
  the DeepSeek Harness writer is enabled -> Expected: the same refusal as EX-002.
- **EX-005** (rejection or error case): Input: `mukade dump deepseek_harness <session-id>` on a log
  whose header says `"version":5` -> Expected: refused with a reason that names version 5; the log
  is not interpreted.

## Rejected Alternatives

- **RA-001**: write Hermes sessions straight into `state.db` now; rejected because Hermes names its
  gateway the only canonical writer, a partial runtime ledger could replay started effects, and an
  upstream issue links concurrent writers to database corruption.
- **RA-002**: land a Hermes session by generating a backup zip and running `hermes import`;
  rejected because the import overwrites files in the Hermes home, which is destructive for a
  user's installation.
- **RA-003**: ship the DeepSeek Harness reader and writer together; rejected by the user, who
  wants the writer after a live-verified session.
- **RA-004**: take Deep Code (`lessweb/deepcode-cli`) as the DeepSeek harness; rejected because the
  user named `deepseek-ai/deepseek-harness`. Deep Code remains a possible separate harness later.
- **RA-005**: treat Hermes as read-write behind a flag that bypasses the refusal; rejected because
  the refusal is the rule that keeps mukade from producing sessions a harness never wrote.

## Enforceable Invariants

- **INV-001**: no mukade code path opens a Hermes `state.db` for writing.
- **INV-002**: a harness is a hop or write destination only if its definition in the library says
  it can be written; the check runs before any plan or write and a source-only destination is
  refused with exit code 2.
- **INV-003**: each reader reads only the store format version it was verified against and refuses
  every other, naming the version it found.
- **INV-004**: the DeepSeek Harness writer is not enabled until a mukade-written session has been
  resumed by a live `dsh` and that run is recorded under `docs/verification/`.

## Consequences

- `HarnessId` grows to six variants and `ALL`, `as_str`, `FromStr` and the error list change with
  it; `catalog/harnesses.json` gets two entries; detection, readers, tool translation and policy
  gain a module per harness.
- The library gains a harness capability (can be a source, can be a destination); the CLI and the
  TUI use it to refuse or hide destinations.
- mukade gains a Zstandard decoder dependency for DeepSeek Harness. It must stay pure Rust so the
  single-executable build is kept.
- "Four harnesses" in `README.md`, the specification and the usage guide becomes six, with
  Hermes marked read-only.
- Live fixtures need both harnesses installed and a model provider key; until then readers are
  tested against recorded sessions with synthetic content.

## Confirmation

- A CLI test per example EX-001 to EX-005 over recorded fixtures (EX-002 and EX-004 assert that
  nothing was written).
- A test that a Hermes fixture opened by mukade is byte-identical afterwards (INV-001).
- A unit test over the harness definitions that every destination refusal happens before planning
  (INV-002).
- Review of every wave touching `src/reader/` checks INV-003; review of the DeepSeek Harness
  writer checks INV-004.
