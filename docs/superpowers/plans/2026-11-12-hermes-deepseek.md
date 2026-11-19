# Hermes Agent and DeepSeek Harness — implementation plan

Spec: `docs/superpowers/specs/2026-11-12-hermes-deepseek-design.md`. Research with file:line
evidence: `D:\Projects\horness-launcher\sidecar\runs\musashi-terminal-4444-2379968-0-134389045251963263-0\tmp\claude\c56a38692edb5f5e\40865358-8ab0-4903-8cee-f73bbc95729b\scratchpad\hermes-deepseek-research.md`.
Branch `hermes-deepseek` (base `4dfdd37`, on the `subagents` branch: `ParentKind` exists).
TDD every task. Global: public API additions only; the one new crate allowed is `ruzstd` (pure Rust;
pin an exact version); tests/architecture.rs green; `cargo test -j 4 --workspace` and
`cargo clippy -j 4 --workspace --all-targets` (no new warnings) before each commit; repository
commit trailer. Tests never read the real stores except `#[ignore]` live tests that say so.
Live model calls only with the free OpenRouter route described in the spec (never paid models).

### Task 1: Two new harness ids, catalog, detection, adapters
**Files:** `src/model/harness.rs`, `catalog/harnesses.json`, `src/roots.rs` (`StoreRoots` fields),
`src/harnesses/{mod.rs,hermes.rs,deepseek_harness.rs}`, `src/detect/{hermes.rs,deepseek_harness.rs}`
+ `layout.rs`, `src/cli.rs` (usage, exe flags), tests (`tests/harnesses.rs` loops over `ALL`).
- [ ] Follow docs/architecture.md "Adding a harness" step by step; note every step the doc misses
      and fix the doc in Task 6.
- [ ] Readers/writers return a clear "not yet" until Tasks 2–5 (no panic).
- [ ] Commit `feat(harnesses): Hermes Agent and DeepSeek Harness ids, catalog and detection`.

### Task 2: Hermes reader
**Files:** `src/reader/hermes.rs`, adapter, test fixture builder (creates a `state.db` from the
v0.23.1 schema subset), `tests/hermes_reader.rs`, live census `tests/hermes_live.rs` (`#[ignore]`).
- [ ] list / list_all / read per spec §2 (gate, sources, children/branches, compaction, content
      shapes, reasoning, tool pairing, legacy chains).
- [ ] Live: every session in this machine's store reads without error; counts printed.
- [ ] Commit `feat(hermes): read Hermes Agent sessions`.

### Task 3: DeepSeek reader
**Files:** `Cargo.toml` (`ruzstd`), `src/reader/deepseek_harness.rs` (+ `zstd_frames.rs`), adapter,
fixtures under `fixtures/deepseek_harness/`, tests, live census (`#[ignore]`).
- [ ] Frame scanner + checksum + torn tail; plain `.jsonl`; header gate; surface rebuild; prompt
      kinds; tool pairing; children/forks; unknown non-ignorable type → refusal.
- [ ] Commit `feat(dsh): read DeepSeek Harness sessions`.

### Task 4: DeepSeek writer
**Files:** `src/writer/deepseek_harness.rs`, adapter `Destination` (offered tools, translate,
validate_write), tests; verification record `docs/verification/2026-11-12-dsh-writer-live.md`.
- [ ] Write v4 zstd (or plain if dsh accepts it in a zstd root — prove which) under the right
      folder names (port `projectKey`/`encodeSegment`); read back with Task 3's reader.
- [ ] Live acceptance: hop a Claude fixture into dsh, then `dsh --profile headless --patch <openrouter
      patch> --session-id <id> "Reply with OK"` resumes it; record the command and output.
- [ ] Commit `feat(dsh): write DeepSeek Harness sessions`.

### Task 5: Hermes writer through `hermes sessions import`
**Files:** `src/writer/hermes.rs`, adapter `Destination`, tests (unit: the intermediate Claude file
has every call in text form; the import output parser), live round trip (`#[ignore]`), verification
record `docs/verification/2026-11-12-hermes-writer-live.md`.
- [ ] Commit `feat(hermes): write Hermes sessions through its own import`.

### Task 6: Summaries, CLI/TUI, docs
- [ ] `--compaction summary` from each source: implement if a no-tool run that leaves no session (or
      one mukade deletes) is possible, verified live with the free route; else a clear refusal.
- [ ] TUI lists both harnesses (it loops over the registry; check), hop dialog offers them.
- [ ] Docs: rewrite `docs/harnesses/hermes-agent.md` and `deepseek-harness.md` to the harness-page
      skeleton with verified facts (research §4 corrections); amend ADR-0002 (Accepted, with
      changes); `docs/usage.md`, `docs/reference/harness-catalog.md`, `docs/tool-conversion-tables.md`
      (dsh and Hermes tool names), `docs/architecture.md` (Adding a harness: the missed steps),
      `README.md`/`docs/README.md` (six harnesses), `next.md`. Style: clear, complete, not wordy;
      tables; one home per fact.
- [ ] Commit `docs: Hermes Agent and DeepSeek Harness supported`.
