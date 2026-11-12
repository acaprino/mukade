//! Conformance tests for the Hermes Agent reader (v0.22.5 and later, `state.db` schema 32 and later).
//!
//! Two sources, one reading: the `hermes sessions export` file (`fixtures/hermes/source`) and a
//! synthetic `state.db` built by `support::hermes_db`. The store tests load the export's own
//! session into a database and require the same [`Session`] back, then vary the store.

use mukade::harnesses;
use mukade::model::{
    CanonicalTool, HarnessId, Kind, MetaKind, ParentKind, Protocol, ResultContent, Session, StepEnd,
};
use mukade::reader::{Lookup, Reader, hermes};
use mukade::roots::StoreRoots;
use serde_json::{Value, json};
use std::path::PathBuf;
use support::hermes_db::{self, HermesDb};

mod support;

fn export() -> PathBuf {
    support::source_path(HarnessId::Hermes)
}

fn store(tag: &str) -> (StoreRoots, HermesDb) {
    let scratch = support::scratch(HarnessId::Hermes, tag);
    let roots = support::roots(&scratch);
    let db = hermes_db::create(roots.hermes_home.as_deref().unwrap());
    (roots, db)
}

fn read(roots: &StoreRoots, id: &str) -> anyhow::Result<Session> {
    hermes::Hermes.read(id, roots)
}

fn kinds(session: &Session) -> Vec<&'static str> {
    session
        .entries
        .iter()
        .map(|e| support::kind_name(&e.kind))
        .collect()
}

fn meta_kinds(session: &Session) -> Vec<MetaKind> {
    session
        .entries
        .iter()
        .filter_map(|e| match &e.kind {
            Kind::Meta { kind } => Some(*kind),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// the export file

#[test]
fn an_export_reads_as_the_conversation_it_holds() {
    let session = hermes::read_file(&export()).expect("the export must read");
    assert_eq!(session.meta.harness, HarnessId::Hermes);
    assert_eq!(session.meta.id, "20261112_120000_a1b2c3");
    assert_eq!(session.meta.title.as_deref(), Some("Line counts"));
    assert_eq!(session.meta.cwd.as_deref(), Some("/work/hermes-project"));
    assert_eq!(session.meta.git_branch.as_deref(), Some("main"));
    assert_eq!(
        session.meta.created.as_deref(),
        Some("2026-11-12T10:40:00.000Z")
    );
    assert_eq!(
        session
            .meta
            .models
            .iter()
            .map(|m| (m.provider.as_deref(), m.id.as_str()))
            .collect::<Vec<_>>(),
        [(Some("openrouter"), "cohere/north-mini-code:free")]
    );
    assert!(session.meta.parent.is_none() && session.meta.parent_kind.is_none());
    assert!(session.meta.source_sha256.is_some());
    // Rows 3 and 4 answer the two parallel calls of row 2: one Results entry, as the model
    // received them.
    assert_eq!(
        kinds(&session),
        [
            "user", "step", "results", "step", "user", "step", "results", "step", "results", "step"
        ]
    );
}

#[test]
fn parallel_calls_keep_their_results_together_and_by_id() {
    let session = hermes::read_file(&export()).unwrap();
    let Kind::Step { calls, end, .. } = &session.entries[1].kind else {
        panic!("a step");
    };
    assert_eq!(*end, StepEnd::ToolUse);
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].id, "call-read-1");
    assert_eq!(calls[0].source_name, "read_file");
    assert_eq!(calls[0].canonical, Some(CanonicalTool::Read));
    assert_eq!(calls[0].input, json!({"path": "notes.txt"}));
    let Kind::Results { results } = &session.entries[2].kind else {
        panic!("results");
    };
    assert_eq!(
        results
            .iter()
            .map(|r| r.call_id.as_str())
            .collect::<Vec<_>>(),
        ["call-read-1", "call-read-2"]
    );
    assert_eq!(
        results[0].content,
        ResultContent::Text("     1|alpha\n     2|beta".into())
    );
    // The group keeps both source rows.
    assert_eq!(session.entries[2].raw.len(), 2);
}

#[test]
fn hermes_tools_have_their_canonical_forms() {
    let session = hermes::read_file(&export()).unwrap();
    let canonical: Vec<(String, CanonicalTool)> = session
        .entries
        .iter()
        .filter_map(|e| match &e.kind {
            Kind::Step { calls, .. } => Some(calls),
            _ => None,
        })
        .flatten()
        .map(|c| (c.source_name.clone(), c.canonical.clone().unwrap()))
        .collect();
    assert_eq!(
        canonical,
        [
            ("read_file".to_string(), CanonicalTool::Read),
            ("read_file".to_string(), CanonicalTool::Read),
            ("patch".to_string(), CanonicalTool::Edit),
            ("terminal".to_string(), CanonicalTool::Bash),
        ]
    );
}

#[test]
fn reasoning_keeps_its_text_and_its_chat_details() {
    let session = hermes::read_file(&export()).unwrap();
    let Kind::Step { reasoning, .. } = &session.entries[1].kind else {
        panic!("a step");
    };
    assert_eq!(reasoning.len(), 1);
    assert_eq!(reasoning[0].text, "Two files to read.");
    let opaque = reasoning[0].opaque.as_ref().expect("the details are kept");
    // The details say `format: unknown`, so the session's own protocol decides.
    assert_eq!(opaque.protocol, Protocol::OpenaiChat);
    let chat = opaque.chat.as_ref().expect("the chat envelope");
    assert_eq!(chat.provider.as_deref(), Some("openrouter"));
    assert_eq!(chat.details.len(), 1);
}

#[test]
fn every_source_row_is_kept_verbatim() {
    let session = hermes::read_file(&export()).unwrap();
    let rows: usize = session.entries.iter().map(|e| e.raw.len()).sum();
    assert_eq!(rows, 11);
    assert_eq!(
        session.entries[0].raw[0]["message_uid"],
        "00000000000000000000000000000001"
    );
}

#[test]
fn a_file_holding_two_sessions_is_refused_and_a_database_is_pointed_at_its_home() {
    let scratch = support::scratch(HarnessId::Hermes, "two");
    let line = std::fs::read_to_string(export()).unwrap();
    let two = scratch.join("two.jsonl");
    std::fs::write(&two, format!("{line}{line}")).unwrap();
    let error = format!("{:#}", hermes::read_file(&two).unwrap_err());
    assert!(error.contains("holds 2 sessions"), "{error}");

    let (_roots, db) = store("sqlite-file");
    let error = format!("{:#}", hermes::read_file(&db.path).unwrap_err());
    assert!(error.contains("HERMES_HOME"), "{error}");
}

// ---------------------------------------------------------------------------------------------
// the store

#[test]
fn the_store_reads_the_same_session_as_its_export() {
    let (roots, db) = store("same");
    db.load_export(&export());
    let from_store = read(&roots, "20261112_120000_a1b2c3").expect("the store must read");
    let mut from_file = hermes::read_file(&export()).unwrap();
    // What differs is where the session was read from.
    from_file.meta.source_path = from_store.meta.source_path.clone();
    from_file.meta.source_sha256 = None;
    from_file.meta.harness_version = from_store.meta.harness_version.clone();
    assert_eq!(from_store.meta, from_file.meta);
    assert_eq!(from_store.entries.len(), from_file.entries.len());
    for (store, file) in from_store.entries.iter().zip(&from_file.entries) {
        assert_eq!(store, file);
    }
    assert_eq!(from_store.meta.harness_version, "schema 33");
}

/// A chat member bound to a Hermes session is read from the place its listing names: the store's
/// `state.db`, which holds every session, by its id.
#[test]
fn a_bound_session_reads_from_the_database_its_listing_names() {
    let (roots, db) = store("bound");
    db.load_export(&export());
    let listed = hermes::Hermes.list(&roots).unwrap();
    let info = listed
        .iter()
        .find(|info| info.id == "20261112_120000_a1b2c3")
        .expect("listed");
    let bound = harnesses::get(HarnessId::Hermes)
        .read_bound(std::path::Path::new(&info.path), &info.id)
        .expect("the bound session reads");
    assert_eq!(bound, read(&roots, &info.id).unwrap());
}

#[test]
fn an_id_prefix_names_a_session_when_it_names_one() {
    let (roots, db) = store("prefix");
    db.session("local-aaaa1111", "cli", 1794480000.0);
    db.session("local-aaaa2222", "cli", 1794480001.0);
    db.session("local-bbbb3333", "cli", 1794480002.0);
    assert_eq!(read(&roots, "local-b").unwrap().meta.id, "local-bbbb3333");
    let ambiguous = read(&roots, "local-aaaa").unwrap_err();
    assert!(
        matches!(
            ambiguous.downcast_ref::<Lookup>(),
            Some(Lookup::Ambiguous(_))
        ),
        "{ambiguous:#}"
    );
    let missing = read(&roots, "nothing").unwrap_err();
    assert!(
        matches!(missing.downcast_ref::<Lookup>(), Some(Lookup::NotFound(_))),
        "{missing:#}"
    );
}

#[test]
fn without_a_home_there_is_no_store() {
    let roots = StoreRoots::default();
    for error in [
        hermes::Hermes.list(&roots).unwrap_err(),
        hermes::Hermes.read("x", &roots).unwrap_err(),
    ] {
        assert!(
            matches!(error.downcast_ref::<Lookup>(), Some(Lookup::NoStore(_))),
            "{error:#}"
        );
    }
}

#[test]
fn reading_leaves_the_store_as_it_was() {
    let (roots, db) = store("readonly");
    db.load_export(&export());
    let before = std::fs::read(&db.path).unwrap();
    let modified = std::fs::metadata(&db.path).unwrap().modified().unwrap();
    drop(db);
    read(&roots, "20261112_120000_a1b2c3").unwrap();
    hermes::Hermes.list_all(&roots).unwrap();
    assert_eq!(
        std::fs::read(roots.hermes_state_db().unwrap()).unwrap(),
        before
    );
    assert_eq!(
        std::fs::metadata(roots.hermes_state_db().unwrap())
            .unwrap()
            .modified()
            .unwrap(),
        modified
    );
}

// ---------------------------------------------------------------------------------------------
// the gate

#[test]
fn an_older_schema_is_refused_with_its_number() {
    let scratch = support::scratch(HarnessId::Hermes, "old");
    let roots = support::roots(&scratch);
    hermes_db::create_at(
        roots.hermes_home.as_deref().unwrap(),
        31,
        hermes_db::schema(),
    );
    let error = format!("{:#}", hermes::Hermes.list(&roots).unwrap_err());
    assert!(
        error.contains("schema 31") && error.contains("32"),
        "{error}"
    );
}

#[test]
fn a_newer_schema_with_the_columns_is_read() {
    let scratch = support::scratch(HarnessId::Hermes, "newer");
    let roots = support::roots(&scratch);
    let db = hermes_db::create_at(
        roots.hermes_home.as_deref().unwrap(),
        41,
        hermes_db::schema(),
    );
    db.session("s1", "cli", 1794480000.0);
    let m = db.message("s1", "user", Some("hello"));
    assert!(m > 0);
    let session = read(&roots, "s1").unwrap();
    assert_eq!(session.meta.harness_version, "schema 41");
    assert_eq!(kinds(&session), ["user"]);
}

#[test]
fn a_store_without_a_needed_column_is_refused_naming_it() {
    let scratch = support::scratch(HarnessId::Hermes, "columns");
    let roots = support::roots(&scratch);
    let schema = hermes_db::schema().replace("    tool_call_id TEXT,\n", "");
    hermes_db::create_at(roots.hermes_home.as_deref().unwrap(), 33, &schema);
    let error = format!("{:#}", hermes::Hermes.list(&roots).unwrap_err());
    assert!(error.contains("messages.tool_call_id"), "{error}");
}

#[test]
fn another_sqlite_database_is_refused() {
    let scratch = support::scratch(HarnessId::Hermes, "other");
    let roots = support::roots(&scratch);
    let home = roots.hermes_home.clone().unwrap();
    std::fs::create_dir_all(&home).unwrap();
    rusqlite::Connection::open(home.join("state.db"))
        .unwrap()
        .execute_batch("CREATE TABLE sessions (id TEXT); CREATE TABLE messages (id TEXT);")
        .unwrap();
    let error = format!("{:#}", hermes::Hermes.list(&roots).unwrap_err());
    assert!(error.contains("schema_version"), "{error}");
}

// ---------------------------------------------------------------------------------------------
// listing and lineage

fn lineage_store(tag: &str) -> (StoreRoots, HermesDb) {
    let (roots, db) = store(tag);
    db.session("root", "cli", 1794480000.0);
    db.message("root", "user", Some("Plan the migration"));
    db.session("branch", "local", 1794480100.0);
    db.set_session("branch", "created_source", &"cli");
    db.set_session("branch", "parent_session_id", &"root");
    db.set_session(
        "branch",
        "model_config",
        &json!({"_branched_from": "root"}).to_string(),
    );
    db.session("child", "subagent", 1794480200.0);
    db.set_session("child", "parent_session_id", &"root");
    db.set_session(
        "child",
        "model_config",
        &json!({"_delegate_from": "root"}).to_string(),
    );
    db.session("orphan", "subagent", 1794480250.0);
    db.set_session(
        "orphan",
        "model_config",
        &json!({"_delegate_from": "__orphaned__"}).to_string(),
    );
    db.session("gone", "cli", 1794480300.0);
    db.set_session("gone", "title", &"Titled by Hermes");
    db.set_session("gone", "archived", &1);
    // A legacy rotating compaction: head -> middle -> tip.
    db.session("head", "cli", 1794480400.0);
    db.set_session("head", "end_reason", &"compression");
    db.session("middle", "cli", 1794480500.0);
    db.set_session("middle", "parent_session_id", &"head");
    db.set_session("middle", "end_reason", &"compression");
    db.session("tip", "cli", 1794480600.0);
    db.set_session("tip", "parent_session_id", &"middle");
    (roots, db)
}

#[test]
fn a_listing_shows_the_sessions_a_picker_shows() {
    let (roots, _db) = lineage_store("list");
    let listed = hermes::Hermes.list(&roots).unwrap();
    let ids: Vec<&str> = listed.iter().map(|i| i.id.as_str()).collect();
    // Newest first; no subagent, no old half of a rotated compaction.
    assert_eq!(ids, ["tip", "gone", "branch", "root"]);
    let branch = listed.iter().find(|i| i.id == "branch").unwrap();
    assert_eq!(branch.parent.as_deref(), Some("root"));
    assert_eq!(branch.parent_kind, Some(ParentKind::fork()));
    let tip = listed.iter().find(|i| i.id == "tip").unwrap();
    assert!(
        tip.parent.is_none(),
        "a continuation is the same conversation, not a child"
    );
    let gone = listed.iter().find(|i| i.id == "gone").unwrap();
    assert!(gone.archived);
    assert_eq!(gone.title.as_deref(), Some("Titled by Hermes"));
    let root = listed.iter().find(|i| i.id == "root").unwrap();
    // No title in the row: the first thing the user said.
    assert_eq!(root.title.as_deref(), Some("Plan the migration"));
}

#[test]
fn the_explorer_listing_adds_subagents_and_every_rotated_half() {
    let (roots, _db) = lineage_store("list-all");
    let listed = hermes::Hermes.list_all(&roots).unwrap();
    assert_eq!(listed.len(), 8);
    let child = listed.iter().find(|i| i.id == "child").unwrap();
    assert_eq!(child.parent.as_deref(), Some("root"));
    assert_eq!(child.parent_kind, Some(ParentKind::spawn()));
    let orphan = listed.iter().find(|i| i.id == "orphan").unwrap();
    assert!(orphan.parent.is_none() && orphan.parent_kind.is_none());
}

/// `list` shows what Hermes's own `sessions list` shows: a `local` session is a person's, and a
/// `oneshot`, `tool` or `kanban` one (finite runs, integrations, workers) is not. The explorer's
/// listing keeps them, and each is readable by id.
#[test]
fn a_listing_hides_the_sources_hermes_hides_and_the_explorer_keeps_them() {
    let (roots, db) = store("sources");
    for (id, source) in [
        ("a", "cli"),
        ("b", "local"),
        ("c", "oneshot"),
        ("d", "telegram"),
        ("e", "tool"),
        ("f", "kanban"),
    ] {
        db.session(id, source, 1794480000.0);
        db.message(id, "user", Some("hello"));
    }
    let ids = |listed: Vec<mukade::reader::SessionInfo>| {
        let mut ids: Vec<String> = listed.into_iter().map(|info| info.id).collect();
        ids.sort();
        ids
    };
    assert_eq!(ids(hermes::Hermes.list(&roots).unwrap()), ["a", "b", "d"]);
    assert_eq!(
        ids(hermes::Hermes.list_all(&roots).unwrap()),
        ["a", "b", "c", "d", "e", "f"]
    );
    for id in ["c", "e", "f"] {
        assert_eq!(read(&roots, id).unwrap().meta.id, id);
    }
}

#[test]
fn the_store_counts_its_sessions_and_its_children() {
    let (roots, _db) = lineage_store("counts");
    let counts = hermes::store_counts(&roots.hermes_state_db().unwrap()).unwrap();
    // `branch`, `child` and `orphan` came from another session.
    assert_eq!(counts, (8, 3));
    let stats = harnesses::get(HarnessId::Hermes).store_stats(&roots);
    assert_eq!(stats.sessions, Ok(8));
    assert_eq!(stats.forks, Some(Ok(3)));
}

#[test]
fn a_subagent_and_a_branch_read_with_their_parent_and_kind() {
    let (roots, _db) = lineage_store("lineage-read");
    let child = read(&roots, "child").unwrap();
    assert_eq!(child.meta.parent.as_deref(), Some("root"));
    assert_eq!(child.meta.parent_kind, Some(ParentKind::spawn()));
    let branch = read(&roots, "branch").unwrap();
    assert_eq!(branch.meta.parent.as_deref(), Some("root"));
    assert_eq!(branch.meta.parent_kind, Some(ParentKind::fork()));
}

#[test]
fn a_rotated_compaction_reads_as_one_conversation_from_its_tip() {
    let (roots, db) = lineage_store("chain");
    db.message("head", "user", Some("first"));
    db.message("head", "assistant", Some("one"));
    db.message("middle", "user", Some("second"));
    db.message("tip", "assistant", Some("three"));
    let session = read(&roots, "tip").unwrap();
    assert_eq!(session.meta.id, "tip");
    assert_eq!(kinds(&session), ["user", "step", "user", "step"]);
    // The conversation began in the head.
    assert_eq!(
        session.meta.created.as_deref(),
        Some("2026-11-12T10:46:40.000Z")
    );
    let head_only = read(&roots, "head").unwrap();
    assert_eq!(kinds(&head_only), ["user", "step"], "a head reads alone");
}

// ---------------------------------------------------------------------------------------------
// what rows are

#[test]
fn a_compaction_in_place_keeps_the_archived_turns_apart_and_the_summary_as_one() {
    let (roots, db) = store("compaction");
    db.session("s", "cli", 1794480000.0);
    let old_user = db.message("s", "user", Some("an old question"));
    let old_answer = db.message("s", "assistant", Some("an old answer"));
    for id in [old_user, old_answer] {
        db.set(id, "active", &0).set(id, "compacted", &1);
    }
    let summary = db.message(
        "s",
        "user",
        Some(
            "[CONTEXT COMPACTION — REFERENCE ONLY] Earlier turns were compacted. Tools stay active; \
             avoid repeating it:\nWe agreed on plan B.\n\n--- END OF CONTEXT SUMMARY — respond to the message below, not the summary above ---",
        ),
    );
    db.set(summary, "_compressed_summary", &1);
    let tail = db.message("s", "user", Some("continue"));
    let session = read(&roots, "s").unwrap();
    assert_eq!(kinds(&session), ["meta", "meta", "compaction", "user"]);
    assert_eq!(
        meta_kinds(&session),
        [MetaKind::OffBranch, MetaKind::OffBranch]
    );
    let Kind::Compaction {
        summary: text,
        kept_from,
        ..
    } = &session.entries[2].kind
    else {
        panic!("a compaction");
    };
    assert_eq!(text, "We agreed on plan B.");
    assert_eq!(kept_from.as_deref(), Some(tail.to_string().as_str()));
}

#[test]
fn a_summary_merged_into_the_first_tail_row_is_the_part_after_the_delimiter() {
    let (roots, db) = store("merged");
    db.session("s", "cli", 1794480000.0);
    let row = db.message(
        "s",
        "user",
        Some(
            "[PRIOR CONTEXT — for reference only; not a new message]\nsomething\n\
             [END OF PRIOR CONTEXT — COMPACTION SUMMARY BELOW]\n[CONTEXT SUMMARY]: The summary.\n\n--- END OF CONTEXT SUMMARY — x ---",
        ),
    );
    db.set(row, "_compressed_summary", &1);
    let session = read(&roots, "s").unwrap();
    let Kind::Compaction {
        summary, kept_from, ..
    } = &session.entries[0].kind
    else {
        panic!("a compaction");
    };
    assert_eq!(summary, "The summary.");
    assert!(kept_from.is_none());
}

#[test]
fn rows_the_model_got_but_the_user_did_not_type_are_not_prompts() {
    let (roots, db) = store("scaffolding");
    db.session("s", "cli", 1794480000.0);
    let typed = db.message("s", "user", Some("a real prompt"));
    let hidden = db.message("s", "user", Some("scaffolding"));
    db.set(hidden, "display_kind", &"hidden");
    let note = db.message("s", "user", Some("process finished"));
    db.set(note, "display_kind", &"process_complete");
    let switch = db.message("s", "user", Some("model switched"));
    db.set(switch, "display_kind", &"model_switch");
    let failed = db.message("s", "assistant", Some("Your request was not processed."));
    db.set(failed, "display_kind", &"failed_turn");
    let steer = db.message("s", "user", Some("also do this"));
    db.set(steer, "display_kind", &"steer");
    assert!(typed < steer);
    let session = read(&roots, "s").unwrap();
    assert_eq!(
        kinds(&session),
        ["user", "meta", "meta", "meta", "meta", "user"]
    );
    assert_eq!(
        meta_kinds(&session),
        [
            MetaKind::Internal,
            MetaKind::Internal,
            MetaKind::ModelChange,
            MetaKind::Synthetic
        ]
    );
}

#[test]
fn content_of_every_cell_type_reads() {
    let (roots, db) = store("content");
    db.session("s", "cli", 1794480000.0);
    let parts = db.message("s", "user", None);
    db.set(
        parts,
        "content",
        &format!(
            "\u{0}json:{}",
            json!([
                {"type": "text", "text": "look at this"},
                {"type": "image_url", "image_url": {"url": "data:image/png;base64,AAAA"}},
                {"type": "text", "text": "and tell me"}
            ])
        ),
    );
    let blob = db.message("s", "user", None);
    db.set(blob, "content", &b"bytes as text".to_vec());
    let number = db.message("s", "user", None);
    db.set(number, "content", &42);
    let null = db.message("s", "user", None);
    let literal = db.message("s", "user", None);
    db.set(
        literal,
        "content",
        &format!(
            "\u{0}json:{}",
            json!("a string that starts with the prefix")
        ),
    );
    assert!(null > 0);
    let session = read(&roots, "s").unwrap();
    let texts: Vec<(String, usize)> = session
        .entries
        .iter()
        .map(|e| match &e.kind {
            Kind::User { text, attachments } => (text.clone(), attachments.len()),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(texts[0], ("look at this\nand tell me".to_string(), 1));
    assert_eq!(texts[1], ("bytes as text".to_string(), 0));
    // A number stored in `content` is text: the column's affinity says so.
    assert_eq!(texts[2], ("42".to_string(), 0));
    assert_eq!(texts[3], (String::new(), 0));
    // The prefix is only a prefix when it opens a list or an object.
    assert_eq!(texts[4].0, "a string that starts with the prefix");
    assert_eq!(session.entries[3].raw[0]["content"], Value::Null);
}

#[test]
fn tool_calls_carry_their_canonical_form_and_their_unit() {
    let (roots, db) = store("tools");
    db.session("s", "cli", 1794480000.0);
    db.message("s", "user", Some("go"));
    db.calls(
        "s",
        "",
        &[
            ("c1", "terminal", json!({"command": "ls", "timeout": 30})),
            (
                "c2",
                "search_files",
                json!({"pattern": "*.rs", "target": "files"}),
            ),
            ("c3", "search_files", json!({"pattern": "fn main"})),
            ("c4", "delegate_task", json!({"goal": "look around"})),
            ("c5", "mcp_github_search", json!({"q": "x"})),
            ("c6", "write_file", json!({"path": "a", "content": "b"})),
        ],
    );
    db.result(
        "s",
        "c1",
        "terminal",
        "{\"output\": \"\", \"exit_code\": 0}",
    );
    db.result("s", "c3", "search_files", "{\"matches\": []}");
    let session = read(&roots, "s").unwrap();
    let Kind::Step { calls, .. } = &session.entries[1].kind else {
        panic!("a step");
    };
    let canon: Vec<_> = calls.iter().map(|c| c.canonical.clone().unwrap()).collect();
    assert_eq!(
        canon,
        [
            CanonicalTool::Bash,
            CanonicalTool::Glob,
            CanonicalTool::Grep,
            CanonicalTool::Agent,
            CanonicalTool::Other("mcp_github_search".into()),
            CanonicalTool::Write,
        ]
    );
    // Seconds in Hermes, milliseconds in the canonical form.
    assert_eq!(calls[0].input["timeout"], json!(30000));
    assert_eq!(calls[0].input["command"], json!("ls"));
    // The calls that never got a result stay unanswered.
    let Kind::Results { results } = &session.entries[2].kind else {
        panic!("results");
    };
    assert_eq!(results.len(), 2);
}

#[test]
fn reasoning_is_kept_in_the_form_the_row_stores_it() {
    let (roots, db) = store("reasoning");
    db.session("s", "cli", 1794480000.0);
    db.set_session(
        "s",
        "model_config",
        &json!({"gateway_runtime": {"provider": "anthropic", "api_mode": "anthropic_messages"}})
            .to_string(),
    );
    db.message("s", "user", Some("think"));
    let plain = db.message("s", "assistant", Some("a"));
    db.set(plain, "reasoning", &"Just thinking.");
    let placeholder = db.message("s", "assistant", Some("b"));
    db.set(placeholder, "reasoning_content", &" ");
    let signed = db.message("s", "assistant", Some("c"));
    db.set(
        signed,
        "reasoning_details",
        &json!([{"type": "reasoning.text", "text": "Signed.", "signature": "SIG", "format": "anthropic-claude-v1"}]).to_string(),
    );
    let responses = db.message("s", "assistant", Some("d"));
    db.set(
        responses,
        "codex_reasoning_items",
        &json!([
            {"type": "reasoning", "summary": [{"type": "summary_text", "text": "Sum."}], "encrypted_content": "ENC"},
            {"type": "compaction", "encrypted_content": "CKPT"}
        ])
        .to_string(),
    );
    let session = read(&roots, "s").unwrap();
    let reasoning = |index: usize| match &session.entries[index].kind {
        Kind::Step { reasoning, .. } => reasoning.clone(),
        other => panic!("{other:?}"),
    };
    let plain = reasoning(1);
    assert_eq!(
        (plain[0].text.as_str(), plain[0].opaque.is_none()),
        ("Just thinking.", true)
    );
    assert!(reasoning(2).is_empty(), "a placeholder is not reasoning");
    let signed = reasoning(3);
    let opaque = signed[0].opaque.as_ref().unwrap();
    assert_eq!(opaque.protocol, Protocol::AnthropicMessages);
    assert_eq!(opaque.signature.as_deref(), Some("SIG"));
    let responses = reasoning(4);
    assert_eq!(
        responses.len(),
        1,
        "a compaction checkpoint is not reasoning"
    );
    assert_eq!(responses[0].text, "Sum.");
    let opaque = responses[0].opaque.as_ref().unwrap();
    assert_eq!(
        (opaque.protocol.clone(), opaque.encrypted.as_deref()),
        (Protocol::OpenaiResponses, Some("ENC"))
    );
}

#[test]
fn a_step_ends_the_way_hermes_says_it_did() {
    let (roots, db) = store("ends");
    db.session("s", "cli", 1794480000.0);
    db.message("s", "user", Some("x"));
    for (reason, text) in [("stop", "a"), ("length", "b"), ("content_filter", "c")] {
        let id = db.message("s", "assistant", Some(text));
        db.set(id, "finish_reason", &reason);
    }
    db.message("s", "assistant", Some("d"));
    let session = read(&roots, "s").unwrap();
    let ends: Vec<StepEnd> = session
        .entries
        .iter()
        .filter_map(|e| match &e.kind {
            Kind::Step { end, .. } => Some(end.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        ends,
        [
            StepEnd::Completed,
            StepEnd::Unknown,
            StepEnd::Error,
            StepEnd::Completed
        ]
    );
}

#[test]
fn only_the_order_of_ids_orders_rows() {
    let (roots, db) = store("order");
    db.session("s", "cli", 1794480000.0);
    let first = db.message("s", "user", Some("one"));
    let second = db.message("s", "user", Some("two"));
    // A clock that went backwards: the later row has the earlier time.
    db.set(second, "timestamp", &1.0);
    db.set(first, "timestamp", &2.0);
    let session = read(&roots, "s").unwrap();
    let texts = support::user_texts(&session);
    assert_eq!(texts, ["one", "two"]);
    assert_eq!(value_ids(&session), [first.to_string(), second.to_string()]);
}

fn value_ids(session: &Session) -> Vec<String> {
    session.entries.iter().map(|e| e.id.clone()).collect()
}

#[test]
fn the_raw_row_is_what_the_store_holds() {
    let (roots, db) = store("raw");
    db.session("s", "cli", 1794480000.0);
    let id = db.message("s", "user", Some("hi"));
    db.set(
        id,
        "display_metadata",
        &json!({"submission_id": "x"}).to_string(),
    );
    let session = read(&roots, "s").unwrap();
    let raw: &Value = &session.entries[0].raw[0];
    assert_eq!(raw["display_metadata"], json!({"submission_id": "x"}));
    assert!(raw.get("display_identity").is_none());
    assert_eq!(raw["session_id"], "s");
}

// ---------------------------------------------------------------------------------------------
// through the adapter, and out

#[test]
fn the_adapter_reads_the_export_and_the_store() {
    let adapter = harnesses::get(HarnessId::Hermes);
    let from_file = adapter.read_file(&export()).expect("the fixture reads");
    assert_eq!(from_file.meta.harness, HarnessId::Hermes);
    let (roots, db) = store("adapter");
    db.load_export(&export());
    let listed = adapter.reader().list(&roots).expect("the store lists");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, from_file.meta.id);
    assert_eq!(listed[0].title.as_deref(), Some("Line counts"));
    assert_eq!(adapter.store_root(&roots), roots.hermes_home.as_deref());
}

#[test]
fn a_hermes_session_hops_into_every_file_destination() {
    let session = hermes::read_file(&export()).unwrap();
    support::assert_hops_keeping(&session, "Append a third line to notes.txt.", "hermes-hop");
}
