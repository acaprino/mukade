//! The command line, end to end: the real binary over the fixtures, on a scratch store.
//!
//! Everything runs with the four store variables pointing inside
//! scratch directory under the system temporary folder, so no real store is
//! in reach.

mod support;

use serde_json::Value;
use mukade::model::HarnessId;
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
    let output = Command::new(BIN)
        .args(args)
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
            std::fs::write(dir.join("2026-10-27T16-00-39-689Z_01a17e17-9f07-743d-8072-8b3b13d65ab5.jsonl"), file)
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
    let printed: Value = serde_json::from_str(stdout.lines().last().unwrap()).expect("write prints one JSON line");
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
    assert_eq!(kinds, vec!["thinking", "text", "tool_use", "tool_use"], "the two parallel calls stay together");
    assert_eq!(first_assistant["content"][0]["opaque"]["signature"], "h27-mock-signature");

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
    assert!(!summaries.is_empty(), "the source summary is in the document");
    assert!(messages.iter().any(|m| m["compaction"].is_object()), "and it is marked as a compaction");
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
        &["--dest-version", "1.2.0", "--dest-protocol", "anthropic-messages"],
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
    assert!(texts.iter().any(|t| t.starts_with("H27-TOOLS read")), "{texts:?}");
    assert!(texts.iter().any(|t| t.contains("H27-SUMMARY")), "the summary survived the hop");
    assert!(back.interrupted_last_turn(), "the killed last turn is still a user turn with no reply");
}

#[test]
fn write_into_the_same_harness_round_trips_the_conversation() {
    let (_, doc) = dump(HarnessId::Codex, "01a17e07");
    let (_, printed) = write_document(
        HarnessId::Codex,
        &doc,
        "cli-write-codex",
        &["--dest-version", "0.164.0", "--dest-shell", "powershell", "--model-provider", "h27"],
    );
    // The dump and the write agree on the conversation: read the written rollout back.
    let back = mukade::reader::codex::read_file(Path::new(printed["location"].as_str().unwrap()))
        .expect("read back");
    let source = mukade::reader::codex::read_file(&support::source_path(HarnessId::Codex)).unwrap();
    assert_eq!(support::roundtrip_script(&back), support::roundtrip_script(&source));
    assert_eq!(back.meta.cwd, source.meta.cwd, "the project travels with the session");
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
        &["hop", "--root", home.to_str().unwrap(), "c45db7bc", "pi", "--dest-version", "1.2.0"],
    );
    assert!(ok, "{stderr}");
    let printed: Value = serde_json::from_str(stdout.lines().last().unwrap()).unwrap();
    let location = printed["location"].as_str().unwrap();
    assert!(Path::new(location).is_file(), "{location}");
}

#[test]
fn a_custom_pi_binary_is_named_in_the_resume_command() {
    let (_, doc) = dump(HarnessId::ClaudeCode, "c45db7bc");
    let exe = if cfg!(windows) { "D:\\pi-portable\\bin\\pi.cmd" } else { "/opt/pi-portable/bin/pi" };
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
    let printed: Value = serde_json::from_str(stdout.lines().last().expect("the receipt is printed"))
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
    let printed: Value = serde_json::from_str(stdout.lines().last().expect("the receipt is printed"))
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
