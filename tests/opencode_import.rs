//! OpenCode's external import, end to end: the destination store, the private import document, a
//! reserved session id, and the distinction between "refused" and "may have written".
//!
//! No real OpenCode is started: every test points `--opencode-exe` at a launcher that behaves like
//! `opencode session import` and records what it was asked.

mod support;

use std::path::{Path, PathBuf};

fn fixture() -> PathBuf {
    support::source_path(mukade::model::HarnessId::OpenCode)
}

/// The arguments shared by every OpenCode import: the fixture as source, a project and model the
/// interchange format requires, and the destination's own version stamp.
fn import_args<'a>(fixture: &'a str, scratch: &'a str, engine: &'a str) -> Vec<&'a str> {
    vec![
        "hop",
        "opencode",
        "fixture",
        "opencode",
        "--from-file",
        fixture,
        "--cwd",
        scratch,
        "--dest-version",
        "2.1.4",
        "--project-id",
        "global",
        "--model-provider",
        "h27",
        "--model",
        "m1",
        "--opencode-exe",
        engine,
    ]
}

/// A launcher for `opencode session import <file>` that records the store and, when asked, the
/// document path (`%3` / `$3`), then answers like a successful import.
fn recording_engine(dir: &Path, capture: &Path) -> PathBuf {
    support::launcher(
        dir,
        "engine",
        &format!(
            "echo %OPENCODE_DB%>\"{0}\"\r\necho %3>\"{0}.file\"\r\necho Imported session: ses_test\r\n",
            capture.display()
        ),
        &format!(
            "printf '%s' \"$OPENCODE_DB\" > '{0}'\necho \"$3\" > '{0}.file'\necho 'Imported session: ses_test'\n",
            capture.display()
        ),
    )
}

#[test]
fn hop_to_the_same_harness_writes_to_the_destination_store_not_the_source() {
    let scratch = support::scratch(mukade::model::HarnessId::OpenCode, "import-same-harness");
    let source = scratch.join("source.db");
    let destination = scratch.join("destination.db");
    let observed = scratch.join("observed.txt");
    let engine = recording_engine(&scratch.join("bin"), &observed);
    let fixture = fixture();
    let mut args = import_args(
        fixture.to_str().unwrap(),
        scratch.to_str().unwrap(),
        engine.to_str().unwrap(),
    );
    args.extend(["--root", source.to_str().unwrap()]);
    let (code, _, stderr) = support::cli_in(
        &scratch,
        &scratch,
        &args,
        &[("OPENCODE_DB", destination.to_str().unwrap())],
    );
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(
        std::fs::read_to_string(&observed).unwrap().trim(),
        destination.to_str().unwrap(),
        "the destination store was used"
    );
    assert!(!source.exists(), "the source store was not created");
}

#[test]
fn an_explicit_data_directory_holds_the_private_import_document() {
    let scratch = support::scratch(mukade::model::HarnessId::OpenCode, "import-data-dir");
    let data = scratch.join("private-data");
    let observed = scratch.join("document-path.txt");
    let engine = recording_engine(&scratch.join("bin"), &observed);
    let fixture = fixture();
    let args = import_args(
        fixture.to_str().unwrap(),
        scratch.to_str().unwrap(),
        engine.to_str().unwrap(),
    );
    let (code, _, stderr) = support::cli_in(
        &scratch,
        &scratch,
        &args,
        &[
            (
                "OPENCODE_DB",
                scratch.join("destination.db").to_str().unwrap(),
            ),
            ("MUKADE_DATA_DIR", data.to_str().unwrap()),
        ],
    );
    assert_eq!(code, Some(0), "{stderr}");
    let path = PathBuf::from(
        std::fs::read_to_string(format!("{}.file", observed.display()))
            .unwrap()
            .trim(),
    );
    assert!(
        path.starts_with(&data),
        "{} is not inside {}",
        path.display(),
        data.display()
    );
    assert!(!path.exists(), "the temporary conversation must be removed");
}

#[test]
fn import_uses_the_reserved_session_id_and_provenance() {
    let scratch = support::scratch(mukade::model::HarnessId::OpenCode, "import-reserved-id");
    let observed = scratch.join("imported.json");
    let metadata = scratch.join("metadata.json");
    let database = scratch.join("destination.db");
    let ready = scratch.join("import-complete");
    let reserved = "ses_0123456789abcdef0123456789abcdef";
    std::fs::write(
        &metadata,
        r#"{"tritechFork":{"source":"claude-code","requestID":"request-1"}}"#,
    )
    .unwrap();
    // The launcher copies the document, then waits for the worker to create the session row, so
    // the write sees its reserved id present when it checks the outcome.
    let engine = support::launcher(
        &scratch.join("bin"),
        "engine",
        &format!(
            "@echo off\r\ncopy /y \"%~3\" \"{}\" >nul\r\nfor /L %%i in (1,1,30) do (\r\n if exist \"{}\" goto done\r\n %SystemRoot%\\System32\\ping.exe -n 1 -w 50 127.0.0.1 >nul\r\n)\r\nexit /b 1\r\n:done\r\necho Imported session: {reserved}\r\n",
            observed.display(),
            ready.display()
        ),
        &format!(
            "#!/bin/sh\ncp \"$3\" '{}'\nfor i in $(seq 1 100); do [ -f '{}' ] && break; sleep 0.05; done\n[ -f '{}' ] || exit 1\necho 'Imported session: {reserved}'\n",
            observed.display(),
            ready.display(),
            ready.display()
        ),
    );
    let worker = {
        let observed = observed.clone();
        let database = database.clone();
        let ready = ready.clone();
        std::thread::spawn(move || {
            for _ in 0..500 {
                if let Ok(bytes) = std::fs::read(&observed)
                    && let Ok(doc) = serde_json::from_slice::<serde_json::Value>(&bytes)
                {
                    let db = rusqlite::Connection::open(&database).unwrap();
                    db.execute_batch(
                        "CREATE TABLE session_v2 (id TEXT PRIMARY KEY, metadata TEXT)",
                    )
                    .unwrap();
                    db.execute(
                        "INSERT INTO session_v2 VALUES (?, ?)",
                        [reserved, &doc["info"]["metadata"].to_string()],
                    )
                    .unwrap();
                    std::fs::write(&ready, "done").unwrap();
                    return true;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            false
        })
    };
    let fixture = fixture();
    let mut args = import_args(
        fixture.to_str().unwrap(),
        scratch.to_str().unwrap(),
        engine.to_str().unwrap(),
    );
    args.extend([
        "--dest-id",
        reserved,
        "--dest-agent",
        "analyst",
        "--metadata-file",
        metadata.to_str().unwrap(),
    ]);
    let (code, stdout, stderr) = support::cli_in(
        &scratch,
        &scratch,
        &args,
        &[
            ("OPENCODE_DB", database.to_str().unwrap()),
            (
                "MUKADE_DATA_DIR",
                scratch.join("data").to_str().unwrap(),
            ),
        ],
    );
    assert!(
        worker.join().unwrap(),
        "the stand-in did not create the session"
    );
    assert_eq!(code, Some(0), "{stderr}");
    let doc: serde_json::Value = serde_json::from_slice(&std::fs::read(observed).unwrap()).unwrap();
    assert_eq!(doc["info"]["id"], reserved);
    assert_eq!(doc["info"]["agent"], "analyst");
    for message in doc["messages"].as_array().unwrap() {
        if message["type"] == "assistant" {
            assert_eq!(message["agent"], "analyst");
        }
    }
    assert_eq!(
        doc["info"]["metadata"]["tritechFork"]["requestID"],
        "request-1"
    );
    let receipt: serde_json::Value = serde_json::from_slice(stdout.as_bytes()).unwrap();
    assert_eq!(receipt["id"], reserved);
}

#[test]
fn a_failed_import_with_the_reserved_id_present_reports_an_unknown_outcome() {
    let scratch = support::scratch(mukade::model::HarnessId::OpenCode, "import-unknown-outcome");
    let database = scratch.join("destination.db");
    let reserved = "ses_0123456789abcdef0123456789abcdef";
    {
        let db = rusqlite::Connection::open(&database).unwrap();
        db.execute_batch("CREATE TABLE session_v2 (id TEXT PRIMARY KEY);")
            .unwrap();
        db.execute("INSERT INTO session_v2 VALUES (?)", [reserved])
            .unwrap();
    }
    let engine = support::launcher(&scratch.join("bin"), "engine", "exit /b 1", "exit 1");
    let fixture = fixture();
    let mut args = import_args(
        fixture.to_str().unwrap(),
        scratch.to_str().unwrap(),
        engine.to_str().unwrap(),
    );
    args.extend(["--dest-id", reserved]);
    let (code, _, stderr) = support::cli_in(
        &scratch,
        &scratch,
        &args,
        &[
            ("OPENCODE_DB", database.to_str().unwrap()),
            (
                "MUKADE_DATA_DIR",
                scratch.join("data").to_str().unwrap(),
            ),
        ],
    );
    assert_eq!(code, Some(5), "{stderr}");
    assert!(stderr.contains("outcome unknown"), "{stderr}");
    assert!(stderr.contains(reserved), "{stderr}");
}

#[test]
fn without_tools_excludes_calls_and_results_and_reports_the_omission() {
    let scratch = support::scratch(mukade::model::HarnessId::OpenCode, "import-without-tools");
    let observed = scratch.join("imported.json");
    let engine = support::launcher(
        &scratch.join("bin"),
        "engine",
        &format!(
            "@echo off\r\ncopy /y \"%~3\" \"{}\" >nul\r\necho Imported session: ses_test\r\n",
            observed.display()
        ),
        &format!(
            "#!/bin/sh\ncp \"$3\" '{}'\necho 'Imported session: ses_test'\n",
            observed.display()
        ),
    );
    let fixture = fixture();
    let mut args = import_args(
        fixture.to_str().unwrap(),
        scratch.to_str().unwrap(),
        engine.to_str().unwrap(),
    );
    args.push("--without-tools");
    let (code, stdout, stderr) = support::cli_in(
        &scratch,
        &scratch,
        &args,
        &[
            (
                "OPENCODE_DB",
                scratch.join("destination.db").to_str().unwrap(),
            ),
            (
                "MUKADE_DATA_DIR",
                scratch.join("data").to_str().unwrap(),
            ),
        ],
    );
    assert_eq!(code, Some(0), "{stderr}");
    let doc: serde_json::Value = serde_json::from_slice(&std::fs::read(observed).unwrap()).unwrap();
    let history = doc["messages"].to_string();
    assert!(!history.contains("\"type\":\"tool\""), "{history}");
    assert!(!history.contains("ALPHA-CONTENT-42"), "{history}");
    let receipt: serde_json::Value = serde_json::from_slice(stdout.as_bytes()).unwrap();
    assert_eq!(receipt["manifest"]["counts"]["tools_omitted"], 2);
    assert!(
        receipt["manifest"]["losses"]
            .to_string()
            .contains("omitted"),
        "{}",
        receipt["manifest"]["losses"]
    );
}

#[test]
fn invalid_reserved_id_is_rejected_before_starting_the_source_engine() {
    let scratch = support::scratch(mukade::model::HarnessId::OpenCode, "import-invalid-id");
    let observed = scratch.join("engine-started");
    let engine = support::launcher(
        &scratch.join("bin"),
        "engine",
        &format!(
            "@echo off\r\necho started>\"{}\"\r\nexit /b 1\r\n",
            observed.display()
        ),
        &format!("#!/bin/sh\necho started > '{}'\nexit 1\n", observed.display()),
    );
    let (code, _, stderr) = support::cli_in(
        &scratch,
        &scratch,
        &[
            "hop",
            "opencode",
            "ses_source",
            "opencode",
            "--dest-id",
            "../invalid",
        ],
        &[
            ("MUKADE_OPENCODE_EXE", engine.to_str().unwrap()),
            (
                "MUKADE_DATA_DIR",
                scratch.join("data").to_str().unwrap(),
            ),
        ],
    );
    assert_ne!(code, Some(0), "{stderr}");
    assert!(
        stderr.contains("invalid destination session ID"),
        "{stderr}"
    );
    assert!(
        !observed.exists(),
        "invalid input must not start the source engine"
    );
}

#[test]
fn a_successful_process_without_the_reserved_session_is_not_reported_as_success() {
    let scratch = support::scratch(mukade::model::HarnessId::OpenCode, "import-missing-id");
    let reserved = "ses_0123456789abcdef0123456789abcdef";
    let engine = support::launcher(
        &scratch.join("bin"),
        "engine",
        &format!("@echo off\r\necho Imported session: {reserved}\r\n"),
        &format!("#!/bin/sh\necho 'Imported session: {reserved}'\n"),
    );
    let fixture = fixture();
    let mut args = import_args(
        fixture.to_str().unwrap(),
        scratch.to_str().unwrap(),
        engine.to_str().unwrap(),
    );
    args.extend(["--dest-id", reserved]);
    let (code, _, stderr) = support::cli_in(
        &scratch,
        &scratch,
        &args,
        &[
            (
                "OPENCODE_DB",
                scratch.join("destination.db").to_str().unwrap(),
            ),
            (
                "MUKADE_DATA_DIR",
                scratch.join("data").to_str().unwrap(),
            ),
        ],
    );
    assert_eq!(code, Some(5), "{stderr}");
    assert!(stderr.contains("outcome unknown"), "{stderr}");
    assert!(!scratch.join("destination.db").exists());
}
