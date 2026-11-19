---
adr_id: ADR-0004
version: 1
title: No session is too large to read; the readers answer an overview, a clipped read and a streamed read, and the chat service becomes a cargo feature
status: Accepted
author: Alfio Caprino
scope:
  paths:
    - src/reader/**
    - src/chat/**
    - src/cli.rs
    - src/harnesses/**
    - Cargo.toml
  modules:
    - "mukade::reader"
    - "mukade::chat"
  domains:
    - session-reading
    - dependency-policy
governs_symbols:
depends_on:
  - ADR-0002
  - ADR-0003
created_from_task: size-agnostic-session-reads
proposed_on: 2026-11-13
proposed_at_sha: b574bc7
accepted_on: 2026-11-13
accepted_at_sha: null
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
superseded_by: null
changes:
---

# ADR-0004: No session is too large to read; the readers answer an overview, a clipped read and a streamed read, and the chat service becomes a cargo feature

Approved by the user on 2026-11-13 through Shinatama ADR-0053 v2 (Accepted the same day), which
decides that no session is too large to resume, view, search or export and that Mukade, not its
caller, owns how a session of any size is read. This record is Mukade's side of that decision.

## Context and Problem Statement

A reader decoded a whole session before it built anything: `read_to_string` of the file, one
`serde_json::Value` per line, then the reconstruction. Measured on a real 10.6 MB Claude Code
session, a release `mukade dump` reads it in 0.2 s and holds 57 MB at its peak: about five times
the file. A session of a few hundred megabytes is a few gigabytes of memory. A caller that could
not afford that refused the session for its size.

Most of a session's bytes are tool output: in that session, `tool_result` blocks are 3.5 MB and
Claude Code's `toolUseResult` copy of them most of the rest of 7.6 MB of `user` rows. Every
string clipped at 4 KiB keeps 37% of the bytes and every structural field.

The library also carried its chat service (an MCP server and client, HTTP, an async runtime) as
part of every build, so a program that only reads sessions took those dependencies with it
(ADR-0003 allowed them for the chat service, not for the readers).

## Decision Drivers

- No operation refuses a session for its size.
- The reading belongs to Mukade; its callers keep no reading logic.
- The readers keep their small dependency set; no new dependency.
- The history of a clipped read is the history of the full read, order and branches included.

## Decision

Every reader answers three more questions, as methods of `reader::Reader` with defaults, so a
harness without a dedicated implementation still answers them:

1. **`overview(id, roots) -> SessionInfo`**: the listing's row for one session, with an exact
   id. Claude Code, Codex and Pi read that session's own file at the two ends the listing reads
   (`reader::json::peek_ends`: the first 128 KiB and the last 384 KiB), never its conversation, so
   the cost does not grow with the session. The default lists the store and picks the row. A
   missing id is `Lookup::NotFound`, a repeated one `Lookup::Ambiguous`.
2. **`read_clipped(id, roots, max_string_bytes) -> Clipped`**: a read that never refuses for
   size. Every string longer than `max_string_bytes` (at least `MIN_STRING_BYTES`, 1 KiB) is cut
   to the longest prefix within the limit that ends on a UTF-8 boundary. Up to
   `STREAM_FROM_BYTES` (16 MiB) the file is read whole as always, and its records are clipped;
   above it the file is decoded one line at a time, each string clipped as the decoder hands it
   over, and the text of the file is never held. In both cases the ordinary reconstruction and
   build run on the clipped records, so the two ways give the same session, and the session's
   entries, ids, kinds and order are the full read's. `source_sha256` is the full file's digest,
   computed while the file streams. A last line cut off by a hard kill is kept as text, clipped.
   The result is a `Clipped { session, clipping }`: a different type from `Session`, so a clipped
   session cannot be written to another harness by mistake, and no serialized format changes.
   `Clipping` says how many strings were cut and how many bytes they lost. Claude Code, Codex and
   Pi stream; the default reads the session whole and clips it (`reader::clip_session`).
3. **`read_streamed(id, roots, on_entry) -> Meta`**: the entries of `read`, in its order, with
   its full content, handed over one at a time, for search and export. Claude Code and Pi do a
   clipped pass that finds the order of the history (which rows are on the resumed branch, how a
   compaction's `kept_from` resolves) and each row's byte offset, then build the entries again
   with the same builder from the full rows read back by offset, and hand each entry over as soon
   as no later row can change it. Memory follows the structure of the session and its biggest
   entry. The default, and Codex, read the session whole first.

`read_streamed_with_meta` is `read_streamed` that also hands over the session's `Meta` before the
first entry, and `wire::DocumentWriter` writes the interchange document from it one message at a
time, byte for byte what `wire::to_document` gives, so a session of any size is exported in that
format too.

Clipping is done on the source records before a reader interprets them. Where a reader parses a
string, that string is not clipped: Codex's tool-call `arguments` (JSON text) and custom
tool-call `input` (the script of an `exec`), which a model wrote and which are therefore bounded.
A text a reader builds from several clipped parts (a user turn of several blocks) is built from
the clipped parts.

**The chat service becomes the cargo feature `chat`**, on by default for Mukade's own programs.
`default-features = false` leaves the readers and writers with `anyhow`, `chrono`, `rusqlite`,
`ruzstd`, `serde`, `serde_json`, `toml_edit`, `uuid` and `windows-sys` (`libc` elsewhere). The
`chat` module, the CLI's `chat` command and every `chat_*` method of the harness adapters are
compiled only with the feature.

## Rejected Alternatives

- **Reading a session from its end along `parentUuid`**: a different history from the full read;
  Mukade's reconstruction recovers summaries, expanded prompts and ancestors over the whole file.
- **Clipping the finished session**: needs the whole session in memory first, which is what a
  large session cannot afford. Kept only for the harnesses that have no streaming reader.
- **A custom JSON tokenizer that never holds a whole record**: the largest record bounds the
  memory instead, which is small next to the session; a tokenizer is code to carry for no
  measured gain.
- **Compression of the store**: saves disk, not reading.
- **A size threshold in the caller**: a refusal by size is what this decision removes.

## Enforceable Invariants

- **INV-001**: no operation of the three above refuses a session for its size.
- **INV-002**: an overview does not read a session's conversation: a file is opened at its head
  and tail windows only (a forked Claude Code session is also scanned for the line that names its
  origin, as its listing does).
- **INV-003**: for one file, `read_clipped` gives the same number of entries, the same ids and
  kinds, in the same order as `read`; every string of a source record in `raw` is the full one
  or its prefix; the same session whether the file is read whole or streamed.
- **INV-004**: `read_streamed` delivers the entries `read` builds, in its order, with the same
  content, and the same `Meta`.
- **INV-005**: no new dependency; `cargo check --no-default-features --lib` is warning-free.
- **INV-006**: no field of `Session`, `Meta` or the wire documents is added for clipping.

## Examples

- **EX-001** (nominal): a 100 MiB Claude Code session is read with `read_clipped(id, roots,
  4096)` -> its entries and their order are the full read's, its long strings are cut at 4 KiB,
  and the file was never held whole.
- **EX-002** (nominal): the same session is exported with `read_streamed` -> each entry arrives
  with its full text, one after the other.
- **EX-003** (nominal): `overview` of a 300 MiB session -> the listing's row, from two windows of
  the file.
- **EX-004** (error): a record in the middle of the file does not decode -> the error names its
  line (`<file>:<line>`), as the full read does; never a size refusal.

## Consequences

- A caller asks Mukade for what each operation needs and keeps no threshold of its own.
- Past the threshold a read costs two passes for `read_streamed` (the second reads the records
  again by offset) and one for `read_clipped`.
- A Codex thread kept over several files (paginated) is read whole and then clipped, and its
  `read_streamed` reads the session whole first: its earlier files are read through their own
  prefix logic, which is not streamed yet. A Pi session with `context_edit` records is read whole
  by `read_streamed` for the same reason.
- The document writer opens with the meta of the clipped pass; it refuses to close a document whose
  meta was not the final one (a fact of the meta longer than 1 KiB), never writes a wrong one.
- A method added to `Reader` is forwarded by `impl Reader for &R` (`src/harnesses/mod.rs`), or a boxed
  reader runs the trait's default.
- A clipped text is a different value from the full one: a caller that shows it says so
  (`Clipping::is_clipped`).

## Confirmation

`tests/size_agnostic_reads.rs`: the invariants over every Claude Code, Codex and Pi fixture, over
copies with inflated tool output, with the streaming threshold forced to zero and not, over a
damaged last line, a bad middle line, a BOM and CRLF; an 18 MiB synthetic session past the
threshold with no forcing; and, ignored by default and run in release, a synthetic session past
100 MiB with its peak memory and time recorded.
