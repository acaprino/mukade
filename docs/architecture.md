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
(`src/harnesses/mod.rs`). It answers for its harness and refers to the harness's own reader,
writer and detector, which stay in their folders.

| file | holds |
|---|---|
| `src/harnesses/<harness>.rs` | the adapter: `impl Harness` |
| `src/reader/<harness>*.rs` | reading the native store (`impl Reader`) |
| `src/writer/<harness>.rs` | writing the native store (`impl Writer`), image shapes |
| `src/detect/<harness>.rs` | recognizing the store layout (`impl StoreLayout`) |
| `catalog/harnesses.json` | data: store and exe variables, CLI exe flag, default locations, layout, lineage, resume command |

## The trait

| group | methods | answers |
|---|---|---|
| identity | `id`, `reader`, `writer` | the harness, its reader, its writer (`None`: a source only) |
| store | `store_root`, `store_root_mut`, `set_store_root`, `resolve_store_path`, `store_is_in_memory`, `apply_store_env`, `store_stats` | where the store is in `StoreRoots`, how a path or variable names it, how a process is pointed at it, what it holds |
| reading | `read_file`, `read_all_branches`, `read_all_branches_by_id` | one session from a file; every branch (`None`: one branch kept) |
| writing rules | `destination_protocol`, `payload_has_place`, `provider_reads_envelope`, `payload_fits`, `validate_write`, `claims_id`, `interchange_document` | the wire protocol, which reasoning payloads replay, what a write may ask, the id shape, the `document` command's output |
| destination config | `configured_model`, `project_id` | the provider and model its own configuration names; the project it files a directory under |
| summary | `summary_command`, `parse_summary_reply`, `after_summary`, `summarizes_from_file` | the command that asks its binary for a summary, the reply, the cleanup |
| tools | `offered`, `translate`, `accepted_keys` | its tools as a destination, one canonical call in its shape, the keys a call may use |
| plan | `failed_steps_are_replies`, `flattens_result_blocks`, `image_in_result`, `image_in_user_turn` | how the plan lays out failed steps, result blocks and images |

Also in `src/harnesses/mod.rs`:

- `InterchangeDocument`: `source_tool_map`, `document` (OpenCode's import file).
- `require_writer(adapter)`: the writer, or exit 2 for a source-only harness, before planning.

## The registry

| call | returns |
|---|---|
| `harnesses::get(id)` | the adapter of `id`: the one `match` on `HarnessId` |
| `harnesses::all()` | every adapter, in `HarnessId::ALL` order |

The public entry points keep their signatures and call the adapter: `reader::for_harness`,
`writer::for_harness`, `StoreRoots::{root, set_root, apply_to, from_env}`,
`tools::{offered, translate, translate_with}`, `summary::{summary_command, parse_reply}`,
`dest::configured_model_in`, `policy::destination_protocol`, `hop::export_path`,
`writer::media::{user_block, result_block}`.

## What stays generic

| file | keeps |
|---|---|
| `src/tools.rs` | the canonical dispatch (`translate_with`), the accepted-keys check, the L11 offered-set safety net, the text form, the shared argument readers (`edit_call`, `grep_call`, …) and shared reasons |
| `src/summary.rs` | running and timing the summary command, trimming and refusing an empty reply |
| `src/hop.rs` | the pipeline: read, compact, plan, write, resume |
| `src/writer/plan.rs` | the rendering plan; it asks the adapter for its flags and image shapes |
| `src/native_compaction.rs` | native compaction records, harness-neutral |
| `src/catalog.rs` | reading `catalog/harnesses.json` |

## Adding a harness

1. `src/model/harness.rs`: a `HarnessId` variant, and its place in `ALL`, `as_str` and `FromStr`.
2. `src/roots.rs`: a `StoreRoots` field (public API, so the list is closed).
3. The reader, `src/reader/<harness>.rs` (`impl Reader`), and its `mod` line in `src/reader/mod.rs`.
4. The writer, `src/writer/<harness>.rs` (`impl Writer`, `image_block`), and its `mod` line in
   `src/writer/mod.rs`. A harness mukade only reads has none: `writer()` returns `None`.
5. The detector, `src/detect/<harness>.rs` (`impl StoreLayout`), its `mod` line in
   `src/detect.rs` and its place in `detect::layout::layouts`.
6. The adapter, `src/harnesses/<harness>.rs` (`impl Harness`), and its `mod` line.
7. The registry arm in `harnesses::get`; the array length in the signatures of `harnesses::all`
   and `detect::layout::layouts` (both state `4`).
8. The catalog entry in `catalog/harnesses.json`.
9. Tests: fixtures under `fixtures/<harness>/`, reader and writer tests, the adapter's rows in
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
3. Skips test code: the item after a `#[cfg(test)]` line, brackets counted outside strings (raw,
   byte and C ones included), character literals and comments. It ends at its own last line, or before a line indented less than the attribute;
   where unsure it ends early, so production code is read, never hidden.
4. Fails on any other line naming `HarnessId::ClaudeCode`, `Codex`, `OpenCode` or `Pi`, listing
   each `file:line`.
5. Guards against passing on nothing: more than 50 files walked, more than 30 checked, hits found
   inside the adapters, and every `ADAPTER_FILES` entry still present.

The scanner has its own tests in the same file: test items of every shape (a multi-line `use`, a
field, a `const`, a match arm, a parameter, a string spanning lines, a module) and every literal
that holds a bracket or a quote (escaped characters, raw byte and C strings, nested comments,
lifetimes and labels), each followed by a production line that must be reported.

`ALLOWED` lists files exempted, each with its reason. It is empty; an entry whose file no longer
names a harness fails the test.
