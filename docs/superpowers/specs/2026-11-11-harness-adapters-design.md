# One adapter per harness

## Why

The user (2026-11-11): one adapter per harness, all extending the same interface, each with its own
logic: "è una questione di architettura e pulizia". Today `Reader` and `Writer` follow that shape,
but about 70 branches on `HarnessId` live elsewhere (inventory below). Adding a harness (Hermes and
DeepSeek are next in line) means touching about ten files, and four facts are copied in several
places. Goal: adding or changing a harness means editing **its adapter file and the catalog**, and
nothing that branches on the harness lives outside them.

## Goals

1. A trait `Harness` in `src/harnesses/mod.rs`; one adapter per harness in
   `src/harnesses/{claude_code,codex,opencode,pi}.rs`; `harnesses::get(HarnessId) -> &'static dyn
   Harness` and `harnesses::all()`.
2. Every per-harness decision outside `src/reader/<h>*.rs`, `src/writer/<h>.rs`, `src/detect/<h>.rs`
   becomes an adapter method or adapter data; the call sites ask the adapter.
3. Facts that exist in the catalog (`catalog/harnesses.json`) are read from it, never restated:
   store env var, exe env var, CLI exe flag, lineage recorded.
4. No behaviour change. The whole suite (550 tests) passes unchanged except tests that restated a
   per-harness table (they ask the adapter instead).
5. Public API stays source-compatible: `reader::for_harness`, `writer::for_harness`,
   `StoreRoots::{root, set_root, apply_to, from_env}`, `tools::{offered, translate, translate_with}`,
   `summary::{summary_command, parse_reply}`, `dest::configured_model_in`,
   `policy::destination_protocol`, `hop::export_path`, `exe::*`, `catalog::*` keep their signatures
   and become thin calls into the adapter. `mukade-tui` and the uncommitted bus work keep compiling.

## Non-goals

- Moving the reader/writer/detect files. They already are per-harness units; the adapter refers to
  them (`fn reader(&self) -> &'static dyn Reader`). A physical regroup into
  `src/harnesses/<h>/` can follow once the parallel bus session has committed (it edits
  `src/reader/mod.rs`).
- Changing `StoreRoots`' four public fields (public API).
- Changing `HarnessId` into an open set. The enum stays; the adapter is the one place each arm lives.

## The interface

Grouped; exact names are the plan's to fix, the grouping is binding.

| group | methods (one line each) | today in |
|---|---|---|
| identity | `id()`; `reader()`; `writer()`; `layout()` (the `StoreLayout`) | `reader::for_harness`, `writer::for_harness`, `detect::layout::layouts` |
| store | `store_root(&StoreRoots)`, `set_store_root(&mut StoreRoots, &Path)`, `apply_store_env(&StoreRoots, &mut Command)`, `store_stats(&StoreRoots)` | `roots.rs` `root`/`set_root`/`apply_to`, `cli.rs` `store_path`/`inspect_store`, `writer/mod.rs` `validate_options`, `reader/opencode.rs` `opencode_environment` |
| reading | `read_file(&Path)`; `read_all_branches(…)` → `Option` (Codex only) | `hop.rs` `export_path`, `export_all_branches`, `all_branches_supported`; `tests/support` copies |
| writing rules | `destination_protocol()`, `payload_fits(&Opaque, provider)`, `validate_write(&WriteOpts)` (agent/metadata, dest path, id format), `configured_model(&StoreRoots)`, `project_id(…)` (OpenCode) | `policy.rs`, `writer/mod.rs` `validate_options` + 4 copies in `hop.rs`/`cli.rs`, `dest.rs`, `cli.rs` + TUI project id |
| tools | `offered()`, `translate(canonical, input, dest_shell)` for this destination, `accepted_keys(canonical)` | `tools.rs` `offered`, the per-destination arms of `translate_with`, `accepted_keys`, `edit`/`grep`/`glob`/`web_*`/`shell` |
| plan flags | `failed_steps_are_replies()`, `flattens_result_blocks()` | `writer/plan.rs` |
| summary | `summary_command(session, opts, scratch)`, `parse_summary_reply(stdout, file)`, `after_summary(…)` (OpenCode fork delete), `model_args(…)` | `summary.rs` |

Generic logic stays where it is: `tools.rs` keeps the canonical dispatch, `uses_only`, the L11
safety net and text form; `summary.rs` keeps running and timing the command; `hop.rs` keeps the
pipeline.

## Decisions after the builder check (2026-11-11)

1. **Writability is part of the interface now.** `writer()` returns `Option<&'static dyn Writer>`
   (all four `Some`). `hop::import`, `hop::hop`, the CLI (`write`, `hop`, `document`) and the TUI
   hop dialog refuse a destination without a writer with exit 2 **before planning**
   (ADR-0002: Hermes is source-only). `writer::for_harness` keeps its signature for the four.
2. **"Adding a harness" is documented honestly**, not as two places: a `HarnessId` variant (plus
   `ALL`, `as_str`, `FromStr`), a `StoreRoots` field, the adapter file with its reader / writer /
   detect files, the catalog entry, the registry arm in `harnesses::get`. Changing an existing
   harness touches its adapter files and the catalog only. The closed enum and `StoreRoots`' fields
   stay (public API).
3. **`document` becomes an adapter capability**: `interchange_document()` returns `Option` of
   what the CLI needs (OpenCode `Some`, others `None`); the CLI's OpenCode-only refusal, its tool
   map for an OpenCode source and the direct `writer::opencode::document_from_plan_report` call go
   through it.
4. **The parallel bus work is left to its session**: `src/reader/origin.rs` `origin_of` and the
   OpenCode check in `src/bus/store.rs` move into adapters in a follow-up once that work is on
   master. No speculative methods now.
5. **An architecture test enforces the rule**: `tests/architecture.rs` reads `src/**/*.rs` and
   `mukade-tui/src/**/*.rs` and fails on `HarnessId::ClaudeCode|Codex|OpenCode|Pi` outside
   `src/harnesses/`, `src/reader/<h>*`, `src/writer/<h>.rs`, `src/detect/<h>.rs`,
   `src/model/harness.rs`, test code (`#[cfg(test)]` modules) and an explicit, commented allowlist
   (empty at the end of this project).

## Duplicates removed

- Store root per harness: `roots.rs` `root`, `writer/mod.rs` `validate_options`, `cli.rs`
  `store_path` → `store_root`.
- Env var names in `roots.rs` (`from_env`, `apply_to`) and `exe.rs` `env_var_name` → catalog
  `store.env` / `binary.env`.
- CLI exe flags `--claude-exe` … → catalog `binary.flag` (parsing and usage text).
- OpenCode-only agent/metadata rule (`writer/mod.rs`, `hop.rs` `import`, `cli.rs` hop and
  `write_opts`) and the destination id rule (2 copies) → `validate_write`.
- Forks "not recorded" in `cli.rs` → catalog `lineage`.
- `tests/support` `read_source`/`read_destination` matches → `harnesses::get(h).read_file`.

## Constraints

- No new dependency.
- Work in the worktree `D:\Projects\mukade-adapters` (branch `harness-adapters`, from
  `origin/master` `7a24d2f`); push to `master` at the end.
- The main tree holds a parallel session's uncommitted bus work (`src/lib.rs` adds `pub mod bus;`,
  `src/reader/mod.rs` adds `pub mod origin;`, `src/portable.rs`). Add `pub mod harnesses;` to
  `src/lib.rs` in its alphabetical place (between `exe` and `hop`, not next to their line). Do not
  edit `src/reader/mod.rs` beyond what `for_harness` delegation needs.
- One concern per commit; the suite green after each.
- Docs: `docs/harnesses/*.md` refer to code by path and symbol; update the ones that name a moved
  symbol, and add an "Adding a harness" section to `docs/README.md` or the architecture doc that
  lists the two places to edit (adapter + catalog) and the trait methods.

## Inventory (2026-11-11, at `ba02882`; symbols, not lines)

1. Store roots/env: `roots.rs` `StoreRoots` fields, `from_env`, `root`, `set_root`, `apply_to`,
   `claude_project_dir_name`; `cli.rs` `inventory_roots`, `store_path`; `writer/mod.rs`
   `validate_options`.
2. Executables: `exe.rs` `binary_name`, `env_var_name`, `resolve`, `resolve_source`,
   `default_candidates`; `detect/engine.rs` `engine_beside_store` (catalog-driven already).
3. Resume command: `catalog.rs` `resume`, `interactive_resume` (catalog data already).
4. Reading by path / all branches / store stats: `hop.rs` `export_path`, `export_all_branches`,
   `export_path_all_branches`, `all_branches_supported`; `cli.rs` `inspect_store`.
5. Tools (dest): `tools.rs` `offered`, `translate_with` arms, `accepted_keys`, `shell`,
   `edit`, `multi_edit`, `grep`, `glob`, `web_fetch`, `web_search`, `call_pi_ls`.
6. Tools (source): `model/tool.rs` `CanonicalTool::from_*`, each called only by its reader — stays.
7. Plan/media: `writer/plan.rs` `plan` flags, `carry_attachments`, `carry_result_content`;
   `writer/media.rs` `user_block`, `result_block` (writer-internal — stays in `writer/`).
8. Reasoning: `policy.rs` `destination_protocol`, `payload_fits`, `chat_destination_fits`.
9. Summary: `summary.rs` `summary_command`, `model_args`, `parse_reply`, `opencode_fork`,
   `summarize_in`, `delete_opencode_fork`; `cli.rs` `--from-file` Pi check.
10. Destination config: `dest.rs` `configured_model_in`, `opencode_config_beside`,
    `opencode_model`, Codex `config.toml`; `cli.rs` OpenCode project id; TUI copy.
11. Write validation: `writer/mod.rs` `validate_id_syntax`, `validate_options`; copies in `hop.rs`
    `import`, `cli.rs` (hop, `write_opts`, `document`).
12. Native compaction: harness-neutral — stays.
13. Catalog/detect: catalog-driven already — stays.
14. CLI flags: `cli.rs` exe flags, `exe_flag`, `write_opts`, `hop_destination`.
15. TUI: `mukade-tui/src/main.rs` `execute_hop`, `resolve_roots`, `compaction_for`;
    `location.rs` — switch to the adapter where it restates a per-harness choice.

## Left to the builder

Method names, whether data-only answers are consts on the adapter or catalog reads, the order of
the commits, error types.
