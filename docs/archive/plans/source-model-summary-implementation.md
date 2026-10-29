# Source-model summary Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `--compaction summary` makes the source harness's own model write a compact summary of the session, and the destination (or an exported document) starts from that summary instead of the whole history.

**Architecture:** A new module `src/summary.rs` builds and runs one non-saving command per source harness (`exe::run_within`) and parses its reply into a `Summary`. `hop.rs` resolves `SummarySource::SourceModel` into `SummarySource::Generated` before the pure `select`, through a new public `compact`, which `hop`, `hop_session` and the CLI's `dump` share. The manifest records where the summary came from.

**Tech Stack:** Rust (crate as is), `serde_json`, `anyhow`; no new crates. Tests: `cargo test`, fake harness launchers (`tests/support::launcher`).

**Spec:** `docs/archive/design/source-model-summary.md` (and `docs/session-interchange-spec.md` §4 for the portable shape it extends).

## Global Constraints

- The source session is never changed. The one exception is OpenCode: a fork is created and then deleted, and it is reported when it cannot be deleted.
- No new crates. mukade holds no API key and calls no provider: the model is reached only through the source harness's binary.
- Every harness process goes through `exe::run_within` with a deadline. The default summary deadline is **300 s**.
- Temporary files live only under `portable::temp_dir()` (`mukade-data/tmp`), in a per-run directory named `summary-<pid>-<nanos>`. It is removed on success and on failure, and `portable::sweep_stale(dir, "summary-", "", STALE_AFTER)` runs first.
- The summary prompt is **one line**, does not start with `-`, and uses only letters, digits, spaces and `.,;:()`. A Windows `.cmd` launcher (npm installs of Pi and OpenCode) cannot take line breaks, and `"%^&|<>!` are unsafe there.
- The handoff line, verbatim: `[mukade handoff v1] Summary of the earlier conversation, written by the source agent's model when the session was moved:`. `PORTABLE_HANDOFF` stays unchanged.
- Exit codes: `HopError::SummaryFailed` → **4**; a refused combination → `HopError::Unsupported` → **2**; a malformed command line → **1**.
- `--keep-turns` defaults to **0**.
- `cargo test` never reaches a real harness. Live runs (Tasks 1 and 5) are scripts in the scratchpad, never part of the test suite, and each one deletes only the throwaway sessions it created itself.
- Commit straight to `master` and `git push origin master`; no branch, no PR. Run `cargo fmt` before each commit. Every commit message ends with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Code reads like the surrounding code: doc comments that say why, spec section references (`§4.2`), messages that name the command and the reason.

## Review Focus

1. **A session whose working directory no longer exists** (a fixture, or a deleted project): the summarizing run starts from mukade's own directory instead of failing to spawn. Test in Task 3.
2. **A model that answers with an error, or by trying to work instead of summarizing** (Claude `is_error: true` / a `subtype` other than `success`, an empty reply): the result is `SummaryFailed`, never a written handoff. Tests in Task 3.
3. **A session with no recorded model** (`meta.models` empty): no `--model`/`-m` flag is passed, and the harness default applies. OpenCode also gets none when the provider is unknown. Test in Task 3.
4. **A non-ASCII summary** (an Italian conversation, CRLF line ends): the text reaches the handoff intact, trimmed, with no mojibake. Test in Task 3 on the Codex output-file path.
5. **An OpenCode fork that cannot be deleted**: the hop goes on, the receipt's manifest names the fork and the exact delete command. Test in Task 3 (fake launcher), loss line in Task 2.

---

### Task 1: Live probe of the four non-saving commands, recorded in the harness pages

No product code. This task settles the four **[U]** points of the spec, and the argv that Task 3 will pin.

**Files:**
- Create: `docs/verification/source-model-summary-live.md` (the record, in the style of `docs/verification/pi-session-and-reasoning-verification.md`)
- Create: `fixtures/claude_code/summary-print.json` (one real `--output-format json` reply, from the throwaway session's synthetic content)
- Create: `fixtures/opencode/summary-run.jsonl` (the real `run --format json` events of the probe)
- Modify: `docs/harnesses/claude-code.md`, `codex.md`, `opencode.md`, `pi.md`: §5, a new `### Ephemeral runs` subsection right after `### Compaction`; `### Compaction` amended with how to compact non-interactively and what that writes; §9 sources (`--help` of the installed binary, the verification record).
- Probe script: the scratchpad only, not committed.

**Interfaces:**
- Produces: the final argv per harness (a table in the verification record, section "Final commands"). Task 3 pins exactly those. Where the record differs from the candidates below, the record wins.

Candidate commands (from the spec, flags read from `--help` on Claude Code 2.1.312, Codex 0.164.0, OpenCode 2.1.4, Pi 1.4.0):

| harness | candidate | fallback if refused |
|---|---|---|
| Claude Code | `claude -p --resume <id> --no-session-persistence --output-format json --tools "" "<prompt>"` | replace `--tools ""` with `--permission-mode dontAsk` |
| Codex | `codex exec resume <id> --ephemeral --skip-git-repo-check -c sandbox_mode=read-only -o <file> "<prompt>"` | none: record the failure |
| Pi | `pi -p --fork <session file> --session-dir <tmp dir> --no-tools "<prompt>"` | drop `--no-tools`, keep the prompt's instruction |
| OpenCode | `opencode run --standalone --session <id> --fork --format json "<prompt>"`, then `opencode session delete <fork id> --standalone` | a temporary `OPENCODE_DB` holding a mukade-written copy |

- [ ] **Step 1: Make one throwaway source session per harness, with at least one tool call**

  In a scratch project folder holding one `README.md`, run each harness with a synthetic prompt that makes it read a file, for example `Read README.md and tell me its first heading`. Record each new session id and file.

- [ ] **Step 2: Hash each throwaway session, run the candidate, hash again**

  For Claude, Codex and Pi, the SHA-256 of the session file before and after. For OpenCode, `opencode session export <id> --standalone` before and after, plus the `session_v2` rows the fork leaves after `session delete`. Expected: identical hashes, no row left. Record the actual results.

- [ ] **Step 3: Record what each run printed or wrote**

  Check that the provider accepted the run with tools removed, where the text came out (stdout, JSON field, `-o` file), the token or cost fields, and how long it took. Save the Claude JSON and the OpenCode events as the two fixtures above. If a candidate is refused, run its fallback and record both.

- [ ] **Step 4: Probe the two edge cases**

  A Codex session whose last turn was interrupted; a session too large for the model's window, or the closest that can be made cheaply (record honestly if it was not made). Record whether the harness fails, compacts in memory, or answers.

- [ ] **Step 5: Delete the throwaway sessions (by recorded id only), write the record and the harness pages**

  Each harness page's `### Ephemeral runs`: the flag(s) that run without saving, which modes they work in (print/exec/run), what they still write, whether they combine with resume, and **[V]** (seen in this probe) or **[U]** marks with the record as source. `### Compaction`: how to compact without the TUI and what that writes.

- [ ] **Step 6: Commit**

```bash
git add docs/verification/source-model-summary-live.md docs/harnesses fixtures/claude_code/summary-print.json fixtures/opencode/summary-run.jsonl
git commit -m "Verify live how each harness runs without saving, and record it per harness"
git push origin master
```

---

### Task 2: A generated summary in the library: types, `compact`, manifest, error

**Files:**
- Modify: `src/hop.rs` (`SummarySource`, `Compaction::name`, `ImportOpts`, `HopError`, `select`, `hop`, `hop_session`, `import`, `manifest_of`, `extra_losses`)
- Modify: `src/manifest.rs` (`Manifest.summary`, `to_json`)
- Create: `src/summary.rs` with **only the types** of this task (the runner comes in Task 3); `src/lib.rs` adds `pub mod summary;`
- Test: `tests/hop.rs`

**Interfaces:**
- Produces, in `src/summary.rs`:
  - `pub const MODEL_SUMMARY_HANDOFF: &str` (the verbatim line above)
  - `#[derive(Debug, Clone, PartialEq, Eq)] pub struct SummarizeOpts { pub model: Option<String>, pub timeout: Duration, pub exe: Option<String> }`, with `Default` (timeout 300 s)
  - `#[derive(Debug, Clone, PartialEq, Eq)] pub struct Summary { pub text: String, pub harness: HarnessId, pub harness_version: Option<String>, pub model: Option<String>, pub usage: Option<String> /* compact JSON as the harness reported it */, pub leftover: Option<String> /* the command that removes a fork left behind */ }`
  - `#[derive(Debug, Clone, PartialEq, Eq)] pub struct SummaryFacts { pub summary: Summary, pub left_out: usize }`
- Produces, in `src/hop.rs`:
  - `SummarySource::SourceModel(SummarizeOpts)` and `SummarySource::Generated(Summary)`
  - `Compaction::name()` returns `"summary"` for `Portable { summary: SourceModel(_) | Generated(_), .. }`
  - `ImportOpts.summary: Option<SummaryFacts>`; every existing struct literal sets `summary: None`
  - `HopError::SummaryFailed(String)`, exit code 4, `Display` = `summary failed: {reason}`
  - `pub fn compact<'a>(source: &'a Session, compaction: &Compaction, roots: &StoreRoots) -> Result<(Cow<'a, Session>, Option<SummaryFacts>), HopError>`. It resolves `SourceModel` through `crate::summary::summarize` (Task 3; until then this arm returns `Unsupported`), then calls `select`. `left_out = source.entries.len() + 1 - selection.entries.len()`.
- Produces, in `src/manifest.rs`: `Manifest.summary: Option<SummaryFacts>`; `options.summary` in JSON is `null`, or `{source: "source_model", harness, version, model, prompt: "v1", chars, usage /* parsed back to JSON, or null */, left_out}`.

- [ ] **Step 1: Write the failing tests in `tests/hop.rs`**

```rust
fn generated(text: &str) -> hop::SummarySource {
    hop::SummarySource::Generated(summary::Summary {
        text: text.into(), harness: HarnessId::Pi, harness_version: Some("1.4.0".into()),
        model: Some("anthropic/claude-x".into()), usage: None, leftover: None,
    })
}

#[test]
fn a_model_summary_replaces_the_whole_history() {
    let source = support::read_source(HarnessId::Pi);
    let compaction = Compaction::Portable { summary: generated("S"), keep_from: KeepFrom::LastTurns(0) };
    let (selection, facts) = hop::compact(&source, &compaction, &StoreRoots::default()).unwrap();
    assert_eq!(selection.entries.len(), 1);
    assert!(matches!(&selection.entries[0].kind,
        Kind::Compaction { summary, .. } if *summary == format!("{}\nS", summary::MODEL_SUMMARY_HANDOFF)));
    assert_eq!(facts.unwrap().left_out, source.entries.len());
}

#[test]
fn a_model_summary_can_keep_the_last_turns() {
    // KeepFrom::LastTurns(2): handoff, then exactly the last two user turns and what follows them.
    // assert: entries[0] is the handoff; the tail's Kind::User count == 2; the tail equals source.entries[cut..]
}

#[test]
fn a_model_summary_hop_says_where_the_summary_came_from() {
    // hop_session(Pi -> ClaudeCode) with Generated("S"), LastTurns(0):
    // manifest["options"]["compaction"] == "summary"; ["options"]["summary"]["harness"] == "pi";
    // ["options"]["summary"]["prompt"] == "v1"; a loss contains "travelled as a summary written by pi's model";
    // no loss contains "the source's own compaction is in the session" or "the source's compaction summary travels".
}

#[test]
fn a_failed_summary_is_a_failure_where_nothing_changed() {
    assert_eq!(HopError::SummaryFailed("x".into()).exit_code(), 4);
}
```

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test --test hop model_summary`
Expected: compile errors (`summary` module, `compact`, `Generated` not defined).

- [ ] **Step 3: Implement the types, `compact`, the `select` arm and the manifest**

  - In `select`, `Generated(s)` uses `MODEL_SUMMARY_HANDOFF` and `s.text`, with `Trigger::Unknown` and the same `carried = None` as `Supplied`. `SourceModel` reaching `select` is `Unsupported` ("resolve it with hop::compact").
  - `hop` and `hop_session` call `compact` and set `opts.summary` (`hop_session` clones the opts it is given).
  - In `import`, when `opts.summary` is set: pass `compaction_carried = false` to `Manifest::losses_for`. `extra_losses` takes `Option<&SummaryFacts>`; when set, instead of its "source's own compaction" line it pushes `the history before {kept_from} ({left_out} entries) travelled as a summary written by {harness}'s model` (with no tail: `the whole history ({left_out} entries) travelled as …`), plus `the summarizing run left a fork in the source store: run {leftover}` when `leftover` is set.

- [ ] **Step 4: Run the whole suite**

Run: `cargo test`
Expected: PASS, the existing portable tests unchanged.

- [ ] **Step 5: Commit**

```bash
cargo fmt && git add src/hop.rs src/manifest.rs src/summary.rs src/lib.rs tests/hop.rs
git commit -m "Carry a summary written by the source's model, and say so in the manifest"
git push origin master
```

---

### Task 3: Run the source harness for the summary

**Files:**
- Modify: `src/summary.rs`
- Modify: `src/hop.rs` (the `SourceModel` arm of `compact` calls `summarize`)
- Create: `tests/summary.rs`

**Interfaces:**
- Consumes: `SummarizeOpts`, `Summary` (Task 2); `exe::resolve`, `exe::spawnable`, `exe::run_within`, `exe::version_of`; `StoreRoots::apply_to`; `portable::temp_dir`, `portable::sweep_stale`; Task 1's "Final commands" table.
- Produces:
  - `pub const SUMMARY_PROMPT_VERSION: &str = "v1";` and `pub const SUMMARY_PROMPT: &str`, the exact text:
    `Write a handoff summary of this whole session for another coding agent that will continue the work from your summary alone. Do not call tools and do not continue the task: reply only with the summary, in the language the user wrote in. Cover: (1) the goal and what the user asked for, quoting the user where wording matters; (2) the current state, what is done and what is half done; (3) the decisions taken and why, and the approaches tried and dropped; (4) the files read or changed and how, and the commands and tests run with their results; (5) open problems, unanswered questions, and the next step. Keep it as short as it can be while a new agent could continue the work from it alone.`
  - `pub struct SummaryCommand { pub program: String, pub args: Vec<String>, pub cwd: Option<PathBuf>, pub output_file: Option<PathBuf> }`
  - `pub fn summary_command(session: &Session, opts: &SummarizeOpts, scratch: &Path) -> Result<SummaryCommand, HopError>`: pure. The prompt is the last argument. `cwd` is `meta.cwd` only when that directory exists. The model is `opts.model`, else `meta.models.last()`: the id for Claude and Codex, `provider/id` (or the bare id) for Pi, `provider/id` for OpenCode only when the provider is known. Pi without `meta.source_path` → `Unsupported`.
  - `pub struct Reply { pub text: String, pub usage: Option<String>, pub fork: Option<String> }`
  - `pub fn parse_reply(harness: HarnessId, stdout: &str, output_file: Option<&str>) -> Result<Reply, String>`. Claude: the JSON's `result`, an error when `is_error` is true or `subtype` is not `"success"`, `usage` = `{total_cost_usd, usage, modelUsage}`. Codex: the output file. Pi: stdout. OpenCode: the assistant text and the fork's session id from the events (the shape of `fixtures/opencode/summary-run.jsonl`). The text is trimmed; empty → `Err("the model returned no summary")`.
  - `pub fn summarize(session: &Session, roots: &StoreRoots, opts: &SummarizeOpts) -> Result<Summary, HopError>`. It runs `summary_command` under `run_within(opts.timeout)` with `roots.apply_to(harness, …)`. For OpenCode it then runs `session delete <fork> --standalone` (30 s) and sets `leftover` when that fails. The scratch directory is always removed. Failures → `SummaryFailed(reason)`, where the reason names the command with the prompt shown as `<summary prompt v1>`, the exit status or the deadline, and stderr.

- [ ] **Step 1: Write the failing tests in `tests/summary.rs`**

```rust
#[test]
fn the_prompt_is_one_line_a_cmd_launcher_can_take() {
    let p = summary::SUMMARY_PROMPT;
    assert!(!p.starts_with('-') && !p.contains('\n') && !p.contains('\r'));
    assert!(p.chars().all(|c| c.is_ascii_alphanumeric() || " .,;:()".contains(c)), "{p}");
}

#[test]
fn each_harness_gets_its_non_saving_command() {
    // For each HarnessId: summary_command(support::read_source(h), &SummarizeOpts::default(), scratch)
    // assert args == Task 1's final argv for h (session id / file / scratch paths substituted), prompt last.
}

#[test]
fn the_sessions_own_model_is_used_unless_one_is_named() {
    // meta.models = [ModelRef{provider: Some("anthropic"), id: "m1"}]:
    //   Claude has ["--model","m1"], Pi ["--model","anthropic/m1"], OpenCode ["-m","anthropic/m1"].
    // opts.model = Some("cheap") wins. meta.models empty: no --model / -m at all.
    // OpenCode with provider None: no -m.
}

#[test]
fn a_session_whose_directory_is_gone_runs_from_mukades_own() { /* meta.cwd = a missing dir → cmd.cwd == None */ }

#[test]
fn replies_are_read_where_each_harness_puts_them() {
    // Claude: fixtures/claude_code/summary-print.json → its result text, usage Some.
    // Claude {"type":"result","subtype":"error_max_turns","is_error":true,...} → Err.
    // Codex: output file "Riassunto: è pronto.\r\n" → "Riassunto: è pronto.".
    // OpenCode: fixtures/opencode/summary-run.jsonl → text non-empty, fork == the fork's id.
    // Any harness, blank text → Err("the model returned no summary").
}

#[test]
fn a_fake_pi_summarizes_and_leaves_nothing_behind() {
    // support::launcher(bin, "pi", "echo The summary.", "echo The summary.");
    // summarize(Pi fixture, roots, opts{exe: launcher}) → text "The summary.";
    // no "summary-*" directory left under portable::temp_dir(); the fixture file's bytes unchanged.
}

#[test]
fn a_summary_that_fails_writes_nothing_and_says_why() {
    // launcher "exit /b 3" / "exit 3" → SummaryFailed, reason contains "<summary prompt v1>" and "3".
    // launcher that echoes nothing → SummaryFailed("…no summary").
    // launcher that sleeps past opts.timeout = 500 ms → SummaryFailed naming the deadline.
}

#[test]
fn an_opencode_fork_that_cannot_be_deleted_is_named() {
    // launcher: `if "%1"=="session" exit /b 1` then type the fixture events; unix: `[ "$1" = session ] && exit 1; cat <fixture>`
    // summarize → Ok, leftover == Some("opencode session delete <fork id> --standalone").
}
```

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test --test summary`
Expected: compile errors (`summary_command`, `parse_reply`, `summarize` not defined).

- [ ] **Step 3: Implement `SUMMARY_PROMPT`, `summary_command`, `parse_reply`, `summarize`; wire the `SourceModel` arm of `hop::compact`**

- [ ] **Step 4: Run the whole suite**

Run: `cargo test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cargo fmt && git add src/summary.rs src/hop.rs tests/summary.rs
git commit -m "Ask the source harness's own model for the summary, saving nothing in its store"
git push origin master
```

---

### Task 4: `--compaction summary` on the command line, for `hop` and `dump`

**Files:**
- Modify: `src/cli.rs` (`USAGE`, `Flags` + parsing, a shared `Flags::compaction`, `dump`, `hop`)
- Test: `tests/cli.rs`

**Interfaces:**
- Consumes: `hop::compact`, `hop::hop_session`, `SummarySource::SourceModel`, `SummarizeOpts`, `KeepFrom::LastTurns`.
- Produces:
  - Flags `--keep-turns <n>`, `--summary-model <id>`, `--summary-timeout <seconds>`; `--compaction none | portable | summary` for both `hop` and `dump`.
  - `fn compaction(&self, source: HarnessId) -> Result<Compaction>` on `Flags`. `summary` → `Portable { summary: SourceModel(SummarizeOpts { model, timeout, exe: self.exe_flag(source) }), keep_from: LastTurns(keep_turns.unwrap_or(0)) }`. Any of the three summary flags without `summary` → a command-line error (exit 1).
  - Refused before anything runs (`HopError::Unsupported`, exit 2): `summary` with `--all-branches`; `summary` with `--from-file` when the source is not Pi.
  - `dump` writes `wire::to_document(&selection)` of `hop::compact(...)`; `write` is unchanged.

- [ ] **Step 1: Write the failing tests in `tests/cli.rs`**

```rust
#[test]
fn dump_exports_a_model_summary_and_write_imports_it() {
    // stage Pi fixture; fake pi = launcher "echo The summary." in scratch/bin
    // dump pi <id> --compaction summary --pi-exe <fake> --output doc.json  → exit 0
    // doc["messages"].len() == 1, its text starts with summary::MODEL_SUMMARY_HANDOFF and contains "The summary."
    // write claude_code doc.json --dest-version 2.1.312 --cwd <work> → exit 0, the written file exists
}

#[test]
fn hop_with_a_model_summary_keeps_the_turns_asked_for() {
    // hop pi <id> claude_code --compaction summary --keep-turns 1 --pi-exe <fake> ... → exit 0
    // printed["manifest"]["options"]["compaction"] == "summary"; ["options"]["summary"]["harness"] == "pi";
    // the destination holds the handoff, then exactly one source user turn
}

#[test]
fn summary_flags_and_combinations_that_cannot_work_are_refused() {
    // --keep-turns 1 alone → exit 1; --summary-model x alone → exit 1
    // --compaction summary --all-branches (codex) → exit 2
    // --compaction summary --from-file <claude fixture> → exit 2
    // --compaction bogus → exit 1, stderr lists "none, portable, summary"
}

#[test]
fn a_summary_that_fails_writes_nothing() {
    // fake pi "exit /b 3" → hop exit 4; the destination home holds no new session file
}
```

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test --test cli summary`
Expected: FAIL (`unknown --compaction summary`).

- [ ] **Step 3: Implement the flags, `Flags::compaction`, the refusals, `dump` through `hop::compact`, and the `USAGE` lines**

  The `USAGE` lines: `--compaction <mode>      none (default) | portable (the source's own summary, then its tail) | summary (the source's model writes one now)`, `--keep-turns <n>         summary: also keep the last n user turns verbatim (default 0)`, `--summary-model <id>     summary: the model to ask (default: the session's own)`, `--summary-timeout <s>    summary: deadline of the summarizing run (default 300)`.

- [ ] **Step 4: Run the whole suite**

Run: `cargo test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cargo fmt && git add src/cli.rs tests/cli.rs
git commit -m "Let hop and dump carry a summary written by the source's model"
git push origin master
```

---

### Task 5: End-to-end live run, and the documentation

**Files:**
- Modify: `docs/verification/source-model-summary-live.md` (a section "End to end")
- Modify: `docs/harnesses/*.md` §8: a `### How mukade summarizes it` subsection (the exact command from `src/summary.rs`, what it saves, what it leaves behind, the output it reads)
- Modify: `docs/session-interchange-spec.md` (new §4.5 "`Portable` with a summary from the source's model", §1.2 surface, §10 open items that are still open)
- Modify: `README.md` ("Compaction and reasoning between hops"), `docs/usage.md` (one use case: export a summary, import it later)
- Modify: `docs/archive/design/source-model-summary.md` (Status: built, with the commit)

- [ ] **Step 1: Run a real hop per source harness with `--compaction summary` on a throwaway session**

  Build with `cargo build --release`. For each source, run `mukade hop <src> <id> <another harness> --compaction summary --resume-prompt "What were we doing"`. Record: the source's hash unchanged, the summary text length, the destination's reply (it answers from the summary), and the time. Delete the throwaway sessions by recorded id.

- [ ] **Step 2: Write the documentation listed above, from the record and the code**

- [ ] **Step 3: Check that the docs and the code agree**

Run: `cargo test && grep -n "summary" README.md docs/usage.md docs/harnesses/*.md | head -40`
Expected: PASS. Every flag named in the docs exists in `USAGE`, and every harness page names the same command as `summary_command`.

- [ ] **Step 4: Commit**

```bash
git add docs README.md
git commit -m "Document the summary written by the source's model, per harness and end to end"
git push origin master
```
