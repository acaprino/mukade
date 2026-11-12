//! `dump --children` and `hop --children` end to end
//! (docs/superpowers/specs/2026-11-12-subagents-design.md, phase 2, task 3).
//!
//! The source is the Claude Code fixture with a `subagents/` folder: a foreground child, a
//! background child with a child of its own, and a teammate.

mod support;

use mukade::model::HarnessId;
use serde_json::Value;
use std::path::Path;
use support::{CLAUDE_PARENT as PARENT, cli_code, stage_claude_subagents};

const FG: &str = "a1111111111111111";
const BG: &str = "a2222222222222222";
const DEEP: &str = "a4444444444444444";

fn staged(tag: &str) -> std::path::PathBuf {
    let scratch = support::scratch(HarnessId::ClaudeCode, tag);
    stage_claude_subagents(&scratch);
    std::fs::create_dir_all(scratch.join("work")).unwrap();
    scratch
}

fn json(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|error| panic!("{error}: {text}"))
}

fn child<'a>(manifest: &'a Value, source_id: &str) -> &'a Value {
    manifest["children"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["source_id"] == source_id)
        .unwrap_or_else(|| panic!("no child {source_id} in {manifest:#}"))
}

/// `dump --children` nests each subagent session's own document under the one it hangs from;
/// without the flag the document has no `children` key.
#[test]
fn dump_children_nests_each_subagent_document() {
    let scratch = staged("cc-dump");

    let (code, stdout, stderr) = cli_code(&scratch, &["dump", "claude_code", PARENT, "--children"]);
    assert_eq!(code, Some(0), "{stderr}");
    let doc = json(&stdout);
    let kids = doc["children"].as_array().unwrap();
    let ids: Vec<&str> = kids
        .iter()
        .map(|k| k["meta"]["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&format!("{PARENT}.{FG}").as_str()), "{ids:?}");
    let bg = kids
        .iter()
        .find(|k| k["meta"]["id"] == format!("{PARENT}.{BG}").as_str())
        .unwrap();
    assert_eq!(
        bg["children"][0]["meta"]["id"],
        format!("{PARENT}.{DEEP}"),
        "a child of a child hangs under it"
    );
    assert!(bg["children"][0]["children"].as_array().unwrap().is_empty());
    assert_eq!(bg["meta"]["parent"], PARENT);

    let (code, stdout, stderr) = cli_code(&scratch, &["dump", "claude_code", PARENT]);
    assert_eq!(code, Some(0), "{stderr}");
    assert!(json(&stdout).get("children").is_none());
}

/// A subagent's own session dumps as itself: `--children` on it lists what hangs below it.
#[test]
fn dump_children_of_a_subagent_lists_what_hangs_below_it() {
    let scratch = staged("cc-dump-sub");
    let id = format!("{PARENT}.{BG}");

    let (code, stdout, stderr) = cli_code(&scratch, &["dump", "claude_code", &id, "--children"]);

    assert_eq!(code, Some(0), "{stderr}");
    let doc = json(&stdout);
    assert_eq!(doc["children"].as_array().unwrap().len(), 1, "{doc:#}");
}

/// `--children` is a `list`, `dump` and `hop` option, and needs the store: not a file.
#[test]
fn children_is_refused_where_it_means_nothing() {
    let scratch = staged("cc-refuse");
    for args in [
        &["write", "pi", "x.json", "--children"][..],
        &["document", "claude_code", "opencode", "x", "--children"][..],
    ] {
        let (code, _, stderr) = cli_code(&scratch, args);
        assert_eq!(code, Some(1), "{args:?}: {stderr}");
        assert!(
            stderr.contains("--children is supported only by list, dump and hop"),
            "{stderr}"
        );
    }
    let (code, _, stderr) = cli_code(
        &scratch,
        &[
            "dump",
            "claude_code",
            "--from-file",
            "x.jsonl",
            "--children",
        ],
    );
    assert_eq!(code, Some(1), "{stderr}");
    assert!(stderr.contains("--from-file"), "{stderr}");
}

/// Into Pi (no link): four sessions of their own, the manifest says so, a line per child on
/// stderr; without the flag nothing about children appears.
#[test]
fn hop_children_into_a_destination_without_a_link() {
    let scratch = staged("cc-hop-pi");
    let manifest_file = scratch.join("manifest.json");

    let (code, stdout, stderr) = cli_code(
        &scratch,
        &[
            "hop",
            "claude_code",
            PARENT,
            "pi",
            "--children",
            "--dest-version",
            "1.2.0",
            "--manifest",
            manifest_file.to_str().unwrap(),
        ],
    );

    assert_eq!(code, Some(0), "{stderr}");
    let receipt = json(&stdout);
    let manifest = &receipt["manifest"];
    let kids = manifest["children"].as_array().unwrap();
    assert_eq!(kids.len(), 4, "{manifest:#}");
    assert!(kids.iter().all(|k| k["link"] == "none"));
    let fg = child(manifest, &format!("{PARENT}.{FG}"));
    assert!(Path::new(fg["dest_path"].as_str().unwrap()).is_file());
    assert_ne!(fg["dest_id"], receipt["id"]);
    assert_eq!(
        stderr.lines().filter(|l| l.starts_with("child ")).count(),
        4,
        "{stderr}"
    );
    assert!(
        stderr.contains(&format!(
            "child {PARENT}.{FG} -> {}",
            fg["dest_id"].as_str().unwrap()
        )),
        "{stderr}"
    );
    let saved = json(&std::fs::read_to_string(manifest_file).unwrap());
    assert_eq!(saved["children"], manifest["children"]);

    let (code, stdout, stderr) = cli_code(
        &scratch,
        &[
            "hop",
            "claude_code",
            PARENT,
            "pi",
            "--dest-version",
            "1.2.0",
        ],
    );
    assert_eq!(code, Some(0), "{stderr}");
    assert!(json(&stdout)["manifest"].get("children").is_none());
    assert!(!stderr.contains("child "), "{stderr}");
}

/// Into Claude Code: the children are the written parent's own subagents, and `list --children`
/// of the destination store shows them under it.
#[test]
fn hop_children_into_claude_code_links_each_to_the_written_parent() {
    let scratch = staged("cc-hop-cc");
    let work = scratch.join("work");

    let (code, stdout, stderr) = cli_code(
        &scratch,
        &[
            "hop",
            "claude_code",
            PARENT,
            "claude_code",
            "--children",
            "--dest-version",
            "2.1.312",
            "--cwd",
            work.to_str().unwrap(),
        ],
    );

    assert_eq!(code, Some(0), "{stderr}");
    let receipt = json(&stdout);
    let dest = receipt["id"].as_str().unwrap();
    let manifest = &receipt["manifest"];
    let fg = child(manifest, &format!("{PARENT}.{FG}"));
    assert_eq!(fg["link"], "native");
    assert!(
        fg["dest_id"]
            .as_str()
            .unwrap()
            .starts_with(&format!("{dest}.a"))
    );

    let (code, stdout, stderr) = cli_code(&scratch, &["list", "claude_code", "--children"]);
    assert_eq!(code, Some(0), "{stderr}");
    let rows: Vec<Value> = stdout.lines().map(json).collect();
    let row = rows.iter().find(|r| r["id"] == fg["dest_id"]).unwrap();
    assert_eq!(row["parent"], dest);
    assert_eq!(row["kind"], "spawn");
}

/// A child that cannot be written fails the hop with exit 5 after the parent is written: the
/// receipt is still printed and the manifest saved.
#[test]
fn a_child_write_failure_is_exit_5_with_the_parents_receipt() {
    let scratch = staged("cc-hop-fail");
    let work = scratch.join("work");
    let reserved = "5ab1c0de-0000-4000-8000-0000000000ff";
    let project = mukade::roots::claude_project_dir_name(&work);
    let folder = scratch.join("home/projects").join(project).join(reserved);
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("subagents"), "in the way").unwrap();
    let manifest_file = scratch.join("manifest.json");

    let (code, stdout, stderr) = cli_code(
        &scratch,
        &[
            "hop",
            "claude_code",
            PARENT,
            "claude_code",
            "--children",
            "--dest-version",
            "2.1.312",
            "--cwd",
            work.to_str().unwrap(),
            "--dest-id",
            reserved,
            "--manifest",
            manifest_file.to_str().unwrap(),
        ],
    );

    assert_eq!(code, Some(5), "{stderr}");
    assert!(stderr.contains("the session was written"), "{stderr}");
    let receipt = json(&stdout);
    assert_eq!(receipt["id"], reserved);
    let saved = json(&std::fs::read_to_string(manifest_file).unwrap());
    assert!(
        saved["losses"].to_string().contains("not written"),
        "{saved:#}"
    );
}

/// `write` imports the session of a document that holds subagent sessions, and says it left them.
#[test]
fn write_says_it_leaves_a_documents_children() {
    let scratch = staged("cc-write-note");
    let file = scratch.join("session.json");
    let (code, _, stderr) = cli_code(
        &scratch,
        &[
            "dump",
            "claude_code",
            PARENT,
            "--children",
            "--output",
            file.to_str().unwrap(),
        ],
    );
    assert_eq!(code, Some(0), "{stderr}");

    let (code, stdout, stderr) = cli_code(
        &scratch,
        &[
            "write",
            "pi",
            file.to_str().unwrap(),
            "--dest-version",
            "1.2.0",
            "--cwd",
            scratch.join("work").to_str().unwrap(),
        ],
    );

    assert_eq!(code, Some(0), "{stderr}");
    assert!(stderr.contains("subagent session(s)"), "{stderr}");
    assert!(json(&stdout)["manifest"].get("children").is_none());
}
