//! The hop registry end to end (docs/superpowers/specs/2026-11-12-hop-registry-design.md): a fork
//! hopped after its origin is linked to the origin's copy, a repeated hop says it was written
//! before, and `MUKADE_REGISTRY=off` turns it all off. Each test has its own data folder.

mod support;

use mukade::model::HarnessId;
use serde_json::{Value, json};
use std::path::Path;

const ORIGIN: &str = "11111111-1111-4111-8111-111111111111";
const FORK: &str = "22222222-2222-4222-8222-222222222222";

fn write_lines(path: &Path, rows: &[Value]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let text: String = rows.iter().map(|row| format!("{row}\n")).collect();
    std::fs::write(path, text).unwrap();
}

/// A Claude Code origin of one turn, and a fork that copied it (`forkedFrom` on each copied row)
/// and went on with a turn of its own.
fn claude_store(scratch: &Path) {
    let row = |session: &str, uuid: &str, parent: Option<&str>, body: Value| {
        let mut row = json!({"uuid": uuid, "parentUuid": parent, "sessionId": session,
            "timestamp": "2026-11-12T10:00:00.000Z", "cwd": "/work", "isSidechain": false});
        row.as_object_mut()
            .unwrap()
            .extend(body.as_object().unwrap().clone());
        row
    };
    let question =
        json!({"type": "user", "message": {"role": "user", "content": "the first question"}});
    let answer = json!({"type": "assistant", "message": {"id": "msg_1", "role": "assistant",
        "model": "claude-x", "stop_reason": "end_turn",
        "content": [{"type": "text", "text": "the first answer"}]}});
    let project = scratch.join("home/projects/p");
    write_lines(
        &project.join(format!("{ORIGIN}.jsonl")),
        &[
            row(ORIGIN, "u-1", None, question.clone()),
            row(ORIGIN, "a-1", Some("u-1"), answer.clone()),
        ],
    );
    let mut copied = vec![
        row(FORK, "f-1", None, question),
        row(FORK, "f-2", Some("f-1"), answer),
    ];
    for (row, origin) in copied.iter_mut().zip(["u-1", "a-1"]) {
        row["forkedFrom"] = json!({"sessionId": ORIGIN, "messageUuid": origin});
    }
    copied.push(row(
        FORK,
        "f-3",
        Some("f-2"),
        json!({"type": "user", "message": {"role": "user", "content": "a question of the fork's own"}}),
    ));
    write_lines(&project.join(format!("{FORK}.jsonl")), &copied);
}

fn hop(scratch: &Path, id: &str, dest: &str, extra: &[(&str, &str)]) -> (Value, String) {
    let work = scratch.join("work");
    std::fs::create_dir_all(&work).unwrap();
    let data = scratch.join("data");
    let mut env = vec![("MUKADE_DATA_DIR", data.to_str().unwrap())];
    env.extend_from_slice(extra);
    let version = if dest == "pi" { "1.2.0" } else { "2.1.312" };
    let (code, stdout, stderr) = support::cli_in(
        scratch,
        scratch,
        &[
            "hop",
            "claude_code",
            id,
            dest,
            "--dest-version",
            version,
            "--cwd",
            work.to_str().unwrap(),
        ],
        &env,
    );
    assert_eq!(code, Some(0), "{stderr}");
    (serde_json::from_str(&stdout).unwrap(), stderr)
}

fn rows_of(path: &str) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// Origin first, then its fork, into Claude Code: the fork's copied rows name the origin's copy
/// and the rows they are there (`forkedFrom`); its own turn names nothing; mukade reads the
/// written fork back as a fork of the copy.
#[test]
fn a_fork_hopped_after_its_origin_is_linked_to_the_origins_copy() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "registry-claude-fork");
    claude_store(&scratch);
    let (origin, _) = hop(&scratch, ORIGIN, "claude_code", &[]);
    let origin_id = origin["id"].as_str().unwrap();
    let origin_uuids: Vec<String> = rows_of(origin["location"].as_str().unwrap())
        .iter()
        .filter_map(|row| row["uuid"].as_str().map(str::to_owned))
        .collect();

    let (fork, _) = hop(&scratch, FORK, "claude_code", &[]);
    assert_eq!(fork["manifest"]["dest"]["parent"], origin_id, "{fork:#}");
    let rows = rows_of(fork["location"].as_str().unwrap());
    let conversation: Vec<&Value> = rows.iter().filter(|row| row["uuid"].is_string()).collect();
    assert_eq!(conversation.len(), 3, "{rows:#?}");
    for (row, uuid) in conversation[..2].iter().zip(&origin_uuids) {
        assert_eq!(
            row["forkedFrom"],
            json!({"sessionId": origin_id, "messageUuid": uuid})
        );
    }
    assert!(conversation[2].get("forkedFrom").is_none());

    let read =
        mukade::reader::claude_code::read_file(Path::new(fork["location"].as_str().unwrap()))
            .unwrap();
    assert_eq!(read.meta.parent.as_deref(), Some(origin_id));
    assert_eq!(
        read.meta.parent_kind,
        Some(mukade::model::ParentKind::Fork {
            at: origin_uuids.get(1).cloned()
        })
    );
}

/// Into Pi the fork's header names the origin's copy (`parentSession`), which Pi reads as its
/// parent.
#[test]
fn a_fork_hopped_into_pi_names_its_origins_copy() {
    let scratch = support::scratch(HarnessId::Pi, "registry-pi-fork");
    claude_store(&scratch);
    let (origin, _) = hop(&scratch, ORIGIN, "pi", &[]);
    let (fork, _) = hop(&scratch, FORK, "pi", &[]);
    assert_eq!(fork["manifest"]["dest"]["parent"], origin["id"]);
    let header = &rows_of(fork["location"].as_str().unwrap())[0];
    assert_eq!(header["parentSession"], origin["location"]);
    let read =
        mukade::reader::pi::read_file(Path::new(fork["location"].as_str().unwrap())).unwrap();
    assert_eq!(read.meta.parent.as_deref(), origin["id"].as_str());
}

/// A fork whose origin was not hopped there is not linked, and the manifest says why; a fork
/// into a destination with no fork link says nothing.
#[test]
fn a_fork_without_its_origins_copy_says_why_it_is_not_linked() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "registry-fork-alone");
    claude_store(&scratch);
    let (fork, _) = hop(&scratch, FORK, "pi", &[]);
    assert!(fork["manifest"]["dest"].get("parent").is_none());
    let losses = fork["manifest"]["losses"].to_string();
    assert!(
        losses.contains(&format!(
            "fork of {ORIGIN}: its origin was not written to this destination"
        )),
        "{losses}"
    );
    let header = &rows_of(fork["location"].as_str().unwrap())[0];
    assert!(header.get("parentSession").is_none());
}

/// The same unchanged session hopped again: a new copy, and a notice naming the earlier one.
#[test]
fn a_repeated_hop_writes_a_new_copy_and_names_the_earlier_one() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "registry-repeat");
    claude_store(&scratch);
    let (first, stderr) = hop(&scratch, ORIGIN, "pi", &[]);
    assert!(first["manifest"].get("earlier").is_none());
    assert!(!stderr.contains("already written"), "{stderr}");
    let (second, stderr) = hop(&scratch, ORIGIN, "pi", &[]);
    assert_ne!(second["id"], first["id"]);
    assert_eq!(second["manifest"]["earlier"]["dest_id"], first["id"]);
    assert!(
        stderr.contains(&format!(
            "already written here, unchanged, as {}",
            first["id"].as_str().unwrap()
        )),
        "{stderr}"
    );
    // A copy that was deleted is not named.
    std::fs::remove_file(second["location"].as_str().unwrap()).unwrap();
    std::fs::remove_file(first["location"].as_str().unwrap()).unwrap();
    let (third, _) = hop(&scratch, ORIGIN, "pi", &[]);
    assert!(third["manifest"].get("earlier").is_none(), "{third:#}");
}

/// `MUKADE_REGISTRY=off`: nothing is remembered, nothing is linked or named.
#[test]
fn the_registry_can_be_turned_off() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "registry-off");
    claude_store(&scratch);
    let off = [("MUKADE_REGISTRY", "off")];
    hop(&scratch, ORIGIN, "pi", &off);
    let (fork, _) = hop(&scratch, FORK, "pi", &off);
    assert!(fork["manifest"]["dest"].get("parent").is_none());
    assert!(!fork["manifest"]["losses"].to_string().contains("fork of"));
    let (again, stderr) = hop(&scratch, ORIGIN, "pi", &off);
    assert!(again["manifest"].get("earlier").is_none());
    assert!(!stderr.contains("already written"));
    assert!(!scratch.join("data/registry").exists());
}
