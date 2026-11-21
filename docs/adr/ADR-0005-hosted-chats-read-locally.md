---
adr_id: ADR-0005
version: 1
title: Chats hosted by ChatGPT are read from the Codex app's local catalogue and listed as unreadable, and their work on disk is shown and handed off without their text
status: Proposed
author: Alfio Caprino
scope:
  paths:
    - src/reader/codex_hosted.rs
    - src/reader/codex.rs
    - src/work.rs
    - src/handoff.rs
    - assets/handoff-prompts/continue.md
    - src/catalog.rs
    - catalog/harnesses.json
    - src/cli.rs
    - apps/desktop/src-tauri/src/actions.rs
    - apps/desktop/src/work.ts
    - apps/desktop/src/handoff.ts
    - apps/desktop/src/detail.ts
  modules:
    - "mukade::reader"
    - "mukade::work"
    - "mukade::handoff"
    - "mukade::catalog"
  domains:
    - hosted-chats
    - work-on-disk
    - session-handoff
governs_symbols:
depends_on:
  - ADR-0002
  - ADR-0004
created_from_task: hosted-chats-phase1
proposed_on: 2026-11-21
proposed_at_sha: dea4c2ae61589b775728af179f8b4af1c80bc4a2
accepted_on: null
accepted_at_sha: null
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
superseded_by: null
changes:
---

# ADR-0005: Chats hosted by ChatGPT are read from the Codex app's local catalogue and listed as unreadable, and their work on disk is shown and handed off without their text

<!-- adr-header:start -->
Version 1 · Proposed 2026-11-21 by Alfio Caprino · not yet Accepted
<!-- adr-header:end -->

Phase 1 of the design [2026-11-21-chatgpt-hosted-chats-design.md](../superpowers/specs/2026-11-21-chatgpt-hosted-chats-design.md)
is built on the branch `feat/hosted-chats`. This record is its decision and its invariants. It stays
Proposed until the owner has run the live check on a released build: `accepted_on` is then set and
the run is recorded under `docs/verification/`. Phase 2, reading ChatGPT's own export, is not built
and is not decided here beyond the line that its text comes only from a file the owner supplies.

## Context and Problem Statement

The Codex desktop app lists chats whose conversation lives on ChatGPT beside the local ones. Mukade
showed none of them: such a chat is not a Codex rollout and has no row in `threads`. The owner had one,
"Capire lo stack di Daodan", that stopped at the context limit, and had to cherry-pick its work by hand
so as not to lose it.

Checked on the owner's machine on 2026-11-21, read-only (Codex desktop 26.1117.41037, codex-cli
0.167.0-alpha.3.2, Windows 11, catalogue schema of 40 migrations):

- The app keeps a catalogue of chats in `<CODEX_HOME>/sqlite/codex.db`, table `local_thread_catalog`
  (137 rows), joined to `local_thread_catalog_hosts`. Every row is hosted by ChatGPT, and 15 of them
  belong to a ChatGPT project.
- A ChatGPT project maps to a local folder through `state_<n>.sqlite`: `projects.metadata` names the
  ChatGPT project id and `project_roots.path` is the folder.
- The text of a hosted chat is not on disk. `codex app-server` does not return it either: `thread/list`
  is empty, `thread/read` answers `thread not loaded` and `thread/items/list` answers `not supported
  yet`. `codex cloud list` shows 0 tasks.
- What is on disk is the work the chat left: repositories one level below the project folder, one of
  them on a branch with no upstream and 133 uncommitted paths.

## Decision Drivers

- The chat must be found, and must not be shown as readable when its text is not here.
- The work it left must be visible and must not be touched.
- The owner must be able to start another harness on that work without the transcript.
- No credentials and no network: the ChatGPT account is not mukade's to use.
- One adapter per harness (ADR-0002 and `tests/architecture.rs`), and Rust that cannot be built on the
  owner's machine, so a change that touches the closed list of harnesses in many places is a risk.

## Decision

1. **Hosted chats are read from the Codex app's local catalogue and listed among Codex's sessions as
   unreadable.** `src/reader/codex_hosted.rs` reads `sqlite/codex.db` and the newest `state_<n>.sqlite`.
   A catalogue row is listed when its host has `host_kind = 'chatgpt'` and its ChatGPT project has a local
   folder (the project's root at position 0). A hosted chat without a folder is not listed in this
   version. The listing's `unreadable` reason is `hosted by ChatGPT: the conversation is not on this
   machine; the work it left is in its folder`, and every read is refused with it. A thread id that Codex
   also has a rollout or a `threads` row for keeps that listing. No `HarnessId` is added: the chats belong
   to the Codex adapter.
2. **The module is best effort and gated by columns.** It checks with `PRAGMA table_info` that the tables
   and columns it uses exist. A database that is missing, locked, damaged or laid out otherwise gives no
   hosted chats and the rest of the Codex listing is what it was. It does not read
   `codex_schema_migrations`.
3. **The work on disk is a read-only view of any session's folder.** `src/work.rs` takes a folder and
   returns its repositories (the folder itself when it is one, the repository that holds it when it is a
   subfolder, else each direct child that is, the first 20) with every
   linked worktree (one that git cannot read keeps its own error and the rest is kept), every local
   branch, its upstream, its commits on no remote and its uncommitted paths.
   It runs the installed `git` with a deadline, never fetches, and sets `GIT_OPTIONAL_LOCKS=0`. What it
   says about a remote is as of the last fetch, and it says so.
4. **The handoff starts a new session without a transcript.** `src/handoff.rs` composes a note from the
   template `handoff-prompts/continue.md` and the work view, saves it under
   `mukade-data/handoffs/<harness>-<id>.md`, and gives the one-line first prompt that names the file.
   The command that starts the destination comes from a new catalog field, `interactive_start`, set for
   Claude Code and Codex CLI only: those are the destinations whose form is in their own help text
   (Claude Code 2.1.330 `claude [prompt]`, codex-cli 0.167 `codex [PROMPT]`). No native session is
   written. The CLI exposes it as `mukade work` and `mukade handoff`, the desktop app as "Work on disk"
   and "Start in…".
5. **The conversation's text, if it ever comes, comes only from a file the owner supplies** (phase 2:
   ChatGPT's export). It is not fetched, and it is not taken from the app's cache or logs.

## Rejected Alternatives

- **Local catalogue only, with no export** (approach 1 of the design): complete on disk, but the
  conversation never travels. Kept as phase 1, not as the end.
- **Fetch the text with the owner's login** (approach 3): immediate, but it uses private endpoints that
  change without notice, is outside the service's terms, and makes mukade handle credentials.
- **A seventh harness, `chatgpt`, in phase 1**: it touches the closed list in some forty places (every
  loop over `HarnessId::ALL`, the catalog's binary and resume accessors, `tests/harnesses.rs`), none of
  which phase 1 needs, and Rust cannot be compiled on the owner's machine to find them one by one. It is
  decided again in phase 2, with the export's format known.
- **`codex app-server`** (the installed Codex asked for the chat): the clean way, dropped after the live
  probe. It serves no hosted chat: `thread/list` returned no thread, `thread/read` returned `thread not
  loaded`, `thread/items/list` returned `not supported yet`, and `codex cloud list` returned 0 tasks.
- **Listing a hosted chat with no folder**: nothing is on disk and nothing mukade can do with it before
  phase 2, so it is left out until then.

## Enforceable Invariants

- **INV-001**: no mukade code opens a Codex credential file (`auth.json`) or makes a network request for
  a chat.
- **INV-002**: `sqlite/codex.db`, `state_<n>.sqlite` and an export are opened read-only.
- **INV-003**: the work view runs no git command that changes a repository or fetches, and sets
  `GIT_OPTIONAL_LOCKS=0` on every one. It runs git with `core.fsmonitor` and `core.untrackedCache` off, and
  without `GIT_DIR`, `GIT_WORK_TREE`, `GIT_INDEX_FILE`, `GIT_COMMON_DIR` and the other variables that make
  git look somewhere other than the folder it is given. This does not make an untrusted folder safe to
  read: a repository's own filters (`filter.<name>.clean`) still run during `git status`, as they do for
  any `git status` in that folder.

## Examples

- **EX-001** (nominal): `mukade list codex` on the owner's machine -> the chat `6b00c623-2d08-83eb-a3cc-df6dadef8474`,
  "Capire lo stack di Daodan", with `cwd` the project's folder and `unreadable` set to the reason above.
- **EX-002** (nominal): `mukade work codex 6b00c623-2d08-83eb-a3cc-df6dadef8474` -> one line for each of
  `daodan`, `imago` and `stack-analyser` with its branch, upstream and counts, then
  `(remote state as of the last fetch)`. Nothing in the three repositories changed.
- **EX-003** (nominal): `mukade handoff codex 6b00c623-2d08-83eb-a3cc-df6dadef8474 claude_code` -> the
  note, a blank line, `note: <path>` and the command as one JSON array. The note file is under
  `mukade-data/handoffs/`, never in the project folder.
- **EX-004** (error): `mukade dump codex 6b00c623-2d08-83eb-a3cc-df6dadef8474` -> exit 2, `not found:
  Codex thread 6b00c623-2d08-83eb-a3cc-df6dadef8474 is hosted by ChatGPT: …`. Nothing is written.
- **EX-005** (error): `mukade handoff codex <id> pi` -> exit 2, `unsupported: pi cannot be started with a
  first prompt yet (verified: claude_code, codex)`. Nothing is written.
- **EX-006** (boundary): a hosted chat whose project has no local folder, or a `codex.db` with another
  layout -> it is not listed, and the rest of the Codex listing is unchanged.

## Consequences

- The Codex listing gains rows with `unreadable` set. A caller that counts projects by the distinct `cwd`
  of the rows sees the hosted chats' folders: that is intended.
- `catalog/harnesses.json` gains `interactive_start`. A destination is offered for a handoff only when
  its entry has one, so a harness is added to the handoff by one catalog entry after its form has been
  seen to work.
- `mukade work` and `mukade handoff` work for any session that has a folder, not only ChatGPT's: a Codex
  thread whose rollout is gone is the same case.
- The handoff note names no ChatGPT project: it says that the conversation is not available and gives
  the listing's reason.
- Not verified at the time of this record: nothing was run on a Windows build (CI is Ubuntu and macOS),
  the one-line prompt has not been seen to pass through Windows Terminal to the harness, and the two CLI
  commands have no tests of their own.
- When the export (phase 2) is added, `read_streamed` and `read_clipped` of its reader follow ADR-0004,
  and INV-002 covers the export.

## Confirmation

Run on GitHub Actions (`cargo test -p mukade --lib --test chat_connect` and `-p mukade-desktop`), never
on the owner's machine:

- `src/reader/codex_hosted.rs`: the listing over fixture databases built from the design's appendix, the
  project folder join, the root at position 0, a row of another host kind, a missing or foreign database,
  `missing_candidate`, no double listing, the refusal on read, and `reading_leaves_both_databases_byte_identical`
  (INV-002).
- `src/work.rs`: repositories the test creates (clean, uncommitted, no upstream, ahead, detached, linked
  worktree, a linked worktree git cannot read, a branch and a tag with one name, a gone upstream, a folder
  that is itself a repository, a subfolder of one, no repository, more repositories than the cap), and
  `reading_changes_nothing` and `a_repository_cannot_run_a_program_through_fsmonitor` (INV-003, the second
  on Unix only).
- `src/handoff.rs`, `src/catalog.rs`: the note, the template, the first prompt, the destinations, and
  `claude_code_and_codex_start_with_a_prompt_and_no_other_harness_does`.
- `apps/desktop/src-tauri/src/actions.rs`: the work, the options and the handoff of an unreadable session.
- `apps/desktop/tests/work.test.ts` and the smoke test of the desktop frontend.
- INV-001 is held by review: `grep -rn "auth.json" src apps/desktop` finds no use.
- Not covered by a test: the `work` and `handoff` arms of `src/cli.rs`. They wait for the owner's live
  check, which is also the first run on Windows.
