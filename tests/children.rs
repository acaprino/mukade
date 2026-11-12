//! The subagent sessions of a session, found and read through the source's reader
//! (docs/superpowers/specs/2026-11-12-subagents-design.md, phase 2, task 1).

mod support;

use mukade::children::{children_of, read_children};
use mukade::hop::{SessionRef, export};
use mukade::model::HarnessId;
use std::path::Path;
use support::CLAUDE_PARENT as PARENT;

const FG: &str = "a1111111111111111";
const BG: &str = "a2222222222222222";
const DEEP: &str = "a4444444444444444";
const MATE: &str = "amate-6666666666666666";

fn child(agent: &str) -> String {
    format!("{PARENT}.{agent}")
}

fn staged(tag: &str) -> mukade::roots::StoreRoots {
    let scratch = support::scratch(HarnessId::ClaudeCode, tag);
    support::stage_claude_subagents(&scratch)
}

/// Every child of the Claude parent, a child's own children after it, a child with no call last.
#[test]
fn claude_children_come_in_spawn_order_with_their_depth() {
    let roots = staged("children-claude");

    let kids = children_of(&SessionRef::id(HarnessId::ClaudeCode, PARENT), &roots).unwrap();

    let seen: Vec<(String, usize)> = kids.iter().map(|k| (k.info.id.clone(), k.depth)).collect();
    assert_eq!(
        seen,
        [
            (child(FG), 1),
            (child(BG), 1),
            (child(DEEP), 2),
            (child(MATE), 1)
        ],
        "the foreground call came first, the background one second, its own child right after \
         it; the teammate has no call"
    );
}

/// `read_children` reads each one and says where it hangs and which call started it.
#[test]
fn claude_children_are_read_with_the_call_and_the_child_they_hang_from() {
    let roots = staged("children-claude-read");
    let parent = export(&SessionRef::id(HarnessId::ClaudeCode, PARENT), &roots).unwrap();

    let read = read_children(&parent, &roots).unwrap();

    let ids: Vec<&str> = read.iter().map(|c| c.info.id.as_str()).collect();
    assert_eq!(ids, [child(FG), child(BG), child(DEEP), child(MATE)]);
    assert!(read.iter().all(|c| c.session.is_ok()), "{read:#?}");
    assert_eq!(read[0].parent, None);
    assert_eq!(read[0].call_id.as_deref(), Some("toolu_fg"));
    assert_eq!(read[1].call_id.as_deref(), Some("toolu_bg"));
    assert_eq!(read[2].parent, Some(1), "the deep child hangs from the background one");
    assert_eq!(read[2].depth, 2);
    assert_eq!(read[2].call_id.as_deref(), Some("toolu_b_inner"));
    assert_eq!(read[3].call_id, None);
    let session = read[0].session.as_ref().unwrap();
    assert_eq!(session.meta.id, child(FG));
}

/// A session with no subagents has no children, and asking is not an error.
#[test]
fn a_session_without_children_has_none() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "children-none");
    let id = support::stage_fixture(HarnessId::ClaudeCode, &scratch);
    let roots = support::roots(&scratch);

    let kids = children_of(&SessionRef::id(HarnessId::ClaudeCode, &id), &roots).unwrap();

    assert!(kids.is_empty(), "{kids:?}");
}

// ---------------------------------------------------------------------------------------------
// Codex: children come from the state database's spawn edges.

const ROOT: &str = "019e0000-0000-7000-8000-0000000000a1";
const KID_1: &str = "019e0000-0000-7000-8000-0000000000b1";
const KID_2: &str = "019e0000-0000-7000-8000-0000000000b2";
const GONE: &str = "019e0000-0000-7000-8000-0000000000b3";

fn record(ordinal: u64, kind: &str, payload: serde_json::Value) -> String {
    serde_json::json!({
        "timestamp": format!("2026-11-12T10:00:{ordinal:02}.000Z"), "ordinal": ordinal,
        "type": kind, "payload": payload,
    })
    .to_string()
        + "\n"
}

fn say(ordinal: u64, role: &str, text: &str) -> String {
    let block = if role == "user" { "input_text" } else { "output_text" };
    record(
        ordinal,
        "response_item",
        serde_json::json!({"type": "message", "role": role, "content": [{"type": block, "text": text}]}),
    )
}

/// One Codex home: a root thread that spawned two readable children, the second call first, and a
/// third child whose rollout is gone. The root's own records link each call to its thread.
fn codex_home(tag: &str) -> mukade::roots::StoreRoots {
    codex_home_with(tag, None)
}

/// [`codex_home`], with the rollout of `forked` naming `forked_from_id`: that child starts from its
/// parent's conversation.
fn codex_home_with(tag: &str, forked: Option<&str>) -> mukade::roots::StoreRoots {
    let scratch = support::scratch(HarnessId::Codex, tag);
    let home = scratch.join("home");
    let day = home.join("sessions/2026/11/12");
    std::fs::create_dir_all(&day).unwrap();
    let file = |id: &str| day.join(format!("rollout-2026-11-12T10-00-00-{id}.jsonl"));
    let spawn = |ordinal: u64, call: &str, task: &str| {
        record(
            ordinal,
            "response_item",
            serde_json::json!({"type": "function_call", "call_id": call, "name": "spawn_agent",
                "namespace": "collaboration", "arguments": format!("{{\"task_name\":\"{task}\"}}")}),
        ) + &record(
            ordinal + 1,
            "response_item",
            serde_json::json!({"type": "function_call_output", "call_id": call,
                "output": format!("{{\"task_name\":\"/root/{task}\"}}")}),
        )
    };
    let link = |ordinal: u64, call: &str, thread: &str| {
        record(
            ordinal,
            "event_msg",
            serde_json::json!({"type": "item_completed", "item": {
                "type": "SubAgentActivity", "id": call, "kind": "spawn", "agent_thread_id": thread}}),
        )
    };
    let root = record(
        0,
        "session_meta",
        serde_json::json!({"id": ROOT, "session_id": ROOT, "cwd": "/work", "cli_version": "0.167.0"}),
    ) + &say(1, "user", "look at both modules")
        + &spawn(2, "call_second", "later")
        + &link(4, "call_second", KID_2)
        + &spawn(5, "call_first", "earlier")
        + &link(7, "call_first", KID_1)
        + &say(8, "assistant", "asked both");
    std::fs::write(file(ROOT), root).unwrap();
    let kid = |id: &str, prompt: &str| {
        let mut meta = serde_json::json!({"id": id, "session_id": ROOT, "parent_thread_id": ROOT,
            "cwd": "/work", "cli_version": "0.167.0",
            "source": {"subagent": {"thread_spawn": {"parent_thread_id": ROOT, "depth": 1,
                "agent_role": "explorer"}}}});
        if forked == Some(id) {
            meta["forked_from_id"] = serde_json::json!(ROOT);
        }
        record(0, "session_meta", meta) + &say(1, "user", prompt)
            + &say(2, "assistant", "done")
    };
    std::fs::write(file(KID_1), kid(KID_1, "first task")).unwrap();
    std::fs::write(file(KID_2), kid(KID_2, "second task")).unwrap();
    codex_state(&home, &file);
    support::roots(&scratch)
}

/// `state_5.sqlite` with the root, its three children and the spawn edges; the third child's
/// rollout was never written.
fn codex_state(home: &Path, file: &dyn Fn(&str) -> std::path::PathBuf) {
    let db = rusqlite::Connection::open(home.join("state_5.sqlite")).unwrap();
    db.execute_batch(
        "CREATE TABLE threads (
             id TEXT PRIMARY KEY, rollout_path TEXT NOT NULL, created_at INTEGER NOT NULL,
             updated_at INTEGER NOT NULL, source TEXT NOT NULL, cwd TEXT NOT NULL,
             title TEXT NOT NULL, archived INTEGER NOT NULL, first_user_message TEXT NOT NULL,
             created_at_ms INTEGER, updated_at_ms INTEGER, thread_source TEXT,
             preview TEXT NOT NULL, name TEXT, agent_role TEXT);
         CREATE TABLE thread_spawn_edges (
             parent_thread_id TEXT NOT NULL, child_thread_id TEXT NOT NULL PRIMARY KEY,
             status TEXT NOT NULL);",
    )
    .unwrap();
    for (id, source, role, updated) in [
        (ROOT, "cli", None, 1_794_149_000_000_i64),
        (KID_1, "subagent-thread_spawn", Some("explorer"), 1_794_149_100_000),
        (KID_2, "subagent-thread_spawn", Some("explorer"), 1_794_149_050_000),
        (GONE, "subagent-thread_spawn", Some("explorer"), 1_794_149_200_000),
    ] {
        db.execute(
            "INSERT INTO threads VALUES (?1, ?2, ?3, ?3, ?4, '/work', '', 0, '', ?5, ?5, ?6, '', NULL, ?7)",
            rusqlite::params![
                id,
                file(id).to_string_lossy(),
                updated / 1000,
                source,
                updated,
                role.map(|_| "subagent"),
                role,
            ],
        )
        .unwrap();
    }
    for child in [KID_1, KID_2, GONE] {
        db.execute(
            "INSERT INTO thread_spawn_edges VALUES (?1, ?2, 'closed')",
            rusqlite::params![ROOT, child],
        )
        .unwrap();
    }
}

/// Codex names no call on the child: the parent's own record links the call to the thread, and
/// that call's place in the parent orders the children, not the update time (the first call
/// started the thread that was updated later).
#[test]
fn codex_children_are_ordered_by_the_call_a_record_of_the_parent_links() {
    let roots = codex_home("children-codex");
    let parent = export(&SessionRef::id(HarnessId::Codex, ROOT), &roots).unwrap();

    let read = read_children(&parent, &roots).unwrap();

    let ids: Vec<&str> = read.iter().map(|c| c.info.id.as_str()).collect();
    assert_eq!(
        ids,
        [KID_2, KID_1, GONE],
        "the call for {KID_2} came first in the parent; the child with no call is last"
    );
    assert_eq!(read[0].call_id.as_deref(), Some("call_second"));
    assert_eq!(read[1].call_id.as_deref(), Some("call_first"));
    assert!(read[0].session.is_ok() && read[1].session.is_ok());
    let gone = read[2].session.as_ref().unwrap_err();
    assert!(
        gone.contains("rollout"),
        "a child whose rollout is gone says so: {gone}"
    );
}

/// The dump document carries each child as a document of its own, nested as it hangs.
#[test]
fn the_dump_document_nests_the_children() {
    let roots = staged("children-doc");
    let parent = export(&SessionRef::id(HarnessId::ClaudeCode, PARENT), &roots).unwrap();

    let doc = mukade::children::document(&parent, None, &roots).unwrap();

    let ids = |doc: &serde_json::Value| -> Vec<String> {
        doc["children"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["meta"]["id"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(doc["meta"]["id"], PARENT);
    assert_eq!(ids(&doc), [child(FG), child(BG), child(MATE)]);
    let bg = &doc["children"][1];
    assert_eq!(ids(bg), [child(DEEP)], "the background agent's own child hangs from it");
    assert_eq!(bg["meta"]["parent"], PARENT);
    assert!(
        bg["messages"].as_array().is_some_and(|m| !m.is_empty()),
        "a child is a full document: {bg}"
    );
    assert!(doc.get("children_lost").is_none(), "nothing was lost: {doc}");
}

/// A child that cannot be read is named, with the reason, and the others are still dumped.
#[test]
fn the_dump_document_names_a_child_it_could_not_read() {
    let roots = codex_home("children-doc-lost");
    let parent = export(&SessionRef::id(HarnessId::Codex, ROOT), &roots).unwrap();

    let doc = mukade::children::document(&parent, None, &roots).unwrap();

    assert_eq!(doc["children"].as_array().unwrap().len(), 2);
    let lost = doc["children_lost"].as_array().unwrap();
    assert_eq!(lost.len(), 1, "{lost:?}");
    assert!(
        lost[0].as_str().unwrap().starts_with(&format!("{GONE}: ")),
        "{lost:?}"
    );
}

/// Pi has no subagents: its `parentSession` is a fork, and a fork is not a child.
#[test]
fn a_pi_session_has_no_children() {
    let scratch = support::scratch(HarnessId::Pi, "children-pi");
    let id = support::stage_fixture(HarnessId::Pi, &scratch);
    let roots = support::roots(&scratch);

    let kids = children_of(&SessionRef::id(HarnessId::Pi, &id), &roots).unwrap();

    assert!(kids.is_empty(), "{kids:?}");
}

/// A Codex child whose rollout says it was forked from its parent is written with what its own file
/// holds, and the manifest says so: the state database's spawn edge does not know it, and must not
/// overwrite what the rollout says.
#[test]
fn a_forked_codex_child_states_inherits_context_though_the_database_does_not() {
    let roots = codex_home_with("children-codex-fork", Some(KID_1));
    let parent = export(&SessionRef::id(HarnessId::Codex, ROOT), &roots).unwrap();
    let scratch = support::scratch(HarnessId::Pi, "children-codex-fork-dst");
    let opts = support::opts(HarnessId::Pi, &scratch);
    let import = support::import_opts(&opts, &parent, None);

    let receipt =
        mukade::children::import_with_children(HarnessId::Pi, &parent, &parent, &roots, &import)
            .unwrap();

    let kids = receipt.manifest["children"].as_array().unwrap();
    let entry = |id: &str| kids.iter().find(|c| c["source_id"] == id).unwrap();
    assert_eq!(entry(KID_1)["inherits_context"], true, "{kids:?}");
    assert!(entry(KID_2).get("inherits_context").is_none(), "{kids:?}");
}

/// A child the listing knows and the store cannot read is a loss line, and the hop succeeds.
#[test]
fn an_unreadable_child_is_a_loss_and_the_hop_goes_on() {
    let roots = codex_home("children-codex-gone");
    let parent = export(&SessionRef::id(HarnessId::Codex, ROOT), &roots).unwrap();
    let scratch = support::scratch(HarnessId::Pi, "children-codex-gone-dst");
    let opts = support::opts(HarnessId::Pi, &scratch);
    let import = support::import_opts(&opts, &parent, None);

    let receipt =
        mukade::children::import_with_children(HarnessId::Pi, &parent, &parent, &roots, &import)
            .unwrap();

    assert_eq!(receipt.manifest["children"].as_array().unwrap().len(), 2);
    let losses = receipt.manifest["losses"].to_string();
    assert!(losses.contains(&format!("child {GONE}: ")), "{losses}");
}
