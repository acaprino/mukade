//! Project and session discovery beyond the transcripts a harness still holds.
//!
//! A harness forgets its transcripts long before it forgets the sessions: Codex keeps a `threads`
//! row in `state_5.sqlite` after its rollout is deleted, Claude Code keeps the prompts in
//! `history.jsonl`, OpenCode keeps older databases beside the current one. `list` still reports
//! those sessions, marked `unreadable`, so a caller can see every project; `dump` and `hop` refuse
//! them with the reason.

mod support;

use mukade::model::HarnessId;
use mukade::reader::Reader;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// The rows `mukade list <harness>` printed.
fn list(harness: HarnessId, scratch: &Path) -> Vec<Value> {
    let (code, stdout, stderr) = support::cli_code(scratch, &["list", harness.as_str()]);
    assert_eq!(code, Some(0), "list failed: {stderr}");
    stdout
        .lines()
        .map(|line| serde_json::from_str(line).expect("one JSON object per line"))
        .collect()
}

/// The row for `id`.
fn row<'a>(rows: &'a [Value], id: &str) -> &'a Value {
    rows.iter()
        .find(|row| row["id"] == id)
        .unwrap_or_else(|| panic!("no row for {id} in {rows:?}"))
}

// ---------------------------------------------------------------------------------------------
// Codex: `state_5.sqlite` outlives the rollouts.

const THREAD_A: &str = "019e0000-0000-7000-8000-00000000000a";
const THREAD_B: &str = "019e0000-0000-7000-8000-00000000000b";
const THREAD_SUB: &str = "019e0000-0000-7000-8000-0000000000c1";
const THREAD_GUARD: &str = "019e0000-0000-7000-8000-0000000000c2";

/// One `threads` row, with the columns Codex 0.166 gives it that matter here.
struct Thread {
    id: &'static str,
    rollout_path: String,
    cwd: &'static str,
    title: &'static str,
    name: Option<&'static str>,
    preview: &'static str,
    source: &'static str,
    thread_source: Option<&'static str>,
    archived: bool,
    updated_ms: i64,
}

impl Thread {
    /// A root thread started from the CLI, whose rollout would sit under `home/sessions`.
    fn root(home: &Path, id: &'static str, cwd: &'static str) -> Thread {
        Thread {
            id,
            rollout_path: home
                .join("sessions/2026/11/08")
                .join(format!("rollout-2026-11-08T10-00-00-{id}.jsonl"))
                .to_string_lossy()
                .into_owned(),
            cwd,
            title: "",
            name: None,
            preview: "",
            source: "cli",
            thread_source: Some("user"),
            archived: false,
            updated_ms: 1_794_149_000_000,
        }
    }
}

/// Make `<home>/<file>` the way Codex lays out its state database, holding `threads` and the
/// `(parent, child)` spawn edges.
fn codex_state(home: &Path, file: &str, threads: &[Thread], edges: &[(&str, &str)]) {
    std::fs::create_dir_all(home).unwrap();
    let db = rusqlite::Connection::open(home.join(file)).unwrap();
    db.execute_batch(
        "CREATE TABLE threads (
             id TEXT PRIMARY KEY, rollout_path TEXT NOT NULL, created_at INTEGER NOT NULL,
             updated_at INTEGER NOT NULL, source TEXT NOT NULL, cwd TEXT NOT NULL,
             title TEXT NOT NULL, archived INTEGER NOT NULL, first_user_message TEXT NOT NULL,
             created_at_ms INTEGER, updated_at_ms INTEGER, thread_source TEXT,
             preview TEXT NOT NULL, name TEXT);
         CREATE TABLE thread_spawn_edges (
             parent_thread_id TEXT NOT NULL, child_thread_id TEXT NOT NULL PRIMARY KEY,
             status TEXT NOT NULL);",
    )
    .unwrap();
    for t in threads {
        db.execute(
            "INSERT INTO threads VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, '', ?9, ?10, ?11, ?12, ?13)",
            rusqlite::params![
                t.id,
                t.rollout_path,
                t.updated_ms / 1000,
                t.updated_ms / 1000,
                t.source,
                t.cwd,
                t.title,
                i64::from(t.archived),
                t.updated_ms,
                t.updated_ms,
                t.thread_source,
                t.preview,
                t.name,
            ],
        )
        .unwrap();
    }
    for (parent, child) in edges {
        db.execute(
            "INSERT INTO thread_spawn_edges VALUES (?1, ?2, 'closed')",
            rusqlite::params![parent, child],
        )
        .unwrap();
    }
}

/// The four threads most tests share: two root threads (one archived), a subagent thread spawned
/// by the first, and a guardian review. None of their rollouts exists.
fn codex_home_without_rollouts(tag: &str) -> PathBuf {
    let scratch = support::scratch(HarnessId::Codex, tag);
    let home = scratch.join("home");
    let mut a = Thread::root(&home, THREAD_A, "D:/work/alpha");
    a.name = Some("Alpha work");
    a.title = "generated title";
    let mut b = Thread::root(&home, THREAD_B, "D:/work/beta");
    b.title = "Beta title";
    b.archived = true;
    b.source = "vscode";
    let mut sub = Thread::root(&home, THREAD_SUB, "D:/work/alpha");
    sub.source = "subagent-thread_spawn";
    sub.thread_source = Some("subagent");
    let mut guard = Thread::root(&home, THREAD_GUARD, "D:/work/gamma");
    guard.source = "subagent-guardian";
    guard.thread_source = Some("guardian_review");
    codex_state(
        &home,
        "state_5.sqlite",
        &[a, b, sub, guard],
        &[(THREAD_A, THREAD_SUB)],
    );
    scratch
}

#[test]
fn codex_threads_whose_rollout_is_gone_are_still_listed_from_the_state_database() {
    let scratch = codex_home_without_rollouts("cov-codex-gone");

    let rows = list(HarnessId::Codex, &scratch);

    let ids: Vec<&str> = rows.iter().map(|r| r["id"].as_str().unwrap()).collect();
    assert_eq!(rows.len(), 2, "root threads only: {ids:?}");
    let a = row(&rows, THREAD_A);
    assert_eq!(a["cwd"], "D:/work/alpha");
    assert_eq!(
        a["title"], "Alpha work",
        "the thread's name wins over the generated title"
    );
    assert_eq!(a["archived"], false);
    assert!(
        a["unreadable"]
            .as_str()
            .is_some_and(|why| why.contains("state_5.sqlite")),
        "the reason names where the thread is still recorded: {a}"
    );
    assert!(
        a["location"]
            .as_str()
            .unwrap()
            .ends_with(&format!("{THREAD_A}.jsonl")),
        "the location is the rollout the row points at: {a}"
    );
    assert!(
        a["timestamp"]
            .as_str()
            .is_some_and(|t| t.starts_with("2026-11-08T")),
        "the update time comes from updated_at_ms: {a}"
    );
    let b = row(&rows, THREAD_B);
    assert_eq!(b["title"], "Beta title");
    assert_eq!(b["archived"], true);
}

#[test]
fn codex_list_all_adds_the_subagent_threads_with_their_parent() {
    let scratch = codex_home_without_rollouts("cov-codex-all");
    let roots = support::roots(&scratch);

    let all = mukade::reader::codex::Codex.list_all(&roots).unwrap();

    assert_eq!(all.len(), 4, "{all:?}");
    let sub = all.iter().find(|s| s.id == THREAD_SUB).unwrap();
    assert_eq!(
        sub.parent.as_deref(),
        Some(THREAD_A),
        "the spawn edge is the parent link"
    );
    let guard = all.iter().find(|s| s.id == THREAD_GUARD).unwrap();
    assert_eq!(guard.parent, None);
    let a = all.iter().find(|s| s.id == THREAD_A).unwrap();
    assert_eq!(a.parent, None);
}

#[test]
fn dumping_a_codex_thread_without_its_rollout_is_refused_with_the_reason() {
    let scratch = codex_home_without_rollouts("cov-codex-dump");

    let (code, stdout, stderr) = support::cli_code(&scratch, &["dump", "codex", THREAD_A]);

    assert_eq!(code, Some(2), "a refusal, not a read failure: {stderr}");
    assert!(stdout.is_empty(), "nothing is printed: {stdout}");
    assert!(
        stderr.contains("state_5.sqlite") && stderr.contains("rollout"),
        "the refusal says the rollout is gone and where the row is: {stderr}"
    );
}

#[test]
fn a_codex_rollout_on_disk_stays_readable_and_takes_its_gaps_from_the_database() {
    let scratch = support::scratch(HarnessId::Codex, "cov-codex-both");
    let home = scratch.join("home");
    let id = support::stage_fixture(HarnessId::Codex, &scratch);
    let full_id = support::source_id(HarnessId::Codex);
    let mut thread = Thread::root(&home, full_id, "D:/db/cwd");
    thread.archived = true;
    thread.rollout_path = support::source_path(HarnessId::Codex)
        .to_string_lossy()
        .into_owned();
    codex_state(&home, "state_5.sqlite", &[thread], &[]);

    let rows = list(HarnessId::Codex, &scratch);

    assert_eq!(rows.len(), 1, "one thread, one row: {rows:?}");
    let r = &rows[0];
    assert_eq!(r["id"], full_id);
    assert!(r["unreadable"].is_null(), "the rollout is on disk: {r}");
    assert!(
        r["cwd"].as_str().unwrap().contains("shinatama-verify"),
        "the rollout's own header still decides the cwd: {r}"
    );
    assert_eq!(
        r["archived"], true,
        "the database knows it was archived: {r}"
    );
    assert!(
        r["timestamp"].is_string(),
        "and when it was last updated: {r}"
    );
    let (ok, _, stderr) = support::cli(&scratch, &["dump", "codex", &id]);
    assert!(ok, "and it still dumps: {stderr}");
}

#[test]
fn a_codex_rollout_outside_sessions_is_found_through_its_database_row() {
    let scratch = support::scratch(HarnessId::Codex, "cov-codex-archived-dir");
    let home = scratch.join("home");
    let full_id = support::source_id(HarnessId::Codex);
    let moved = home
        .join("archived_sessions")
        .join(format!("rollout-2026-10-27T16-43-08-{full_id}.jsonl"));
    std::fs::create_dir_all(moved.parent().unwrap()).unwrap();
    std::fs::copy(support::source_path(HarnessId::Codex), &moved).unwrap();
    let mut thread = Thread::root(&home, full_id, "D:/db/cwd");
    thread.archived = true;
    thread.rollout_path = moved.to_string_lossy().into_owned();
    codex_state(&home, "state_5.sqlite", &[thread], &[]);

    let rows = list(HarnessId::Codex, &scratch);

    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(
        rows[0]["unreadable"].is_null(),
        "the file exists where the row says: {}",
        rows[0]
    );
    assert_eq!(rows[0]["location"], moved.to_string_lossy().as_ref());
    let (ok, stdout, stderr) = support::cli(&scratch, &["dump", "codex", &full_id[..8]]);
    assert!(ok, "dump reads it through the row: {stderr}");
    assert!(
        stdout.contains(full_id),
        "the dumped session is that thread"
    );
}

#[test]
fn a_codex_state_database_that_cannot_be_read_does_not_hide_the_rollouts() {
    let scratch = support::scratch(HarnessId::Codex, "cov-codex-damaged");
    let home = scratch.join("home");
    support::stage_fixture(HarnessId::Codex, &scratch);
    std::fs::write(home.join("state_5.sqlite"), b"this is not a database").unwrap();

    let rows = list(HarnessId::Codex, &scratch);

    assert_eq!(rows.len(), 1, "the rollout is still listed: {rows:?}");
    assert!(rows[0]["unreadable"].is_null());

    // A database without the `threads` table (another Codex version) is just as harmless.
    std::fs::remove_file(home.join("state_5.sqlite")).unwrap();
    let db = rusqlite::Connection::open(home.join("state_5.sqlite")).unwrap();
    db.execute_batch("CREATE TABLE unrelated (x INTEGER);")
        .unwrap();
    drop(db);
    assert_eq!(list(HarnessId::Codex, &scratch).len(), 1);
}

#[test]
fn the_newest_codex_state_database_is_the_one_read() {
    let scratch = support::scratch(HarnessId::Codex, "cov-codex-newest");
    let home = scratch.join("home");
    codex_state(
        &home,
        "state_5.sqlite",
        &[Thread::root(&home, THREAD_A, "D:/old")],
        &[],
    );
    codex_state(
        &home,
        "state_6.sqlite",
        &[Thread::root(&home, THREAD_B, "D:/new")],
        &[],
    );

    let rows = list(HarnessId::Codex, &scratch);

    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["id"], THREAD_B);
}

#[test]
fn a_listed_session_says_whether_it_is_readable_and_whether_it_is_live() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "cov-keys");
    support::stage_fixture(HarnessId::ClaudeCode, &scratch);

    let rows = list(HarnessId::ClaudeCode, &scratch);

    assert_eq!(rows.len(), 1, "{rows:?}");
    let row = &rows[0];
    assert!(
        row.get("unreadable").is_some_and(Value::is_null),
        "a session with a transcript has no reason to be unreadable: {row}"
    );
    assert!(
        row.get("live").is_some_and(Value::is_null),
        "no process registry in this store: live is unknown, not false: {row}"
    );
}

// ---------------------------------------------------------------------------------------------
// OpenCode: older databases beside the current one.

/// A session row as the `session_v2` and the legacy `session` tables both lay it out.
const SESSION_COLUMNS: &str = "id TEXT PRIMARY KEY, parent_id TEXT, title TEXT, directory TEXT, \
                               time_updated INTEGER, time_archived INTEGER";

/// Make an OpenCode database at `path`: `v2` rows go to `session_v2`, `legacy` rows to `session`,
/// each `(id, parent, title, directory, time_updated)`.
fn opencode_db(
    path: &Path,
    v2: Option<&[(&str, Option<&str>, &str, &str, i64)]>,
    legacy: Option<&[(&str, Option<&str>, &str, &str, i64)]>,
) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let db = rusqlite::Connection::open(path).unwrap();
    for (table, rows) in [("session_v2", v2), ("session", legacy)] {
        let Some(rows) = rows else { continue };
        db.execute_batch(&format!("CREATE TABLE {table} ({SESSION_COLUMNS});"))
            .unwrap();
        for (id, parent, title, directory, updated) in rows {
            db.execute(
                &format!("INSERT INTO {table} VALUES (?1, ?2, ?3, ?4, ?5, NULL)"),
                rusqlite::params![id, parent, title, directory, updated],
            )
            .unwrap();
        }
    }
}

/// `mukade list opencode`, with `OPENCODE_DB` pointing at `database` and no OpenCode to run.
fn list_opencode(scratch: &Path, database: &Path) -> Vec<Value> {
    let (code, stdout, stderr) = support::cli_in(
        scratch,
        scratch,
        &["list", "opencode"],
        &[("OPENCODE_DB", database.to_str().unwrap())],
    );
    assert_eq!(code, Some(0), "list failed: {stderr}");
    stdout
        .lines()
        .map(|line| serde_json::from_str(line).expect("one JSON object per line"))
        .collect()
}

#[test]
fn opencode_lists_the_sessions_of_the_database_files_beside_the_current_one() {
    let scratch = support::scratch(HarnessId::OpenCode, "cov-oc-siblings");
    let dir = scratch.join("data");
    let current = dir.join("opencode.db");
    opencode_db(
        &current,
        Some(&[("ses_now", None, "Now", "D:/work/now", 5000)]),
        None,
    );
    // The channel build's database from before the migration: only the legacy table.
    opencode_db(
        &dir.join("opencode-.db"),
        None,
        Some(&[
            ("ses_old1", None, "Old one", "D:/work/old", 3000),
            ("ses_old2", None, "Old two", "D:/work/older", 1000),
            (
                "ses_oldchild",
                Some("ses_old1"),
                "Child",
                "D:/work/old",
                4000,
            ),
        ]),
    );

    let rows = list_opencode(&scratch, &current);

    let ids: Vec<&str> = rows.iter().map(|r| r["id"].as_str().unwrap()).collect();
    assert_eq!(
        ids,
        ["ses_now", "ses_old1", "ses_old2"],
        "roots only, newest first across both files"
    );
    assert!(row(&rows, "ses_now")["unreadable"].is_null());
    let old = row(&rows, "ses_old1");
    assert_eq!(old["cwd"], "D:/work/old");
    assert!(
        old["location"].as_str().unwrap().ends_with("opencode-.db"),
        "the location is the file the session is in: {old}"
    );
    assert!(
        old["unreadable"]
            .as_str()
            .is_some_and(|why| why.contains("legacy")),
        "a legacy-schema row is something OpenCode cannot export: {old}"
    );
}

#[test]
fn a_session_in_another_current_schema_database_says_which_file_to_select() {
    let scratch = support::scratch(HarnessId::OpenCode, "cov-oc-other-v2");
    let dir = scratch.join("data");
    let current = dir.join("opencode.db");
    opencode_db(&current, Some(&[]), None);
    opencode_db(
        &dir.join("opencode-dev.db"),
        Some(&[("ses_dev", None, "Dev", "D:/work/dev", 2000)]),
        None,
    );

    let rows = list_opencode(&scratch, &current);

    assert_eq!(rows.len(), 1, "{rows:?}");
    let reason = rows[0]["unreadable"]
        .as_str()
        .expect("not readable from here");
    assert!(
        reason.contains("opencode-dev.db") && reason.contains("--root"),
        "the reason names the file and how to select it: {reason}"
    );
}

#[test]
fn a_session_known_to_both_tables_of_one_database_is_listed_once_and_readable() {
    let scratch = support::scratch(HarnessId::OpenCode, "cov-oc-both-tables");
    let current = scratch.join("data/opencode.db");
    opencode_db(
        &current,
        Some(&[("ses_same", None, "Migrated", "D:/work/a", 2000)]),
        Some(&[
            ("ses_same", None, "Legacy copy", "D:/work/a", 1000),
            ("ses_left", None, "Left behind", "D:/work/b", 900),
        ]),
    );

    let rows = list_opencode(&scratch, &current);

    assert_eq!(rows.len(), 2, "{rows:?}");
    let same = row(&rows, "ses_same");
    assert_eq!(
        same["title"], "Migrated",
        "session_v2 is the truth where it has the row"
    );
    assert!(same["unreadable"].is_null());
    assert!(row(&rows, "ses_left")["unreadable"].is_string());
}

#[test]
fn a_database_that_is_not_named_like_opencodes_has_no_siblings() {
    let scratch = support::scratch(HarnessId::OpenCode, "cov-oc-custom-name");
    let dir = scratch.join("data");
    opencode_db(
        &dir.join("sessions.db"),
        Some(&[("ses_mine", None, "Mine", "D:/work/mine", 2000)]),
        None,
    );
    opencode_db(
        &dir.join("opencode.db"),
        Some(&[("ses_real", None, "The user's own", "D:/work/real", 3000)]),
        None,
    );

    let rows = list_opencode(&scratch, &dir.join("sessions.db"));

    let ids: Vec<&str> = rows.iter().map(|r| r["id"].as_str().unwrap()).collect();
    assert_eq!(
        ids,
        ["ses_mine"],
        "an explicitly chosen file is the whole store"
    );
}

#[test]
fn a_sibling_database_that_cannot_be_read_does_not_fail_the_listing() {
    let scratch = support::scratch(HarnessId::OpenCode, "cov-oc-bad-sibling");
    let dir = scratch.join("data");
    let current = dir.join("opencode.db");
    opencode_db(
        &current,
        Some(&[("ses_now", None, "Now", "D:/work/now", 5000)]),
        None,
    );
    std::fs::write(dir.join("opencode-broken.db"), b"not a database at all").unwrap();
    opencode_db(&dir.join("opencode-empty.db"), None, None);

    let rows = list_opencode(&scratch, &current);

    assert_eq!(rows.len(), 1, "{rows:?}");
}

#[test]
fn reading_a_session_that_lives_in_another_database_is_refused_with_its_file() {
    let scratch = support::scratch(HarnessId::OpenCode, "cov-oc-read-elsewhere");
    let dir = scratch.join("data");
    let current = dir.join("opencode.db");
    opencode_db(&current, Some(&[]), None);
    opencode_db(
        &dir.join("opencode-.db"),
        None,
        Some(&[("ses_old1", None, "Old", "D:/work/old", 3000)]),
    );

    let (code, stdout, stderr) = support::cli_in(
        &scratch,
        &scratch,
        &["dump", "opencode", "ses_old1"],
        &[("OPENCODE_DB", current.to_str().unwrap())],
    );

    assert_eq!(
        code,
        Some(2),
        "a refusal, and no OpenCode was needed: {stderr}"
    );
    assert!(stdout.is_empty());
    assert!(stderr.contains("opencode-.db"), "{stderr}");
}
