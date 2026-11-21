# Hosted Chats, Phase 1 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** mukade lists the ChatGPT-hosted chats of the Codex desktop app that have a local folder, shows the git work in a session's folder, and starts a new Claude Code or Codex CLI session there with a handoff note.

**Architecture:** A new read-only module beside `reader/codex_state.rs` reads the app's catalogue and the Codex reader merges its rows into the listing as unreadable sessions. Two harness-neutral library modules, `work` (git state of a folder) and `handoff` (note, file, start command), are used by two new CLI commands and by the desktop app. No new `HarnessId`.

**Tech Stack:** Rust (library `mukade`, `rusqlite` bundled, the installed `git`), Tauri 2 (`apps/desktop/src-tauri`), TypeScript with Imago controls (`apps/desktop/src`).

**Spec:** `docs/superpowers/specs/2026-11-21-chatgpt-hosted-chats-design.md` (phase 1 only; phase 2 is planned when the owner's ChatGPT export has arrived).

## Global Constraints

- **Never run `cargo`, `tauri dev` or `tauri build` on this machine** (`cargo fmt` is the only exception). Rust is compiled and tested by GitHub Actions only. "Run the tests" in this plan means: commit, push the branch, `gh run watch`, and on failure `gh run view --log-failed`.
- Work on the short branch `feat/hosted-chats` from `develop`, with a pull request into `develop` (the `Verify desktop` workflow runs on it). Merge into `develop` at the end and delete the branch, locally and on GitHub. Never touch `main`, versions or tags.
- CI runs `cargo test --locked -p mukade --lib --test chat_connect` and `cargo test --locked -p mukade-desktop`, on Ubuntu and macOS, and `npm run build && npm test && npm run smoke`. A Rust test outside the library's unit tests and the desktop crate is not run: every Rust test of this plan is a `#[cfg(test)]` unit test.
- `npm run build`, `npm test` and `npm run smoke` may be run locally.
- Nothing outside a harness's adapter files names a `HarnessId` variant (`tests/architecture.rs`). The new modules `work` and `handoff` take the harness as a value and never match on it.
- Read-only everywhere: SQLite opened with `SQLITE_OPEN_READ_ONLY`; every `git` process gets `GIT_OPTIONAL_LOCKS=0` and no command that changes a repository, and none that fetches.
- `auth.json` and any other credential file of Codex are never opened. No network request.
- Dependencies: none added.
- Desktop UI: read `node_modules/@acaprino/imago/docs/design-file.md` and use Imago's controls, behaviours and tokens.
- Exact copy, from the spec:
  - unreadable reason: `hosted by ChatGPT: the conversation is not on this machine; the work it left is in its folder`
  - first prompt: `Read <path> and continue from it.`
  - note file: `<mukade-data>/handoffs/<harness>-<id>.md`
  - template folder: `handoff-prompts`, file `continue.md`

## Review Focus

1. **A prompt with spaces through `wt new-tab -- <exe> <prompt>` on Windows.** The owner expects the harness to receive the whole one-line prompt as one argument. CI has no Windows job: Task 6 keeps the argv a plain `Vec<String>` and Task 9 records a live check on the owner's machine with a released build.
2. **A folder that no longer exists** (the project folder was moved or deleted). The work view must say so, not fail the detail panel: `work_in_a_missing_folder_says_so` (Task 3).
3. **A repository `git` refuses** (dubious ownership, a broken `.git`). One repository's error must not hide the others: `a_repository_git_refuses_keeps_its_error_and_the_others` (Task 3).
4. **A thread id with characters unfit for a file name** in the note's path: `the_note_file_name_keeps_only_safe_characters` (Task 4).
5. **The app writing its catalogue while mukade reads it** (WAL, a lock). A busy database must give no hosted chats and leave the Codex listing as it was: `a_locked_or_foreign_database_gives_no_hosted_chats` (Task 1).

---

### Task 1: The hosted-chat catalogue reader

**Files:**
- Create: `src/reader/codex_hosted.rs`
- Modify: `src/reader/mod.rs` (add `mod codex_hosted;` beside `mod codex_state;`)

**Interfaces:**
- Produces:

```rust
/// The reason a hosted chat cannot be read.
pub(crate) const HOSTED: &str = "hosted by ChatGPT: the conversation is not on this machine; the work it left is in its folder";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HostedChat {
    pub id: String,
    pub title: Option<String>,
    /// RFC 3339, from `source_updated_at` (epoch seconds, fractional).
    pub updated: Option<String>,
    /// The project's root at position 0.
    pub folder: PathBuf,
    /// `<home>/sqlite/codex.db`.
    pub database: PathBuf,
}

/// The hosted chats of a Codex home that have a local folder. Empty when there is nothing to read.
pub(crate) fn hosted_chats(home: &Path) -> Vec<HostedChat>;
```

The queries, both `SQLITE_OPEN_READ_ONLY | SQLITE_OPEN_NO_MUTEX` with a 2 s busy timeout, as `codex_state::read_threads` opens its database:

- `<home>/sqlite/codex.db`: rows of `local_thread_catalog` joined to `local_thread_catalog_hosts` on `host_id` where `host_kind = 'chatgpt'` and `project_id IS NOT NULL`. Columns needed: `host_id`, `thread_id`, `display_title`, `source_updated_at`, `project_id`; hosts: `host_id`, `host_kind`.
- the newest `<home>/state_<n>.sqlite` (make `codex_state::newest_database` `pub(super)` and reuse it): `projects(id, metadata)` and `project_roots(project_id, position, path)`. A project's ChatGPT id is `metadata["codex.chatGptBacking"]`, a JSON **string** that is parsed again, then its `projectId`.

A needed column that is absent, a database that cannot be opened, or metadata that does not parse gives no rows for that part; nothing panics and nothing is returned as an error. The table definitions are in the spec's appendix.

- [ ] **Step 1: Write the failing tests** in `src/reader/codex_hosted.rs`, `#[cfg(test)] mod tests`, building the two databases with `rusqlite` in `crate::detect::tests::scratch("hosted")` from the appendix's `CREATE TABLE` statements:

```rust
#[test] fn a_hosted_chat_of_a_project_with_a_folder_is_listed() {
    // catalogue row: thread "t1", title "Capire lo stack di Daodan", source_updated_at 1795254778.231,
    // project_id "g-p-abc"; project metadata backing "g-p-abc"; root position 0 = <scratch>/Daodan
    let chats = hosted_chats(&home);
    assert_eq!(chats.len(), 1);
    assert_eq!(chats[0].id, "t1");
    assert_eq!(chats[0].title.as_deref(), Some("Capire lo stack di Daodan"));
    assert_eq!(chats[0].folder, scratch.join("Daodan"));
    assert_eq!(chats[0].updated.as_deref(), Some("2026-11-21T03:12:58.231+00:00"));
    assert_eq!(chats[0].database, home.join("sqlite").join("codex.db"));
}
#[test] fn a_chat_without_a_project_or_whose_project_has_no_root_is_left_out() { /* two rows → empty */ }
#[test] fn a_row_of_another_host_kind_is_left_out() { /* host_kind 'local' → empty */ }
#[test] fn the_root_at_position_zero_is_the_folder() { /* roots at 1 and 0 → the one at 0 */ }
#[test] fn a_missing_candidate_row_is_listed_like_the_others() { /* missing_candidate = 1 → listed */ }
#[test] fn a_locked_or_foreign_database_gives_no_hosted_chats() {
    // (a) no sqlite/ folder; (b) codex.db is a text file; (c) local_thread_catalog lacks project_id;
    // (d) projects.metadata is "not json": each → hosted_chats(&home).is_empty()
}
#[test] fn reading_leaves_both_databases_byte_identical() { /* bytes before == bytes after */ }
```

Compute the expected `updated` string from `1795254778.231` with `chrono` in the test rather than trusting the literal above.

- [ ] **Step 2: Implement `hosted_chats`** and the `mod` line. `updated`: `chrono::DateTime::from_timestamp_millis((seconds * 1000.0).round() as i64)` then `to_rfc3339()`, as `codex_state` formats its times.

- [ ] **Step 3: Format, commit, push, read the run**

```bash
cargo fmt
git checkout -b feat/hosted-chats
git add src/reader/codex_hosted.rs src/reader/mod.rs src/reader/codex_state.rs
git commit -m "Read the Codex app's catalogue of ChatGPT-hosted chats"
git push -u origin feat/hosted-chats
gh pr create --base develop --title "List ChatGPT-hosted chats and hand their work off" --body "Phase 1 of docs/superpowers/specs/2026-11-21-chatgpt-hosted-chats-design.md"
gh run watch
```

Expected: `Verify desktop` green; the seven tests appear in the `cargo test -p mukade --lib` step.

---

### Task 2: Hosted chats in the Codex listing, refused on read

**Files:**
- Modify: `src/reader/codex.rs` (`list_in`, about line 1311; the read by id that refuses a gone rollout, about line 1519)
- Test: `src/reader/codex_hosted.rs` (same test module; `src/reader/codex.rs` has no unit tests and its integration tests are not run by CI)

**Interfaces:**
- Consumes: `codex_hosted::{hosted_chats, HostedChat, HOSTED}`.
- Produces: no new name. `Reader::list`, `list_all` and `overview` of the Codex reader return, for each hosted chat whose id is not already listed, `SessionInfo { id, title, cwd: Some(folder), updated, path: database, unreadable: Some(HOSTED.into()), ..Default::default() }`. `Reader::read` (and the clipped and streamed reads, which go through the same lookup) of a hosted id fails with an error whose text contains `HOSTED`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn the_codex_listing_holds_the_hosted_chat_as_unreadable() {
    let roots = StoreRoots { codex_home: Some(home.clone()), ..Default::default() };
    let listed = crate::reader::codex::Codex.list(&roots).unwrap();
    let chat = listed.iter().find(|info| info.id == "t1").unwrap();
    assert_eq!(chat.cwd.as_deref(), Some(folder.to_string_lossy().as_ref()));
    assert_eq!(chat.unreadable.as_deref(), Some(HOSTED));
    assert_eq!(crate::reader::codex::Codex.overview("t1", &roots).unwrap().id, "t1");
}
#[test] fn a_thread_codex_already_lists_is_not_added_twice() {
    // state db `threads` row with id "t1" (rollout gone) and a hosted row "t1" → exactly one "t1",
    // with the reason of the threads row
}
#[test] fn reading_a_hosted_chat_is_refused_with_the_reason() {
    let error = crate::reader::codex::Codex.read("t1", &roots).unwrap_err();
    assert!(format!("{error:#}").contains(HOSTED));
}
```

Use the Codex reader's real unit-struct name (the `impl Reader for …` at `src/reader/codex.rs:1275`).

- [ ] **Step 2: Implement.** In `list_in`, after the `threads` rows are inserted and before the sort: for each `hosted_chats(home)` not in `listed` and matching `only` when it is set, insert the `SessionInfo`. In the read-by-id lookup, when no rollout and no `threads` row match, check `hosted_chats(home)` for the id and return the refusal built the way the gone-rollout one is, with `HOSTED` as the reason.

- [ ] **Step 3: Format, commit, push, read the run**

```bash
cargo fmt
git add -A src/reader && git commit -m "List ChatGPT-hosted chats with a local folder among Codex's sessions" && git push && gh run watch
```

Expected: green; three more tests pass.

---

### Task 3: The work on disk

**Files:**
- Create: `src/work.rs`
- Modify: `src/lib.rs` (`pub mod work;`)

**Interfaces:**
- Produces:

```rust
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Work {
    pub folder: PathBuf,
    pub repositories: Vec<Repository>,
    /// Why there is nothing to show: the folder is missing, git is not installed, no repository.
    pub note: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Repository {
    pub path: PathBuf,
    pub worktrees: Vec<Worktree>,
    pub branches: Vec<Branch>,
    /// What git answered when it refused this repository; the lists are then empty.
    pub error: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Worktree {
    pub path: PathBuf,
    /// `None`: a detached head.
    pub branch: Option<String>,
    /// The short commit id.
    pub head: String,
    /// Tracked changes and untracked files; ignored files are not counted.
    pub uncommitted: usize,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Branch {
    pub name: String,
    pub upstream: Option<String>,
    /// Commits ahead of the upstream; `None` without one.
    pub ahead: Option<usize>,
    /// Commits reachable from the branch and from no remote-tracking ref.
    pub unpushed: usize,
}
impl Repository {
    /// Whether work here exists nowhere else: uncommitted paths, or a branch with unpushed commits.
    pub fn at_risk(&self) -> bool;
}
/// The repositories of `folder`: itself when it is one, else each direct child that is, by name.
pub fn work_in(folder: &Path) -> Work;
/// One line per repository and per branch or worktree that has something to say, for a terminal
/// and for the handoff note.
pub fn lines(work: &Work) -> Vec<String>;
```

Git commands, each with `GIT_OPTIONAL_LOCKS=0`, `-C <path>`, and `crate::exe::run_within(command, 10 s)`:

| fact | command |
|---|---|
| is a repository (a folder holding `.git`, file or folder) | no process: `path.join(".git").exists()` |
| worktrees | `worktree list --porcelain` |
| uncommitted | `status --porcelain=v1 -z --untracked-files=all --no-renames` in each worktree, entries counted (`--no-renames`: with `-z` a rename would be two entries) |
| branches, upstreams | `for-each-ref --format=%(refname:short)%09%(upstream:short) refs/heads` |
| ahead | `rev-list --count <upstream>..<branch>` |
| unpushed | `rev-list --count <branch> --not --remotes` |

`lines` format, pinned by the tests: `daodan: branch develop, upstream origin/develop, in sync, 1 uncommitted path`; `stack-analyser: branch feature/stack-detection, no upstream, 3 commits on no remote, 133 uncommitted paths`; a clean repository in sync: `imago: branch develop, upstream origin/develop, clean`. Singular and plural as shown. A last line when any repository was read: `(remote state as of the last fetch)`.

- [ ] **Step 1: Write the failing tests** in `src/work.rs`. A helper `git(dir, args)` runs git with `-c user.name=t -c user.email=t@t -c init.defaultBranch=main -c commit.gpgsign=false`; a "remote" is a second repository cloned with `git clone`. Skip a test with a printed note when `git --version` fails.

```rust
#[test] fn a_clean_clone_in_sync_has_nothing_at_risk() { /* unpushed 0, ahead Some(0), uncommitted 0, !at_risk() */ }
#[test] fn uncommitted_paths_count_tracked_changes_and_untracked_files() { /* 1 modified + 2 new → 3; an ignored file not counted */ }
#[test] fn a_branch_with_no_upstream_counts_its_commits_on_no_remote() { /* new branch, 2 commits → upstream None, ahead None, unpushed 2, at_risk() */ }
#[test] fn commits_ahead_of_the_upstream_are_counted() { /* 1 local commit on main → ahead Some(1), unpushed 1 */ }
#[test] fn a_linked_worktree_is_listed_with_its_own_branch_and_changes() { /* `worktree add ../wt -b side` + 1 new file → 2 worktrees, the second uncommitted 1 */ }
#[test] fn a_detached_head_has_no_branch() { /* checkout --detach → branch None, head 7+ hex chars */ }
#[test] fn a_folder_that_is_a_repository_is_the_only_one() { /* repositories.len() == 1, path == folder */ }
#[test] fn the_repositories_one_level_down_are_found_by_name() { /* children b, a, plain folder c → [a, b] */ }
#[test] fn work_in_a_missing_folder_says_so() { /* repositories empty, note contains "does not exist" */ }
#[test] fn a_folder_with_no_repository_says_so() { /* note contains "no git repository" */ }
#[test] fn a_repository_git_refuses_keeps_its_error_and_the_others() { /* child x with an empty `.git` file, child y valid → x.error is Some, y read */ }
#[test] fn reading_changes_nothing() { /* `.git/index` bytes and mtime equal before and after work_in on a repository with a stale index */ }
#[test] fn the_lines_say_what_the_note_needs() { /* the three example lines above from built Work values, plus the last-fetch line */ }
```

- [ ] **Step 2: Implement `src/work.rs`** and the `mod` line.

- [ ] **Step 3: Format, commit, push, read the run**

```bash
cargo fmt
git add src/work.rs src/lib.rs && git commit -m "Report the git work in a session's folder, read-only" && git push && gh run watch
```

Expected: green on Ubuntu and macOS.

---

### Task 4: The handoff note and start command

**Files:**
- Create: `src/handoff.rs`, `assets/handoff-prompts/continue.md`
- Modify: `src/lib.rs` (`pub mod handoff;`), `src/catalog.rs` (field and accessor), `catalog/harnesses.json` (two entries)

**Interfaces:**
- Consumes: `work::{Work, lines}`, `reader::SessionInfo`.
- Produces, in `src/catalog.rs`:

```rust
// Entry: the argv that starts a new interactive session with a first prompt; empty = not verified.
#[serde(default)] pub interactive_start: Vec<String>,
/// `{exe}` and `{prompt}` filled in; `None` for a harness whose form has not been verified.
pub fn interactive_start(harness: HarnessId, exe: &str, prompt: &str) -> Option<Vec<String>>;
```

`catalog/harnesses.json`: `"interactive_start": ["{exe}", "{prompt}"]` for `claude_code` and for `codex`; no other entry gets the field.

- Produces, in `src/handoff.rs`:

```rust
pub const FOLDER: &str = "handoff-prompts";
pub const DEFAULT_TEMPLATE: &str = include_str!("../assets/handoff-prompts/continue.md");
/// `<dir>/continue.md` when it exists and is not empty, else the default. `dir` is the folder
/// beside the executable; the default is written there once, best effort, so it can be edited.
pub fn template(dir: Option<&Path>) -> String;
/// The note, from the template: `{title}`, `{harness}`, `{id}`, `{updated}`, `{reason}`, `{folder}`, `{work}`.
pub fn note(template: &str, harness: HarnessId, info: &SessionInfo, work: &Work) -> String;
/// `<data_dir>/handoffs/<harness>-<id>.md`, the id reduced to `[A-Za-z0-9._-]` (others → `_`).
pub fn note_path(data_dir: &Path, harness: HarnessId, id: &str) -> PathBuf;
/// Writes the note, creating `handoffs/`.
pub fn save(text: &str, path: &Path) -> std::io::Result<()>;
/// `Read <path> and continue from it.`
pub fn first_prompt(note_file: &Path) -> String;
/// Why a session cannot be handed off: it has no folder. `None`: it can.
pub fn refusal(info: &SessionInfo) -> Option<String>;
/// The harnesses a handoff can start, in `HarnessId::ALL` order.
pub fn destinations() -> Vec<HarnessId>;
```

`assets/handoff-prompts/continue.md`, exactly:

```text
This continues a session that can no longer be used: "{title}" ({harness}, id {id}, last active {updated}).
Its conversation is not available here: {reason}.

Work it left in {folder}:
{work}

Start by reading the state of these repositories. Do not push, discard or overwrite anything before asking.
```

`refusal` text: `this session has no folder on this machine: there is nothing local to continue from`. A missing title is the id; a missing `updated` is `unknown`; a missing `unreadable` reason is `it was not read`.

- [ ] **Step 1: Write the failing tests**

```rust
// src/catalog.rs tests
#[test] fn claude_code_and_codex_start_with_a_prompt_and_no_other_harness_does() {
    assert_eq!(interactive_start(HarnessId::ClaudeCode, "claude", "p q"), Some(vec!["claude".into(), "p q".into()]));
    assert_eq!(interactive_start(HarnessId::Codex, "codex", "p"), Some(vec!["codex".into(), "p".into()]));
    for other in [HarnessId::OpenCode, HarnessId::Pi, HarnessId::Hermes, HarnessId::DeepSeekHarness] {
        assert_eq!(interactive_start(other, "x", "p"), None);
    }
}
// src/handoff.rs tests
#[test] fn the_note_names_the_session_the_reason_the_folder_and_the_work() { /* every placeholder replaced; contains each of work::lines(&work); no "{" left */ }
#[test] fn a_session_without_title_or_time_is_still_described() { /* id in place of the title, "unknown" */ }
#[test] fn the_note_file_is_in_the_data_folder_named_by_harness_and_id() {
    assert_eq!(note_path(Path::new("D"), HarnessId::Codex, "6b00-ab"), Path::new("D").join("handoffs").join("codex-6b00-ab.md"));
}
#[test] fn the_note_file_name_keeps_only_safe_characters() { /* id "a/b\\c:d..e" → "codex-a_b_c_d..e.md", inside handoffs/ */ }
#[test] fn the_first_prompt_is_one_line_naming_the_file() {
    assert_eq!(first_prompt(Path::new("D/handoffs/codex-x.md")), format!("Read {} and continue from it.", Path::new("D/handoffs/codex-x.md").display()));
}
#[test] fn an_edited_template_replaces_the_default_and_an_empty_one_does_not() { /* scratch dir */ }
#[test] fn the_default_template_is_written_once_for_editing() { /* template(Some(dir)) creates dir/continue.md; a second call leaves an edited file alone */ }
#[test] fn a_session_without_a_folder_is_refused() { /* cwd None → Some(text); cwd Some → None */ }
#[test] fn the_destinations_are_claude_code_and_codex() { assert_eq!(destinations(), vec![HarnessId::ClaudeCode, HarnessId::Codex]); }
```

`src/handoff.rs` is not an adapter file: its tests are test code (skipped by the architecture test), and its production code names no variant; `destinations` filters `HarnessId::ALL` by `catalog::interactive_start(h, "", "").is_some()`.

- [ ] **Step 2: Implement** the catalog field, the accessor, the two JSON lines, the asset and `src/handoff.rs`.

- [ ] **Step 3: Format, commit, push, read the run**

```bash
cargo fmt
git add src/handoff.rs src/lib.rs src/catalog.rs catalog/harnesses.json assets/handoff-prompts
git commit -m "Compose a handoff note and the command that starts a harness on it" && git push && gh run watch
```

---

### Task 5: `mukade work` and `mukade handoff`

**Files:**
- Modify: `src/cli.rs` (the `match command.as_str()` at about line 548, and `USAGE`)

**Interfaces:**
- Consumes: `Reader::overview`, `work::{work_in, lines}`, `handoff::*`, `catalog::interactive_start`, `exe::resolve`, `portable::data_dir`.
- Produces two commands, both taking the session from `reader.overview(id, &roots)`:
  - `mukade work <harness> <id> [--json]`: prints `work::lines`, or the `Work` as one JSON object. A session without a folder: exit 2 with `handoff::refusal`'s text.
  - `mukade handoff <harness> <id> <dest> [--run]`: writes the note file, prints the note, then the command as one JSON array line. With `--run`, runs the command in the session's folder with inherited stdio and exits with its status. A `dest` not in `handoff::destinations()`: exit 2, `<dest> cannot be started with a first prompt yet (verified: claude_code, codex)`. No data folder: exit 2 naming `MUKADE_DATA_DIR`.
- `USAGE` gains the two commands beside `overview`, in the file's existing style.

- [ ] **Step 1: Implement** the two arms and the usage lines. Exit 2 is how the file already refuses (`HopError::Unsupported` / the existing refusal helper): use the same path.

- [ ] **Step 2: Format, commit, push, read the run.** There is no test step: CLI tests are integration tests (`tests/cli.rs`), which CI does not run; the logic these arms call is covered by Tasks 3 and 4, and CI compiles the arms.

```bash
cargo fmt
git add src/cli.rs && git commit -m "Add the work and handoff commands" && git push && gh run watch
```

---

### Task 6: Desktop commands

**Files:**
- Modify: `apps/desktop/src-tauri/src/actions.rs`, `apps/desktop/src-tauri/src/commands.rs`, `apps/desktop/src-tauri/src/main.rs` (the `generate_handler!` list)

**Interfaces:**
- Consumes: `AppState::{location, listed_session, locations}`, `Location::{engine, key, harness, label}`, `terminal::{launch, store_env}`, the library's `work` and `handoff`.
- Produces, in `actions.rs` (none of them calls `readable`: an unreadable session is their subject):

```rust
pub fn work(state: &AppState, key: &str, id: &str) -> CmdResult<mukade::work::Work>;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct HandoffOptions {
    /// Why this session cannot be handed off; the other fields are then empty.
    #[serde(skip_serializing_if = "Option::is_none")] pub refusal: Option<String>,
    /// Locations of a harness in `handoff::destinations()`, as `Destination { key, label, .. }`.
    pub destinations: Vec<Destination>,
    /// The composed note, for the person to edit.
    pub note: String,
    pub cwd: Option<String>,
}
pub fn handoff_options(state: &AppState, key: &str, id: &str) -> CmdResult<HandoffOptions>;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct HandedOff { pub launched: bool, pub command: Vec<String>, pub note_path: String, pub cwd: String }
/// Saves `note` as edited, then opens `dest`'s harness on it in a terminal, in the session's folder.
pub fn handoff(state: &AppState, key: &str, id: &str, dest: &str, note: &str) -> CmdResult<HandedOff>;
```

`work`: a session without `cwd` → `CmdError::refused(handoff::refusal text)`. `handoff`: refused when the folder is not a directory (`<folder> does not exist`), when `dest` is not a destination, when there is no data folder; the terminal environment is `terminal::store_env(|c| dest.roots.apply_to(dest.harness, c))`, as `resume` builds it. The template folder is `handoff::FOLDER` beside the executable (`std::env::current_exe()`'s parent).

- `commands.rs`: `work`, `handoff_options`, `handoff`, each `async` through `blocking`, with the parameter names `installation`, `id`, and for `handoff` `dest`, `note` (as `resume` names its own); all three added to `generate_handler!`.

- [ ] **Step 1: Write the failing tests** in `actions.rs`'s `mod tests`, with the test state that module already builds (a scratch Codex home holding the two fixture databases of Task 1, written by a small helper in the test module):

```rust
#[test] fn the_work_of_an_unreadable_session_is_reported() { /* work(..).folder == the project folder; no refusal */ }
#[test] fn the_work_of_a_session_without_a_folder_is_refused_with_the_reason() { /* CmdError kind refused */ }
#[test] fn the_handoff_offers_the_locations_of_claude_code_and_codex() { /* destinations' keys are exactly those two harnesses' locations; note contains the title */ }
#[test] fn a_handoff_toward_a_harness_without_a_start_command_is_refused() { /* dest = the pi location's key */ }
```

- [ ] **Step 2: Implement** the three actions, the three commands and the handler entries.

- [ ] **Step 3: Format, commit, push, read the run**

```bash
cargo fmt
git add apps/desktop/src-tauri/src && git commit -m "Desktop: report a session's work on disk and hand it off" && git push && gh run watch
```

Expected: `cargo test -p mukade-desktop` green on both systems.

---

### Task 7: Desktop interface

**Files:**
- Modify: `apps/desktop/src/types.ts`, `apps/desktop/src/api.ts`, `apps/desktop/src/mock.ts`, `apps/desktop/src/detail.ts`, `apps/desktop/src/main.ts`, `apps/desktop/src/sessions.ts`, `apps/desktop/src/sessions.css`
- Create: `apps/desktop/src/work.ts`, `apps/desktop/src/handoff.ts`, `apps/desktop/tests/work.test.ts`
- Read first: `node_modules/@acaprino/imago/docs/design-file.md`, and `apps/desktop/src/hop.ts` for how this app builds an Imago form dialog

**Interfaces:**
- Consumes the three commands of Task 6.
- Produces:

```ts
// types.ts
export interface Branch { name: string; upstream: string | null; ahead: number | null; unpushed: number }
export interface Worktree { path: string; branch: string | null; head: string; uncommitted: number }
export interface Repository { path: string; worktrees: Worktree[]; branches: Branch[]; error: string | null }
export interface Work { folder: string; repositories: Repository[]; note: string | null }
export interface HandoffOptions { refusal?: string; destinations: Destination[]; note: string; cwd?: string }
export interface HandedOff { launched: boolean; command: string[]; note_path: string; cwd: string }
// api.ts
work(installation: string, id: string): Promise<Work>
handoffOptions(installation: string, id: string): Promise<HandoffOptions>
handoff(installation: string, id: string, dest: string, note: string): Promise<HandedOff>
// work.ts
export function atRisk(repo: Repository): boolean                 // uncommitted > 0 in a worktree, or unpushed > 0 on a branch
export function repositoryLine(repo: Repository): string          // "feature/stack-detection · no upstream · 3 commits on no remote · 133 uncommitted"
export function renderWork(work: Work): HTMLElement
// handoff.ts
export function canStartIn(row: SessionRow): boolean              // Boolean(row.unreadable && row.cwd)
export function openHandoff(row: SessionRow, done: (result: HandedOff) => void): void
```

Behaviour:
- `detail.ts`: a "Work on disk" section for a selected row with a `cwd`, loaded when the row is selected, replaced by a newer selection's result (a late answer for a row no longer selected is dropped). It shows the folder, then one row per repository: its name, `repositoryLine`, and an Imago warning chip `at risk` when `atRisk`. `work.note` or a repository's `error` is shown as a note.
- `detail.ts`: a button `Start in…` beside `Resume`, shown when `canStartIn(row)`. For an unreadable row it is the primary button; the disabled ones stay as they are.
- `handoff.ts`: an Imago form dialog: a destination select (`destinations`), the note in a multi-line field, `Start` and `Cancel`. A `refusal` is shown in place of the form. After `Start`, the status line says `Started <label> in <cwd>; note saved to <note_path>`.
- `sessions.ts`: an unreadable row whose reason starts with `hosted by ChatGPT` shows the chip `hosted` in the neutral style instead of `unreadable` in the bad one; the title attribute stays the reason.
- `mock.ts`: one hosted row under Codex (`Capire lo stack di Daodan`, a `cwd`, the reason), a `Work` with the three repositories of the spec, and the three commands, so `npm run dev` and the smoke test show the feature.

- [ ] **Step 1: Write the failing tests** in `apps/desktop/tests/work.test.ts` (`node:test`, as `tree.test.ts`):

```ts
test('a repository with uncommitted paths or commits on no remote is at risk', …)   // true, true, and false for a clean one in sync
test('the line names branch, upstream, unpushed commits and uncommitted paths', () => {
  assert.equal(repositoryLine(stackAnalyser), 'feature/stack-detection · no upstream · 3 commits on no remote · 133 uncommitted');
  assert.equal(repositoryLine(imago), 'develop · origin/develop · clean');
});
test('a detached worktree is named by its commit', …)                                // 'detached at 41d916d · clean'
test('only an unreadable row with a folder can be started elsewhere', …)             // canStartIn: true / false / false
```

- [ ] **Step 2: Run them to see them fail**

Run: `npm test`
Expected: FAIL, `work.ts` and `handoff.ts` not found.

- [ ] **Step 3: Implement** the types, the api calls, the mock, `work.ts`, `handoff.ts`, and the changes to `detail.ts`, `main.ts` and `sessions.ts`.

- [ ] **Step 4: Run the checks**

Run: `npm run build && npm test && npm run smoke`
Expected: all pass. Then `npm run dev`, open the hosted chat in the mock, and look at the detail panel and the dialog in the light and the dark theme.

- [ ] **Step 5: Commit, push, read the run**

```bash
git add apps/desktop && git commit -m "Desktop: show a session's work on disk and start a harness on it" && git push && gh run watch
```

---

### Task 8: Documents

**Files:**
- Create: `docs/adr/ADR-0005-hosted-chats-read-locally.md` (the header and sections of `ADR-0004`)
- Modify: `docs/adr/README.md`, `docs/harnesses/codex.md`, `docs/usage.md`, `docs/desktop.md`, `docs/architecture.md`, `README.md`, `docs/reference/harness-catalog.md`

- [ ] **Step 1: Write them.**
  - ADR-0005, status Proposed, author Alfio Caprino. Decision: hosted chats are read from the Codex app's local catalogue and listed among Codex's sessions as unreadable; their text is taken only from a file the owner supplies (phase 2). Invariants: **INV-001** no mukade code opens a Codex credential file or makes a network request for a chat; **INV-002** `sqlite/codex.db`, `state_<n>.sqlite` and an export are opened read-only; **INV-003** the work view runs no git command that changes a repository or fetches, and sets `GIT_OPTIONAL_LOCKS=0`. Rejected alternatives: the three of the spec's "Approaches" that were not chosen, and `codex app-server` with what the probe returned.
  - `docs/harnesses/codex.md`: a section "Chats hosted by ChatGPT": where the catalogue is, which rows are listed, why they are unreadable, the table definitions from the spec's appendix, the versions checked.
  - `docs/usage.md`: `mukade work` and `mukade handoff`, with the Daodan example and the note file's place.
  - `docs/desktop.md`: "Work on disk" and "Start in…".
  - `docs/architecture.md`, "What stays generic": rows for `src/work.rs` and `src/handoff.rs`.
  - `README.md`: one bullet under the opening list, and "hosted chats of the Codex app" in the Codex row of the harness table.
  - `docs/reference/harness-catalog.md`: the `interactive_start` field.

- [ ] **Step 2: Commit, push, read the run**

```bash
git add docs README.md && git commit -m "Document hosted chats, the work view and the handoff" && git push && gh run watch
```

---

### Task 9: Integrate, and check on the owner's machine

- [ ] **Step 1: Merge.** With the last run green: merge the pull request into `develop` (`gh pr merge --merge --delete-branch`), `git checkout develop && git pull --ff-only`, delete the local branch. `develop` and `main` are the only branches left.

- [ ] **Step 2: Hand the live check to the owner.** It needs a released build (the owner opens and merges `develop` → `main`; nothing is built here). With `mukade.exe` and `mukade-ui.exe` of that release:

```text
mukade list codex | findstr Daodan                 → the chat, with "unreadable": "hosted by ChatGPT: …"
mukade work codex 6b00c623-2d08-83eb-a3cc-df6dadef8474
                                                   → daodan, imago, stack-analyser with their branches and counts
mukade handoff codex 6b00c623-2d08-83eb-a3cc-df6dadef8474 claude_code
                                                   → the note, the file under mukade-data\handoffs, the command
```

In the desktop app: the chat under Codex with the chip `hosted`, "Work on disk" with `stack-analyser` marked at risk, `Start in…` → Claude Code opens in `Documents\ChatGPT\Daodan` and reads the note (Review Focus 1: the whole prompt arrives as one argument through Windows Terminal).

- [ ] **Step 3: Record it** in `docs/verification/2026-11-21-hosted-chats-live.md` (what was run, the versions, what was seen), set ADR-0005 to Accepted, commit on `develop`.
