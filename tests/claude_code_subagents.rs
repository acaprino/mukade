//! Claude Code subagent sessions (docs/superpowers/specs/2026-11-12-subagents-design.md, phase 1).
//!
//! A subagent's transcript is `projects/<p>/<parent-id>/subagents/agent-<agentId>.jsonl`, every
//! conversation row `isSidechain`, beside an `agent-<agentId>.meta.json` that names the call and
//! the agent type. The fixture (`fixtures/claude_code/subagents`, synthetic) holds a parent with a
//! foreground child, a background child that spawns one of its own, a teammate, a meta.json with
//! no transcript and a forked-skill record.

use mukade::model::{Kind, ParentKind, Session};
use mukade::reader::claude_code::{self, ClaudeCode};
use mukade::reader::{Lookup, Reader};
use support::CLAUDE_PARENT as PARENT;

mod support;

const FG: &str = "a1111111111111111";
const BG: &str = "a2222222222222222";
const DEEP: &str = "a4444444444444444";
const MATE: &str = "amate-6666666666666666";

fn child(agent: &str) -> String {
    format!("{PARENT}.{agent}")
}

fn staged(tag: &str) -> mukade::roots::StoreRoots {
    let scratch = support::scratch(mukade::model::HarnessId::ClaudeCode, tag);
    support::stage_claude_subagents(&scratch)
}

fn spawn(call_id: Option<&str>, agent: &str) -> Option<ParentKind> {
    Some(ParentKind::Spawn {
        call_id: call_id.map(str::to_owned),
        agent: Some(agent.into()),
    })
}

/// One line per conversation entry: its kind and its first words.
fn script(session: &Session) -> Vec<String> {
    session
        .conversation()
        .map(|entry| match &entry.kind {
            Kind::User { text, .. } => format!("user: {text}"),
            Kind::Step { text, calls, .. } => {
                let calls: Vec<&str> = calls.iter().map(|c| c.source_name.as_str()).collect();
                format!("step: {} {calls:?}", text.join(" "))
            }
            Kind::Results { results } => format!("results: {}", results.len()),
            other => format!("{other:?}"),
        })
        .collect()
}

/// `list` is unchanged: the roots only. `list_all` adds every child that has a transcript, with
/// its qualified id, the title its meta.json gives, and how it came from its parent.
#[test]
fn list_all_lists_each_child_that_has_a_transcript() {
    let roots = staged("sub-list");

    let roots_only = ClaudeCode.list(&roots).unwrap();
    let ids: Vec<&str> = roots_only.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, [PARENT]);
    assert_eq!(roots_only[0].parent, None);
    assert_eq!(roots_only[0].parent_kind, None);

    let all = ClaudeCode.list_all(&roots).unwrap();
    let ids: Vec<&str> = all.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            PARENT.to_string(),
            child(FG),
            child(BG),
            child(DEEP),
            child(MATE)
        ],
        "no child without a transcript, no forked-skill record; each child after its parent"
    );
    let info = |agent: &str| all.iter().find(|s| s.id == child(agent)).unwrap();

    let fg = info(FG);
    assert_eq!(fg.title.as_deref(), Some("Read module A"));
    assert_eq!(fg.parent.as_deref(), Some(PARENT));
    assert_eq!(fg.parent_kind, spawn(Some("toolu_fg"), "Explore"));
    assert_eq!(fg.cwd.as_deref(), Some(r"D:\work\modules"));
    assert!(
        fg.path.ends_with(&format!("agent-{FG}.jsonl")),
        "the location is the transcript: {}",
        fg.path
    );
    assert!(fg.updated.is_some());
    assert_eq!(fg.unreadable, None);

    assert_eq!(
        info(BG).parent_kind,
        spawn(Some("toolu_bg"), "general-purpose")
    );
    let deep = info(DEEP);
    assert_eq!(
        deep.parent.as_deref(),
        Some(child(BG).as_str()),
        "a child of a child names the agent that spawned it"
    );
    assert_eq!(deep.parent_kind, spawn(Some("toolu_b_inner"), "Explore"));
    let mate = info(MATE);
    assert_eq!(mate.title.as_deref(), Some("Changelog keeper"));
    assert_eq!(mate.parent_kind, spawn(None, "in_process_teammate"));
}

/// The cache keeps the children's transcripts in a table of their own; a second listing from it
/// is the first one.
#[test]
fn list_all_from_the_cache_lists_the_same_children() {
    let scratch = support::scratch(mukade::model::HarnessId::ClaudeCode, "sub-cache");
    let roots = support::stage_claude_subagents(&scratch);
    let cache = scratch.join("cache");
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(3_600);
    for file in support::files_under(&scratch.join("home")) {
        std::fs::File::options()
            .write(true)
            .open(&file)
            .unwrap()
            .set_modified(old)
            .unwrap();
    }

    let cold = claude_code::list_all_cached(&roots, Some(&cache)).unwrap();
    let warm = claude_code::list_all_cached(&roots, Some(&cache)).unwrap();

    assert_eq!(cold.len(), 5);
    assert_eq!(cold, warm);
    let tables: Vec<String> = std::fs::read_dir(&cache)
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        tables.iter().any(|name| name.contains("claude-subagents")),
        "{tables:?}"
    );
}

/// A child is read by its qualified id: its sidechain rows are its conversation, and its meta
/// names it, its parent and the call that started it.
#[test]
fn a_child_is_read_by_its_qualified_id() {
    let roots = staged("sub-read");

    let fg = ClaudeCode.read(&child(FG), &roots).unwrap();

    assert_eq!(fg.meta.id, child(FG));
    assert_eq!(fg.meta.parent.as_deref(), Some(PARENT));
    assert_eq!(fg.meta.parent_kind, spawn(Some("toolu_fg"), "Explore"));
    assert_eq!(fg.meta.title.as_deref(), Some("Read module A"));
    assert_eq!(fg.meta.cwd.as_deref(), Some(r"D:\work\modules"));
    assert_eq!(
        script(&fg),
        [
            "user: Read module A and report what it exports.",
            "step:  [\"Read\"]",
            "results: 1",
            "step: Module A exports parse(). []",
        ]
    );

    let deep = ClaudeCode.read(&child(DEEP), &roots).unwrap();
    assert_eq!(deep.meta.parent.as_deref(), Some(child(BG).as_str()));
}

/// A qualified id is matched exactly: an agent id's prefix, or the parent id's, finds nothing.
#[test]
fn a_child_id_is_never_a_prefix() {
    let roots = staged("sub-exact");
    for id in [
        format!("{PARENT}.a111"),
        format!("{}.{FG}", &PARENT[..8]),
        format!("{PARENT}.a3333333333333333"),
    ] {
        let error = ClaudeCode.read(&id, &roots).unwrap_err();
        assert!(
            matches!(error.downcast_ref::<Lookup>(), Some(Lookup::NotFound(_))),
            "{id}: {error:#}"
        );
    }
}

/// `--from-file` of a child transcript reads the same session as its id. Without its meta.json
/// beside it, what the rows say is all there is: the parent, and a spawn with nothing recorded.
#[test]
fn a_child_transcript_reads_as_the_child_from_its_file() {
    let roots = staged("sub-file");
    let folder = roots
        .claude_projects()
        .unwrap()
        .join("D--work-modules")
        .join(PARENT)
        .join("subagents");
    let file = folder.join(format!("agent-{FG}.jsonl"));

    let from_file = claude_code::read_file(&file).unwrap();
    let by_id = ClaudeCode.read(&child(FG), &roots).unwrap();
    assert_eq!(from_file, by_id);

    let rows: Vec<serde_json::Value> = std::fs::read_to_string(&file)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let bare = claude_code::parse(&rows, None).unwrap();
    assert_eq!(bare.meta.id, child(FG));
    assert_eq!(bare.meta.parent.as_deref(), Some(PARENT));
    assert_eq!(bare.meta.parent_kind, Some(ParentKind::spawn()));
    assert_eq!(script(&bare), script(&by_id));
    assert_eq!(
        bare.meta.title.as_deref(),
        Some("Read module A and report what it exports.")
    );
}

/// The parent reads as before: no parent of its own, and its children stay in their own files.
#[test]
fn the_parent_reads_as_a_root() {
    let roots = staged("sub-parent");
    let parent = ClaudeCode.read(&PARENT[..8], &roots).unwrap();
    assert_eq!(parent.meta.id, PARENT);
    assert_eq!(parent.meta.parent, None);
    assert_eq!(parent.meta.parent_kind, None);
    assert_eq!(script(&parent)[0], "user: What do modules A and B export?");
}

/// `mukade dump claude_code <parent-id>.<agentId>` prints the child's document, which states its
/// parent and how it came from it.
#[test]
fn a_child_is_dumped_by_its_qualified_id() {
    let scratch = support::scratch(mukade::model::HarnessId::ClaudeCode, "sub-dump");
    support::stage_claude_subagents(&scratch);

    let (ok, stdout, stderr) = support::cli(&scratch, &["dump", "claude_code", &child(BG)]);

    assert!(ok, "{stderr}");
    let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(doc["meta"]["id"], child(BG));
    assert_eq!(doc["meta"]["parent"], PARENT);
    assert_eq!(
        doc["meta"]["parent_kind"],
        serde_json::json!({"kind": "spawn", "call_id": "toolu_bg", "agent": "general-purpose"})
    );
    assert_eq!(
        doc["messages"][0]["content"][0]["text"],
        "Read module B and report what it exports."
    );
}

/// A subagent's opening row is the prompt its parent gave. Claude Code marks it `isMeta` when it
/// holds an expanded instruction rather than typed text; it is still the child's prompt, so the
/// child does not start with a reply to nothing, and rows written out of order are still read.
#[test]
fn an_opening_prompt_marked_is_meta_is_the_childs_prompt() {
    let row = |uuid: &str, parent: Option<&str>, kind: &str, message: serde_json::Value| {
        serde_json::json!({
            "parentUuid": parent, "isSidechain": true, "agentId": "a7777777777777777",
            "type": kind, "message": message, "uuid": uuid,
            "timestamp": format!("2026-11-12T10:00:0{}.000Z", uuid.len()),
            "cwd": "D:/work", "sessionId": PARENT, "version": "2.1.318",
        })
    };
    let mut opening = row(
        "o",
        None,
        "user",
        serde_json::json!({"role": "user", "content": "Review the diff along ten angles."}),
    );
    opening["isMeta"] = true.into();
    let reply = |uuid: &str, parent: &str, text: &str| {
        row(
            uuid,
            Some(parent),
            "assistant",
            serde_json::json!({"id": format!("msg_{uuid}"), "role": "assistant",
                               "model": "claude-opus-5-5", "stop_reason": "end_turn",
                               "content": [{"type": "text", "text": text}]}),
        )
    };
    let follow_up = row(
        "u2",
        Some("a1"),
        "user",
        serde_json::json!({"role": "user", "content": "And the tests?"}),
    );
    let (a1, a2) = (reply("a1", "o", "Done."), reply("a2", "u2", "Covered."));
    let expected = [
        "user: Review the diff along ten angles.",
        "step: Done. []",
        "user: And the tests?",
        "step: Covered. []",
    ];
    // In file order, and with the follow-up written before the opening row: the chain decides.
    for rows in [
        vec![opening.clone(), a1.clone(), follow_up.clone(), a2.clone()],
        vec![follow_up.clone(), opening.clone(), a1.clone(), a2.clone()],
    ] {
        let session = claude_code::parse(&rows, None).unwrap();
        assert_eq!(script(&session), expected);
        assert_eq!(
            session.meta.title.as_deref(),
            Some("Review the diff along ten angles.")
        );
    }
}
