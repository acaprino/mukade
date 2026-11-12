# Architecture: one adapter per harness

Every decision that depends on which harness a session comes from or goes to is made in that
harness's **adapter**. Nothing else branches on `HarnessId`: `tests/architecture.rs` enforces it.

Outside the adapters, these name one harness without branching on `HarnessId` (public API or
parse-time checks):

| where | what |
|---|---|
| `src/roots.rs` | `StoreRoots::{claude_projects, codex_sessions, codex_archived_sessions, pi_sessions}`, `claude_project_dir_name` |
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
| store | `store_root`, `store_root_mut`, `set_store_root`, `resolve_store_path`, `store_is_in_memory`, `apply_store_env`, `store_stats` | where the store is in `StoreRoots`, how a path or variable names it, how a process is pointed at it, what it holds |
| reading | `read_file`, `read_all_branches`, `read_all_branches_by_id` | one session from a file; every branch (`None`: one branch kept) |
| summary | `summary_command`, `parse_summary_reply`, `after_summary`, `summarizes_from_file` | the command that asks its binary for a summary, the reply, the cleanup |

`Destination`, what only a harness mukade writes answers:

| group | methods | answers |
|---|---|---|
| writer | `writer` | the writer of new sessions in its store |
| writing rules | `protocol`, `payload_has_place`, `provider_reads_envelope`, `payload_fits`, `validate_write`, `claims_id`, `interchange_document` | the wire protocol, which reasoning payloads replay, what a write may ask, the id shape, the `document` command's output |
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
| `src/children.rs` | a session's subagent sessions: finding and reading them through the reader, `dump --children`, `hop --children` (`Destination::links_children`, `Writer::child_id` say what a destination does with one) |
| `src/writer/plan.rs` | the rendering plan; it asks the adapter for its flags and image shapes |
| `src/native_compaction.rs` | native compaction records, harness-neutral |
| `src/catalog.rs` | reading `catalog/harnesses.json` |

## Adding a harness

1. `src/model/harness.rs`: a `HarnessId` variant, and its place in `ALL`, `as_str` and `FromStr`.
2. `src/roots.rs`: a `StoreRoots` field (public API, so the list is closed).
3. The reader, `src/reader/<harness>.rs` (`impl Reader`), and its `mod` line in `src/reader/mod.rs`.
4. For a destination: the writer, `src/writer/<harness>.rs` (`impl Writer`, `image_block`), and
   its `mod` line in `src/writer/mod.rs`. A harness mukade only reads (a source only) has none.
5. The detector, `src/detect/<harness>.rs` (`impl StoreLayout`), its `mod` line in
   `src/detect.rs` and its place in `detect::layout::layouts`.
6. The adapter, `src/harnesses/<harness>.rs`, and its `mod` line. A source only implements
   `Harness` and keeps the default `destination()`, `None`: every write toward it is refused with
   exit 2 before anything is planned. A destination also implements `Destination` and answers
   `destination()` with `Some(self)`.
7. The registry arm in `harnesses::get`; the array length in the signatures of `harnesses::all`
   and `detect::layout::layouts` (both state `4`).
8. The catalog entry in `catalog/harnesses.json`.
9. The per-harness lines of `USAGE` in `src/cli.rs`: its CLI name in the `harnesses:` line, its
   `--<harness>-exe` flag in the options, and its `MUKADE_*_EXE` variable in the executable
   resolution (`the_usage_text_names_what_the_catalog_names` in `tests/harnesses.rs` checks the
   flag and the variable against the catalog). In `docs/usage.md`: its row in "Stores and
   executables" and its section in "Per-harness examples".
10. Tests: fixtures under `fixtures/<harness>/`, reader and writer tests, the adapter's rows in
    `tests/harnesses.rs`, and its reader, writer and detector files in `ADAPTER_FILES` of
    `tests/architecture.rs`.

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
4. Fails on any other line naming `HarnessId::ClaudeCode`, `Codex`, `OpenCode` or `Pi`, listing
   each `file:line`.
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
