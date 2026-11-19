# Architecture: one adapter per harness

Every decision that depends on which harness a session comes from or goes to is made in that
harness's **adapter**. Nothing else branches on `HarnessId`: `tests/architecture.rs` enforces it.

Outside the adapters, these name one harness without branching on `HarnessId` (public API or
parse-time checks):

| where | what |
|---|---|
| `src/roots.rs` | `StoreRoots::{claude_projects, codex_sessions, codex_archived_sessions, pi_sessions, hermes_state_db, dsh_sessions}`, `claude_project_dir_name` |
| `src/detect/layout.rs` | `layouts`: one entry per detector, listed by hand |
| `src/cli.rs` | `writer::opencode::validate_agent` on `--dest-agent`, while the flags are parsed |

## The adapter

An adapter is one file, `src/harnesses/<harness>.rs`, that implements the trait `Harness`
(`src/harnesses/mod.rs`) and, for a harness mukade writes, the trait `Destination`. It answers
for its harness and refers to the harness's own reader, writer and detector, which stay in their
folders.

| file | holds |
|---|---|
| `src/harnesses/<harness>.rs` | the adapter: `impl Harness`, and `impl Destination` for a destination |
| `src/reader/<harness>*.rs` | reading the native store (`impl Reader`) |
| `src/writer/<harness>.rs` | writing the native store (`impl Writer`), image shapes |
| `src/detect/<harness>.rs` | recognizing the store layout (`impl StoreLayout`) |
| `catalog/harnesses.json` | data: store and exe variables, CLI exe flag, default locations, layout, lineage, resume command |

## The traits

`Harness`, what every harness answers, a source only included:

| group | methods | answers |
|---|---|---|
| identity | `id`, `reader`, `destination` | the harness, its reader, itself as a destination (`None`: a source only) |
| store | `store_root`, `store_root_mut`, `set_store_root`, `resolve_store_path`, `store_is_in_memory`, `store_folder`, `store_holds`, `apply_store_env`, `store_stats` | where the store is in `StoreRoots`, how a path or variable names it, the folder that holds it, whether a listed location belongs to it, how a process is pointed at it, what it holds |
| reading | `read_file`, `read_bound`, `read_all_branches`, `read_all_branches_by_id` | one session from a file; the session a chat member bound, from the place its listing named (a database holds every session: Hermes, OpenCode); every branch (`None`: one branch kept) |
| chat | `user_origin`, `chat_hook_output`, `chat_launch`, `chat_mcp_target` | who produced a user entry (`Typed`, `Injected`, `Unknown`); what a chat hook prints in the harness's own hook format (OpenCode and Pi: mukade's plugin format, which their launch plugin hands over); the command that starts the harness as a member of a chat room, after writing its launch kit (`None`: no chat adapter for it); where the harness keeps its user-scope MCP servers (`None`: mukade writes none for it) |
| resume | `resume_refusal` | why a listed session cannot be opened in the harness's own interface (`None`: it can): a Claude subagent, any DeepSeek Harness session |
| summary | `summary_command`, `parse_summary_reply`, `after_summary`, `summarizes_from_file` | the command that asks its binary for a summary, the reply, the cleanup. A harness with no run that leaves the session untouched refuses with its reason (Hermes, DeepSeek Harness) |

`Destination`, what only a harness mukade writes answers:

| group | methods | answers |
|---|---|---|
| writer | `writer` | the writer of new sessions in its store; its `Writer` methods `new_id`, `child_id`, `child_ids_hold_root` choose ids before the session is written |
| writing rules | `protocol`, `payload_has_place`, `provider_reads_envelope`, `payload_fits`, `validate_write`, `links_children`, `links_forks`, `claims_id`, `interchange_document` | the wire protocol, which reasoning payloads replay, what a write may ask (each option the harness cannot take is refused, not ignored), whether a child session is linked to its parent natively, whether a fork can name its origin's copy, the id shape, the `document` command's output |
| destination config | `configured_model`, `project_id`, `default_shell` | the provider and model its own configuration names; the project it files a directory under; the shell it runs |
| tools | `offered`, `offered_in`, `translate`, `accepted_keys` | its tools, pinned and as its configuration turns them on; one canonical call in its shape; the keys a call may use |
| plan | `failed_steps_are_replies`, `flattens_result_blocks`, `image_in_result`, `image_in_user_turn` | how the plan lays out failed steps, result blocks and images |

Also in `src/harnesses/mod.rs`:

- `InterchangeDocument`: `source_tool_map`, `document` (OpenCode's import file).
- `destination(id)`: the harness as a destination, or exit 2 for a source-only harness, before
  planning. `require_writer(adapter)` is the same refusal, for the writer.

## The registry

| call | returns |
|---|---|
| `harnesses::get(id)` | the adapter of `id`: the one `match` on `HarnessId` |
| `harnesses::all()` | every adapter, in `HarnessId::ALL` order |

The public entry points keep their signatures and call the adapter: `reader::for_harness`,
`writer::for_harness`, `StoreRoots::{root, set_root, apply_to, from_env}`,
`tools::{offered, offered_in, translate, translate_with}`, `summary::{summary_command, parse_reply}`,
`dest::{configured_model_in, shell_in}`, `policy::{destination_protocol, payload_fits,
chat_destination_fits}`, `hop::export_path`, `writer::media::{user_block, result_block}`.

Asked of a harness that is no destination, the destination ones give the nothing answer: `None`,
an empty `ToolSet`, the text form with the reason "not a destination", `false`, a `null` image,
and from `writer::for_harness` a writer that refuses every write (exit 2) without panicking.

## What stays generic

| file | keeps |
|---|---|
| `src/tools.rs` | the canonical dispatch (`translate_with`), the accepted-keys check, the L11 offered-set safety net, the text form, the shared argument readers (`edit_call`, `grep_call`, …) and shared reasons |
| `src/summary.rs` | running and timing the summary command, trimming and refusing an empty reply |
| `src/hop.rs` | the pipeline: read, compact, plan, write, resume |
| `src/registry.rs` | the hop registry in `mukade-data/registry`: what each hop and write wrote where, block fingerprints and record ids, read back to link a fork to its origin's copy and to name an earlier copy |
| `src/children.rs` | a session's subagent sessions: finding and reading them through the reader, `dump --children`, `hop --children`, `write --children` (`Destination::links_children`, `Writer::child_id` say what a destination does with one) |
| `src/writer/plan.rs` | the rendering plan; it asks the adapter for its flags and image shapes |
| `src/native_compaction.rs` | native compaction records, harness-neutral |
| `src/catalog.rs` | reading `catalog/harnesses.json` |

## Reading a session of any size

[ADR-0004](adr/ADR-0004-size-agnostic-session-reads.md). A reader that decodes the whole file into
`serde_json::Value`s before it builds holds about five times the file. Three more `Reader` methods
(`overview`, `read_clipped`, `read_streamed`, with defaults over `list_all` and `read`) answer without
that. Claude Code, Codex and Pi have their own. The shared code is `src/reader/clip.rs`; the guide for
callers is [reading-large-sessions](reading-large-sessions.md).

**The threshold.** `reader::STREAM_FROM_BYTES` is 16 MiB. A file at or under it is read as always
(`json::read_jsonl`: `read_to_string`, lines decoded on up to 8 threads, the digest on another) and
its records are then clipped in place. A file over it is read by `clip::decode_streaming`.

**How the streaming decode clips.** The file is read with `BufRead::read_until(b'\n')` into one
reused buffer. Each line is decoded by a `serde::de::DeserializeSeed` (`clip::Seed`) that builds the
`Value` itself: when the decoder hands over a string longer than `max_string_bytes`, the seed keeps a
prefix cut at a UTF-8 boundary and the rest is dropped, so the whole text is never held. Object keys
keep their order (`preserve_order`). Numbers, booleans and nulls are copied. Paths a reader parses and
must see whole (Codex's `payload.arguments` and `payload.input`) are in a keep list the seed does not
cut; the whole-then-clip way applies the same list, so the two give the same records.
`clip::tests::decoding_clipped_equals_decoding_whole_then_clipping` pins that.

**Why the history is the same.** Clipping is done on the source records, before a reader reads them,
and only shortens strings. The reconstruction and the build the full read runs (Claude Code:
`order::reconstruct` and `Builder`; Pi: `active_branch` and `Builder`; Codex: `rolled_back` and
`Builder`) look at the structure of the records: their types, ids, parent links, roles, and the start
of a few texts. They run unchanged on the clipped records, so the same rows are on the resumed
branch, in the same order, merged into the same entries, with the same ids. The tests compare entry
counts, ids, kinds and order with the full read on every fixture, at several limits, with the streaming
threshold forced to zero and not. A text a reader builds from several clipped blocks is shorter than
the full one, never longer.

**`source_sha256`.** The digest of the file's bytes, as the full read gives it. The streaming decode
feeds every byte it reads, in order, to `sha256::Sha256` (`update` per line), so the fingerprint costs
no second pass. A leading BOM is hashed, as the full read hashes it, and dropped before the first
line is decoded.

**A cut-off last line.** A line that does not parse is held until the next non-blank line shows whether
it was the last. A later line makes it an error naming `<file>:<line>`, the full read's message; at the
end it becomes the session's `Unfinished` entry, as text, clipped.

**Memory.** Past the threshold the file's text is never held. What is held is the clipped records (a
string is at most the limit; the structure is small), the entries built from them, and one line. The
largest record, an image pasted into a prompt, bounds the buffer. In the 229 MiB measurement the
clipped read peaks at 70 MiB against 612 MiB for the full read.

**`overview`.** Claude Code, Codex and Pi find the one file from the id and run the listing's own
code on it: `peek` (Claude Code, Pi) over `json::peek_ends` (first 128 KiB, last 384 KiB) or Codex's
`list_in` limited to that thread (`only`), then the store's other facts the listing adds. The file name
is matched exactly, the rows are not decoded beyond the windows. `reader::pick_overview` turns zero or
several matches into `Lookup::NotFound` or `Lookup::Ambiguous`. The default is `list_all` and the same
pick.

**`read_streamed` and the re-read by offset.** Claude Code and Pi do two passes:

1. A clipped pass, `clip::clipped_rows` at the smallest limit, which keeps each record's byte offset
   (`ClippedRows::offsets`). `parse_planned` runs the full build on it, and returns, beside the
   session, the `Plan`: the read order of the rows (`order`), which are on the branch (`on_branch`)
   and, for Claude Code, how many `Relinked` entries follow.
2. A second build with the same `Builder`, fed the **full** rows in the plan's order, each read back
   by its offset (`clip::RowReader::row_at`, a seek only where the order jumps). After each row,
   `Builder::take_final` hands over the entries no later row can change and leaves a placeholder, so
   the builder's indexes stay valid. Only the entries still open stay: the step or results being
   collected, a compaction waiting for its summary, and the last conversation entry, which an
   interruption marker may mark aborted. A compaction's `kept_from`, which needs every later entry,
   is taken from the clipped pass. Every handed-over entry's id is checked against the clipped
   pass's: a mismatch is an error, not a wrong answer.

A Pi session with `context_edit` records, which rewrite earlier records, and a Codex rollout, are read
whole first and handed over entry by entry.

**`wire::DocumentWriter`.** `to_document` projects a session into the interchange document in memory.
`message_of(entry, n)` is its one-entry step (an entry is one message; `messages_of` is `message_of`
over the conversation entries), and `DocumentWriter` writes the document around it to an `io::Write`:
`{"meta":…,"library":…,"messages":[` at `new`, each message from `entry`, `entry_with` or `message`,
then `]`, the branches if any, `}` at `finish`. The bytes are those of `to_writer(to_document(..))`; a
test compares them on every fixture. The document opens with `meta`, which a streamed read has only at
the end, so `Reader::read_streamed_with_meta` hands over the clipped pass's meta first (the first
pass's records are clipped at 1 KiB: the same, unless a fact of the meta is longer) and `finish` checks
it against the final one, refusing rather than closing a wrong document.

**A boxed reader.** `reader::for_harness` boxes the adapters' `&'static dyn Reader`, and `impl Reader
for &R` in `src/harnesses/mod.rs` forwards each method. A method added to `Reader` is forwarded there
too, or a boxed reader runs the trait's default; `a_boxed_or_borrowed_reader_forwards_the_three_reads`
holds it.

## Adding a harness

Done for Hermes Agent and DeepSeek Harness on 2026-11-12; the steps that the first version of this
list missed are marked (added).

1. `src/model/harness.rs`: a `HarnessId` variant, and its place in `ALL`, `as_str` and `FromStr`
   (the unknown-harness error lists the names).
2. `src/roots.rs`: a `StoreRoots` field, its place in `from_env`, `root`, `set_root` and `apply_to`,
   and a helper that names the store's file or folder when there is one (`hermes_state_db`,
   `dsh_sessions`). The struct is public API, so the list is closed.
3. The reader, `src/reader/<harness>.rs` (`impl Reader`), and its `mod` line in `src/reader/mod.rs`.
4. For a destination: the writer, `src/writer/<harness>.rs` (`impl Writer`; Claude Code, Codex, OpenCode and Pi also keep a free `image_block` function that their adapters call for an image), and
   its `mod` line in `src/writer/mod.rs`. A harness mukade only reads (a source only) has none.
5. The detector, `src/detect/<harness>.rs` (`impl StoreLayout`), its `mod` line in
   `src/detect.rs` and its place in `detect::layout::layouts`.
6. The tool names: `CanonicalTool::from_<harness>` in `src/model/tool.rs`, used by the reader
   (added).
7. The adapter, `src/harnesses/<harness>.rs`, and its `mod` line. A source only implements
   `Harness` and keeps the default `destination()`, `None`: every write toward it is refused with
   exit 2 before anything is planned. A destination also implements `Destination` and answers
   `destination()` with `Some(self)`. Answer the questions the defaults get wrong (added):
   `read_bound` (a database holds every session), `user_origin` (what the chat observer shows),
   `chat_mcp_target`, `store_folder` / `store_holds`, `resume_refusal`, `links_children`, and the
   summary methods (refuse with the reason when the harness has no run that leaves the session as it
   was).
8. The registry arm in `harnesses::get`; the array length in the signatures of `harnesses::all`
   and `detect::layout::layouts` (both state `6` now).
9. The catalog entry in `catalog/harnesses.json`: `binary`, `store` (`default_on` for a platform that
   differs), `lineage`, `resume`, and `interactive_resume` when the harness has a terminal UI.
10. The per-harness lines of `USAGE` in `src/cli.rs`: its CLI name in the `harnesses:` line, its
    `--<harness>-exe` flag in the options, and its `MUKADE_*_EXE` variable in the executable
    resolution (`the_usage_text_names_what_the_catalog_names` in `tests/harnesses.rs` checks the
    flag and the variable against the catalog). In `docs/usage.md`: its row in "Stores and
    executables", its section in "Per-harness notes" and its rows in the destinations table.
11. Tests: fixtures under `fixtures/<harness>/`; reader and writer tests; the shared scaffolding
    `tests/support/mod.rs` (fixture path, id, `stage_fixture`, `roots`), which every loop over
    `ALL` uses (added); the adapter's rows in `tests/harnesses.rs`; its reader, writer and detector
    files in `ADAPTER_FILES` and its `HarnessId::` name in `HARNESS_ARMS` of `tests/architecture.rs`;
    the origin test in `tests/bus_origin.rs` (added).
12. `mukade-tui` needs no code: it loops over the registry, so listing and hopping work. Check the
    resume (`every_harness_has_the_exact_interactive_resume_argv`) and say in `resume_refusal` when
    the harness has no interface to open a session in (added).
13. Docs (added): the harness page (skeleton: Status in mukade, Quick start, Install and locate,
    Session store, Features, How mukade reads / writes / summarizes it, Open points, Sources),
    `docs/reference/harness-catalog.md`, `docs/tool-conversion-tables.md`, the lists in `README.md`
    and `docs/README.md`, and a verification record under `docs/verification/` for a destination.

## Changing a harness

Edit its adapter files (`src/harnesses/<harness>.rs` and its reader, writer and detector) and its
catalog entry. Nothing else branches on it.

## The architecture test

`tests/architecture.rs`, `no_harness_branch_outside_its_adapter`:

1. Walks every `.rs` file under `src/` and `mukade-tui/src/`.
2. Skips the adapter files (`ADAPTER_FILES`, exact files and folders): `src/harnesses/`, the
   readers, writers and detectors of one harness, `src/model/harness.rs`.
3. Skips test code: the item after a `#[cfg(test)]` line. One lexer reads each file from its
   first line, so the attribute counts only on a line that starts in code, never inside a string
   or a comment. Brackets are counted outside strings (raw, byte and C ones included), character
   literals and comments. The item ends at its own last line, or before a line indented less than
   the attribute; where unsure it ends early, so production code is read, never hidden.
4. Fails on any other line naming `HarnessId::ClaudeCode`, `Codex`, `OpenCode`, `Pi`, `Hermes` or
   `DeepSeekHarness` (`HARNESS_ARMS` at the top of the test), listing each `file:line`.
5. Guards against passing on nothing: more than 50 files walked, more than 30 checked, hits found
   inside the adapters, and every `ADAPTER_FILES` entry still present.

The scanner has its own tests in the same file: test items of every shape (a multi-line `use`, a
field, a `const`, a match arm, a parameter, a string spanning lines, a module), every literal
that holds a bracket or a quote (escaped characters, raw byte and C strings, nested comments,
lifetimes and labels), and the attribute's text inside a raw string and inside a block comment,
each followed by a production line that must be reported.

`ALLOWED` lists files exempted, each with the number of harness names it holds and its reason. It
is empty. A file with more or fewer names than its entry declares fails the test, so an allowed
file cannot gain one unseen; `an_allowlisted_file_holds_the_names_it_declares` checks the rule on
a synthetic list.
