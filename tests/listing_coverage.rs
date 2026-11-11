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
    // A folder no scan of the home looks in: only the row can lead there.
    let moved = scratch
        .join("moved-rollouts")
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

// ---------------------------------------------------------------------------------------------
// Claude Code: `history.jsonl` outlives the transcripts, `sessions/` says what is running.

const LOST: &str = "aaaaaaaa-0000-4000-8000-000000000001";
const FRESH: &str = "bbbbbbbb-0000-4000-8000-000000000002";
const STALE: &str = "cccccccc-0000-4000-8000-000000000003";

/// A line of Claude Code's `history.jsonl`: one prompt the user typed.
fn prompt(session: &str, project: &str, timestamp: i64, display: &str) -> String {
    serde_json::json!({
        "display": display,
        "pastedContents": {},
        "timestamp": timestamp,
        "project": project,
        "sessionId": session,
    })
    .to_string()
}

/// A process id nothing runs under: above every platform's limit, so no process can ever take it
/// (a child that has exited could have its id reused before the listing looks).
fn dead_pid() -> u32 {
    u32::MAX - 1
}

/// `<home>/sessions/<pid>.json`, the registration Claude Code keeps while a process runs a session.
fn registration(home: &Path, pid: u32, session: &str, cwd: &str, extra: Value) {
    let dir = home.join("sessions");
    std::fs::create_dir_all(&dir).unwrap();
    let mut body = serde_json::json!({
        "pid": pid,
        "sessionId": session,
        "cwd": cwd,
        "startedAt": 1_794_143_992_646_i64,
        "kind": "interactive",
        "name": "a name",
        "status": "idle",
        "updatedAt": 1_794_149_286_106_i64,
    });
    if let (Some(body), Some(extra)) = (body.as_object_mut(), extra.as_object()) {
        for (key, value) in extra {
            body.insert(key.clone(), value.clone());
        }
    }
    std::fs::write(dir.join(format!("{pid}.json")), body.to_string()).unwrap();
}

#[test]
fn claude_sessions_known_only_from_history_are_listed_as_unreadable() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "cov-claude-history");
    let home = scratch.join("home");
    let kept = support::source_id(HarnessId::ClaudeCode);
    support::stage_fixture(HarnessId::ClaudeCode, &scratch);
    let history = [
        prompt(LOST, "D:\\work\\lost", 1_794_100_000_000, "/clear"),
        prompt(
            LOST,
            "D:\\work\\lost",
            1_794_100_100_000,
            "first real\nprompt  of the lost session",
        ),
        prompt(LOST, "D:\\work\\lost", 1_794_100_500_000, "a later one"),
        prompt(
            kept,
            "D:\\work\\kept",
            1_794_100_200_000,
            "this one has its transcript",
        ),
        "{not json at all".to_string(),
        // An older Claude Code wrote no session id: the prompt names a project, not a session.
        r#"{"display":"old","project":"D:\\work\\nosession","timestamp":1}"#.to_string(),
        String::new(),
    ]
    .join("\n");
    std::fs::write(home.join("history.jsonl"), history).unwrap();

    let rows = list(HarnessId::ClaudeCode, &scratch);

    assert_eq!(
        rows.len(),
        2,
        "the kept session once, the lost one once: {rows:?}"
    );
    assert!(row(&rows, kept)["unreadable"].is_null());
    let lost = row(&rows, LOST);
    assert_eq!(lost["cwd"], "D:\\work\\lost");
    assert_eq!(
        lost["title"], "first real prompt of the lost session",
        "the first prompt that is not a slash command, on one line"
    );
    assert!(
        lost["timestamp"]
            .as_str()
            .is_some_and(|t| t.starts_with("2026-")),
        "the last prompt's time: {lost}"
    );
    assert!(
        lost["location"]
            .as_str()
            .unwrap()
            .ends_with("history.jsonl"),
        "{lost}"
    );
    assert!(
        lost["unreadable"]
            .as_str()
            .is_some_and(|why| why.contains("history.jsonl")),
        "{lost}"
    );
}

#[test]
fn dumping_a_claude_session_known_only_from_history_is_refused_with_the_reason() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "cov-claude-history-dump");
    let home = scratch.join("home");
    std::fs::write(
        home.join("history.jsonl"),
        prompt(LOST, "D:\\work\\lost", 1_794_100_000_000, "hello"),
    )
    .unwrap();

    let (code, stdout, stderr) = support::cli_code(&scratch, &["dump", "claude_code", LOST]);

    assert_eq!(code, Some(2), "a refusal: {stderr}");
    assert!(stdout.is_empty());
    assert!(
        stderr.contains("history.jsonl") && stderr.contains("transcript"),
        "the refusal says the transcript is gone and where the session is still seen: {stderr}"
    );
}

#[test]
fn a_session_a_running_process_registered_is_live() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "cov-claude-live");
    let home = scratch.join("home");
    let kept = support::source_id(HarnessId::ClaudeCode);
    support::stage_fixture(HarnessId::ClaudeCode, &scratch);
    // The test process stands in for a running Claude Code; a child that has exited is a process
    // that is gone, and a registration that outlived it says nothing about its session.
    registration(
        &home,
        std::process::id(),
        kept,
        "D:\\work\\kept",
        Value::Null,
    );
    registration(&home, dead_pid(), STALE, "D:\\work\\stale", Value::Null);

    let rows = list(HarnessId::ClaudeCode, &scratch);

    let kept_row = row(&rows, kept);
    assert_eq!(kept_row["live"], true, "{kept_row}");
    assert!(kept_row["unreadable"].is_null());
    assert!(
        rows.iter().all(|r| r["id"] != STALE),
        "a registration whose process is gone is not a session: {rows:?}"
    );
}

#[test]
fn a_running_session_without_a_transcript_yet_is_listed_live_and_unreadable() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "cov-claude-fresh");
    let home = scratch.join("home");
    registration(
        &home,
        std::process::id(),
        FRESH,
        "D:\\work\\fresh",
        serde_json::json!({ "name": "fresh start" }),
    );

    let rows = list(HarnessId::ClaudeCode, &scratch);

    assert_eq!(rows.len(), 1, "{rows:?}");
    let fresh = &rows[0];
    assert_eq!(fresh["id"], FRESH);
    assert_eq!(fresh["cwd"], "D:\\work\\fresh");
    assert_eq!(fresh["title"], "fresh start");
    assert_eq!(fresh["live"], true);
    assert!(
        fresh["unreadable"]
            .as_str()
            .is_some_and(|why| why.contains("transcript")),
        "{fresh}"
    );
}

#[test]
fn sessions_with_no_running_process_are_not_live_when_the_store_keeps_a_registry() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "cov-claude-not-live");
    let home = scratch.join("home");
    let kept = support::source_id(HarnessId::ClaudeCode);
    support::stage_fixture(HarnessId::ClaudeCode, &scratch);
    std::fs::create_dir_all(home.join("sessions")).unwrap();

    let rows = list(HarnessId::ClaudeCode, &scratch);

    assert_eq!(
        row(&rows, kept)["live"],
        false,
        "an empty registry is an answer"
    );
}

#[test]
fn a_registration_from_another_machine_leaves_its_session_unknown() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "cov-claude-foreign");
    let home = scratch.join("home");
    let kept = support::source_id(HarnessId::ClaudeCode);
    support::stage_fixture(HarnessId::ClaudeCode, &scratch);
    registration(
        &home,
        std::process::id(),
        kept,
        "D:\\work\\kept",
        serde_json::json!({ "pidDomain": "plan9:elsewhere" }),
    );

    let rows = list(HarnessId::ClaudeCode, &scratch);

    assert!(
        row(&rows, kept)["live"].is_null(),
        "a process id means nothing outside the machine that issued it: {}",
        row(&rows, kept)
    );
}

#[test]
fn damaged_registrations_and_history_are_ignored() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "cov-claude-damaged");
    let home = scratch.join("home");
    support::stage_fixture(HarnessId::ClaudeCode, &scratch);
    let sessions = home.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    std::fs::write(sessions.join("123.json"), "{broken").unwrap();
    std::fs::write(sessions.join("notapid.json"), "{}").unwrap();
    std::fs::write(sessions.join("123.abc.key"), "{}").unwrap();
    std::fs::write(sessions.join("456.json"), r#"{"pid": 456}"#).unwrap();
    std::fs::write(home.join("history.jsonl"), b"\xff\xfe\n{}\n[]\n").unwrap();

    let rows = list(HarnessId::ClaudeCode, &scratch);

    assert_eq!(rows.len(), 1, "{rows:?}");
}

#[test]
fn a_process_id_taken_by_another_process_is_not_the_session() {
    use mukade::process::{self, Process};
    // Only where a process's start time can be read (Windows, Linux) is a reused id told from the
    // process. The unit is the platform's: a FILETIME on Windows, clock ticks since boot on Linux.
    let Process::Running {
        started: Some(started),
    } = process::inspect(std::process::id())
    else {
        return;
    };
    let scratch = support::scratch(HarnessId::ClaudeCode, "cov-claude-reused-pid");
    let home = scratch.join("home");
    let kept = support::source_id(HarnessId::ClaudeCode);
    support::stage_fixture(HarnessId::ClaudeCode, &scratch);
    let pid = std::process::id();
    let json = |procstart: u64| serde_json::json!({ "procStart": procstart.to_string() });
    let live = |procstart: u64| {
        registration(&home, pid, kept, "D:\\work\\kept", json(procstart));
        row(&list(HarnessId::ClaudeCode, &scratch), kept)["live"].clone()
    };

    if cfg!(windows) {
        assert_eq!(live(started + 5_000_000), true, "half a second apart is the same process");
        let an_hour = 3_600 * 10_000_000;
        assert_eq!(
            live(started - an_hour),
            false,
            "the registration is from a process that started an hour before this one"
        );
        // A creation time in some other unit says nothing about the process.
        assert_eq!(live(1_794_143_992_646), true);
    } else {
        assert_eq!(live(started), true, "the kernel's own count, read the same way");
        assert_eq!(
            live(started + 1),
            false,
            "a process that started one tick later is another process"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Claude Code: the listing remembers what it read from a session file.

/// Set a file's modification time.
fn touch(file: &Path, when: std::time::SystemTime) {
    std::fs::OpenOptions::new()
        .write(true)
        .open(file)
        .unwrap()
        .set_modified(when)
        .unwrap();
}

/// A Claude Code store holding the fixture session, last modified `age` ago, and the session file.
fn cached_store(
    tag: &str,
    age: std::time::Duration,
) -> (PathBuf, mukade::roots::StoreRoots, PathBuf) {
    let scratch = support::scratch(HarnessId::ClaudeCode, tag);
    support::stage_fixture(HarnessId::ClaudeCode, &scratch);
    let roots = support::roots(&scratch);
    let file = scratch.join("home/projects/claude-project").join(format!(
        "{}.jsonl",
        support::source_id(HarnessId::ClaudeCode)
    ));
    touch(&file, std::time::SystemTime::now() - age);
    (scratch, roots, file)
}

/// The fixture's working directory, as the listing reports it.
fn listed_cwd(roots: &mukade::roots::StoreRoots, cache: &Path) -> String {
    let listed = mukade::reader::claude_code::list_cached(roots, Some(cache)).unwrap();
    assert_eq!(listed.len(), 1, "{listed:?}");
    listed[0].cwd.clone().expect("the fixture names its cwd")
}

/// Change the file without changing its size.
fn change_in_place(file: &Path) {
    let text = std::fs::read_to_string(file).unwrap();
    assert!(text.contains("shinatama-verify"));
    std::fs::write(file, text.replace("shinatama-verify", "SHINATAMA-VERIFY")).unwrap();
}

#[test]
fn a_session_file_with_the_same_size_and_time_is_not_read_again() {
    let hour = std::time::Duration::from_secs(3_600);
    let (scratch, roots, file) = cached_store("cov-cache-hit", hour);
    let cache = scratch.join("cache");
    let modified = std::fs::metadata(&file).unwrap().modified().unwrap();
    assert!(listed_cwd(&roots, &cache).contains("shinatama-verify"));

    change_in_place(&file);
    touch(&file, modified);
    assert!(
        listed_cwd(&roots, &cache).contains("shinatama-verify"),
        "same size, same time: what was read last time is still the answer"
    );

    touch(&file, modified - std::time::Duration::from_secs(1));
    assert!(
        listed_cwd(&roots, &cache).contains("SHINATAMA-VERIFY"),
        "another modification time: the file is read again"
    );
}

#[test]
fn a_session_file_with_another_size_is_read_again() {
    let hour = std::time::Duration::from_secs(3_600);
    let (scratch, roots, file) = cached_store("cov-cache-size", hour);
    let cache = scratch.join("cache");
    let modified = std::fs::metadata(&file).unwrap().modified().unwrap();
    assert!(listed_cwd(&roots, &cache).contains("shinatama-verify"));

    let text = std::fs::read_to_string(&file).unwrap();
    std::fs::write(
        &file,
        text.replace("shinatama-verify", "shinatama-verified"),
    )
    .unwrap();
    touch(&file, modified);

    assert!(listed_cwd(&roots, &cache).contains("shinatama-verified"));
}

#[test]
fn a_file_written_a_moment_ago_is_never_taken_from_the_cache() {
    // A clock that ticks every few milliseconds cannot tell two writes in one tick apart.
    let (scratch, roots, file) =
        cached_store("cov-cache-racy", std::time::Duration::from_millis(300));
    let cache = scratch.join("cache");
    let modified = std::fs::metadata(&file).unwrap().modified().unwrap();
    assert!(listed_cwd(&roots, &cache).contains("shinatama-verify"));

    change_in_place(&file);
    touch(&file, modified);

    assert!(
        listed_cwd(&roots, &cache).contains("SHINATAMA-VERIFY"),
        "too recent to trust"
    );
}

#[test]
fn a_damaged_or_unwritable_cache_is_only_a_cold_one() {
    let hour = std::time::Duration::from_secs(3_600);
    let (scratch, roots, _) = cached_store("cov-cache-damaged", hour);
    let cache = scratch.join("cache");
    assert!(listed_cwd(&roots, &cache).contains("shinatama-verify"));
    for entry in std::fs::read_dir(&cache).unwrap().flatten() {
        std::fs::write(entry.path(), b"{broken").unwrap();
    }
    assert!(listed_cwd(&roots, &cache).contains("shinatama-verify"));

    // Where the cache folder cannot be made, there is no cache and nothing fails.
    let blocked = scratch.join("blocked");
    std::fs::write(&blocked, "a file, not a folder").unwrap();
    assert!(listed_cwd(&roots, &blocked.join("cache")).contains("shinatama-verify"));
}

#[test]
fn the_cache_does_not_keep_files_that_are_gone() {
    let hour = std::time::Duration::from_secs(3_600);
    let (scratch, roots, file) = cached_store("cov-cache-forget", hour);
    let cache = scratch.join("cache");
    listed_cwd(&roots, &cache);
    let kept: Vec<String> = std::fs::read_dir(&cache)
        .unwrap()
        .flatten()
        .map(|entry| std::fs::read_to_string(entry.path()).unwrap())
        .collect();
    assert!(
        kept.iter().any(|text| text.contains("shinatama-verify")),
        "the first listing was remembered: {kept:?}"
    );

    std::fs::remove_file(&file).unwrap();
    let listed = mukade::reader::claude_code::list_cached(&roots, Some(&cache)).unwrap();

    assert!(listed.is_empty());
    let kept: Vec<String> = std::fs::read_dir(&cache)
        .unwrap()
        .flatten()
        .map(|entry| std::fs::read_to_string(entry.path()).unwrap())
        .collect();
    assert!(
        kept.iter().all(|text| !text.contains("shinatama-verify")),
        "a listing that no longer has the file no longer remembers it: {kept:?}"
    );
}

#[test]
fn many_session_files_are_listed_whole_and_in_order() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "cov-many");
    let fixture = std::fs::read_to_string(support::source_path(HarnessId::ClaudeCode)).unwrap();
    let project = scratch.join("home/projects/many");
    std::fs::create_dir_all(&project).unwrap();
    let source = support::source_id(HarnessId::ClaudeCode);
    let mut ids = Vec::new();
    for n in 0..40 {
        let id = format!("00000000-0000-4000-8000-{n:012}");
        std::fs::write(
            project.join(format!("{id}.jsonl")),
            fixture.replace(source, &id),
        )
        .unwrap();
        ids.push(id);
    }
    let roots = support::roots(&scratch);

    let first =
        mukade::reader::claude_code::list_cached(&roots, Some(&scratch.join("cache"))).unwrap();
    let second =
        mukade::reader::claude_code::list_cached(&roots, Some(&scratch.join("cache"))).unwrap();
    let uncached = mukade::reader::claude_code::ClaudeCode
        .list(&roots)
        .unwrap();

    let listed: Vec<&str> = first.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(listed, ids, "every file once, in path order");
    assert_eq!(first, second, "the cache changes nothing about the answer");
    assert_eq!(first.len(), uncached.len());
    assert!(first.iter().all(|s| s.cwd.is_some() && s.title.is_some()));
}

// ---------------------------------------------------------------------------------------------
// Archived sessions: `--archived true` (the default) lists them, `--archived false` leaves them
// out, the same way for every harness.

/// The ids `mukade list <harness> <extra…>` printed, in order.
fn listed_ids(harness: HarnessId, scratch: &Path, extra: &[&str]) -> Vec<String> {
    let mut args = vec!["list", harness.as_str()];
    args.extend_from_slice(extra);
    let (code, stdout, stderr) = support::cli_code(scratch, &args);
    assert_eq!(code, Some(0), "list {extra:?} failed: {stderr}");
    stdout
        .lines()
        .map(|line| {
            let row: Value = serde_json::from_str(line).expect("one JSON object per line");
            row["id"].as_str().expect("an id").to_string()
        })
        .collect()
}

#[test]
fn archived_false_leaves_out_the_archived_sessions_of_every_harness_and_true_keeps_them() {
    // Codex: THREAD_B is archived in the state database.
    let codex = codex_home_without_rollouts("cov-archived-codex");
    // OpenCode: one open and one archived root session in the store's database.
    let opencode = support::scratch(HarnessId::OpenCode, "cov-archived-opencode");
    let db = rusqlite::Connection::open(opencode.join("home").join("opencode.db")).unwrap();
    db.execute_batch(&format!(
        "CREATE TABLE session_v2 ({SESSION_COLUMNS});
         INSERT INTO session_v2 VALUES ('ses_open', NULL, 'Open', 'D:/work', 2000, NULL);
         INSERT INTO session_v2 VALUES ('ses_gone', NULL, 'Archived', 'D:/work', 1000, 1500);"
    ))
    .unwrap();
    drop(db);
    // Claude Code and Pi record no archived flag: nothing of theirs is ever left out.
    let claude = support::scratch(HarnessId::ClaudeCode, "cov-archived-claude");
    support::stage_fixture(HarnessId::ClaudeCode, &claude);
    let pi = support::scratch(HarnessId::Pi, "cov-archived-pi");
    support::stage_fixture(HarnessId::Pi, &pi);

    let cases = [
        (
            HarnessId::Codex,
            &codex,
            vec![THREAD_A, THREAD_B],
            vec![THREAD_A],
        ),
        (
            HarnessId::OpenCode,
            &opencode,
            vec!["ses_open", "ses_gone"],
            vec!["ses_open"],
        ),
        (
            HarnessId::ClaudeCode,
            &claude,
            vec![support::source_id(HarnessId::ClaudeCode)],
            vec![support::source_id(HarnessId::ClaudeCode)],
        ),
        (
            HarnessId::Pi,
            &pi,
            vec![support::source_id(HarnessId::Pi)],
            vec![support::source_id(HarnessId::Pi)],
        ),
    ];
    for (harness, scratch, with_archived, without_archived) in cases {
        let sorted = |mut ids: Vec<String>| {
            ids.sort();
            ids
        };
        let expected = |ids: &[&str]| sorted(ids.iter().map(|id| id.to_string()).collect());
        assert_eq!(
            sorted(listed_ids(harness, scratch, &[])),
            expected(&with_archived),
            "{harness:?}: archived sessions are listed by default"
        );
        assert_eq!(
            sorted(listed_ids(harness, scratch, &["--archived", "true"])),
            expected(&with_archived),
            "{harness:?}: --archived true keeps them"
        );
        assert_eq!(
            sorted(listed_ids(harness, scratch, &["--archived", "false"])),
            expected(&without_archived),
            "{harness:?}: --archived false leaves them out"
        );
    }
}

#[test]
fn archived_takes_only_true_or_false() {
    let scratch = codex_home_without_rollouts("cov-archived-value");

    let (code, stdout, stderr) =
        support::cli_code(&scratch, &["list", "codex", "--archived", "maybe"]);

    assert_ne!(code, Some(0), "an unknown value is refused: {stdout}");
    assert!(stdout.is_empty(), "nothing is listed: {stdout}");
    assert!(
        stderr.contains("--archived takes true or false"),
        "the error names the accepted values: {stderr}"
    );
}

#[test]
fn archived_is_refused_outside_list() {
    let scratch = support::scratch(HarnessId::Codex, "cov-archived-dump");
    let id = support::stage_fixture(HarnessId::Codex, &scratch);

    let (code, _, stderr) =
        support::cli_code(&scratch, &["dump", "codex", &id, "--archived", "false"]);

    assert_ne!(code, Some(0));
    assert!(
        stderr.contains("--archived is supported only by list"),
        "{stderr}"
    );
}

#[test]
fn a_codex_rollout_in_archived_sessions_is_listed_as_archived_without_a_state_database() {
    let scratch = support::scratch(HarnessId::Codex, "cov-codex-archived-nodb");
    let home = scratch.join("home");
    let full_id = support::source_id(HarnessId::Codex);
    // `codex archive` moves the rollout out of `sessions/` into a flat `archived_sessions/`.
    let moved = home
        .join("archived_sessions")
        .join(format!("rollout-2026-10-27T16-43-08-{full_id}.jsonl"));
    std::fs::create_dir_all(moved.parent().unwrap()).unwrap();
    std::fs::copy(support::source_path(HarnessId::Codex), &moved).unwrap();

    let rows = list(HarnessId::Codex, &scratch);

    assert_eq!(rows.len(), 1, "the archived rollout is listed: {rows:?}");
    let r = &rows[0];
    assert_eq!(r["id"], full_id);
    assert_eq!(r["archived"], true, "its folder says it is archived: {r}");
    assert!(r["unreadable"].is_null(), "the file is there: {r}");
    assert_eq!(r["location"], moved.to_string_lossy().as_ref());
    assert!(
        listed_ids(HarnessId::Codex, &scratch, &["--archived", "false"]).is_empty(),
        "--archived false leaves it out"
    );
    let (ok, stdout, stderr) = support::cli(&scratch, &["dump", "codex", &full_id[..8]]);
    assert!(ok, "dump reads it in archived_sessions/: {stderr}");
    assert!(
        stdout.contains(full_id),
        "the dumped session is that thread"
    );
}

// ---------------------------------------------------------------------------------------------
// A paginated Codex thread that was archived: its page continues from a base file of the same
// home, under `sessions/` or `archived_sessions/`, and never from a file outside the home.

const PAGED: &str = "019e0000-0000-7000-8000-0000000000d1";
const PAGE: &str = "019e0000-0000-7000-8000-0000000000d2";

fn rollout_record(ordinal: u64, kind: &str, payload: Value) -> String {
    serde_json::json!({
        "timestamp": format!("2026-10-28T10:{ordinal:02}:00.000Z"), "ordinal": ordinal,
        "type": kind, "payload": payload,
    })
    .to_string()
        + "\n"
}

fn rollout_say(ordinal: u64, role: &str, text: &str) -> String {
    let block = if role == "user" {
        "input_text"
    } else {
        "output_text"
    };
    rollout_record(
        ordinal,
        "response_item",
        serde_json::json!({"type": "message", "role": role, "content": [{"type": block, "text": text}]}),
    )
}

/// The thread `PAGED` over two files, as Codex 0.165 writes a thread whose later turns were
/// redone: the base file's text, and the text of a page continuing from its first three records.
fn paged_thread() -> (String, String) {
    let meta = serde_json::json!({"id": PAGED, "session_id": PAGED, "cwd": "D:/work/paged",
                                  "cli_version": "0.165.0", "history_mode": "paginated"});
    let kept = rollout_record(0, "session_meta", meta.clone())
        + &rollout_say(1, "user", "first")
        + &rollout_say(2, "assistant", "one");
    let base = kept.clone() + &rollout_say(3, "user", "abandoned");
    let mut page_meta = meta;
    page_meta["history_base"] = serde_json::json!({
        "thread_id": PAGED, "end_ordinal_exclusive": 3, "end_byte_offset": kept.len(),
    });
    let page = rollout_record(3, "session_meta", page_meta)
        + &rollout_say(4, "user", "redo")
        + &rollout_say(5, "assistant", "two");
    (base, page)
}

fn base_name() -> String {
    format!("rollout-2026-10-28T10-38-11-{PAGED}.jsonl")
}

fn page_name() -> String {
    format!("rollout-2026-10-28T13-29-27-{PAGED}_{PAGE}.jsonl")
}

fn write(file: &Path, text: &str) {
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}

#[test]
fn an_archived_page_continues_from_its_base_in_either_folder_of_its_home() {
    let scratch = support::scratch(HarnessId::Codex, "cov-codex-archived-page");
    let home = scratch.join("home");
    let (base, page) = paged_thread();
    write(&home.join("sessions/2026/10/28").join(base_name()), &base);
    write(&home.join("archived_sessions").join(page_name()), &page);

    let session = mukade::reader::codex::Codex
        .read(PAGED, &support::roots(&scratch))
        .expect("the archived page reads with its base");

    assert_eq!(support::user_texts(&session), ["first", "redo"]);
}

#[test]
fn an_archived_page_never_takes_its_base_from_outside_its_home() {
    let scratch = support::scratch(HarnessId::Codex, "cov-codex-archived-decoy");
    let home = scratch.join("home");
    let (base, page) = paged_thread();
    write(&home.join("archived_sessions").join(page_name()), &page);
    // A copy of the base beside the home, not in it: a backup, another home.
    write(&scratch.join("elsewhere").join(base_name()), &base);

    let error = mukade::reader::codex::Codex
        .read(PAGED, &support::roots(&scratch))
        .expect_err("the base is not in the page's home");

    let error = format!("{error:#}");
    assert!(error.contains("history_base"), "{error}");
    assert!(
        error.contains("archived_sessions"),
        "the error names the folders it looked in: {error}"
    );
}

// ---------------------------------------------------------------------------------------------
// `timestamp`: the last activity, for every harness.

/// Pi's list reads a session file's two ends: the newest record time there is its last activity.
#[test]
fn a_pi_row_says_when_the_session_was_last_active() {
    let scratch = support::scratch(HarnessId::Pi, "cov-pi-updated");
    support::stage_fixture(HarnessId::Pi, &scratch);
    let id = "01a17e17-9f07-743d-8072-8b3b13d65ab5";
    let last = std::fs::read_to_string(support::source_path(HarnessId::Pi))
        .unwrap()
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(|row| row["timestamp"].as_str().map(str::to_owned))
        .max()
        .expect("Pi records carry their time");
    let rows = list(HarnessId::Pi, &scratch);
    assert_eq!(row(&rows, id)["timestamp"], last.as_str());
}

/// A Codex rollout is only appended to: without a database row its file time is its last activity.
#[test]
fn a_codex_row_without_a_database_says_when_its_rollout_was_last_written() {
    let scratch = support::scratch(HarnessId::Codex, "cov-codex-updated");
    support::stage_fixture(HarnessId::Codex, &scratch);
    let rows = list(HarnessId::Codex, &scratch);
    let stamp = row(&rows, "01a17e07-93f7-7a73-85c7-9f0205836600")["timestamp"]
        .as_str()
        .expect("a time")
        .to_owned();
    assert!(
        chrono::DateTime::parse_from_rfc3339(&stamp).is_ok(),
        "{stamp} is an RFC 3339 time"
    );
}
