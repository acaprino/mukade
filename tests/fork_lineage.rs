//! How a fork names its origin: the model's `ParentKind::Fork { at }`, and each reader's lineage
//! (design `docs/superpowers/specs/2026-11-12-forks-branches-design.md`, part A).

use mukade::model::ParentKind;

#[test]
fn a_fork_may_say_where_it_left_its_parent_and_the_old_form_still_reads() {
    assert_eq!(ParentKind::fork(), ParentKind::Fork { at: None });
    assert_eq!(ParentKind::fork().name(), "fork");
    // The unit form a dump written before this change holds.
    let old: ParentKind = serde_json::from_str(r#"{"kind":"fork"}"#).unwrap();
    assert_eq!(old, ParentKind::fork());
    // No boundary known: the document is as it was.
    assert_eq!(
        serde_json::to_string(&ParentKind::fork()).unwrap(),
        r#"{"kind":"fork"}"#
    );
    let at = ParentKind::Fork {
        at: Some("entry-7".into()),
    };
    let text = serde_json::to_string(&at).unwrap();
    assert_eq!(text, r#"{"kind":"fork","at":"entry-7"}"#);
    assert_eq!(serde_json::from_str::<ParentKind>(&text).unwrap(), at);
}

use mukade::model::HarnessId;
use mukade::reader::{Reader, SessionInfo};
use mukade::roots::StoreRoots;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

mod support;

fn write_lines(path: &Path, rows: &[Value]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut text = String::new();
    for row in rows {
        text.push_str(&row.to_string());
        text.push('\n');
    }
    std::fs::write(path, text).unwrap();
}

fn age(path: &Path) {
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(old)
        .unwrap();
}

fn by_id<'a>(listed: &'a [SessionInfo], id: &str) -> &'a SessionInfo {
    listed
        .iter()
        .find(|info| info.id == id)
        .unwrap_or_else(|| panic!("{id} in {listed:#?}"))
}

// ---------------------------------------------------------------------------------------------
// Claude Code: `forkedFrom` on every copied row

const CLAUDE_ORIGIN: &str = "11111111-1111-4111-8111-111111111111";
const CLAUDE_FORK: &str = "22222222-2222-4222-8222-222222222222";

fn claude_rows(session: &str, forked: Option<&str>) -> Vec<Value> {
    let mut rows = vec![
        json!({"type": "user", "uuid": "u-1", "parentUuid": null, "sessionId": session,
            "timestamp": "2026-11-12T10:00:00.000Z", "cwd": "/work", "isSidechain": false,
            "message": {"role": "user", "content": "the first question"}}),
        json!({"type": "assistant", "uuid": "a-1", "parentUuid": "u-1", "sessionId": session,
            "timestamp": "2026-11-12T10:00:01.000Z", "isSidechain": false,
            "message": {"id": "msg_1", "role": "assistant", "model": "claude-x", "stop_reason": "end_turn",
                "content": [{"type": "text", "text": "the first answer"}]}}),
    ];
    if let Some(origin) = forked {
        for row in &mut rows {
            let uuid = row["uuid"].clone();
            row["forkedFrom"] = json!({"sessionId": origin, "messageUuid": uuid});
        }
    }
    rows
}

fn claude_store(tag: &str) -> (StoreRoots, PathBuf) {
    claude_store_in(&support::scratch(HarnessId::ClaudeCode, tag))
}

fn claude_store_in(scratch: &Path) -> (StoreRoots, PathBuf) {
    let roots = support::roots(scratch);
    let project = scratch.join("home/projects/p");
    write_lines(
        &project.join(format!("{CLAUDE_ORIGIN}.jsonl")),
        &claude_rows(CLAUDE_ORIGIN, None),
    );
    let fork = project.join(format!("{CLAUDE_FORK}.jsonl"));
    write_lines(&fork, &claude_rows(CLAUDE_FORK, Some(CLAUDE_ORIGIN)));
    (roots, fork)
}

#[test]
fn a_claude_fork_names_its_origin_in_the_listing_and_when_read() {
    use mukade::reader::claude_code::ClaudeCode;
    let (roots, _) = claude_store("claude-fork");
    let listed = ClaudeCode.list(&roots).unwrap();
    let fork = by_id(&listed, CLAUDE_FORK);
    assert_eq!(fork.parent.as_deref(), Some(CLAUDE_ORIGIN));
    // The last row the copy holds, in the origin's own uuids.
    assert_eq!(
        fork.parent_kind,
        Some(ParentKind::Fork {
            at: Some("a-1".into())
        })
    );
    let origin = by_id(&listed, CLAUDE_ORIGIN);
    assert_eq!((&origin.parent, &origin.parent_kind), (&None, &None));

    let read = ClaudeCode.read(CLAUDE_FORK, &roots).unwrap().meta;
    assert_eq!(read.parent.as_deref(), Some(CLAUDE_ORIGIN));
    assert_eq!(read.parent_kind, fork.parent_kind);
    let plain = ClaudeCode.read(CLAUDE_ORIGIN, &roots).unwrap().meta;
    assert_eq!((plain.parent, plain.parent_kind), (None, None));
}

/// A fork as long as the rows of `copied` from `origins` (the origin of each copied row, in order),
/// then `own` rows of its own: user and assistant turns, each padded to `pad` bytes.
fn claude_long_fork(origins: &[(&str, usize)], own: usize, pad: usize) -> Vec<Value> {
    let filler = "x".repeat(pad);
    let mut rows = Vec::new();
    let mut parent: Option<String> = None;
    let mut n = 0usize;
    let mut push = |rows: &mut Vec<Value>, origin: Option<&str>| {
        let uuid = format!("c-{n}");
        let (kind, message) = if n.is_multiple_of(2) {
            (
                "user",
                json!({"role": "user", "content": format!("question {n} {filler}")}),
            )
        } else {
            (
                "assistant",
                json!({"id": format!("msg_{n}"), "role": "assistant",
                "model": "claude-x", "stop_reason": "end_turn",
                "content": [{"type": "text", "text": format!("answer {n} {filler}")}]}),
            )
        };
        let mut row = json!({"type": kind, "uuid": uuid, "parentUuid": parent,
            "sessionId": CLAUDE_FORK, "timestamp": "2026-11-12T10:00:00.000Z", "cwd": "/work",
            "isSidechain": false, "message": message});
        if let Some(origin) = origin {
            row["forkedFrom"] = json!({"sessionId": origin, "messageUuid": uuid});
        }
        parent = Some(uuid);
        rows.push(row);
        n += 1;
    };
    for &(origin, count) in origins {
        for _ in 0..count {
            push(&mut rows, Some(origin));
        }
    }
    for _ in 0..own {
        push(&mut rows, None);
    }
    rows
}

/// A fork longer than the two ends a listing peeks: its last copied row sits between them, and
/// the listing names the same split point a read does.
#[test]
fn a_long_claude_fork_lists_the_split_point_a_read_gives() {
    use mukade::reader::claude_code::ClaudeCode;
    let scratch = support::scratch(HarnessId::ClaudeCode, "claude-fork-long");
    let roots = support::roots(&scratch);
    let fork = scratch.join(format!("home/projects/p/{CLAUDE_FORK}.jsonl"));
    write_lines(&fork, &claude_long_fork(&[(CLAUDE_ORIGIN, 200)], 150, 3000));
    assert!(std::fs::metadata(&fork).unwrap().len() > 1024 * 1024);
    let expected = Some(ParentKind::Fork {
        at: Some("c-199".into()),
    });
    let read = ClaudeCode.read(CLAUDE_FORK, &roots).unwrap().meta;
    assert_eq!(read.parent_kind, expected);
    let listed = ClaudeCode.list(&roots).unwrap();
    assert_eq!(by_id(&listed, CLAUDE_FORK).parent_kind, expected);
}

/// A fork of a fork: the rows it took from its parent may keep the `forkedFrom` of the session
/// that parent was forked from. The last copied rows name the session the copy came from.
#[test]
fn a_fork_of_a_fork_names_the_session_its_last_copied_rows_came_from() {
    use mukade::reader::claude_code::ClaudeCode;
    const MIDDLE: &str = "33333333-3333-4333-8333-333333333333";
    let scratch = support::scratch(HarnessId::ClaudeCode, "claude-fork-of-fork");
    let roots = support::roots(&scratch);
    let fork = scratch.join(format!("home/projects/p/{CLAUDE_FORK}.jsonl"));
    write_lines(
        &fork,
        &claude_long_fork(&[(CLAUDE_ORIGIN, 2), (MIDDLE, 2)], 2, 10),
    );
    let read = ClaudeCode.read(CLAUDE_FORK, &roots).unwrap().meta;
    assert_eq!(read.parent.as_deref(), Some(MIDDLE));
    assert_eq!(
        read.parent_kind,
        Some(ParentKind::Fork {
            at: Some("c-3".into())
        })
    );
    let listed = ClaudeCode.list(&roots).unwrap();
    assert_eq!(by_id(&listed, CLAUDE_FORK).parent.as_deref(), Some(MIDDLE));
}

/// A copied row that names its origin but no `messageUuid` leaves the split point where it was.
#[test]
fn a_copied_row_without_a_message_uuid_keeps_the_split_point() {
    use mukade::reader::claude_code::ClaudeCode;
    let scratch = support::scratch(HarnessId::ClaudeCode, "claude-fork-no-uuid");
    let roots = support::roots(&scratch);
    let fork = scratch.join(format!("home/projects/p/{CLAUDE_FORK}.jsonl"));
    let mut rows = claude_long_fork(&[(CLAUDE_ORIGIN, 3)], 1, 10);
    rows[2]["forkedFrom"] = json!({"sessionId": CLAUDE_ORIGIN});
    write_lines(&fork, &rows);
    let read = ClaudeCode.read(CLAUDE_FORK, &roots).unwrap().meta;
    assert_eq!(
        read.parent_kind,
        Some(ParentKind::Fork {
            at: Some("c-1".into())
        })
    );
}

#[test]
fn a_claude_listing_remembers_a_fork_in_its_cache() {
    use mukade::reader::claude_code::list_cached;
    let (roots, fork) = claude_store("claude-fork-cache");
    age(&fork);
    let table = support::scratch(HarnessId::ClaudeCode, "claude-fork-cache-table");
    for _ in 0..2 {
        let listed = list_cached(&roots, Some(&table)).unwrap();
        let info = by_id(&listed, CLAUDE_FORK);
        assert_eq!(info.parent.as_deref(), Some(CLAUDE_ORIGIN));
        assert_eq!(
            info.parent_kind,
            Some(ParentKind::Fork {
                at: Some("a-1".into())
            })
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Codex: `session_meta.forked_from_id`

const CODEX_ORIGIN: &str = "01a10000-0000-7000-8000-00000000000a";
const CODEX_FORK: &str = "01a10000-0000-7000-8000-00000000000b";
const CODEX_SUBAGENT: &str = "01a10000-0000-7000-8000-00000000000c";

fn codex_record(ordinal: u64, kind: &str, payload: Value) -> Value {
    json!({"timestamp": format!("2026-11-12T10:00:{ordinal:02}.000Z"), "ordinal": ordinal,
        "type": kind, "payload": payload})
}

fn codex_say(ordinal: u64, role: &str, text: &str) -> Value {
    let block = if role == "user" {
        "input_text"
    } else {
        "output_text"
    };
    codex_record(
        ordinal,
        "response_item",
        json!({"type": "message", "role": role, "content": [{"type": block, "text": text}]}),
    )
}

fn codex_rollout(home: &Path, id: &str, meta: Value, rest: Vec<Value>) {
    let mut payload = json!({"id": id, "cwd": "/work", "cli_version": "0.167.0",
        "timestamp": "2026-11-12T10:00:00.000Z"});
    for (key, value) in meta.as_object().unwrap() {
        payload[key] = value.clone();
    }
    let mut rows = vec![codex_record(0, "session_meta", payload)];
    rows.extend(rest);
    write_lines(
        &home.join(format!(
            "sessions/2026/11/12/rollout-2026-11-12T10-00-00-{id}.jsonl"
        )),
        &rows,
    );
}

fn codex_store(tag: &str) -> StoreRoots {
    let scratch = support::scratch(HarnessId::Codex, tag);
    let roots = support::roots(&scratch);
    let home = roots.codex_home.clone().unwrap();
    let history = |last: &str| {
        vec![
            codex_say(1, "user", "first question"),
            codex_say(2, "assistant", "first answer"),
            codex_say(3, "user", last),
        ]
    };
    codex_rollout(&home, CODEX_ORIGIN, json!({}), history("origin goes on"));
    // A copy fork: the history is in its own rollout.
    codex_rollout(
        &home,
        CODEX_FORK,
        json!({"forked_from_id": CODEX_ORIGIN, "forked_from_ordinal_exclusive": 3}),
        history("fork goes elsewhere"),
    );
    // A subagent that started from its parent's turns: a spawn, not a fork.
    codex_rollout(
        &home,
        CODEX_SUBAGENT,
        json!({"forked_from_id": CODEX_ORIGIN, "parent_thread_id": CODEX_ORIGIN}),
        history("child task"),
    );
    roots
}

/// Codex's fork ordinal is exclusive; `at` is the last record the copy holds, the one before it.
/// A fork that copied nothing names no record.
#[test]
fn a_codex_fork_names_the_last_record_it_copied() {
    use mukade::reader::codex::Codex;
    let scratch = support::scratch(HarnessId::Codex, "codex-fork-at");
    let roots = support::roots(&scratch);
    let home = roots.codex_home.clone().unwrap();
    codex_rollout(
        &home,
        CODEX_FORK,
        json!({"forked_from_id": CODEX_ORIGIN, "forked_from_ordinal_exclusive": 0}),
        vec![codex_say(1, "user", "fresh start")],
    );
    let listed = Codex.list_all(&roots).unwrap();
    let fork = by_id(&listed, CODEX_FORK);
    assert_eq!(fork.parent.as_deref(), Some(CODEX_ORIGIN));
    assert_eq!(fork.parent_kind, Some(ParentKind::Fork { at: None }));
}

#[test]
fn a_codex_fork_names_its_origin_and_a_forked_subagent_stays_a_spawn() {
    use mukade::reader::codex::Codex;
    let roots = codex_store("codex-fork");
    let listed = Codex.list_all(&roots).unwrap();
    let fork = by_id(&listed, CODEX_FORK);
    assert_eq!(fork.parent.as_deref(), Some(CODEX_ORIGIN));
    assert_eq!(
        fork.parent_kind,
        Some(ParentKind::Fork {
            at: Some("2".into())
        })
    );
    let child = by_id(&listed, CODEX_SUBAGENT);
    assert_eq!(child.parent.as_deref(), Some(CODEX_ORIGIN));
    assert!(matches!(
        child.parent_kind,
        Some(ParentKind::Spawn {
            inherits_context: true,
            ..
        })
    ));
    let origin = by_id(&listed, CODEX_ORIGIN);
    assert_eq!((&origin.parent, &origin.parent_kind), (&None, &None));

    let read = Codex.read(CODEX_FORK, &roots).unwrap().meta;
    assert_eq!(read.parent.as_deref(), Some(CODEX_ORIGIN));
    assert_eq!(read.parent_kind, fork.parent_kind);
    let child = Codex.read(CODEX_SUBAGENT, &roots).unwrap().meta;
    assert!(matches!(
        child.parent_kind,
        Some(ParentKind::Spawn {
            inherits_context: true,
            ..
        })
    ));
    // `list` is the roots: a fork is one; the subagent is not (the database is what says so, and
    // this home has none, so it is listed too).
    assert!(
        Codex
            .list(&roots)
            .unwrap()
            .iter()
            .any(|i| i.id == CODEX_FORK)
    );
}

// ---------------------------------------------------------------------------------------------
// OpenCode: `fork_session_id` with its `fork_boundary`

#[test]
fn an_opencode_fork_in_the_listing_names_the_message_it_ends_at() {
    use mukade::reader::opencode::Opencode;
    let scratch = support::scratch(HarnessId::OpenCode, "opencode-fork");
    let roots = support::roots(&scratch);
    let db_path = roots.opencode_db.clone().unwrap();
    let db = rusqlite::Connection::open(&db_path).unwrap();
    db.execute_batch(
        "CREATE TABLE session_v2 (
             id TEXT PRIMARY KEY, title TEXT, directory TEXT, time_updated INTEGER,
             time_archived INTEGER, parent_id TEXT, fork_session_id TEXT, fork_boundary TEXT
         );
         INSERT INTO session_v2 VALUES ('root', 'Root', '/work', 1, NULL, NULL, NULL, NULL);
         INSERT INTO session_v2 VALUES ('fork', 'Fork', '/work', 3, NULL, NULL, 'root',
             '{\"type\":\"through\",\"messageID\":\"msg_7\"}');",
    )
    .unwrap();
    drop(db);
    let listed = Opencode.list(&roots).unwrap();
    let fork = by_id(&listed, "fork");
    assert_eq!(fork.parent.as_deref(), Some("root"));
    assert_eq!(
        fork.parent_kind,
        Some(ParentKind::Fork {
            at: Some("msg_7".into())
        })
    );
}

/// An OpenCode store whose `root` session has three messages, in `seq` order `msg_b`, `msg_a`,
/// `msg_c` (not the order of their ids), another session's message between them, and forks of
/// `root` with a `before` boundary at each message, at one not in the store, and through one.
fn opencode_before_forks(db_path: &Path) {
    let db = rusqlite::Connection::open(db_path).unwrap();
    db.execute_batch(
        "CREATE TABLE session_v2 (
             id TEXT PRIMARY KEY, title TEXT, directory TEXT, time_updated INTEGER,
             time_archived INTEGER, parent_id TEXT, fork_session_id TEXT, fork_boundary TEXT
         );
         CREATE TABLE session_message (
             id TEXT PRIMARY KEY, session_id TEXT, type TEXT, seq INTEGER, data TEXT
         );
         INSERT INTO session_v2 VALUES ('root', 'Root', '/work', 1, NULL, NULL, NULL, NULL);
         INSERT INTO session_v2 VALUES ('other', 'Other', '/work', 2, NULL, NULL, NULL, NULL);
         INSERT INTO session_message VALUES ('msg_b', 'root', 'user', 1, '{}');
         INSERT INTO session_message VALUES ('msg_x', 'other', 'user', 2, '{}');
         INSERT INTO session_message VALUES ('msg_a', 'root', 'assistant', 3, '{}');
         INSERT INTO session_message VALUES ('msg_c', 'root', 'user', 5, '{}');
         INSERT INTO session_v2 VALUES ('before_c', 'F', '/work', 3, NULL, NULL, 'root',
             '{\"type\":\"before\",\"messageID\":\"msg_c\"}');
         INSERT INTO session_v2 VALUES ('before_a', 'F', '/work', 4, NULL, NULL, 'root',
             '{\"type\":\"before\",\"messageID\":\"msg_a\"}');
         INSERT INTO session_v2 VALUES ('before_b', 'F', '/work', 5, NULL, NULL, 'root',
             '{\"type\":\"before\",\"messageID\":\"msg_b\"}');
         INSERT INTO session_v2 VALUES ('before_gone', 'F', '/work', 6, NULL, NULL, 'root',
             '{\"type\":\"before\",\"messageID\":\"msg_gone\"}');
         INSERT INTO session_v2 VALUES ('through_a', 'F', '/work', 7, NULL, NULL, 'root',
             '{\"type\":\"through\",\"messageID\":\"msg_a\"}');",
    )
    .unwrap();
}

/// A `before` boundary names the message after the fork's last: the fork ends at the parent's
/// message just before it in `seq` order, as OpenCode 2.2.5 copies it. Before the first message,
/// or at a message the store does not hold, nothing is named.
#[test]
fn an_opencode_fork_before_a_message_ends_at_the_parents_message_before_it() {
    use mukade::reader::opencode::Opencode;
    let scratch = support::scratch(HarnessId::OpenCode, "opencode-fork-before");
    let roots = support::roots(&scratch);
    opencode_before_forks(roots.opencode_db.as_deref().unwrap());
    let listed = Opencode.list_all(&roots).unwrap();
    let at = |id: &str| match &by_id(&listed, id).parent_kind {
        Some(ParentKind::Fork { at }) => at.clone(),
        other => panic!("{id} is not a fork: {other:?}"),
    };
    assert_eq!(at("before_c").as_deref(), Some("msg_a"));
    assert_eq!(at("before_a").as_deref(), Some("msg_b"));
    assert_eq!(at("before_b"), None);
    assert_eq!(at("before_gone"), None);
    assert_eq!(at("through_a").as_deref(), Some("msg_a"));
}

/// The same when the fork is read through `opencode session export`: its `info.fork.boundary`
/// is looked up in the store the export came from, and the dump says where the fork ends.
#[test]
fn an_exported_opencode_fork_before_a_message_ends_at_the_parents_message_before_it() {
    let scratch = support::scratch(HarnessId::OpenCode, "opencode-fork-before-export");
    let database = scratch.join("opencode.db");
    opencode_before_forks(&database);
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("opencode")
        .join("source")
        .join("session-export.json");
    let mut doc: Value = serde_json::from_str(&std::fs::read_to_string(fixture).unwrap()).unwrap();
    doc["info"]["id"] = "before_c".into();
    doc["info"]["fork"] = json!({"sessionID": "root",
        "boundary": {"type": "before", "messageID": "msg_c"}});
    let export = scratch.join("export.json");
    std::fs::write(&export, doc.to_string()).unwrap();
    let opencode = support::launcher(
        &scratch.join("bin"),
        "opencode",
        &format!("type \"{}\"\r\nexit /b 0", export.display()),
        &format!("cat \"{}\"", export.display()),
    );
    let (code, stdout, stderr) = support::cli_in(
        &scratch,
        &scratch,
        &[
            "dump",
            "opencode",
            "before_c",
            "--root",
            database.to_str().unwrap(),
        ],
        &[("MUKADE_OPENCODE_EXE", opencode.to_str().unwrap())],
    );
    assert_eq!(code, Some(0), "{stderr}");
    let dumped: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(dumped["meta"]["parent"], "root");
    assert_eq!(
        dumped["meta"]["parent_kind"],
        json!({"kind": "fork", "at": "msg_a"})
    );
}

// ---------------------------------------------------------------------------------------------
// Pi: `parentSession` is a file path; the parent is the id in that file's header

fn pi_header(id: &str, parent: Option<&str>) -> Value {
    let mut header = json!({"type": "session", "version": 3, "id": id,
        "timestamp": "2026-11-12T10:00:00.000Z", "cwd": "/work"});
    if let Some(parent) = parent {
        header["parentSession"] = parent.into();
    }
    header
}

fn pi_entry(id: &str, parent: Option<&str>, role: &str, text: &str) -> Value {
    json!({"type": "message", "id": id, "parentId": parent, "timestamp": "2026-11-12T10:00:01.000Z",
        "message": {"role": role, "content": [{"type": "text", "text": text}]}})
}

const PI_ORIGIN: &str = "01a20000-0000-7000-8000-00000000000a";
const PI_FORK: &str = "01a20000-0000-7000-8000-00000000000b";
const PI_LOOSE: &str = "01a20000-0000-7000-8000-00000000000c";

#[test]
fn a_pi_forks_parent_is_the_id_in_its_origins_header_in_the_listing_and_when_read() {
    use mukade::reader::pi::Pi;
    let scratch = support::scratch(HarnessId::Pi, "pi-fork");
    let roots = support::roots(&scratch);
    let dir = scratch.join("home/sessions/--work--");
    let origin = dir.join(format!("2026-11-12T10-00-00-000Z_{PI_ORIGIN}.jsonl"));
    write_lines(
        &origin,
        &[
            pi_header(PI_ORIGIN, None),
            pi_entry("e1", None, "user", "one"),
        ],
    );
    let fork = dir.join(format!("2026-11-12T10-05-00-000Z_{PI_FORK}.jsonl"));
    write_lines(
        &fork,
        &[
            pi_header(PI_FORK, Some(origin.to_str().unwrap())),
            pi_entry("e1", None, "user", "one"),
        ],
    );
    // A parent that is not a file here stays as written: no new field, no guess.
    let loose = dir.join(format!("2026-11-12T10-06-00-000Z_{PI_LOOSE}.jsonl"));
    write_lines(
        &loose,
        &[
            pi_header(PI_LOOSE, Some("/gone/elsewhere.jsonl")),
            pi_entry("e1", None, "user", "one"),
        ],
    );

    let listed = Pi.list(&roots).unwrap();
    let info = by_id(&listed, PI_FORK);
    assert_eq!(info.parent.as_deref(), Some(PI_ORIGIN));
    assert_eq!(info.parent_kind, Some(ParentKind::fork()));
    let read = Pi.read(PI_FORK, &roots).unwrap().meta;
    assert_eq!(read.parent, info.parent);
    assert_eq!(read.parent_kind, info.parent_kind);
    let loose = by_id(&listed, PI_LOOSE);
    assert_eq!(loose.parent.as_deref(), Some("/gone/elsewhere.jsonl"));
    assert_eq!(Pi.read(PI_LOOSE, &roots).unwrap().meta.parent, loose.parent);
    assert_eq!(by_id(&listed, PI_ORIGIN).parent, None);
}

// ---------------------------------------------------------------------------------------------
// Codex: what a replay drops

fn codex_session_of(roots: &StoreRoots, id: &str, all: bool) -> mukade::model::Session {
    use mukade::reader::codex::{Branches, read_with};
    read_with(
        id,
        roots,
        if all { Branches::All } else { Branches::Active },
    )
    .unwrap()
}

fn texts_of(session: &mukade::model::Session) -> Vec<String> {
    use mukade::model::Kind;
    session
        .conversation()
        .map(|entry| match &entry.kind {
            Kind::User { text, .. } => format!("user: {text}"),
            Kind::Step { text, .. } => format!("assistant: {}", text.join(" ")),
            other => format!("{other:?}"),
        })
        .collect()
}

fn off_branch(session: &mukade::model::Session) -> usize {
    use mukade::model::{Kind, MetaKind};
    session
        .entries
        .iter()
        .filter(|entry| {
            matches!(
                entry.kind,
                Kind::Meta {
                    kind: MetaKind::OffBranch
                }
            )
        })
        .count()
}

/// Codex replays `thread_rolled_back { num_turns }` by dropping the last `num_turns` user turns
/// (and what followed them) of what it had so far: they are an abandoned branch, not history.
#[test]
fn a_rolled_back_turn_is_an_abandoned_branch_and_not_history() {
    let scratch = support::scratch(HarnessId::Codex, "codex-rollback");
    let roots = support::roots(&scratch);
    let home = roots.codex_home.clone().unwrap();
    codex_rollout(
        &home,
        CODEX_ORIGIN,
        json!({}),
        vec![
            codex_say(1, "user", "q1"),
            codex_say(2, "assistant", "a1"),
            codex_say(3, "user", "q2"),
            codex_say(4, "assistant", "a2"),
            codex_say(5, "user", "q3"),
            codex_say(6, "assistant", "a3"),
            codex_record(
                7,
                "event_msg",
                json!({"type": "thread_rolled_back", "num_turns": 2}),
            ),
            codex_say(8, "user", "q4"),
            codex_say(9, "assistant", "a4"),
            // A second rollback counts the turns that are still there.
            codex_record(
                10,
                "event_msg",
                json!({"type": "thread_rolled_back", "num_turns": 1}),
            ),
            codex_say(11, "user", "q5"),
        ],
    );
    let session = codex_session_of(&roots, CODEX_ORIGIN, false);
    assert_eq!(
        texts_of(&session),
        ["user: q1", "assistant: a1", "user: q5"]
    );
    // q2, a2, q3, a3 and then q4, a4: nothing is lost, each record is kept as an abandoned one.
    assert_eq!(off_branch(&session), 6);
    // A rollback of more turns than there are drops them all.
    codex_rollout(
        &home,
        CODEX_FORK,
        json!({}),
        vec![
            codex_say(1, "user", "only"),
            codex_record(
                2,
                "event_msg",
                json!({"type": "thread_rolled_back", "num_turns": 9}),
            ),
        ],
    );
    let session = codex_session_of(&roots, CODEX_FORK, false);
    assert!(texts_of(&session).is_empty(), "{:?}", texts_of(&session));
    assert_eq!(off_branch(&session), 1);
}

/// A fork that continues its origin by reference (`history_base` names the origin's file): the
/// origin's records up to the cut are its history; what the origin did after them is the
/// origin's, not a branch the fork left.
#[test]
fn a_referenced_fork_reads_the_origins_prefix_and_leaves_no_branch() {
    let scratch = support::scratch(HarnessId::Codex, "codex-referenced-fork");
    let roots = support::roots(&scratch);
    let home = roots.codex_home.clone().unwrap();
    let origin_rows = vec![
        codex_record(
            0,
            "session_meta",
            json!({"id": CODEX_ORIGIN, "cwd": "/work", "cli_version": "0.167.0",
                "timestamp": "2026-11-12T10:00:00.000Z"}),
        ),
        codex_say(1, "user", "q1"),
        codex_say(2, "assistant", "a1"),
        codex_say(3, "user", "origin q2"),
        codex_say(4, "assistant", "origin a2"),
    ];
    let origin_path = home.join(format!(
        "sessions/2026/11/12/rollout-2026-11-12T10-00-00-{CODEX_ORIGIN}.jsonl"
    ));
    write_lines(&origin_path, &origin_rows);
    let cut: usize = origin_rows[..3]
        .iter()
        .map(|row| row.to_string().len() + 1)
        .sum();
    codex_rollout(
        &home,
        CODEX_FORK,
        json!({"forked_from_id": CODEX_ORIGIN,
            "history_base": {"thread_id": CODEX_ORIGIN, "end_ordinal_exclusive": 3,
                "end_byte_offset": cut}}),
        vec![
            codex_say(3, "user", "fork q2"),
            codex_say(4, "assistant", "fork a2"),
        ],
    );
    for all in [false, true] {
        let session = codex_session_of(&roots, CODEX_FORK, all);
        assert_eq!(
            texts_of(&session),
            [
                "user: q1",
                "assistant: a1",
                "user: fork q2",
                "assistant: fork a2"
            ],
            "all: {all}"
        );
        assert_eq!(off_branch(&session), 0, "all: {all}");
        assert_eq!(session.meta.parent.as_deref(), Some(CODEX_ORIGIN));
        assert_eq!(
            session.meta.parent_kind,
            Some(ParentKind::Fork {
                at: Some("2".into())
            })
        );
    }
}

/// A subagent started with its parent's turns copies them into its own rollout, after the
/// parent's own `session_meta`: that record is bookkeeping, the turns are what the child saw.
#[test]
fn a_forked_subagents_copied_parent_header_is_not_conversation() {
    let scratch = support::scratch(HarnessId::Codex, "codex-forked-subagent");
    let roots = support::roots(&scratch);
    let home = roots.codex_home.clone().unwrap();
    codex_rollout(
        &home,
        CODEX_SUBAGENT,
        json!({"forked_from_id": CODEX_ORIGIN, "parent_thread_id": CODEX_ORIGIN}),
        vec![
            codex_record(
                1,
                "session_meta",
                json!({"id": CODEX_ORIGIN, "cwd": "/parent", "cli_version": "0.167.0"}),
            ),
            codex_say(2, "user", "the parent's prompt"),
            codex_say(3, "assistant", "the parent's answer"),
            codex_say(4, "user", "You are now a newly spawned child agent"),
        ],
    );
    let session = codex_session_of(&roots, CODEX_SUBAGENT, false);
    assert_eq!(session.meta.id, CODEX_SUBAGENT);
    assert_eq!(session.meta.cwd.as_deref(), Some("/work"));
    assert_eq!(
        texts_of(&session),
        [
            "user: the parent's prompt",
            "assistant: the parent's answer",
            "user: You are now a newly spawned child agent"
        ]
    );
    assert_eq!(off_branch(&session), 0);
}

// ---------------------------------------------------------------------------------------------
// Counts

#[test]
fn the_inventory_counts_forks_for_claude_code_and_codex() {
    let (claude, _) = claude_store("claude-fork-count");
    let stats = mukade::harnesses::get(HarnessId::ClaudeCode).store_stats(&claude);
    assert_eq!(stats.sessions, Ok(2));
    assert_eq!(stats.forks, Some(Ok(1)));
    let codex = codex_store("codex-fork-count");
    let stats = mukade::harnesses::get(HarnessId::Codex).store_stats(&codex);
    // The origin, the fork and the subagent (this home has no database to say it is not a root).
    assert_eq!(stats.sessions, Ok(3));
    // A subagent that started from its parent's turns is a spawn, not a fork.
    assert_eq!(stats.forks, Some(Ok(1)));
}

// ---------------------------------------------------------------------------------------------
// The manifest says what the source was a fork of

/// A hop of a fork is a self-contained copy; the manifest still says where the source came from.
#[test]
fn the_manifest_names_the_session_a_forked_source_came_from() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "manifest-fork");
    claude_store_in(&scratch);
    let hop = |id: &str| {
        let (code, stdout, stderr) = support::cli_code(
            &scratch,
            &["hop", "claude_code", id, "pi", "--cwd", "/work"],
        );
        assert_eq!(code, Some(0), "{stdout}{stderr}");
        let receipt: Value = serde_json::from_str(stdout.trim().lines().last().unwrap()).unwrap();
        receipt["manifest"]["source"].clone()
    };
    let fork = hop(CLAUDE_FORK);
    assert_eq!(fork["parent"], CLAUDE_ORIGIN);
    assert_eq!(fork["parent_kind"], json!({"kind": "fork", "at": "a-1"}));
    let plain = hop(CLAUDE_ORIGIN);
    assert!(
        plain.get("parent").is_none() && plain.get("parent_kind").is_none(),
        "{plain}"
    );
}
