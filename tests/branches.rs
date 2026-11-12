//! Every branch through dump and hop (design `docs/superpowers/specs/2026-11-12-forks-branches-design.md`,
//! part B): the model, the dump document, the readers' `--all-branches`, the writers and the CLI.

use mukade::model::{Branch, Entry, HarnessId, Kind, Meta, Session, StepEnd};
use serde_json::{Value, json};

mod support;

fn entry(id: &str, kind: Kind) -> Entry {
    Entry {
        id: id.into(),
        kind,
        raw: Vec::new(),
    }
}

fn user(id: &str, text: &str) -> Entry {
    entry(
        id,
        Kind::User {
            text: text.into(),
            attachments: Vec::new(),
        },
    )
}

fn assistant(id: &str, text: &str) -> Entry {
    entry(
        id,
        Kind::Step {
            model: None,
            reasoning: Vec::new(),
            text: vec![text.into()],
            calls: Vec::new(),
            end: StepEnd::Completed,
        },
    )
}

fn meta() -> Meta {
    Meta {
        harness: HarnessId::Pi,
        harness_version: "1".into(),
        id: "src".into(),
        cwd: Some("/work".into()),
        title: None,
        created: None,
        updated: None,
        models: Vec::new(),
        git_branch: None,
        parent: None,
        parent_kind: None,
        source_path: None,
        source_sha256: None,
    }
}

/// u1 a1 u2 a2, and a branch that left after a1: u2b a2b.
fn branched() -> Session {
    Session {
        meta: meta(),
        entries: vec![
            user("u1", "q1"),
            assistant("a1", "r1"),
            user("u2", "q2"),
            assistant("a2", "r2"),
        ],
        branches: vec![Branch {
            from: Some("a1".into()),
            entries: vec![user("u2b", "q2 again"), assistant("a2b", "r2b")],
        }],
    }
}

fn texts(messages: &Value) -> Vec<String> {
    messages
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["content"][0]["text"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn a_session_can_carry_the_branches_it_left_through_its_document() {
    let session = branched();
    let doc = mukade::wire::to_document(&session);
    assert_eq!(texts(&doc["messages"]), ["q1", "r1", "q2", "r2"]);
    assert_eq!(doc["branches"].as_array().unwrap().len(), 1);
    assert_eq!(doc["branches"][0]["from"], "a1");
    assert_eq!(texts(&doc["branches"][0]["messages"]), ["q2 again", "r2b"]);

    let back = mukade::wire::from_document(&doc).unwrap();
    assert_eq!(back.entries.len(), 4);
    assert_eq!(back.branches.len(), 1);
    assert_eq!(back.branches[0].from.as_deref(), Some("a1"));
    let ids: Vec<&str> = back.branches[0]
        .entries
        .iter()
        .map(|e| e.id.as_str())
        .collect();
    assert_eq!(ids, ["u2b", "a2b"]);
    assert_eq!(mukade::wire::to_document(&back), doc);
}

#[test]
fn a_session_without_branches_has_no_branches_in_its_document() {
    let mut session = branched();
    session.branches.clear();
    let doc = mukade::wire::to_document(&session);
    assert!(doc.get("branches").is_none(), "{doc}");
    // A branch that leaves before the first entry says so with null.
    session.branches.push(Branch {
        from: None,
        entries: vec![user("v1", "another start")],
    });
    let doc = mukade::wire::to_document(&session);
    assert!(doc["branches"][0]["from"].is_null());
    assert_eq!(
        mukade::wire::from_document(&doc).unwrap().branches[0].from,
        None
    );
    // A session that serializes as JSON keeps them only when there are some.
    assert!(
        serde_json::to_value(&session)
            .unwrap()
            .get("branches")
            .is_some()
    );
    session.branches.clear();
    assert!(
        serde_json::to_value(&session)
            .unwrap()
            .get("branches")
            .is_none()
    );
    let _ = json!(null);
}

// ---------------------------------------------------------------------------------------------
// readers: the branches a tree-shaped session left

use mukade::model::MetaKind;
use std::path::Path;

fn write_lines(path: &Path, rows: &[Value]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut text = String::new();
    for row in rows {
        text.push_str(&row.to_string());
        text.push('\n');
    }
    std::fs::write(path, text).unwrap();
}

fn conversation(entries: &[Entry]) -> Vec<String> {
    entries
        .iter()
        .filter_map(|entry| match &entry.kind {
            Kind::User { text, .. } => Some(format!("user: {text}")),
            Kind::Step { text, .. } => Some(format!("assistant: {}", text.join(" "))),
            _ => None,
        })
        .collect()
}

fn off_branch(entries: &[Entry]) -> usize {
    entries
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

fn id_of_text<'a>(entries: &'a [Entry], text: &str) -> &'a str {
    entries
        .iter()
        .find(|entry| match &entry.kind {
            Kind::User { text: t, .. } => t == text,
            Kind::Step { text: t, .. } => t.join(" ") == text,
            _ => false,
        })
        .unwrap_or_else(|| panic!("{text} in {entries:#?}"))
        .id
        .as_str()
}

fn claude_row(kind: &str, uuid: &str, parent: Option<&str>, text: &str, at: u32) -> Value {
    let mut row = json!({"type": kind, "uuid": uuid, "parentUuid": parent,
        "sessionId": "s-1", "cwd": "/work", "isSidechain": false,
        "timestamp": format!("2026-11-12T10:00:{at:02}.000Z")});
    row["message"] = if kind == "user" {
        json!({"role": "user", "content": text})
    } else {
        json!({"id": format!("msg_{uuid}"), "role": "assistant", "model": "claude-x",
            "stop_reason": "end_turn", "content": [{"type": "text", "text": text}]})
    };
    row
}

/// The resumed path is q1 r1 q2 r2; an earlier attempt at the second prompt, q2b r2b, left from r1,
/// and r2b was answered twice (r2c is a branch of a branch).
fn claude_tree() -> Vec<Value> {
    vec![
        claude_row("user", "u1", None, "q1", 0),
        claude_row("assistant", "a1", Some("u1"), "r1", 1),
        claude_row("user", "u2b", Some("a1"), "q2b", 2),
        claude_row("assistant", "a2b", Some("u2b"), "r2b", 3),
        claude_row("assistant", "a2c", Some("u2b"), "r2c", 4),
        claude_row("user", "u2", Some("a1"), "q2", 5),
        claude_row("assistant", "a2", Some("u2"), "r2", 6),
    ]
}

#[test]
fn claude_code_reads_every_branch_of_its_tree_as_a_branch() {
    use mukade::reader::claude_code;
    let scratch = support::scratch(HarnessId::ClaudeCode, "claude-branches");
    let file = scratch.join("home/projects/p/s-1.jsonl");
    write_lines(&file, &claude_tree());

    // Default: the branch Claude Code resumes; the others stay as records of an abandoned path.
    let plain = claude_code::read_file(&file).unwrap();
    assert_eq!(
        conversation(&plain.entries),
        ["user: q1", "assistant: r1", "user: q2", "assistant: r2"]
    );
    assert!(plain.branches.is_empty());
    assert_eq!(off_branch(&plain.entries), 3);

    let all = claude_code::read_file_all_branches(&file).unwrap();
    assert_eq!(conversation(&all.entries), conversation(&plain.entries));
    assert_eq!(
        off_branch(&all.entries),
        0,
        "every record is in a branch now"
    );
    assert_eq!(all.branches.len(), 2);
    let first = &all.branches[0];
    assert_eq!(
        conversation(&first.entries),
        ["user: q2b", "assistant: r2b"]
    );
    assert_eq!(first.from.as_deref(), Some(id_of_text(&all.entries, "r1")));
    let second = &all.branches[1];
    assert_eq!(conversation(&second.entries), ["assistant: r2c"]);
    // Hung off the entry of the branch that holds its parent record.
    assert_eq!(
        second.from.as_deref(),
        Some(id_of_text(&first.entries, "q2b"))
    );
    // Every source record is kept once.
    let rows: usize = all
        .entries
        .iter()
        .chain(all.branches.iter().flat_map(|b| b.entries.iter()))
        .map(|entry| entry.raw.len())
        .sum();
    assert_eq!(rows, 7);
}

fn pi_message(id: &str, parent: Option<&str>, role: &str, text: &str) -> Value {
    json!({"type": "message", "id": id, "parentId": parent, "timestamp": "2026-11-12T10:00:01.000Z",
        "message": {"role": role, "content": [{"type": "text", "text": text}]}})
}

#[test]
fn pi_reads_every_branch_of_its_tree_as_a_branch() {
    use mukade::reader::pi;
    let scratch = support::scratch(HarnessId::Pi, "pi-branches");
    let file = scratch.join("home/sessions/--work--/2026-11-12T10-00-00-000Z_01a30000-0000-7000-8000-000000000001.jsonl");
    write_lines(
        &file,
        &[
            json!({"type": "session", "version": 3, "id": "01a30000-0000-7000-8000-000000000001",
                "timestamp": "2026-11-12T10:00:00.000Z", "cwd": "/work"}),
            pi_message("e1", None, "user", "q1"),
            pi_message("e2", Some("e1"), "assistant", "r1"),
            pi_message("e3", Some("e2"), "user", "q2 first try"),
            pi_message("e4", Some("e3"), "assistant", "r2 first try"),
            pi_message("e5", Some("e2"), "user", "q2"),
            pi_message("e6", Some("e5"), "assistant", "r2"),
        ],
    );
    let plain = pi::read_file(&file).unwrap();
    assert_eq!(
        conversation(&plain.entries),
        ["user: q1", "assistant: r1", "user: q2", "assistant: r2"]
    );
    assert_eq!(off_branch(&plain.entries), 2);
    assert!(plain.branches.is_empty());

    let all = pi::read_file_all_branches(&file).unwrap();
    assert_eq!(conversation(&all.entries), conversation(&plain.entries));
    assert_eq!(off_branch(&all.entries), 0);
    assert_eq!(all.branches.len(), 1);
    assert_eq!(
        conversation(&all.branches[0].entries),
        ["user: q2 first try", "assistant: r2 first try"]
    );
    assert_eq!(all.branches[0].from.as_deref(), Some("e2"));
}

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

#[test]
fn codex_reads_the_turns_a_rollback_dropped_as_branches() {
    use mukade::reader::codex::{Branches, read_with};
    const ID: &str = "01a10000-0000-7000-8000-0000000000aa";
    let scratch = support::scratch(HarnessId::Codex, "codex-rollback-branches");
    let roots = support::roots(&scratch);
    let home = roots.codex_home.clone().unwrap();
    write_lines(
        &home.join(format!(
            "sessions/2026/11/12/rollout-2026-11-12T10-00-00-{ID}.jsonl"
        )),
        &[
            codex_record(
                0,
                "session_meta",
                json!({"id": ID, "cwd": "/work", "cli_version": "0.167.0",
                    "timestamp": "2026-11-12T10:00:00.000Z"}),
            ),
            codex_say(1, "user", "q1"),
            codex_say(2, "assistant", "a1"),
            codex_say(3, "user", "q2"),
            codex_say(4, "assistant", "a2"),
            codex_record(
                5,
                "event_msg",
                json!({"type": "thread_rolled_back", "num_turns": 1}),
            ),
            codex_say(6, "user", "q3"),
            codex_say(7, "assistant", "a3"),
            codex_record(
                8,
                "event_msg",
                json!({"type": "thread_rolled_back", "num_turns": 1}),
            ),
            codex_say(9, "user", "q4"),
        ],
    );
    let plain = read_with(ID, &roots, Branches::Active).unwrap();
    assert_eq!(
        conversation(&plain.entries),
        ["user: q1", "assistant: a1", "user: q4"]
    );
    assert!(plain.branches.is_empty());
    assert_eq!(off_branch(&plain.entries), 4);

    let all = read_with(ID, &roots, Branches::All).unwrap();
    assert_eq!(conversation(&all.entries), conversation(&plain.entries));
    assert_eq!(off_branch(&all.entries), 0);
    assert_eq!(all.branches.len(), 2);
    // Both attempts left after the first answer.
    let a1 = id_of_text(&all.entries, "a1");
    assert_eq!(all.branches[0].from.as_deref(), Some(a1));
    assert_eq!(
        conversation(&all.branches[0].entries),
        ["user: q2", "assistant: a2"]
    );
    assert_eq!(all.branches[1].from.as_deref(), Some(a1));
    assert_eq!(
        conversation(&all.branches[1].entries),
        ["user: q3", "assistant: a3"]
    );
}
