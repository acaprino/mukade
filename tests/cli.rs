//! The command line, end to end: the real binary over the fixtures, on a scratch store.
//!
//! Everything runs with the four store variables pointing inside
//! scratch directory under the system temporary folder, so no real store is
//! in reach.

mod support;

use mukade::model::HarnessId;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_mukade");

/// A process with every harness root pointed at a scratch home.
fn cli(scratch: &Path, args: &[&str]) -> (bool, String, String) {
    let (code, stdout, stderr) = cli_code(scratch, args);
    (code == Some(0), stdout, stderr)
}

/// [`cli`], keeping the exit code.
fn cli_code(scratch: &Path, args: &[&str]) -> (Option<i32>, String, String) {
    let home = scratch.join("home");
    // No real harness can be reached: PATH holds an empty directory, and the homes the
    // well-known install locations hang from point inside the scratch.
    let isolated = scratch.join("isolated");
    std::fs::create_dir_all(isolated.join("bin")).unwrap();
    let output = Command::new(BIN)
        .args(args)
        .env("PATH", isolated.join("bin"))
        .env("APPDATA", &isolated)
        .env("LOCALAPPDATA", &isolated)
        .env("USERPROFILE", &isolated)
        .env("HOME", &isolated)
        .env_remove("OPENCODE_CONFIG")
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("XDG_DATA_HOME")
        .env_remove("XDG_STATE_HOME")
        .env_remove("XDG_CACHE_HOME")
        .env("CLAUDE_CONFIG_DIR", &home)
        .env("CODEX_HOME", &home)
        .env("PI_CODING_AGENT_DIR", &home)
        .env("OPENCODE_DB", home.join("opencode.db"))
        // The developer's own overrides must not leak into the assertions.
        .env_remove("MUKADE_CLAUDE_EXE")
        .env_remove("MUKADE_CODEX_EXE")
        .env_remove("MUKADE_OPENCODE_EXE")
        .env_remove("MUKADE_PI_EXE")
        .output()
        .expect("the binary must run");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Put a harness's fixture where that harness's reader looks for its sessions.
fn stage_fixture(harness: HarnessId, scratch: &Path) -> String {
    let home = scratch.join("home");
    match harness {
        HarnessId::ClaudeCode => {
            let file = std::fs::read_to_string(support::source_path(harness)).unwrap();
            let dir = home.join("projects").join("claude-project");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("c45db7bc-542c-4201-89de-b590c6fdb797.jsonl"), file).unwrap();
            "c45db7bc".to_string()
        }
        HarnessId::Pi => {
            let file = std::fs::read_to_string(support::source_path(harness)).unwrap();
            let dir = home.join("sessions").join("--pi-project--");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("2026-10-27T16-00-39-689Z_01a17e17-9f07-743d-8072-8b3b13d65ab5.jsonl"),
                file,
            )
            .unwrap();
            "01a17e17".to_string()
        }
        HarnessId::Codex => {
            let file = std::fs::read_to_string(support::source_path(harness)).unwrap();
            let dir = home.join("sessions").join("2026").join("10").join("27");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("rollout-2026-10-27T16-43-08-01a17e07-93f7-7a73-85c7-9f0205836600.jsonl"),
                file,
            )
            .unwrap();
            "01a17e07".to_string()
        }
        HarnessId::OpenCode => "ses_e82023514ffe02y4T8OoUu99VZ".to_string(),
    }
}

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
    let (ok, _, stderr) = cli(&scratch, &["dump", "nonesuch", "x"]);
    assert!(!ok);
    assert!(stderr.contains("unknown harness"), "{stderr}");
    let (ok, _, stderr) = cli(&scratch, &["fly", "pi"]);
    assert!(!ok);
    assert!(stderr.contains("unknown command"), "{stderr}");
    let (ok, _, stderr) = cli(&scratch, &["dump", "claude_code", "c45db7bc"]);
    assert!(!ok, "nothing is staged in this home");
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

/// Exit code 4: the destination refused the write and nothing changed.
#[test]
fn a_write_the_destination_refuses_exits_4() {
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
    assert_eq!(code, Some(4), "{stderr}");
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
