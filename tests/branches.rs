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
