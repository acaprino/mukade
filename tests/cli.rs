//! The command line, end to end: the real binary over the fixtures, on a scratch store.
//!
//! Everything runs with the four store variables pointing inside Cargo's scratch area, so no real
//! store is in reach.

mod support;

use mukade::model::HarnessId;
use mukade::reader::for_harness;
use mukade::roots::StoreRoots;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

use support::{BIN, cli, cli_code, stage_fixture};

fn dump(harness: HarnessId, id: &str) -> (PathBuf, Value) {
    let scratch = support::scratch(harness, &format!("cli-dump-{}", harness.as_str()));
    let _ = stage_fixture(harness, &scratch);
    let (ok, stdout, stderr) = cli(&scratch, &["dump", harness.as_str(), id]);
    assert!(ok, "dump failed: {stderr}");
    let doc: Value = serde_json::from_str(&stdout).expect("dump prints one JSON document");
    (scratch, doc)
}

fn write_document(dest: HarnessId, doc: &Value, tag: &str, extra: &[&str]) -> (PathBuf, Value) {
    let scratch = support::scratch(dest, tag);
    let file = scratch.join("document.json");
    std::fs::write(&file, serde_json::to_string(doc).unwrap()).unwrap();
    let mut args = vec!["write", dest.as_str(), file.to_str().unwrap()];
    args.extend_from_slice(extra);
    let (ok, stdout, stderr) = cli(&scratch, &args);
    assert!(ok, "write failed: {stderr}");
    let printed: Value =
        serde_json::from_str(stdout.lines().last().unwrap()).expect("write prints one JSON line");
    (scratch, printed)
}

#[test]
fn dump_prints_the_conversation_as_one_message_per_step() {
    let (_, doc) = dump(HarnessId::ClaudeCode, "c45db7bc");
    assert_eq!(doc["meta"]["harness"], "claude_code");
    assert_eq!(doc["meta"]["harness_version"], "2.1.311");
    let messages = doc["messages"].as_array().expect("messages");
    let first_assistant = messages
        .iter()
        .find(|m| m["role"] == "assistant")
        .expect("an assistant message");
    let kinds: Vec<&str> = first_assistant["content"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["type"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        vec!["thinking", "text", "tool_use", "tool_use"],
        "the two parallel calls stay together"
    );
    assert_eq!(
        first_assistant["content"][0]["opaque"]["signature"],
        "h27-mock-signature"
    );

    let first_results = messages
        .iter()
        .find(|m| m["content"][0]["type"] == "tool_result")
        .expect("a results message");
    assert_eq!(first_results["content"].as_array().unwrap().len(), 2);

    // L4: the source's own compaction summary travels as a user message.
    let summaries: Vec<&str> = messages
        .iter()
        .filter_map(|m| m["content"][0]["text"].as_str())
        .filter(|t| t.contains("H27-SUMMARY"))
        .collect();
    assert!(
        !summaries.is_empty(),
        "the source summary is in the document"
    );
    assert!(
        messages.iter().any(|m| m["compaction"].is_object()),
        "and it is marked as a compaction"
    );
}

#[test]
fn list_reports_the_session_it_can_read() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "cli-list");
    let _ = stage_fixture(HarnessId::ClaudeCode, &scratch);
    let (ok, stdout, stderr) = cli(&scratch, &["list", "claude_code"]);
    assert!(ok, "{stderr}");
    let line: Value = serde_json::from_str(stdout.lines().next().expect("one line")).unwrap();
    assert_eq!(line["id"], "c45db7bc-542c-4201-89de-b590c6fdb797");
    assert!(line["location"].as_str().unwrap().ends_with(".jsonl"));
}

/// The pair the bench uses: `dump` a source session, `write` it into another harness, read it back.
#[test]
fn dump_then_write_produces_a_session_the_destination_reader_understands() {
    let (_, doc) = dump(HarnessId::ClaudeCode, "c45db7bc");
    let (scratch, printed) = write_document(
        HarnessId::Pi,
        &doc,
        "cli-write-pi",
        &[
            "--dest-version",
            "1.2.0",
            "--dest-protocol",
            "anthropic-messages",
        ],
    );
    let location = printed["location"].as_str().expect("a location");
    assert!(location.contains("mukade-tests"), "{location}");
    assert_eq!(printed["resume"][0], "pi");
    let _ = scratch;

    let back = mukade::reader::pi::read_file(Path::new(location)).expect("read back");
    assert_eq!(back.meta.id, printed["id"].as_str().unwrap());
    let texts: Vec<String> = back
        .conversation()
        .filter_map(|e| match &e.kind {
            mukade::model::Kind::User { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert!(
        texts.iter().any(|t| t.starts_with("H27-TOOLS read")),
        "{texts:?}"
    );
    assert!(
        texts.iter().any(|t| t.contains("H27-SUMMARY")),
        "the summary survived the hop"
    );
    assert!(
        back.interrupted_last_turn(),
        "the killed last turn is still a user turn with no reply"
    );
}

#[test]
fn write_into_the_same_harness_round_trips_the_conversation() {
    let (_, doc) = dump(HarnessId::Codex, "01a17e07");
    let (_, printed) = write_document(
        HarnessId::Codex,
        &doc,
        "cli-write-codex",
        &[
            "--dest-version",
            "0.164.0",
            "--dest-shell",
            "powershell",
            "--model-provider",
            "h27",
        ],
    );
    // The dump and the write agree on the conversation: read the written rollout back.
    let back = mukade::reader::codex::read_file(Path::new(printed["location"].as_str().unwrap()))
        .expect("read back");
    let source = mukade::reader::codex::read_file(&support::source_path(HarnessId::Codex)).unwrap();
    assert_eq!(
        support::roundtrip_script(&back),
        support::roundtrip_script(&source)
    );
    assert_eq!(
        back.meta.cwd, source.meta.cwd,
        "the project travels with the session"
    );
}

#[test]
fn detect_recognizes_a_store_and_root_infers_its_harness() {
    let scratch = support::scratch(HarnessId::Pi, "cli-detect-pi");
    let home = scratch.join("home");
    let _ = stage_fixture(HarnessId::Pi, &scratch);
    let (ok, stdout, stderr) = cli(&scratch, &["detect", home.to_str().unwrap()]);
    assert!(ok, "{stderr}");
    let doc: Value = serde_json::from_str(&stdout).unwrap();
    let harnesses: Vec<&str> = doc["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["harness"].as_str().unwrap())
        .collect();
    assert_eq!(harnesses, vec!["pi"], "{doc}");

    // The harness can be omitted: the directory's own layout says it.
    let (ok, stdout, stderr) = cli(&scratch, &["list", "--root", home.to_str().unwrap()]);
    assert!(ok, "{stderr}");
    let line: Value = serde_json::from_str(stdout.lines().next().expect("one line")).unwrap();
    assert_eq!(line["id"], "01a17e17-9f07-743d-8072-8b3b13d65ab5");
}

#[test]
fn an_empty_root_and_an_ambiguous_root_are_refused_for_inference() {
    let scratch = support::scratch(HarnessId::Pi, "cli-detect-empty");
    let home = scratch.join("home");
    let (ok, stdout, _) = cli(&scratch, &["detect", home.to_str().unwrap()]);
    assert!(ok);
    let doc: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(doc["candidates"].as_array().unwrap().len(), 0);
    let (ok, _, stderr) = cli(&scratch, &["list", "--root", home.to_str().unwrap()]);
    assert!(!ok);
    assert!(stderr.contains("no harness store recognized"), "{stderr}");

    let scratch = support::scratch(HarnessId::ClaudeCode, "cli-detect-ambiguous");
    let home = scratch.join("home");
    let _ = stage_fixture(HarnessId::ClaudeCode, &scratch);
    let _ = stage_fixture(HarnessId::Pi, &scratch);
    let (ok, _, stderr) = cli(&scratch, &["list", "--root", home.to_str().unwrap()]);
    assert!(!ok);
    assert!(stderr.contains("more than one"), "{stderr}");
}

#[test]
fn hop_infers_the_source_harness_from_root() {
    let scratch = support::scratch(HarnessId::Pi, "cli-hop-infer");
    let home = scratch.join("home");
    let _ = stage_fixture(HarnessId::ClaudeCode, &scratch);
    let (ok, stdout, stderr) = cli(
        &scratch,
        &[
            "hop",
            "--root",
            home.to_str().unwrap(),
            "c45db7bc",
            "pi",
            "--dest-version",
            "1.2.0",
        ],
    );
    assert!(ok, "{stderr}");
    let printed: Value = serde_json::from_str(stdout.lines().last().unwrap()).unwrap();
    let location = printed["location"].as_str().unwrap();
    assert!(Path::new(location).is_file(), "{location}");
}

#[test]
fn a_custom_pi_binary_is_named_in_the_resume_command() {
    let (_, doc) = dump(HarnessId::ClaudeCode, "c45db7bc");
    let exe = if cfg!(windows) {
        "D:\\pi-portable\\bin\\pi.cmd"
    } else {
        "/opt/pi-portable/bin/pi"
    };
    let (_, printed) = write_document(
        HarnessId::Pi,
        &doc,
        "cli-write-pi-exe",
        &["--dest-version", "1.2.0", "--pi-exe", exe],
    );
    assert_eq!(printed["resume"][0], exe);
    assert_eq!(printed["resume"][1], "-p");
}

#[test]
fn an_empty_exe_flag_falls_back_to_the_default_binary_name() {
    let (_, doc) = dump(HarnessId::ClaudeCode, "c45db7bc");
    let (_, printed) = write_document(
        HarnessId::Pi,
        &doc,
        "cli-write-pi-empty-exe",
        &["--dest-version", "1.2.0", "--pi-exe", ""],
    );
    assert_eq!(printed["resume"][0], "pi");
}

#[test]
fn an_empty_root_is_rejected() {
    let scratch = support::scratch(HarnessId::Pi, "cli-empty-root");
    let (ok, _, stderr) = cli(&scratch, &["list", "pi", "--root", ""]);
    assert!(!ok);
    assert!(stderr.contains("non-empty"), "{stderr}");
}

#[test]
fn bad_arguments_fail_loudly() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "cli-bad");
    let (code, _, stderr) = cli_code(&scratch, &["dump", "nonesuch", "x"]);
    assert_eq!(code, Some(1), "a usage error");
    assert!(stderr.contains("unknown harness"), "{stderr}");
    let (code, _, stderr) = cli_code(&scratch, &["fly", "pi"]);
    assert_eq!(code, Some(1), "a usage error");
    assert!(stderr.contains("unknown command"), "{stderr}");
    let (code, _, stderr) = cli_code(&scratch, &["dump", "claude_code", "c45db7bc"]);
    assert_eq!(code, Some(2), "a session that is not there is refused");
    assert!(stderr.contains("no Claude Code session"), "{stderr}");
}

/// Once the session is written, its receipt reaches stdout whatever fails after: a caller that
/// sees no receipt writes the session again.
#[test]
fn a_manifest_that_cannot_be_saved_still_prints_the_written_session() {
    let (_, doc) = dump(HarnessId::ClaudeCode, "c45db7bc");
    let scratch = support::scratch(HarnessId::Pi, "cli-write-manifest-unsaved");
    let file = scratch.join("document.json");
    std::fs::write(&file, serde_json::to_string(&doc).unwrap()).unwrap();
    let manifest = scratch.join("no-such-dir").join("manifest.json");
    let (code, stdout, stderr) = cli_code(
        &scratch,
        &[
            "write",
            "pi",
            file.to_str().unwrap(),
            "--dest-version",
            "1.2.0",
            "--manifest",
            manifest.to_str().unwrap(),
        ],
    );
    assert_ne!(code, Some(0), "the manifest was not saved");
    let printed: Value =
        serde_json::from_str(stdout.lines().last().expect("the receipt is printed"))
            .expect("the receipt is one JSON line");
    assert!(Path::new(printed["location"].as_str().unwrap()).exists());
    assert!(stderr.contains("was written"), "{stderr}");
}

/// §5: a resume that fails after the write still hands back the receipt and the manifest.
#[test]
fn a_failed_resume_prints_the_written_session_and_saves_its_manifest() {
    let (_, doc) = dump(HarnessId::ClaudeCode, "c45db7bc");
    let scratch = support::scratch(HarnessId::Pi, "cli-write-resume-fails");
    let file = scratch.join("document.json");
    std::fs::write(&file, serde_json::to_string(&doc).unwrap()).unwrap();
    let work = scratch.join("work");
    std::fs::create_dir_all(&work).unwrap();
    let pi = support::launcher(&scratch.join("bin"), "pi", "exit /b 3", "exit 3");
    let manifest = scratch.join("manifest.json");
    let (code, stdout, stderr) = cli_code(
        &scratch,
        &[
            "write",
            "pi",
            file.to_str().unwrap(),
            "--dest-version",
            "1.2.0",
            "--cwd",
            work.to_str().unwrap(),
            "--pi-exe",
            pi.to_str().unwrap(),
            "--resume-prompt",
            "hello",
            "--manifest",
            manifest.to_str().unwrap(),
        ],
    );
    assert_eq!(code, Some(5), "{stderr}");
    let printed: Value =
        serde_json::from_str(stdout.lines().last().expect("the receipt is printed"))
            .expect("the receipt is one JSON line");
    assert!(Path::new(printed["location"].as_str().unwrap()).exists());
    let saved: Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest).expect("the manifest is saved"))
            .unwrap();
    assert!(
        saved["losses"]
            .as_array()
            .unwrap()
            .iter()
            .any(|loss| loss.as_str().unwrap().contains("resume prompt")),
        "{saved}"
    );
}

/// A written document's manifest names the harness the document came from, and fingerprints the
/// document itself: it holds no source records.
#[test]
fn a_written_document_names_its_source_and_has_a_fingerprint() {
    let (_, doc) = dump(HarnessId::ClaudeCode, "c45db7bc");
    let (_, printed) = write_document(
        HarnessId::Pi,
        &doc,
        "cli-write-source-facts",
        &["--dest-version", "1.2.0"],
    );
    let source = &printed["manifest"]["source"];
    assert_eq!(source["harness"], "claude_code", "{source}");
    let empty = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    assert_ne!(source["content_sha256"], empty, "{source}");
}

/// Exit codes (technical design 5.5.2): a session that is not there is refused (2); one that is
/// there but cannot be read needs recovery (5). Neither writes anything.
#[test]
fn a_missing_session_is_refused_and_a_corrupt_one_is_unreadable() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "cli-exit-codes");
    let _ = stage_fixture(HarnessId::ClaudeCode, &scratch);
    let dir = scratch.join("home").join("projects").join("claude-project");
    std::fs::write(
        dir.join("deadbeef-0000-4000-8000-000000000000.jsonl"),
        "{not json\n{\"type\":\"user\"}\n",
    )
    .unwrap();
    let hop = |id: &str| {
        cli_code(
            &scratch,
            &["hop", "claude_code", id, "pi", "--dest-version", "1.2.0"],
        )
    };
    let (code, _, stderr) = hop("ffffffff");
    assert_eq!(code, Some(2), "{stderr}");
    let (code, _, stderr) = hop("deadbeef");
    assert_eq!(code, Some(5), "{stderr}");
    assert!(
        !scratch.join("home").join("sessions").exists(),
        "nothing was written"
    );
}

/// A flag without its value does not take the next flag as one, and nothing is left unread.
#[test]
fn a_flag_without_its_value_and_a_stray_operand_are_refused() {
    let scratch = support::scratch(HarnessId::Pi, "cli-parsing");
    let (code, _, stderr) = cli_code(
        &scratch,
        &[
            "write",
            "pi",
            "doc.json",
            "--manifest",
            "--dest-version",
            "1.2.0",
        ],
    );
    assert_ne!(code, Some(0));
    assert!(stderr.contains("needs a value"), "{stderr}");
    let (code, _, stderr) = cli_code(&scratch, &["dump", "pi", "an-id", "another"]);
    assert_ne!(code, Some(0));
    assert!(stderr.contains("unexpected"), "{stderr}");
}

/// `mukade --help` is a request, not an error.
#[test]
fn help_prints_the_usage_and_succeeds() {
    let scratch = support::scratch(HarnessId::Pi, "cli-help");
    let (code, stdout, _) = cli_code(&scratch, &["--help"]);
    assert_eq!(code, Some(0));
    assert!(stdout.contains("mukade hop"), "{stdout}");
}

/// An empty store variable is not a store: it would put the session beside the current directory.
#[test]
fn an_empty_store_variable_is_not_a_store() {
    let scratch = support::scratch(HarnessId::Pi, "cli-empty-root");
    let (_, doc) = dump(HarnessId::ClaudeCode, "c45db7bc");
    let file = scratch.join("document.json");
    std::fs::write(&file, serde_json::to_string(&doc).unwrap()).unwrap();
    let output = Command::new(BIN)
        .args([
            "write",
            "pi",
            file.to_str().unwrap(),
            "--dest-version",
            "1.2.0",
        ])
        .current_dir(&scratch)
        .env("PI_CODING_AGENT_DIR", "")
        .env_remove("MUKADE_PI_EXE")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!scratch.join("sessions").exists(), "nothing beside the cwd");
}

/// `dump --output` writes the document as UTF-8 itself: a shell pipe would re-encode it with the
/// console's code page (PowerShell's `Set-Content` turns an em dash into mojibake).
#[test]
fn dump_writes_utf8_to_a_file() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "cli-dump-output");
    let _ = stage_fixture(HarnessId::ClaudeCode, &scratch);
    let file = scratch.join("document.json");
    let (code, stdout, stderr) = cli_code(
        &scratch,
        &[
            "dump",
            "claude_code",
            "c45db7bc",
            "--output",
            file.to_str().unwrap(),
        ],
    );
    assert_eq!(code, Some(0), "{stderr}");
    assert!(
        stdout.trim().is_empty(),
        "the document goes to the file: {stdout}"
    );
    let written: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    let (_, printed) = dump(HarnessId::ClaudeCode, "c45db7bc");
    assert_eq!(written["messages"], printed["messages"]);
    assert!(
        written.to_string().contains("—"),
        "the em dash of the fixture survives"
    );
}

/// A source can be read from a file alone: no store is staged, and the document still names the
/// session it came from.
#[test]
fn dump_reads_a_source_from_a_file_without_a_store() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "cli-dump-from-file");
    let fixture = support::source_path(HarnessId::ClaudeCode);
    let (code, stdout, stderr) = cli_code(
        &scratch,
        &[
            "dump",
            "claude_code",
            "--from-file",
            fixture.to_str().unwrap(),
        ],
    );
    assert_eq!(code, Some(0), "{stderr}");
    let doc: Value = serde_json::from_str(&stdout).expect("one document");
    assert_eq!(doc["meta"]["id"], "c45db7bc-542c-4201-89de-b590c6fdb797");
    assert!(!scratch.join("home").join("projects").exists(), "no store was needed");
}

/// `--timing` states where the time went, phase by phase, and leaves stdout for the receipt.
#[test]
fn timing_reports_each_phase_on_stderr() {
    let scratch = support::scratch(HarnessId::Pi, "cli-timing");
    let fixture = support::source_path(HarnessId::ClaudeCode);
    let (code, stdout, stderr) = cli_code(
        &scratch,
        &[
            "hop",
            "claude_code",
            "fixture",
            "pi",
            "--from-file",
            fixture.to_str().unwrap(),
            "--cwd",
            scratch.to_str().unwrap(),
            "--dest-version",
            "1.4.0",
            "--timing",
        ],
    );
    assert_eq!(code, Some(0), "{stderr}");
    for phase in ["read file", "plan", "strict check", "write files", "total"] {
        assert!(
            stderr.lines().any(|line| line.contains(phase)),
            "no {phase:?} phase in:\n{stderr}"
        );
    }
    let receipt: Value = serde_json::from_str(stdout.lines().last().unwrap()).expect("a receipt");
    assert!(receipt["location"].is_string(), "{receipt}");
}

/// A failed external import may have written before failing, so its outcome needs reconciliation.
#[test]
fn a_failed_external_import_reports_an_unknown_outcome() {
    let (_, doc) = dump(HarnessId::ClaudeCode, "c45db7bc");
    let scratch = support::scratch(HarnessId::OpenCode, "cli-exit-4");
    let file = scratch.join("document.json");
    std::fs::write(&file, serde_json::to_string(&doc).unwrap()).unwrap();
    let opencode = support::launcher(&scratch.join("bin"), "opencode", "exit /b 1", "exit 1");
    let (code, stdout, stderr) = cli_code(
        &scratch,
        &[
            "write",
            "opencode",
            file.to_str().unwrap(),
            "--opencode-exe",
            opencode.to_str().unwrap(),
            "--dest-version",
            "2.1.4",
            "--project-id",
            "global",
            "--model-provider",
            "h27",
            "--model",
            "m1",
        ],
    );
    assert_eq!(code, Some(5), "{stderr}");
    assert!(stderr.contains("outcome unknown"), "{stderr}");
    assert!(
        stdout.trim().is_empty(),
        "no receipt for a write that did not happen"
    );
}

/// `--all-branches` carries a Codex paginated thread whole; without it only the active part
/// travels. Another harness refuses the flag before anything is read or written.
#[test]
fn all_branches_carries_a_paginated_codex_thread_whole() {
    let scratch = support::scratch(HarnessId::Codex, "cli-all-branches");
    let thread = "01a181df-cfb6-79e2-8977-37b2cfde0b79";
    let day = scratch.join("home/sessions/2026/10/28");
    std::fs::create_dir_all(&day).unwrap();
    let line = |ordinal: u64, kind: &str, payload: Value| {
        serde_json::json!({"timestamp": format!("2026-10-28T10:{ordinal:02}:00.000Z"),
                           "ordinal": ordinal, "type": kind, "payload": payload})
        .to_string()
            + "\n"
    };
    let say = |ordinal: u64, text: &str| {
        line(
            ordinal,
            "response_item",
            serde_json::json!({"type": "message", "role": "user",
                               "content": [{"type": "input_text", "text": text}]}),
        )
    };
    let meta = serde_json::json!({"id": thread, "session_id": thread, "cwd": "D:/w"});
    let kept = line(0, "session_meta", meta.clone()) + &say(1, "KEPT-TURN");
    std::fs::write(
        day.join(format!("rollout-2026-10-28T10-38-11-{thread}.jsonl")),
        kept.clone() + &say(2, "ABANDONED-TURN"),
    )
    .unwrap();
    let mut page_meta = meta;
    page_meta["history_base"] = serde_json::json!({"thread_id": thread,
        "end_ordinal_exclusive": 2, "end_byte_offset": kept.len()});
    std::fs::write(
        day.join(format!(
            "rollout-2026-10-28T13-29-27-{thread}_01a1827c-9cd7-7180-8a25-b3d7e423149a.jsonl"
        )),
        line(2, "session_meta", page_meta) + &say(3, "ACTIVE-TURN"),
    )
    .unwrap();

    let (ok, active, stderr) = cli(&scratch, &["dump", "codex", thread]);
    assert!(ok, "{stderr}");
    assert!(active.contains("KEPT-TURN") && active.contains("ACTIVE-TURN"));
    assert!(!active.contains("ABANDONED-TURN"), "{active}");
    let (ok, all, stderr) = cli(&scratch, &["dump", "codex", thread, "--all-branches"]);
    assert!(ok, "{stderr}");
    for turn in ["KEPT-TURN", "ABANDONED-TURN", "ACTIVE-TURN"] {
        assert!(all.contains(turn), "{turn}: {all}");
    }
    let (code, _, stderr) = cli_code(&scratch, &["dump", "claude_code", "x", "--all-branches"]);
    assert_eq!(code, Some(2), "{stderr}");
    assert!(stderr.contains("--all-branches"), "{stderr}");
}

/// Codex 0.165 keeps its compaction summary encrypted: `message` is empty and the summary is an
/// encrypted `compaction` item. A portable hop has nothing to carry and refuses before writing; a
/// whole-history hop leaves the empty summary out and counts it.
#[test]
fn an_encrypted_codex_summary_is_never_an_empty_message() {
    let scratch = support::scratch(HarnessId::Codex, "cli-encrypted-summary");
    let thread = "01a181e2-4cd9-7c63-82b8-f75e3d6bb172";
    let day = scratch.join("home/sessions/2026/10/28");
    std::fs::create_dir_all(&day).unwrap();
    let line = |ordinal: u64, kind: &str, payload: Value| {
        serde_json::json!({"timestamp": format!("2026-10-28T10:{ordinal:02}:00.000Z"),
                           "ordinal": ordinal, "type": kind, "payload": payload})
        .to_string()
            + "\n"
    };
    let say = |ordinal: u64, role: &str, text: &str| {
        let block = if role == "user" {
            "input_text"
        } else {
            "output_text"
        };
        line(
            ordinal,
            "response_item",
            serde_json::json!({"type": "message", "role": role,
                               "content": [{"type": block, "text": text}]}),
        )
    };
    let rollout = line(
        0,
        "session_meta",
        serde_json::json!({"id": thread, "session_id": thread, "cwd": "D:/w"}),
    ) + &say(1, "user", "BEFORE")
        + &say(2, "assistant", "REPLY")
        + &line(
            3,
            "compacted",
            serde_json::json!({"message": "", "replacement_history": [
                {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "BEFORE"}]},
                {"type": "compaction", "id": "cmp_1", "encrypted_content": "gAAAAB-ENCRYPTED"}]}),
        )
        + &say(4, "user", "AFTER");
    std::fs::write(
        day.join(format!("rollout-2026-10-28T10-40-54-{thread}.jsonl")),
        rollout,
    )
    .unwrap();
    let work = scratch.join("work");
    std::fs::create_dir_all(&work).unwrap();
    let hop = |extra: &[&str]| {
        let mut args = vec![
            "hop",
            "codex",
            thread,
            "pi",
            "--cwd",
            work.to_str().unwrap(),
            "--dest-version",
            "1.4.0",
        ];
        args.extend_from_slice(extra);
        cli_code(&scratch, &args)
    };

    let (code, stdout, stderr) = hop(&["--compaction", "portable"]);
    assert_eq!(code, Some(2), "{stdout}{stderr}");
    assert!(stderr.contains("no readable text"), "{stderr}");

    let (code, stdout, stderr) = hop(&[]);
    assert_eq!(code, Some(0), "{stderr}");
    let receipt: Value = serde_json::from_str(&stdout).unwrap();
    let losses = receipt["manifest"]["losses"].to_string();
    assert!(losses.contains("no readable text"), "{losses}");
    let written = std::fs::read_to_string(receipt["location"].as_str().unwrap()).unwrap();
    assert!(written.contains("BEFORE") && written.contains("AFTER"));
    assert!(
        !written.contains(r#""text":"""#),
        "no empty message: {written}"
    );
}

/// A fake Pi that answers the summary prompt with a fixed text, in the scratch's own `bin`.
fn summarizing_pi(scratch: &Path, answer: &str) -> String {
    let windows = format!("echo {answer}");
    let unix = format!("echo '{answer}'");
    support::launcher(&scratch.join("bin"), "pi", &windows, &unix)
        .to_string_lossy()
        .into_owned()
}

/// Export the summary to a file with `dump`, import it later with `write`.
#[test]
fn dump_exports_a_model_summary_and_write_imports_it() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "cli-dump-summary");
    let id = stage_fixture(HarnessId::Pi, &scratch);
    let pi = summarizing_pi(&scratch, "The summary.");
    let doc_file = scratch.join("summary.json");
    let (code, _, stderr) = cli_code(
        &scratch,
        &[
            "dump",
            "pi",
            &id,
            "--compaction",
            "summary",
            "--pi-exe",
            &pi,
            "--output",
            doc_file.to_str().unwrap(),
        ],
    );
    assert_eq!(code, Some(0), "{stderr}");
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(&doc_file).unwrap()).unwrap();
    let messages = doc["messages"].as_array().expect("messages");
    assert_eq!(messages.len(), 1, "{messages:?}");
    let text = messages[0]["content"][0]["text"].as_str().unwrap();
    assert!(
        text.starts_with(mukade::summary::MODEL_SUMMARY_HANDOFF),
        "{text}"
    );
    assert!(text.contains("The summary."), "{text}");

    let work = scratch.join("work");
    std::fs::create_dir_all(&work).unwrap();
    let (_, printed) = write_document(
        HarnessId::ClaudeCode,
        &doc,
        "cli-write-summary",
        &["--dest-version", "2.1.312", "--cwd", work.to_str().unwrap()],
    );
    assert!(Path::new(printed["location"].as_str().unwrap()).is_file());
    // The imported document still says who wrote its summary.
    let manifest = &printed["manifest"];
    assert_eq!(manifest["options"]["compaction"], "summary");
    assert_eq!(manifest["options"]["summary"]["harness"], "pi");
    let losses = manifest["losses"].to_string();
    assert!(
        losses.contains("travelled as a summary written by pi's model"),
        "{losses}"
    );
    assert!(
        !losses.contains("the source's compaction summary travels"),
        "{losses}"
    );
    assert!(
        !losses.contains("the source's own compaction is in the session"),
        "{losses}"
    );

    // A fork the summarizing run could not delete is still named when the document is imported.
    let mut with_leftover = doc.clone();
    with_leftover["summary"]["summary"]["leftover"] =
        Value::from("opencode session delete ses_fork1 --standalone");
    let (_, printed) = write_document(
        HarnessId::ClaudeCode,
        &with_leftover,
        "cli-write-summary-leftover",
        &["--dest-version", "2.1.312", "--cwd", work.to_str().unwrap()],
    );
    let losses = printed["manifest"]["losses"].to_string();
    assert!(losses.contains("session delete ses_fork1"), "{losses}");
}

#[test]
fn hop_with_a_model_summary_keeps_the_turns_asked_for() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "cli-hop-summary");
    let id = stage_fixture(HarnessId::Pi, &scratch);
    let pi = summarizing_pi(&scratch, "The summary.");
    let work = scratch.join("work");
    std::fs::create_dir_all(&work).unwrap();
    let (code, stdout, stderr) = cli_code(
        &scratch,
        &[
            "hop",
            "pi",
            &id,
            "claude_code",
            "--compaction",
            "summary",
            "--keep-turns",
            "1",
            "--pi-exe",
            &pi,
            "--dest-version",
            "2.1.312",
            "--cwd",
            work.to_str().unwrap(),
        ],
    );
    assert_eq!(code, Some(0), "{stderr}");
    let printed: Value = serde_json::from_str(stdout.lines().last().unwrap()).unwrap();
    assert_eq!(printed["manifest"]["options"]["compaction"], "summary");
    assert_eq!(printed["manifest"]["options"]["summary"]["harness"], "pi");
    let written =
        mukade::reader::claude_code::read_file(Path::new(printed["location"].as_str().unwrap()))
            .unwrap();
    let users: Vec<String> = written
        .conversation()
        .filter_map(|entry| match &entry.kind {
            mukade::model::Kind::User { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(users.len(), 2, "the handoff, then one kept turn: {users:?}");
    assert!(users[0].contains("The summary."), "{users:?}");
}

#[test]
fn intents_lists_the_editable_prompt_folder_and_custom_files() {
    let scratch = support::scratch(HarnessId::Pi, "cli-intents");
    let folder = scratch.join("custom-intents");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("security.md"), "Check every input boundary.").unwrap();
    let (code, stdout, stderr) = support::cli_in(
        &scratch,
        &scratch,
        &["intents"],
        &[("MUKADE_INTENT_DIR", folder.to_str().unwrap())],
    );
    assert_eq!(code, Some(0), "{stderr}");
    let printed: Value = serde_json::from_str(&stdout).expect("one JSON document");
    assert_eq!(printed["default"], "generic");
    assert_eq!(printed["directory"], folder.display().to_string());
    let names: Vec<&str> = printed["prompts"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|entry| entry["name"].as_str())
        .collect();
    for expected in [
        "generic",
        "continue",
        "execute",
        "review",
        "analyze",
        "brainstorm",
        "plan",
        "security",
    ] {
        assert!(names.contains(&expected), "{names:?}");
    }
    let security = printed["prompts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["name"] == "security")
        .unwrap();
    assert_eq!(security["builtin"], false);
    assert_eq!(security["custom"], true);

    let (code, _, stderr) = support::cli_in(
        &scratch,
        &scratch,
        &["intents"],
        &[("MUKADE_INTENT_DIR", "relative-intents")],
    );
    assert_eq!(code, Some(1), "{stderr}");
    assert!(
        stderr.contains("MUKADE_INTENT_DIR must be an absolute folder"),
        "{stderr}"
    );
}

#[test]
fn a_named_intent_prompt_reaches_the_source_model() {
    let scratch = support::scratch(HarnessId::Pi, "cli-intent-prompt");
    let id = stage_fixture(HarnessId::Pi, &scratch);
    let intents = scratch.join("intent-prompts");
    std::fs::create_dir_all(&intents).unwrap();
    std::fs::write(intents.join("custom.md"), "CUSTOMINTENTMARKER").unwrap();
    let observed = scratch.join("observed-args.txt");
    let pi = support::launcher(
        &scratch.join("bin"),
        "pi",
        &format!("echo %*>>\"{}\"\r\necho The summary.", observed.display()),
        &format!(
            "printf '%s\\n' \"$*\" >> '{}'\necho The summary.",
            observed.display()
        ),
    );
    let output = scratch.join("summary.json");
    let (code, _, stderr) = support::cli_in(
        &scratch,
        &scratch,
        &[
            "dump",
            "pi",
            &id,
            "--intent",
            "custom",
            "--pi-exe",
            pi.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ],
        &[("MUKADE_INTENT_DIR", intents.to_str().unwrap())],
    );
    assert_eq!(code, Some(0), "{stderr}");
    let args = std::fs::read_to_string(&observed).expect("the fake pi records its arguments");
    assert!(args.contains("CUSTOMINTENTMARKER"), "{args}");
    let doc: Value =
        serde_json::from_str(&std::fs::read_to_string(output).unwrap()).expect("a document");
    let text = doc["messages"][0]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("The summary."), "{text}");
    assert_eq!(doc["summary"]["summary"]["prompt"], "custom", "{doc}");
}

#[test]
fn summary_flags_and_combinations_that_cannot_work_are_refused() {
    let scratch = support::scratch(HarnessId::Pi, "cli-summary-refused");
    let id = stage_fixture(HarnessId::Pi, &scratch);
    for (args, expected) in [
        (
            vec!["hop", "pi", id.as_str(), "claude_code", "--keep-turns", "1"],
            1,
        ),
        (
            vec![
                "hop",
                "pi",
                id.as_str(),
                "claude_code",
                "--summary-model",
                "x",
            ],
            1,
        ),
        (vec!["dump", "pi", id.as_str(), "--summary-timeout", "5"], 1),
        (
            vec![
                "hop",
                "pi",
                id.as_str(),
                "claude_code",
                "--compaction",
                "summary",
                "--keep-turns",
                "x",
            ],
            1,
        ),
    ] {
        let (code, _, stderr) = cli_code(&scratch, &args);
        assert_eq!(code, Some(expected), "{args:?}: {stderr}");
    }
    let (code, _, stderr) = cli_code(
        &scratch,
        &["hop", "pi", &id, "claude_code", "--compaction", "bogus"],
    );
    assert_eq!(code, Some(1), "{stderr}");
    assert!(stderr.contains("none, portable, summary"), "{stderr}");

    // An intent is a summary prompt: it implies the summary transport, and cannot be combined
    // with another one. An unknown intent is refused before the source is read.
    for (extra, message) in [
        (
            vec!["--intent", "review", "--compaction", "none"],
            "goes with --compaction summary",
        ),
        (
            vec!["--intent", "no-such-intent"],
            "unknown intent no-such-intent",
        ),
    ] {
        let mut args = vec!["hop", "pi", "not-there", "claude_code"];
        args.extend(extra);
        let (code, _, stderr) = cli_code(&scratch, &args);
        assert_eq!(code, Some(1), "{args:?}: {stderr}");
        assert!(stderr.contains(message), "{args:?}: {stderr}");
    }
    let (code, _, stderr) = cli_code(&scratch, &["list", "pi", "--intent", "review"]);
    assert_eq!(code, Some(1), "{stderr}");
    assert!(
        stderr.contains("--intent is supported only by dump and hop"),
        "{stderr}"
    );

    let codex_id = stage_fixture(HarnessId::Codex, &scratch);
    let (code, _, stderr) = cli_code(
        &scratch,
        &[
            "hop",
            "codex",
            &codex_id,
            "pi",
            "--compaction",
            "summary",
            "--all-branches",
        ],
    );
    assert_eq!(code, Some(2), "{stderr}");
    let claude_file = support::source_path(HarnessId::ClaudeCode);
    let (code, _, stderr) = cli_code(
        &scratch,
        &[
            "hop",
            "claude_code",
            "x",
            "pi",
            "--compaction",
            "summary",
            "--from-file",
            claude_file.to_str().unwrap(),
        ],
    );
    assert_eq!(code, Some(2), "{stderr}");
}

#[test]
fn a_summary_that_fails_writes_nothing() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "cli-summary-fails");
    let id = stage_fixture(HarnessId::Pi, &scratch);
    let pi = support::launcher(&scratch.join("bin"), "pi", "exit /b 3", "exit 3");
    let (code, stdout, stderr) = cli_code(
        &scratch,
        &[
            "hop",
            "pi",
            &id,
            "claude_code",
            "--compaction",
            "summary",
            "--pi-exe",
            pi.to_str().unwrap(),
            "--dest-version",
            "2.1.312",
        ],
    );
    assert_eq!(code, Some(4), "{stdout}{stderr}");
    assert!(stderr.contains("summary failed"), "{stderr}");
    assert!(
        !scratch.join("home").join("projects").exists(),
        "nothing was written to the destination"
    );
}

#[test]
fn harnesses_lists_supported_binaries_stores_and_native_lineage_info() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "cli-harnesses");
    stage_fixture(HarnessId::ClaudeCode, &scratch);
    stage_fixture(HarnessId::Pi, &scratch);
    let pi_fork = scratch.join("home/sessions/--pi-project--/2026-10-30T14-00-00-000Z_fork.jsonl");
    std::fs::write(
        pi_fork,
        "{\"type\":\"session\",\"id\":\"fork\",\"parentSession\":\"parent\"}\n",
    )
    .unwrap();
    let (ok, stdout, stderr) = cli(&scratch, &["harnesses"]);
    assert!(ok, "{stderr}");
    for harness in ["claude_code", "codex", "opencode", "pi"] {
        assert!(stdout.contains(harness), "missing {harness}: {stdout}");
    }
    assert!(
        stdout.contains("not recorded (claude --fork-session writes no lineage field"),
        "{stdout}"
    );
    assert!(
        stdout.contains("recorded: parent_id, fork_session_id, fork_boundary"),
        "{stdout}"
    );
    assert!(stdout.contains("recorded: parentSession"), "{stdout}");
    assert!(stdout.contains("status:   recognized"), "{stdout}");
    assert!(stdout.contains("sessions: 1"), "{stdout}");
    assert!(
        stdout.contains("forks:    unknown (not recorded)"),
        "{stdout}"
    );
    assert!(
        stdout.contains("layout:   projects/<project>/<session-id>.jsonl"),
        "{stdout}"
    );
    assert!(
        stdout.contains("catalog mukade.harness-catalog/1"),
        "{stdout}"
    );
    let pi_section = stdout.split("\npi (Pi) - ").nth(1).expect("Pi section");
    assert!(pi_section.contains("sessions: 2"), "{pi_section}");
    assert!(pi_section.contains("forks:    1"), "{pi_section}");
}

#[test]
fn harness_inventory_uses_standard_store_roots_when_the_variables_are_unset() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "cli-harnesses-default-roots");
    let home = scratch.join("home");
    let isolated = scratch.join("isolated");
    let bin = isolated.join("bin");
    std::fs::create_dir_all(&bin).unwrap();

    let claude_file = home.join(".claude/projects/project/claude.jsonl");
    std::fs::create_dir_all(claude_file.parent().unwrap()).unwrap();
    std::fs::copy(support::source_path(HarnessId::ClaudeCode), &claude_file).unwrap();

    let codex_file =
        home.join(".codex/sessions/2026/10/30/rollout-2026-10-30T14-00-00-codex.jsonl");
    std::fs::create_dir_all(codex_file.parent().unwrap()).unwrap();
    std::fs::copy(support::source_path(HarnessId::Codex), &codex_file).unwrap();

    let pi_file = home.join(".pi/agent/sessions/--project--/2026-10-30T14-00-00-000Z_pi.jsonl");
    std::fs::create_dir_all(pi_file.parent().unwrap()).unwrap();
    std::fs::copy(support::source_path(HarnessId::Pi), &pi_file).unwrap();

    let xdg_data = home.join("xdg-data");
    let db_path = xdg_data.join("opencode").join("opencode.db");
    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    let db = rusqlite::Connection::open(&db_path).unwrap();
    db.execute_batch(
        "CREATE TABLE session_v2 (
             id TEXT PRIMARY KEY, title TEXT, directory TEXT, time_updated INTEGER,
             time_archived INTEGER, parent_id TEXT, fork_session_id TEXT
         );
         INSERT INTO session_v2 VALUES ('root', 'Root', 'D:/work', 1, NULL, NULL, NULL);",
    )
    .unwrap();
    drop(db);

    let output = Command::new(BIN)
        .arg("harnesses")
        .env("PATH", &bin)
        .env("USERPROFILE", &home)
        .env("HOME", &home)
        .env("APPDATA", &isolated)
        .env("LOCALAPPDATA", &isolated)
        .env("XDG_DATA_HOME", &xdg_data)
        .env_remove("CLAUDE_CONFIG_DIR")
        .env_remove("CODEX_HOME")
        .env_remove("OPENCODE_DB")
        .env_remove("PI_CODING_AGENT_DIR")
        .env_remove("MUKADE_CLAUDE_EXE")
        .env_remove("MUKADE_CODEX_EXE")
        .env_remove("MUKADE_OPENCODE_EXE")
        .env_remove("MUKADE_PI_EXE")
        .env_remove("MUKADE_DATA_DIR")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for (harness, expected_root) in [
        ("claude_code", home.join(".claude")),
        ("codex", home.join(".codex")),
        ("opencode", db_path),
        ("pi", home.join(".pi").join("agent")),
    ] {
        let section = stdout
            .split(&format!("\n{harness} ("))
            .nth(1)
            .expect("harness section");
        let section = section.split("\n\n").next().unwrap_or(section);
        assert!(section.contains("status:   recognized"), "{section}");
        assert!(section.contains("sessions: 1"), "{section}");
        assert!(
            section.contains(&expected_root.display().to_string()),
            "expected {} in {section}",
            expected_root.display()
        );
    }
}

#[test]
fn harness_inventory_counts_all_opencode_sessions_and_linked_forks() {
    let scratch = support::scratch(HarnessId::OpenCode, "cli-harnesses-opencode");
    let db_path = scratch.join("home/opencode.db");
    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    let db = rusqlite::Connection::open(&db_path).unwrap();
    db.execute_batch(
        "CREATE TABLE session_v2 (
             id TEXT PRIMARY KEY, title TEXT, directory TEXT, time_updated INTEGER,
             time_archived INTEGER, parent_id TEXT, fork_session_id TEXT
         );
         INSERT INTO session_v2 VALUES ('root', 'Root', 'D:/work', 1, NULL, NULL, NULL);
         INSERT INTO session_v2 VALUES ('child-a', 'Fork A', 'D:/work', 2, NULL, 'root', NULL);
         INSERT INTO session_v2 VALUES ('child-b', 'Fork B', 'D:/work', 3, NULL, NULL, 'root');",
    )
    .unwrap();
    drop(db);

    let (ok, stdout, stderr) = cli(&scratch, &["harnesses"]);
    assert!(ok, "{stderr}");
    let section = stdout
        .split("\nopencode (")
        .nth(1)
        .expect("OpenCode section");
    assert!(section.contains("sessions: 3"), "{section}");
    assert!(section.contains("forks:    2"), "{section}");
    assert!(
        section.contains("layout:   opencode.db (SQLite; session_v2, session_message)"),
        "{section}"
    );
}

#[test]
fn reader_list_all_includes_opencode_forks_while_list_keeps_roots() {
    let scratch = support::scratch(HarnessId::OpenCode, "reader-list-all");
    let db_path = scratch.join("home/opencode.db");
    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    let db = rusqlite::Connection::open(&db_path).unwrap();
    db.execute_batch(
        "CREATE TABLE session_v2 (
             id TEXT PRIMARY KEY, title TEXT, directory TEXT, time_updated INTEGER,
             time_archived INTEGER, parent_id TEXT, fork_session_id TEXT
         );
         INSERT INTO session_v2 VALUES ('root', 'Root', 'D:/work', 1, NULL, NULL, NULL);
         INSERT INTO session_v2 VALUES ('child-a', 'Fork A', 'D:/work', 2, NULL, 'root', NULL);
         INSERT INTO session_v2 VALUES ('child-b', 'Fork B', 'D:/work', 3, NULL, NULL, 'root');",
    )
    .unwrap();
    drop(db);

    let roots = StoreRoots {
        opencode_db: Some(db_path),
        ..Default::default()
    };
    let reader = for_harness(HarnessId::OpenCode).unwrap();
    let listed = reader.list(&roots).unwrap();
    let all = reader.list_all(&roots).unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(all.len(), 3);
    assert!(listed.iter().all(|info| info.parent.is_none()));
    assert!(
        all.iter()
            .any(|info| info.parent.as_deref() == Some("root"))
    );
}

/// Run the inventory with isolated homes and binaries, then apply only this test's environment.
fn harnesses_with(scratch: &Path, overrides: &[(&str, &str)]) -> (bool, String, String) {
    let home = scratch.join("home");
    let isolated = scratch.join("isolated");
    std::fs::create_dir_all(isolated.join("bin")).unwrap();
    let mut command = Command::new(BIN);
    command
        .arg("harnesses")
        .env("PATH", isolated.join("bin"))
        .env("USERPROFILE", &home)
        .env("HOME", &home)
        .env("APPDATA", &isolated)
        .env("LOCALAPPDATA", &isolated)
        .env("XDG_DATA_HOME", home.join("xdg-data"))
        .env("CLAUDE_CONFIG_DIR", &home)
        .env("CODEX_HOME", &home)
        .env("OPENCODE_DB", home.join("opencode.db"))
        .env("PI_CODING_AGENT_DIR", &home)
        .env_remove("MUKADE_CLAUDE_EXE")
        .env_remove("MUKADE_CODEX_EXE")
        .env_remove("MUKADE_OPENCODE_EXE")
        .env_remove("MUKADE_PI_EXE")
        .env_remove("MUKADE_DATA_DIR");
    for (name, value) in overrides {
        command.env(name, value);
    }
    let output = command.output().expect("the inventory must run");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn create_opencode_inventory_db(path: &Path, forked: bool) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let db = rusqlite::Connection::open(path).unwrap();
    db.execute_batch(
        "CREATE TABLE session_v2 (
             id TEXT PRIMARY KEY, title TEXT, directory TEXT, time_updated INTEGER,
             time_archived INTEGER, parent_id TEXT, fork_session_id TEXT
         );
         INSERT INTO session_v2 VALUES ('root', 'Root', 'D:/work', 1, NULL, NULL, NULL);",
    )
    .unwrap();
    if forked {
        db.execute(
            "INSERT INTO session_v2 VALUES ('fork', 'Fork', 'D:/work', 2, NULL, 'root', NULL)",
            [],
        )
        .unwrap();
    }
}

#[test]
fn harness_inventory_reads_an_explicit_opencode_database_with_a_custom_name() {
    let scratch = support::scratch(HarnessId::OpenCode, "cli-harnesses-custom-db");
    let db_path = scratch.join("home/custom-name.db");
    create_opencode_inventory_db(&db_path, true);
    let (ok, stdout, stderr) =
        harnesses_with(&scratch, &[("OPENCODE_DB", db_path.to_str().unwrap())]);
    assert!(ok, "{stderr}");
    let section = stdout.split("\nopencode (").nth(1).unwrap();
    assert!(section.contains("status:   recognized"), "{section}");
    assert!(section.contains("sessions: 2"), "{section}");
    assert!(section.contains("forks:    1"), "{section}");
}

#[test]
fn harness_inventory_resolves_a_relative_opencode_database_under_its_data_directory() {
    let scratch = support::scratch(HarnessId::OpenCode, "cli-harnesses-relative-db");
    let db_path = scratch
        .join("home")
        .join("xdg-data")
        .join("opencode")
        .join("custom.db");
    create_opencode_inventory_db(&db_path, false);
    let (ok, stdout, stderr) = harnesses_with(&scratch, &[("OPENCODE_DB", "custom.db")]);
    assert!(ok, "{stderr}");
    let section = stdout.split("\nopencode (").nth(1).unwrap();
    assert!(
        section.contains(&db_path.display().to_string()),
        "{section}"
    );
    assert!(section.contains("status:   recognized"), "{section}");
    assert!(section.contains("sessions: 1"), "{section}");
}

#[test]
fn harness_inventory_does_not_treat_opencode_memory_as_a_file() {
    let scratch = support::scratch(HarnessId::OpenCode, "cli-harnesses-memory-db");
    let (ok, stdout, stderr) = harnesses_with(&scratch, &[("OPENCODE_DB", ":memory:")]);
    assert!(ok, "{stderr}");
    let section = stdout.split("\nopencode (").nth(1).unwrap();
    assert!(
        section.contains("status:   non-persistent (:memory:)"),
        "{section}"
    );
    assert!(section.contains("sessions: unknown"), "{section}");
}

#[test]
fn harness_inventory_counts_an_explicit_empty_store_as_zero() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "cli-harnesses-empty-store");
    let (ok, stdout, stderr) = harnesses_with(&scratch, &[]);
    assert!(ok, "{stderr}");
    let section = stdout.split("\nclaude_code (").nth(1).unwrap();
    assert!(section.contains("status:   recognized"), "{section}");
    assert!(section.contains("sessions: 0"), "{section}");
}

#[test]
fn harness_inventory_probes_a_configured_binary_without_creating_probe_files() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "cli-harnesses-version-probe");
    let executable = support::launcher(
        &scratch.join("bin"),
        "claude",
        "echo claude-code 9.9.9",
        "echo claude-code 9.9.9",
    );
    let data = scratch.join("probe-data");
    let (ok, stdout, stderr) = harnesses_with(
        &scratch,
        &[
            ("MUKADE_CLAUDE_EXE", executable.to_str().unwrap()),
            ("MUKADE_DATA_DIR", data.to_str().unwrap()),
        ],
    );
    assert!(ok, "{stderr}");
    let section = stdout.split("\nclaude_code (").nth(1).unwrap();
    assert!(section.contains("version:  9.9.9"), "{section}");
    assert!(section.contains("[MUKADE_CLAUDE_EXE]"), "{section}");
    assert!(
        !data.exists(),
        "the version probe wrote to {}",
        data.display()
    );
}

#[test]
fn harness_inventory_uses_a_managed_pi_launcher_from_the_catalog() {
    let scratch = support::scratch(HarnessId::Pi, "cli-harnesses-managed-pi");
    let executable = support::launcher(
        &scratch.join("home").join(".pi").join("agent").join("bin"),
        "pi",
        "echo pi 9.8.7",
        "echo pi 9.8.7",
    );
    let (ok, stdout, stderr) = harnesses_with(&scratch, &[]);
    assert!(ok, "{stderr}");
    let section = stdout.split("\npi (").nth(1).unwrap();
    assert!(
        section.contains(&executable.display().to_string()),
        "{section}"
    );
    assert!(section.contains("[catalog location]"), "{section}");
    assert!(section.contains("version:  9.8.7"), "{section}");
}

#[test]
fn harness_inventory_reports_pi_header_errors_instead_of_zero_forks() {
    let scratch = support::scratch(HarnessId::Pi, "cli-harnesses-malformed-pi");
    let file = scratch.join("home/sessions/--project--/bad.jsonl");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, "not-json\n").unwrap();
    let (ok, stdout, stderr) = harnesses_with(&scratch, &[]);
    assert!(ok, "{stderr}");
    let section = stdout.split("\npi (").nth(1).unwrap();
    assert!(section.contains("sessions: 1"), "{section}");
    assert!(
        section.contains("forks:    unknown (parse Pi session header"),
        "{section}"
    );
}

#[test]
fn harnesses_rejects_options_it_does_not_define() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "cli-harnesses-options");
    let (code, _, stderr) = cli_code(&scratch, &["harnesses", "--root", "elsewhere"]);
    assert_eq!(code, Some(1), "{stderr}");
    assert!(stderr.contains("harnesses takes no options"), "{stderr}");
}

/// `--identity` is machine-readable and takes no arguments.
#[test]
fn identity_prints_the_name_version_and_target() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "cli-identity");
    let (code, stdout, stderr) = cli_code(&scratch, &["--identity"]);
    assert_eq!(code, Some(0), "{stderr}");
    let identity: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(identity["name"], "mukade");
    assert_eq!(identity["version"], env!("CARGO_PKG_VERSION"));
    let expected_target = if cfg!(all(
        windows,
        target_arch = "x86_64",
        target_env = "msvc"
    )) {
        "x86_64-pc-windows-msvc"
    } else {
        "other"
    };
    assert_eq!(identity["target"], expected_target);
    let (code, _, stderr) = cli_code(&scratch, &["--identity", "extra"]);
    assert_ne!(code, Some(0));
    assert!(stderr.contains("takes no arguments"), "{stderr}");
}

/// Every option states which commands it belongs to, and every value is checked before a store is
/// read: a caller cannot believe a typo took effect.
#[test]
fn options_are_refused_where_they_cannot_apply() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "cli-option-refusals");
    let (_, doc) = dump(HarnessId::ClaudeCode, "c45db7bc");
    let document = scratch.join("document.json");
    std::fs::write(&document, serde_json::to_string(&doc).unwrap()).unwrap();
    let document = document.to_str().unwrap();

    let hash = "0".repeat(64);
    let cases: Vec<(Vec<&str>, &str)> = vec![
        (
            vec!["list", "claude_code", "--without-tools"],
            "--without-tools is supported only by hop, write and document",
        ),
        (
            vec!["dump", "claude_code", "c45db7bc", "--expect-sha256", "abc"],
            "--expect-sha256 is supported only by hop",
        ),
        (
            vec!["hop", "claude_code", "x", "pi", "--expect-sha256", "zz"],
            "--expect-sha256 must be a 64-digit SHA-256",
        ),
        (
            vec!["list", "claude_code", "--tool-map", "tools.json"],
            "--tool-map is supported only by hop, write and document",
        ),
        (
            vec!["list", "claude_code", "--dest-id", "x"],
            "--dest-id, --dest-path, --dest-agent and --metadata-file are supported only",
        ),
        (
            vec!["write", "pi", document, "--dest-agent", "analyst"],
            "--dest-agent and --metadata-file are supported only for an OpenCode destination",
        ),
        (
            vec!["write", "pi", document, "--reasoning", "nope"],
            "unknown --reasoning nope",
        ),
        (
            vec!["write", "pi", document, "--dest-shell", "nope"],
            "unknown --dest-shell nope",
        ),
        (
            vec!["write", "pi", document, "--compaction", "bogus"],
            "--compaction, --keep-turns, --summary-model and --summary-timeout are supported only by dump and hop",
        ),
        (
            vec!["write", "pi", document, "--summary-timeout", "5"],
            "--compaction, --keep-turns, --summary-model and --summary-timeout are supported only by dump and hop",
        ),
        (
            vec!["write", "pi", document, "--dest-id", "not-a-uuid"],
            "invalid destination session ID",
        ),
        (
            vec![
                "write",
                "opencode",
                document,
                "--dest-id",
                "../invalid",
            ],
            "invalid destination session ID",
        ),
        (
            vec![
                "hop",
                "claude_code",
                "x",
                "pi",
                "--compaction",
                "summary",
                "--summary-timeout",
                "0",
            ],
            "not a number of seconds",
        ),
        (
            vec![
                "hop",
                "claude_code",
                "x",
                "pi",
                "--compaction",
                "summary",
                "--keep-turns",
                "-1",
            ],
            "not a number of turns",
        ),
    ];
    for (args, expected) in cases {
        // A hash of zeros passes the syntax check, so the hop fails on the source, not the flag.
        if args.contains(&hash.as_str()) {
            continue;
        }
        let (code, _, stderr) = cli_code(&scratch, &args);
        assert_eq!(code, Some(1), "{args:?} was not a usage error: {stderr}");
        assert!(stderr.contains(expected), "{args:?}: {stderr}");
    }
}

/// A `dump` that cannot write its output file says why and prints no document to stdout.
#[test]
fn dump_output_to_an_unwritable_path_fails_with_a_reason() {
    let scratch = support::scratch(HarnessId::ClaudeCode, "cli-dump-unwritable");
    stage_fixture(HarnessId::ClaudeCode, &scratch);
    let output = scratch.join("no-such-dir").join("document.json");
    let (code, stdout, stderr) = cli_code(
        &scratch,
        &[
            "dump",
            "claude_code",
            "c45db7bc",
            "--output",
            output.to_str().unwrap(),
        ],
    );
    assert_ne!(code, Some(0));
    assert!(stdout.trim().is_empty(), "{stdout}");
    assert!(stderr.contains("no-such-dir"), "{stderr}");
    assert!(!output.exists());
}

/// A document whose content the interchange model does not know is refused before any write.
#[test]
fn write_refuses_a_document_with_an_unknown_block() {
    let scratch = support::scratch(HarnessId::Pi, "cli-write-unknown-block");
    let (_, mut doc) = dump(HarnessId::ClaudeCode, "c45db7bc");
    doc["messages"][0]["content"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"type": "hologram", "data": 1}));
    let document = scratch.join("document.json");
    std::fs::write(&document, serde_json::to_string(&doc).unwrap()).unwrap();
    let (code, _, stderr) = cli_code(
        &scratch,
        &[
            "write",
            "pi",
            document.to_str().unwrap(),
            "--dest-version",
            "1.2.0",
        ],
    );
    assert_ne!(code, Some(0));
    assert!(stderr.contains("hologram"), "{stderr}");
    assert!(
        !scratch.join("home").join("sessions").exists(),
        "nothing was written"
    );
}
#[test]
fn list_opencode_reads_root_sessions_from_the_selected_store_without_changing_it() {
    let scratch = support::scratch(HarnessId::OpenCode, "cli-list-opencode");
    let database = scratch.join("sessions.db");
    {
        let db = rusqlite::Connection::open(&database).unwrap();
        db.execute_batch(
            "CREATE TABLE session_v2 (id TEXT PRIMARY KEY, parent_id TEXT, title TEXT,
                 directory TEXT, time_updated INTEGER, time_archived INTEGER);
             INSERT INTO session_v2 VALUES ('ses_open', NULL, 'Open', 'D:/work', 2000, NULL);
             INSERT INTO session_v2 VALUES ('ses_archived', NULL, 'Archived', 'D:/other', 1000, 1500);
             INSERT INTO session_v2 VALUES ('ses_child', 'ses_open', 'Child', 'D:/work', 3000, NULL);",
        )
        .unwrap();
    }
    let before = std::fs::read(&database).unwrap();
    let (code, stdout, stderr) = support::cli_in(
        &scratch,
        &scratch,
        &["list", "opencode", "--root", database.to_str().unwrap()],
        &[
            (
                "OPENCODE_DB",
                scratch.join("wrong.db").to_str().unwrap(),
            ),
            (
                "MUKADE_OPENCODE_EXE",
                scratch.join("no-engine").to_str().unwrap(),
            ),
        ],
    );
    assert_eq!(code, Some(0), "{stderr}");
    let rows: Vec<Value> = stdout
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), 2, "two root sessions: {rows:?}");
    assert_eq!(rows[0]["id"], "ses_open");
    assert_eq!(rows[0]["title"], "Open");
    assert_eq!(rows[0]["cwd"], "D:/work");
    assert_eq!(rows[0]["archived"], false);
    assert_eq!(rows[1]["id"], "ses_archived");
    assert_eq!(rows[1]["archived"], true);
    assert_eq!(
        std::fs::read(&database).unwrap(),
        before,
        "the source database must stay unchanged"
    );
    assert!(!scratch.join("wrong.db").exists());
}

/// A live store may hold sessions only in its WAL: `list` sees them without checkpointing, and
/// neither file is rewritten.
#[test]
fn list_opencode_sees_uncheckpointed_wal_without_modifying_the_store() {
    let scratch = support::scratch(HarnessId::OpenCode, "cli-list-opencode-wal");
    let database = scratch.join("sessions.db");
    let db = rusqlite::Connection::open(&database).unwrap();
    db.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0;
         CREATE TABLE session_v2 (id TEXT PRIMARY KEY, parent_id TEXT, title TEXT,
             directory TEXT, time_updated INTEGER, time_archived INTEGER);
         INSERT INTO session_v2 VALUES ('ses_live', NULL, 'Live', 'D:/work', 2000, NULL);",
    )
    .unwrap();
    let wal = PathBuf::from(format!("{}-wal", database.display()));
    let before_db = std::fs::read(&database).unwrap();
    let before_wal = std::fs::read(&wal).unwrap();
    let (code, stdout, stderr) = support::cli_in(
        &scratch,
        &scratch,
        &["list", "opencode", "--root", database.to_str().unwrap()],
        &[(
            "MUKADE_OPENCODE_EXE",
            scratch.join("no-engine").to_str().unwrap(),
        )],
    );
    assert_eq!(code, Some(0), "{stderr}");
    let rows: Vec<Value> = stdout
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], "ses_live");
    assert_eq!(std::fs::read(&database).unwrap(), before_db);
    assert_eq!(std::fs::read(&wal).unwrap(), before_wal);
    drop(db);
}
