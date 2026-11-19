# One adapter per harness — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every per-harness decision lives in one adapter per harness behind one trait `Harness`; nothing outside the adapters (and the per-harness reader/writer/detect files) branches on `HarnessId`.

**Architecture:** `src/harnesses/mod.rs` defines `trait Harness` and the registry (`get`, `all`); `src/harnesses/{claude_code,codex,opencode,pi}.rs` implement it, referring to the existing `reader::<h>` and `writer::<h>` units. Existing public functions keep their signatures and delegate. One concern per task; the suite stays green after each.

**Tech Stack:** Rust 2024, no new crates.

**Spec:** `docs/superpowers/specs/2026-11-11-harness-adapters-design.md` (read its Inventory and "Decisions after the builder check").

## Global Constraints

- Worktree `D:\Projects\mukade-adapters`, branch `harness-adapters`. Never touch `D:\Projects\mukade` (a parallel session's uncommitted work lives there).
- No behaviour change: every existing test passes unchanged, except tests that restated a per-harness table, which switch to the adapter.
- Public API source-compatible (list in the spec, Goal 5). `mukade-tui` must build: run `cargo test -j 4 --workspace`.
- `src/lib.rs`: add `pub mod harnesses;` between `pub mod exe;` and `pub mod hop;`. `src/reader/mod.rs`: change only `for_harness`'s body.
- Code comments state facts, in the repository's voice (short declarative sentences, spec references like "(L11)").
- Cargo transient `os error 1455` / `LNK1171` → rerun with `-j 4`.
- Commit messages end with:
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>` and
  `Claude-Session: https://claude.ai/code/session_01Cd6KB3XbaqfokuKXrGKFZ6`

## Review Focus

1. OpenCode's store root is a file (`opencode.db`) while the others are folders: `set_store_root` with a folder must give `<dir>/opencode.db`, with a file the file — test `an_opencode_root_folder_means_its_database` (Task 1).
2. A destination without a writer must be refused before any planning or writing, exit code 2 — test `a_destination_without_a_writer_is_refused_before_planning` (Task 3).
3. Env var names and CLI exe flags read from the catalog must equal today's literal strings — test `names_from_the_catalog_are_the_ones_users_type` (Task 2).
4. Moving per-destination tool translation must not change one translated call: the whole `tests/tools_translation.rs` and `tests/hop.rs` suites are the proof (Task 6).
5. The architecture test must not pass vacuously: it must find the source files (assert the count of scanned files > 50) — test `no_harness_branch_outside_its_adapter` (Task 7).

---

### Task 1: Registry, identity, store roots, reading by path

**Files:** Create `src/harnesses/mod.rs`, `src/harnesses/{claude_code,codex,opencode,pi}.rs`, `tests/harnesses.rs`. Modify `src/lib.rs`, `src/reader/mod.rs` (`for_harness`), `src/writer/mod.rs` (`for_harness`, the root mapping in `validate_options`), `src/roots.rs` (`root`, `set_root`, `apply_to`), `src/hop.rs` (`export_path`), `src/cli.rs` (`store_path`), `src/reader/opencode.rs` (`opencode_environment` moves to the OpenCode adapter), `tests/support/mod.rs` (`read_source`, `read_destination`).

**Interfaces — Produces:**
```rust
pub trait Harness: Send + Sync {
    fn id(&self) -> HarnessId;
    fn reader(&self) -> &'static dyn Reader;
    fn writer(&self) -> Option<&'static dyn Writer>;
    fn read_file(&self, path: &Path) -> anyhow::Result<Session>;
    fn store_root<'a>(&self, roots: &'a StoreRoots) -> Option<&'a Path>;
    fn set_store_root(&self, roots: &mut StoreRoots, root: &Path);
    fn apply_store_env(&self, roots: &StoreRoots, command: &mut std::process::Command);
}
pub fn get(id: HarnessId) -> &'static dyn Harness;
pub fn all() -> [&'static dyn Harness; 4];   // in HarnessId::ALL order
```

- [ ] **Step 1: Failing tests** in `tests/harnesses.rs`: `every_id_has_its_own_adapter` (for each `HarnessId::ALL`, `get(id).id() == id`, `get(id).reader().harness() == id`, `get(id).writer().unwrap().harness() == id`); `the_store_root_is_the_roots_field` (a `StoreRoots` with four distinct paths: each adapter answers its own field); `an_opencode_root_folder_means_its_database` (folder → `<dir>/opencode.db`; a `.db` file → that file; others: the folder); `read_file_reads_each_fixture` (each `support::source_path(h)` reads with `get(h).read_file` and its `meta.harness == h`).
- [ ] **Step 2: Run** `cargo test -j 4 --test harnesses` → FAIL (module missing).
- [ ] **Step 3: Implement** the trait, four adapters, registry; make the listed functions delegate (bodies become one call). `StoreRoots::apply_to` keeps its signature.
- [ ] **Step 4: Run** `cargo test -j 4 --workspace` → all PASS.
- [ ] **Step 5: Commit** `refactor(harnesses): one adapter per harness, with its reader, writer and store root`.

### Task 2: Names users type come from the catalog

**Files:** Modify `src/roots.rs` (`from_env`, `apply_to`'s variable names), `src/exe.rs` (`env_var_name`), `src/cli.rs` (exe flags parsing, usage text, `exe_flag`, `lineage_info`, `inspect_store` forks), `src/catalog.rs` (accessors if missing: `store_env`, `binary_env`, `binary_flag`, `lineage_recorded`). Test: `tests/harnesses.rs`.

- [ ] **Step 1: Failing test** `names_from_the_catalog_are_the_ones_users_type`: for each harness, `catalog` store env / exe env / exe flag equal the literals `CLAUDE_CONFIG_DIR`/`CODEX_HOME`/`OPENCODE_DB`/`PI_CODING_AGENT_DIR`, `MUKADE_CLAUDE_EXE`/`MUKADE_CODEX_EXE`/`MUKADE_OPENCODE_EXE`/`MUKADE_PI_EXE`, `--claude-exe`/`--codex-exe`/`--opencode-exe`/`--pi-exe`, and `exe::env_var_name(h)` returns the catalog value. (It fails until the accessors exist.)
- [ ] **Step 2: Run** `cargo test -j 4 --test harnesses names_from` → FAIL.
- [ ] **Step 3: Implement**: the code reads those names from the catalog; the CLI parses `--<x>-exe` by looking up the catalog flag; the forks column reads catalog lineage.
- [ ] **Step 4: Run** `cargo test -j 4 --workspace` → all PASS (`tests/cli.rs` covers the flags).
- [ ] **Step 5: Commit** `refactor: store variables, exe variables and exe flags come from the catalog`.

### Task 3: Writing rules, writability, the interchange document

**Files:** Modify `src/harnesses/*.rs`, `src/policy.rs` (`destination_protocol`, `payload_fits`, `chat_destination_fits` delegate), `src/writer/mod.rs` (`validate_options`, `validate_id_syntax`), `src/hop.rs` (`import`, `hop`, `hop_into`: writer check first; agent/metadata copy removed), `src/cli.rs` (hop and `write_opts` copies removed; `document` command through the capability), `mukade-tui/src/main.rs` (hop dialog lists only writable destinations). Test: `tests/harnesses.rs`, `src/harnesses/mod.rs` unit test.

**Interfaces — Produces (added to `Harness`):**
```rust
fn destination_protocol(&self) -> Option<Protocol>;
fn payload_fits(&self, opaque: &Opaque, provider: Option<&str>) -> bool;  // payload_fits + chat_destination_fits
fn validate_write(&self, opts: &WriteOpts) -> anyhow::Result<()>;       // agent/metadata, dest path, dest id format, store root
fn interchange_document(&self) -> Option<&'static dyn InterchangeDocument>; // OpenCode only
pub fn require_writer(h: &dyn Harness) -> Result<&'static dyn Writer, HopError>; // HopError::Unsupported, exit 2
```
`InterchangeDocument` is a small trait in `src/harnesses/mod.rs` covering what the `document` command needs (the OpenCode tool map rule and `document_from_plan_report`).

- [ ] **Step 1: Failing tests**: `a_destination_without_a_writer_is_refused_before_planning` (unit test in `src/harnesses/mod.rs` with a test adapter whose `writer()` is `None`: `require_writer` gives `HopError::Unsupported`, `exit_code() == 2`); `only_opencode_takes_an_agent_or_metadata` (`validate_write` per adapter with `dest_agent: Some(..)`); `only_opencode_exports_an_interchange_document`.
- [ ] **Step 2: Run** them → FAIL.
- [ ] **Step 3: Implement**; delete the four copies of the agent/metadata rule and the two of the id rule.
- [ ] **Step 4: Run** `cargo test -j 4 --workspace` → all PASS.
- [ ] **Step 5: Commit** `refactor(harnesses): writing rules, writability and the interchange document live in the adapters`.

### Task 4: Destination configuration, store statistics, all branches

**Files:** Modify `src/harnesses/*.rs`, `src/dest.rs` (`configured_model_in` delegates; OpenCode/Codex config readers move to their adapters), `src/cli.rs` (`inspect_store`, OpenCode project id), `src/reader/opencode.rs` (`project_id_for` stays public, the adapter calls it), `src/hop.rs` (`export_all_branches`, `export_path_all_branches`, `all_branches_supported`), `mukade-tui/src/main.rs` (`execute_hop` project id). Test: `tests/harnesses.rs`.

**Interfaces — Produces:**
```rust
fn configured_model(&self, roots: &StoreRoots) -> Option<(String, Option<String>)>;
fn project_id(&self, cwd: &str, roots: &StoreRoots) -> anyhow::Result<Option<String>>; // OpenCode Some, others Ok(None)
fn store_stats(&self, roots: &StoreRoots) -> StoreStats;                                 // pub struct: sessions, forks (Option<usize> each), state note
fn read_all_branches(&self, path: &Path) -> Option<anyhow::Result<Session>>;             // Codex Some
fn summarizes_from_file(&self) -> bool;                                                  // Pi true (the --from-file rule)
```

- [ ] **Step 1: Failing tests**: `only_codex_reads_every_branch`, `only_pi_summarizes_from_a_file`, `store_stats_count_the_fixture_store` (stage each fixture with `support::stage_fixture`; sessions ≥ 1).
- [ ] **Step 2–4:** run → FAIL; implement; `cargo test -j 4 --workspace` → PASS.
- [ ] **Step 5: Commit** `refactor(harnesses): destination configuration, store statistics and branches per adapter`.

### Task 5: Summary by the source's own model

**Files:** Modify `src/summary.rs` (keeps `summarize`, `summarize_in`'s run/timing/cleanup loop, `SummaryCommand`, `Reply`; `summary_command`, `parse_reply` delegate), `src/harnesses/*.rs` (each builds its command line and parses its reply; OpenCode's fork cleanup). Test: existing `tests/summary.rs` is the proof; add `every_adapter_builds_a_summary_command` in `tests/harnesses.rs` (each fixture session → `Ok` with the catalog binary name as program, except Pi without `source_path` → `Unsupported`).

**Interfaces — Produces:**
```rust
fn summary_command(&self, session: &Session, opts: &SummarizeOpts, scratch: &Path) -> Result<SummaryCommand, HopError>;
fn parse_summary_reply(&self, stdout: &str, output_file: Option<&str>) -> Result<Reply, String>;
fn after_summary(&self, command: &SummaryCommand, roots: &StoreRoots, reply: &Reply) -> Option<String>; // OpenCode deletes its fork; others None
```

- [ ] **Steps 1–5** as above. Commit `refactor(harnesses): each adapter runs its own summary`.

### Task 6: Tools toward a destination, plan flags

**Files:** Modify `src/tools.rs` (keeps canonical dispatch, `uses_only`, the L11 safety net, text form, patch reading; the per-destination argument shapes move), `src/harnesses/*.rs` (or `src/harnesses/<h>/tools.rs` when a file passes ~600 lines), `src/writer/plan.rs` (the Pi and Claude/Codex flags). Test: `tests/tools_translation.rs`, `tests/hop.rs`, `tests/writer_plan.rs` unchanged and green.

**Interfaces — Produces:**
```rust
fn offered(&self) -> ToolSet;
fn translate(&self, canonical: &CanonicalTool, source_name: &str, input: &Value, dest_shell: Option<Shell>) -> Translation;
fn accepted_keys(&self, canonical: &CanonicalTool) -> &'static [&'static str];
fn failed_steps_are_replies(&self) -> bool;   // false for Pi
fn flattens_result_blocks(&self) -> bool;     // true for Claude Code and Codex
```

- [ ] **Step 1:** add `translation_is_unchanged_for_every_destination` to `tests/tools_translation.rs`: a fixed list of ~20 calls (one per canonical tool and shape already used in that file) translated toward each of the four destinations; snapshot the result with `assert_eq!` against literals captured **before** the move (write the test first, run it green on the old code — it is a characterization test, the one test here that passes before the change).
- [ ] **Step 2:** move the code; **Step 3:** `cargo test -j 4 --workspace` → PASS; **Step 4: Commit** `refactor(harnesses): tool shapes toward each destination live in its adapter`.

### Task 7: Architecture test and docs

**Files:** Create `tests/architecture.rs`, `docs/architecture.md`. Modify `docs/README.md` (link), `docs/harnesses/*.md` (code references to moved symbols), any remaining `HarnessId::` arm the test finds (TUI `resolve_roots`, `compaction_for`, `location.rs`).

- [ ] **Step 1: Test** `no_harness_branch_outside_its_adapter`: walks `src/` and `mukade-tui/src/`; skips `src/harnesses/`, `src/reader/{claude_code,claude_history,claude_live,codex,codex_state,opencode,pi}*`, `src/reader/codex/`, `src/reader/claude_code/`, `src/writer/{claude_code,codex,opencode,pi}.rs`, `src/detect/{claude_code,codex,opencode,pi}.rs`, `src/model/harness.rs`, and text after a line `#[cfg(test)]`; fails listing every `file:line` with `HarnessId::ClaudeCode|HarnessId::Codex|HarnessId::OpenCode|HarnessId::Pi`; asserts it scanned > 50 files. Allowlist constant, empty at the end, each entry with a reason.
- [ ] **Step 2: Run** → FAIL listing the remaining sites; move each into its adapter; **run** `cargo test -j 4 --workspace` → PASS.
- [ ] **Step 3: Docs**: `docs/architecture.md` — the trait (group table), the registry, what stays generic, "Adding a harness" (the honest list from spec Decision 2, numbered), "Changing a harness" (adapter files + catalog). Fix `docs/harnesses/*.md` references to moved symbols (grep each symbol the spec Inventory lists).
- [ ] **Step 4: Commit** `test(architecture): no harness branch outside its adapter; docs: architecture`.
