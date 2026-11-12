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
    let scratch = support::scratch(HarnessId::ClaudeCode, tag);
    let roots = support::roots(&scratch);
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
            at: Some("3".into())
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
