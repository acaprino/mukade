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

/// Every path of the tree, root to leaf, as the texts on it: what the session says however its
/// branches are cut into runs.
fn paths(session: &Session) -> std::collections::BTreeSet<Vec<String>> {
    use std::collections::{BTreeSet, HashMap};
    let mut chains: Vec<(&[Entry], &Option<String>)> = vec![(&session.entries, &None)];
    chains.extend(session.branches.iter().map(|b| (&b.entries[..], &b.from)));
    let mut at: HashMap<&str, (usize, usize)> = HashMap::new();
    for (c, (entries, _)) in chains.iter().enumerate() {
        for (i, entry) in entries.iter().enumerate() {
            at.insert(entry.id.as_str(), (c, i));
        }
    }
    let texts = |entry: &Entry| match &entry.kind {
        Kind::User { text, .. } => Some(format!("user: {text}")),
        Kind::Step { text, .. } => Some(format!("assistant: {}", text.join(" "))),
        _ => None,
    };
    // The path to the end of chain `c` (through `upto` entries of it).
    fn path_to(
        chains: &[(&[Entry], &Option<String>)],
        at: &HashMap<&str, (usize, usize)>,
        c: usize,
        upto: usize,
        texts: &dyn Fn(&Entry) -> Option<String>,
    ) -> Vec<String> {
        let mut path = match chains[c].1 {
            Some(from) => {
                let (pc, pi) = at[from.as_str()];
                path_to(chains, at, pc, pi + 1, texts)
            }
            None => Vec::new(),
        };
        path.extend(chains[c].0.iter().take(upto).filter_map(texts));
        path
    }
    let mut leaves = BTreeSet::new();
    for (c, (entries, _)) in chains.iter().enumerate() {
        // A run ends in a leaf unless something hangs off its last entry.
        let last = entries.last().map(|e| e.id.as_str());
        let hung = chains
            .iter()
            .any(|(_, from)| from.as_deref().is_some_and(|f| Some(f) == last));
        if !hung {
            leaves.insert(path_to(&chains, &at, c, entries.len(), &texts));
        }
    }
    leaves
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

// ---------------------------------------------------------------------------------------------
// writers: branches travel where the destination keeps a tree

use mukade::writer::plan::{BranchKeeping, PlannedBlock, plan};
use mukade::writer::{WriteOpts, Writer};

fn opts_for(_dest: HarnessId) -> WriteOpts {
    WriteOpts {
        cwd: Some("/work".into()),
        dest_version: Some("1.0.0".into()),
        ..WriteOpts::default()
    }
}

#[test]
fn the_plan_keeps_a_branch_where_the_destination_has_a_tree_or_a_rollback() {
    let session = branched();
    for dest in [HarnessId::ClaudeCode, HarnessId::Pi, HarnessId::Codex] {
        let planned = plan(&session, dest, &opts_for(dest));
        assert_eq!(planned.branches.len(), 1, "{dest}");
        assert_eq!(planned.counts.branches_kept, 1, "{dest}");
        assert_eq!(planned.counts.branches_dropped, 0, "{dest}");
        // It leaves after the first answer: block 1 of the active path.
        assert_eq!(planned.branches[0].parent, Some((0, 1)), "{dest}");
        assert!(matches!(
            planned.branches[0].blocks.first(),
            Some(PlannedBlock::User { .. })
        ));
        assert_eq!(planned.branches[0].blocks.len(), 2, "{dest}");
    }
    assert_eq!(
        mukade::writer::for_harness(HarnessId::ClaudeCode).keeps_branches(),
        BranchKeeping::Tree
    );
    assert_eq!(
        mukade::writer::for_harness(HarnessId::Codex).keeps_branches(),
        BranchKeeping::Rollbacks
    );
}

#[test]
fn a_destination_with_one_conversation_leaves_every_branch_out_and_counts_it() {
    let session = branched();
    for dest in [
        HarnessId::OpenCode,
        HarnessId::Hermes,
        HarnessId::DeepSeekHarness,
    ] {
        let planned = plan(&session, dest, &opts_for(dest));
        assert!(planned.branches.is_empty(), "{dest}");
        assert_eq!(planned.counts.branches_kept, 0, "{dest}");
        assert_eq!(planned.counts.branches_dropped, 1, "{dest}");
    }
}

/// A branch that leaves in the middle of a turn, or from a record that is not there, cannot be
/// judged with what it shares: it is left out and counted, never written half.
#[test]
fn a_branch_that_cannot_stand_with_what_it_shares_is_left_out() {
    use mukade::model::{ResultContent, ToolCall, ToolResult};
    let call = ToolCall {
        id: "c1".into(),
        source_name: "Bash".into(),
        canonical: Some(mukade::model::CanonicalTool::Bash),
        input: json!({"command": "ls", "description": "list"}),
    };
    let step = entry(
        "s1",
        Kind::Step {
            model: None,
            reasoning: Vec::new(),
            text: Vec::new(),
            calls: vec![call],
            end: StepEnd::ToolUse,
        },
    );
    let results = |id: &str| {
        entry(
            id,
            Kind::Results {
                results: vec![ToolResult {
                    call_id: "c1".into(),
                    content: ResultContent::Text("out".into()),
                    is_error: false,
                    truncated_by_source: false,
                }],
            },
        )
    };
    let session = Session {
        meta: meta(),
        entries: vec![
            user("u1", "run it"),
            step,
            results("r1"),
            assistant("a1", "done"),
        ],
        branches: vec![
            // Leaves between the call and its answer: the prefix has an unanswered call.
            Branch {
                from: Some("s1".into()),
                entries: vec![results("r1b"), assistant("a1b", "done again")],
            },
            // Leaves from a record the session does not hold.
            Branch {
                from: Some("nowhere".into()),
                entries: vec![user("u2b", "lost")],
            },
            // Leaves after the answer: fine.
            Branch {
                from: Some("r1".into()),
                entries: vec![user("u2c", "and then"), assistant("a2c", "ok")],
            },
        ],
    };
    let planned = plan(
        &session,
        HarnessId::ClaudeCode,
        &opts_for(HarnessId::ClaudeCode),
    );
    assert_eq!(planned.counts.branches_kept, 1);
    assert_eq!(planned.counts.branches_dropped, 2);
    assert_eq!(planned.branches.len(), 1);
    assert!(matches!(
        planned.branches[0].blocks.first(),
        Some(PlannedBlock::User { .. })
    ));

    // Codex writes a branch only where a rollback can undo it: one that starts a user turn.
    let mut midway = session.clone();
    midway.branches = vec![Branch {
        from: Some("r1".into()),
        entries: vec![assistant("a1b", "an answer with no question")],
    }];
    let tree = plan(&midway, HarnessId::Pi, &opts_for(HarnessId::Pi));
    assert_eq!(tree.counts.branches_kept, 1);
    let rollback = plan(&midway, HarnessId::Codex, &opts_for(HarnessId::Codex));
    assert_eq!(rollback.counts.branches_kept, 0);
    assert_eq!(rollback.counts.branches_dropped, 1);
}

fn hop_with_branches(scratch: &Path, dest: &str, extra: &[&str]) -> (Value, std::path::PathBuf) {
    let mut args = vec![
        "hop",
        "claude_code",
        "s-1",
        dest,
        "--all-branches",
        "--cwd",
        "/work",
    ];
    args.extend_from_slice(extra);
    let (code, stdout, stderr) = support::cli_code(scratch, &args);
    assert_eq!(code, Some(0), "{stdout}{stderr}");
    let receipt: Value = serde_json::from_str(stdout.trim().lines().last().unwrap()).unwrap();
    let location = std::path::PathBuf::from(receipt["location"].as_str().unwrap());
    (receipt["manifest"].clone(), location)
}

fn staged_claude_tree(tag: &str) -> std::path::PathBuf {
    let scratch = support::scratch(HarnessId::ClaudeCode, tag);
    write_lines(&scratch.join("home/projects/p/s-1.jsonl"), &claude_tree());
    scratch
}

#[test]
fn a_claude_tree_hopped_into_claude_code_reads_back_with_the_same_branches() {
    use mukade::reader::claude_code;
    let scratch = staged_claude_tree("branches-claude-to-claude");
    let (manifest, file) = hop_with_branches(&scratch, "claude_code", &[]);
    assert_eq!(manifest["counts"]["branches_kept"], 2, "{manifest}");
    assert_eq!(manifest["counts"]["branches_dropped"], 0);

    // The resumed path is the source's, so the leaf is where it was.
    let active = claude_code::read_file(&file).unwrap();
    assert_eq!(
        conversation(&active.entries),
        ["user: q1", "assistant: r1", "user: q2", "assistant: r2"]
    );
    assert!(active.branches.is_empty());
    let all = claude_code::read_file_all_branches(&file).unwrap();
    assert_eq!(conversation(&all.entries), conversation(&active.entries));
    // The same tree: every path from the first prompt to a leaf (how the runs are cut may differ:
    // the reader follows the first child it meets in the file).
    let source =
        claude_code::read_file_all_branches(&scratch.join("home/projects/p/s-1.jsonl")).unwrap();
    assert_eq!(paths(&all), paths(&source));
    assert_eq!(paths(&all).len(), 3);
    assert_eq!(all.branches.len(), 2);
    // The active path's last row is the file's last conversation row (and `last-prompt` names it).
    let text = std::fs::read_to_string(&file).unwrap();
    let rows: Vec<Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let leaf = rows
        .iter()
        .rev()
        .find(|row| row["type"] == "assistant")
        .unwrap();
    assert_eq!(leaf["message"]["content"][0]["text"], "r2");
    let last_prompt = rows.iter().rev().find(|row| row["type"] == "last-prompt");
    if let Some(row) = last_prompt {
        assert_eq!(row["leafUuid"], leaf["uuid"]);
    }
}

#[test]
fn a_claude_tree_hopped_into_pi_reads_back_with_the_same_branches() {
    use mukade::reader::pi;
    let scratch = staged_claude_tree("branches-claude-to-pi");
    let (manifest, file) = hop_with_branches(&scratch, "pi", &[]);
    assert_eq!(manifest["counts"]["branches_kept"], 2, "{manifest}");
    let active = pi::read_file(&file).unwrap();
    assert_eq!(
        conversation(&active.entries),
        ["user: q1", "assistant: r1", "user: q2", "assistant: r2"]
    );
    let all = pi::read_file_all_branches(&file).unwrap();
    assert_eq!(all.branches.len(), 2);
    let source = mukade::reader::claude_code::read_file_all_branches(
        &scratch.join("home/projects/p/s-1.jsonl"),
    )
    .unwrap();
    assert_eq!(paths(&all), paths(&source));
}

#[test]
fn a_claude_tree_hopped_into_codex_keeps_the_branches_a_rollback_can_undo() {
    use mukade::reader::codex::{Branches, read_file_with};
    let scratch = staged_claude_tree("branches-claude-to-codex");
    let (manifest, file) = hop_with_branches(&scratch, "codex", &["--dest-version", "0.167.0"]);
    // q2b starts a turn: kept, then rolled back. r2c hangs off an assistant record: left out.
    assert_eq!(manifest["counts"]["branches_kept"], 1, "{manifest}");
    assert_eq!(manifest["counts"]["branches_dropped"], 1);
    assert!(
        manifest["losses"]
            .as_array()
            .unwrap()
            .iter()
            .any(|loss| loss
                .as_str()
                .unwrap()
                .contains("branches of the source not written")),
        "{manifest}"
    );
    let text = std::fs::read_to_string(&file).unwrap();
    assert_eq!(text.matches("thread_rolled_back").count(), 1);

    // Codex replays the rollback: the thread is the active path.
    let active = read_file_with(&file, Branches::Active).unwrap();
    assert_eq!(
        conversation(&active.entries),
        ["user: q1", "assistant: r1", "user: q2", "assistant: r2"]
    );
    let all = read_file_with(&file, Branches::All).unwrap();
    assert_eq!(all.branches.len(), 1);
    assert_eq!(
        conversation(&all.branches[0].entries),
        ["user: q2b", "assistant: r2b"]
    );
    assert_eq!(
        all.branches[0].from.as_deref(),
        Some(id_of_text(&all.entries, "r1"))
    );
}

#[test]
fn a_destination_with_one_conversation_gets_the_active_path_and_a_count() {
    let scratch = staged_claude_tree("branches-claude-to-dsh");
    let (manifest, _) = hop_with_branches(&scratch, "deepseek_harness", &[]);
    assert_eq!(manifest["counts"]["branches_kept"], 0, "{manifest}");
    assert_eq!(manifest["counts"]["branches_dropped"], 2);
    assert!(
        manifest["losses"]
            .as_array()
            .unwrap()
            .iter()
            .any(|loss| loss.as_str().unwrap().ends_with("(2)")
                && loss.as_str().unwrap().contains("branches of the source")),
        "{manifest}"
    );
    assert_eq!(manifest["counts"]["messages_out"], 4);
}
