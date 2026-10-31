---
adr_id: ADR-0001
version: 1
title: Installation scanning is a library capability the CLI exposes first
status: Proposed
author: Alfio Caprino
scope:
  paths:
    - src/detect.rs
    - src/cli.rs
    - mukade-tui/src/**
  modules:
    - "mukade::detect"
    - "mukade::cli"
    - mukade-tui
  domains:
    - installation-discovery
governs_symbols:
depends_on:
created_from_task: brainstorm-installation-scan
proposed_on: 2026-10-31
proposed_at_sha: e1146a4dca385f7e6a9fe1c3f8eea348c47b91f7
accepted_on: null
accepted_at_sha: null
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
superseded_by: null
changes:
---

# ADR-0001: Installation scanning is a library capability the CLI exposes first

<!-- adr-header:start -->
Version 1 · Proposed 2026-10-31 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

`mukade detect <dir>` (`src/detect.rs:21`, `detect_store`) recognizes a harness store only when
`<dir>` is the store root itself: on `C:\Users\alfio` it finds nothing although `.claude` is a
child, and on a portable JanuraCode folder it finds nothing because the OpenCode database sits in
`data\share\opencode\opencode.db`. The terminal browser `mukade-tui` resolves its stores only from
the harnesses' environment variables or the catalog defaults (`mukade-tui/src/main.rs`,
`resolve_roots`) and cannot open another folder.

The user wants mukade to recognize every harness installation inside a directory, portable and
custom ones included (the motivating case: hop a Claude Code session into a portable JanuraCode),
by scanning the directory and matching the on-disk structures of the harnesses. The question this
record answers is where that capability lives.

## Decision Drivers

- The user stated it: "prima di tutto è una capability da aggiungere alla cli, solo dopo di
  riflesso alla tui".
- The TUI reads and writes only through the library and never parses a harness store itself
  (`mukade-tui/README.md:7-8`); scanning must not become the first exception.
- One recognition rule set, so the CLI and the TUI cannot disagree on what a directory holds.

## Decision

Recognizing harness installations inside a directory is a function of the `mukade` library,
built beside `detect_store` in `src/detect.rs`, and is exposed first as a `mukade` CLI command.
`mukade-tui` gains the capability only afterwards, by calling that same library function; it
contains no store-layout or installation matching of its own.

## Verified Interview Examples

- **EX-001** (nominal case): Input: the scan of a directory that contains a portable JanuraCode
  folder, run from the `mukade` CLI -> Expected: the result lists that folder as an OpenCode
  installation, without the TUI being involved.
- **EX-002** (boundary case): Input: the same directory opened in `mukade-tui` -> Expected: the
  TUI shows exactly the installations the CLI reported for it.
- **EX-003** (rejection or error case): Input: a change that adds layout matching (a check for
  `projects/`, `sessions/`, `opencode.db`, `models.json` or an executable path) inside
  `mukade-tui/src/` -> Expected: refused in review as a breach of INV-001.

## Rejected Alternatives

- **RA-001**: implement the scan inside `mukade-tui` first and move it to the library later;
  rejected because the user asked for a CLI capability first, and because two copies of the
  recognition rules would drift.
- **RA-002**: a TUI-only "open folder" that sets one store root per harness without scanning;
  rejected because it does not recognize installations, which is the requested capability.

## Enforceable Invariants

- **INV-001**: `mukade-tui` contains no harness-layout or installation recognition; every
  installation it shows comes from the library scan function.
- **INV-002**: for the same directory and the same file system state, the CLI command and the TUI
  report the same set of installations.

## Consequences

- `src/detect.rs` grows a recursive scan; `src/cli.rs` gets the command that prints its result.
- The TUI change is a later, separate step that depends on the library function.
- `docs/usage.md` documents the command before the TUI documents its use.

## Confirmation

- A CLI test over a fixture tree with nested and portable installations covers EX-001.
- A TUI test feeds the same fixture to the TUI loader and compares it with the library result
  (EX-002, INV-002).
- Review of every wave touching `mukade-tui/src/` checks INV-001.
