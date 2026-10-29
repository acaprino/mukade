//! The CLI must keep the source and destination stores separate, even for the same harness.

use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("mukade-root-test-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn hop_to_the_same_harness_writes_to_the_destination_store_not_the_source() {
    let scratch = Scratch::new();
    let source = scratch.0.join("source.db");
    let destination = scratch.0.join("destination.db");
    let observed = scratch.0.join("observed.txt");
    let launcher = scratch.0.join(if cfg!(windows) {
        "engine.cmd"
    } else {
        "engine"
    });
    let script = if cfg!(windows) {
        format!(
            "@echo off\r\necho %OPENCODE_DB%>\"{}\"\r\necho Imported session: ses_test\r\n",
            observed.display()
        )
    } else {
        format!(
            "#!/bin/sh\nprintf '%s' \"$OPENCODE_DB\" > '{}'\necho 'Imported session: ses_test'\n",
            observed.display()
        )
    };
    std::fs::write(&launcher, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/opencode/source/session-export.json");
    let output = Command::new(env!("CARGO_BIN_EXE_mukade"))
        .args(["hop", "opencode", "fixture", "opencode", "--from-file"])
        .arg(&fixture)
        .arg("--root")
        .arg(&source)
        .arg("--cwd")
        .arg(&scratch.0)
        .args([
            "--dest-version",
            "2.1.4",
            "--project-id",
            "global",
            "--model-provider",
            "tri-llm-gateway",
            "--model",
            "tritech-auto",
            "--opencode-exe",
        ])
        .arg(&launcher)
        .env("OPENCODE_DB", &destination)
        .env_remove("OPENCODE_CONFIG")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual = std::fs::read_to_string(&observed).unwrap();
    assert_eq!(actual.trim(), destination.to_str().unwrap());
}

#[test]
fn an_explicit_data_directory_holds_the_private_import_document() {
    let scratch = Scratch::new();
    let data = scratch.0.join("private-data");
    let observed = scratch.0.join("document-path.txt");
    let launcher = scratch.0.join(if cfg!(windows) {
        "engine.cmd"
    } else {
        "engine"
    });
    let script = if cfg!(windows) {
        format!(
            "@echo off\r\necho %~3>\"{}\"\r\necho Imported session: ses_test\r\n",
            observed.display()
        )
    } else {
        format!(
            "#!/bin/sh\nprintf '%s' \"$3\" > '{}'\necho 'Imported session: ses_test'\n",
            observed.display()
        )
    };
    std::fs::write(&launcher, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/opencode/source/session-export.json");
    let output = Command::new(env!("CARGO_BIN_EXE_mukade"))
        .args(["hop", "opencode", "fixture", "opencode", "--from-file"])
        .arg(&fixture)
        .arg("--cwd")
        .arg(&scratch.0)
        .args([
            "--dest-version",
            "2.1.4",
            "--project-id",
            "global",
            "--model-provider",
            "tri-llm-gateway",
            "--model",
            "tritech-auto",
            "--opencode-exe",
        ])
        .arg(&launcher)
        .env("OPENCODE_DB", scratch.0.join("destination.db"))
        .env("MUKADE_DATA_DIR", &data)
        .env_remove("OPENCODE_CONFIG")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let path = PathBuf::from(std::fs::read_to_string(observed).unwrap().trim());
    assert!(
        path.starts_with(&data),
        "{} is not inside {}",
        path.display(),
        data.display()
    );
    assert!(!path.exists(), "the temporary conversation must be removed");
}

#[test]
fn list_opencode_reads_root_sessions_from_the_selected_store_without_changing_it() {
    let scratch = Scratch::new();
    let database = scratch.0.join("sessions.db");
    {
        let db = rusqlite::Connection::open(&database).unwrap();
        db.execute_batch("CREATE TABLE session_v2 (id TEXT PRIMARY KEY, parent_id TEXT, title TEXT, directory TEXT, time_updated INTEGER, time_archived INTEGER);
            INSERT INTO session_v2 VALUES ('ses_open', NULL, 'Open', 'D:/work', 2000, NULL);
            INSERT INTO session_v2 VALUES ('ses_archived', NULL, 'Archived', 'D:/other', 1000, 1500);
            INSERT INTO session_v2 VALUES ('ses_child', 'ses_open', 'Child', 'D:/work', 3000, NULL);").unwrap();
    }
    let before = std::fs::read(&database).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_mukade"))
        .args(["list", "opencode", "--root"])
        .arg(&database)
        .env("OPENCODE_DB", scratch.0.join("wrong.db"))
        .env("MUKADE_OPENCODE_EXE", scratch.0.join("no-engine"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let rows: Vec<serde_json::Value> = stdout
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), 2);
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
    assert!(!scratch.0.join("wrong.db").exists());
}

#[test]
fn list_opencode_sees_uncheckpointed_wal_without_modifying_the_store() {
    let scratch = Scratch::new();
    let database = scratch.0.join("sessions.db");
    let db = rusqlite::Connection::open(&database).unwrap();
    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0;
        CREATE TABLE session_v2 (id TEXT PRIMARY KEY, parent_id TEXT, title TEXT, directory TEXT, time_updated INTEGER, time_archived INTEGER);
        INSERT INTO session_v2 VALUES ('ses_live', NULL, 'Live', 'D:/work', 2000, NULL);").unwrap();
    let wal = PathBuf::from(format!("{}-wal", database.display()));
    let before_db = std::fs::read(&database).unwrap();
    let before_wal = std::fs::read(&wal).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_mukade"))
        .args(["list", "opencode", "--root"])
        .arg(&database)
        .env("MUKADE_OPENCODE_EXE", scratch.0.join("no-engine"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<serde_json::Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], "ses_live");
    assert_eq!(std::fs::read(&database).unwrap(), before_db);
    assert_eq!(std::fs::read(&wal).unwrap(), before_wal);
    drop(db);
}

#[test]
fn a_relative_data_directory_is_refused_before_import() {
    let scratch = Scratch::new();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/opencode/source/session-export.json");
    let output = Command::new(env!("CARGO_BIN_EXE_mukade"))
        .args(["hop", "opencode", "fixture", "opencode", "--from-file"])
        .arg(&fixture)
        .arg("--cwd")
        .arg(&scratch.0)
        .args([
            "--dest-version",
            "2.1.4",
            "--project-id",
            "global",
            "--model-provider",
            "tri-llm-gateway",
            "--model",
            "tritech-auto",
            "--opencode-exe",
        ])
        .arg(scratch.0.join("no-engine"))
        .env("OPENCODE_DB", scratch.0.join("destination.db"))
        .env("MUKADE_DATA_DIR", "relative-data")
        .current_dir(&scratch.0)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("MUKADE_DATA_DIR must be absolute"));
    assert!(!scratch.0.join("relative-data").exists());
    assert!(!scratch.0.join("destination.db").exists());
}

#[test]
fn a_relative_data_directory_is_refused_even_when_the_destination_needs_no_temporary_file() {
    let scratch = Scratch::new();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/opencode/source/session-export.json");
    let output = Command::new(env!("CARGO_BIN_EXE_mukade"))
        .args(["hop", "opencode", "fixture", "pi", "--from-file"])
        .arg(&fixture)
        .arg("--cwd")
        .arg(&scratch.0)
        .args(["--dest-version", "1.4.0"])
        .env("PI_CODING_AGENT_DIR", &scratch.0)
        .env("MUKADE_DATA_DIR", "relative-data")
        .current_dir(&scratch.0)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("MUKADE_DATA_DIR must be absolute"));
    assert!(!scratch.0.join("sessions").exists());
}

#[test]
fn import_uses_the_reserved_session_id_and_provenance() {
    let scratch = Scratch::new();
    let observed = scratch.0.join("imported.json");
    let metadata = scratch.0.join("metadata.json");
    let database = scratch.0.join("destination.db");
    let ready = scratch.0.join("import-complete");
    let reserved = "ses_0123456789abcdef0123456789abcdef";
    std::fs::write(
        &metadata,
        r#"{"tritechFork":{"source":"claude-code","requestID":"request-1"}}"#,
    )
    .unwrap();
    let launcher = scratch.0.join(if cfg!(windows) {
        "engine.cmd"
    } else {
        "engine"
    });
    let script = if cfg!(windows) {
        format!(
            "@echo off\r\ncopy /y \"%~3\" \"{}\" >nul\r\nfor /L %%i in (1,1,30) do (\r\n if exist \"{}\" goto done\r\n powershell -NoProfile -Command \"Start-Sleep -Milliseconds 50\" >nul\r\n)\r\nexit /b 1\r\n:done\r\necho Imported session: {reserved}\r\n",
            observed.display(),
            ready.display()
        )
    } else {
        format!(
            "#!/bin/sh\ncp \"$3\" '{}'\nfor i in $(seq 1 100); do [ -f '{}' ] && break; sleep 0.05; done\n[ -f '{}' ] || exit 1\necho 'Imported session: {reserved}'\n",
            observed.display(),
            ready.display(),
            ready.display()
        )
    };
    std::fs::write(&launcher, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let worker = {
        let observed = observed.clone();
        let database = database.clone();
        let ready = ready.clone();
        std::thread::spawn(move || {
            for _ in 0..500 {
                if let Ok(bytes) = std::fs::read(&observed) {
                    if let Ok(doc) = serde_json::from_slice::<serde_json::Value>(&bytes) {
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
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            false
        })
    };
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/opencode/source/session-export.json");
    let output = Command::new(env!("CARGO_BIN_EXE_mukade"))
        .args(["hop", "opencode", "fixture", "opencode", "--from-file"])
        .arg(&fixture)
        .arg("--cwd")
        .arg(&scratch.0)
        .args([
            "--dest-version",
            "2.1.4",
            "--project-id",
            "global",
            "--model-provider",
            "tri-llm-gateway",
            "--model",
            "tritech-auto",
            "--opencode-exe",
        ])
        .arg(&launcher)
        .args([
            "--dest-id",
            reserved,
            "--dest-agent",
            "analyst",
            "--metadata-file",
        ])
        .arg(&metadata)
        .env("OPENCODE_DB", &database)
        .env("MUKADE_DATA_DIR", scratch.0.join("data"))
        .output()
        .unwrap();
    assert!(
        worker.join().unwrap(),
        "the stand-in did not create the session"
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
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
    let receipt: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(receipt["id"], reserved);
}

#[test]
fn a_file_writer_honors_a_reserved_destination_and_refuses_to_overwrite_it() {
    use mukade::model::HarnessId;
    use mukade::writer::{WriteOpts, plan};
    let scratch = Scratch::new();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/opencode/source/session-export.json");
    let source = mukade::reader::opencode::read_file(&fixture).unwrap();
    let options = WriteOpts {
        roots: mukade::roots::StoreRoots {
            pi_agent_dir: Some(scratch.0.clone()),
            ..Default::default()
        },
        cwd: Some(scratch.0.to_string_lossy().into_owned()),
        dest_id: Some("00000000-0000-4000-8000-000000000001".into()),
        dest_path: Some(scratch.0.join("sessions").join("reserved.jsonl")),
        ..Default::default()
    };
    let planned = plan::plan(&source, HarnessId::Pi, &options);
    let writer = mukade::writer::for_harness(HarnessId::Pi);
    let written = writer.write_planned(&source, &planned, &options).unwrap();
    assert_eq!(written.id, "00000000-0000-4000-8000-000000000001");
    assert_eq!(written.location, scratch.0.join("sessions").join("reserved.jsonl").to_string_lossy());
    assert!(writer.write_planned(&source, &planned, &options).is_err());
}

#[test]
fn a_failed_import_with_the_reserved_id_present_reports_an_unknown_outcome() {
    let scratch = Scratch::new();
    let database = scratch.0.join("destination.db");
    let reserved = "ses_0123456789abcdef0123456789abcdef";
    {
        let db = rusqlite::Connection::open(&database).unwrap();
        db.execute_batch("CREATE TABLE session_v2 (id TEXT PRIMARY KEY);")
            .unwrap();
        db.execute("INSERT INTO session_v2 VALUES (?)", [reserved])
            .unwrap();
    }
    let launcher = scratch.0.join(if cfg!(windows) {
        "engine.cmd"
    } else {
        "engine"
    });
    std::fs::write(
        &launcher,
        if cfg!(windows) {
            "@echo off\r\nexit /b 1\r\n"
        } else {
            "#!/bin/sh\nexit 1\n"
        },
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/opencode/source/session-export.json");
    let output = Command::new(env!("CARGO_BIN_EXE_mukade"))
        .args(["hop", "opencode", "fixture", "opencode", "--from-file"])
        .arg(&fixture)
        .arg("--cwd")
        .arg(&scratch.0)
        .args([
            "--dest-version",
            "2.1.4",
            "--project-id",
            "global",
            "--model-provider",
            "tri-llm-gateway",
            "--model",
            "tritech-auto",
            "--opencode-exe",
        ])
        .arg(&launcher)
        .args(["--dest-id", reserved])
        .env("OPENCODE_DB", &database)
        .env("MUKADE_DATA_DIR", scratch.0.join("data"))
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(5),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("outcome unknown"), "{error}");
    assert!(error.contains(reserved), "{error}");
}

#[test]
fn without_tools_excludes_calls_and_results_and_reports_the_omission() {
    let scratch = Scratch::new();
    let observed = scratch.0.join("imported.json");
    let launcher = scratch.0.join(if cfg!(windows) {
        "engine.cmd"
    } else {
        "engine"
    });
    let script = if cfg!(windows) {
        format!(
            "@echo off\r\ncopy /y \"%~3\" \"{}\" >nul\r\necho Imported session: ses_test\r\n",
            observed.display()
        )
    } else {
        format!(
            "#!/bin/sh\ncp \"$3\" '{}'\necho 'Imported session: ses_test'\n",
            observed.display()
        )
    };
    std::fs::write(&launcher, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/opencode/source/session-export.json");
    let output = Command::new(env!("CARGO_BIN_EXE_mukade"))
        .args(["hop", "opencode", "fixture", "opencode", "--from-file"])
        .arg(&fixture)
        .arg("--cwd")
        .arg(&scratch.0)
        .args([
            "--dest-version",
            "2.1.4",
            "--project-id",
            "global",
            "--model-provider",
            "tri-llm-gateway",
            "--model",
            "tritech-auto",
            "--opencode-exe",
        ])
        .arg(&launcher)
        .arg("--without-tools")
        .env("OPENCODE_DB", scratch.0.join("destination.db"))
        .env("MUKADE_DATA_DIR", scratch.0.join("data"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let doc: serde_json::Value = serde_json::from_slice(&std::fs::read(observed).unwrap()).unwrap();
    let history = doc["messages"].to_string();
    assert!(!history.contains("\"type\":\"tool\""));
    assert!(!history.contains("ALPHA-CONTENT-42"));
    let receipt: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(receipt["manifest"]["counts"]["tools_omitted"], 2);
    assert!(
        receipt["manifest"]["losses"]
            .to_string()
            .contains("omitted")
    );
}

#[test]
fn invalid_reserved_id_is_rejected_before_starting_the_source_engine() {
    let scratch = Scratch::new();
    let observed = scratch.0.join("engine-started");
    let launcher = scratch.0.join(if cfg!(windows) {
        "engine.cmd"
    } else {
        "engine"
    });
    let script = if cfg!(windows) {
        format!(
            "@echo off\r\necho started>\"{}\"\r\nexit /b 1\r\n",
            observed.display()
        )
    } else {
        format!(
            "#!/bin/sh\necho started > '{}'\nexit 1\n",
            observed.display()
        )
    };
    std::fs::write(&launcher, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let output = Command::new(env!("CARGO_BIN_EXE_mukade"))
        .args([
            "hop",
            "opencode",
            "ses_source",
            "opencode",
            "--dest-id",
            "../invalid",
        ])
        .env("OPENCODE_DB", scratch.0.join("destination.db"))
        .env("MUKADE_OPENCODE_EXE", &launcher)
        .env("MUKADE_DATA_DIR", scratch.0.join("data"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("invalid destination session ID")
    );
    assert!(
        !observed.exists(),
        "invalid input must not start the source engine"
    );
}

#[test]
fn a_successful_process_without_the_reserved_session_is_not_reported_as_success() {
    let scratch = Scratch::new();
    let reserved = "ses_0123456789abcdef0123456789abcdef";
    let launcher = scratch.0.join(if cfg!(windows) {
        "engine.cmd"
    } else {
        "engine"
    });
    let script = if cfg!(windows) {
        format!("@echo off\r\necho Imported session: {reserved}\r\n")
    } else {
        format!("#!/bin/sh\necho 'Imported session: {reserved}'\n")
    };
    std::fs::write(&launcher, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/opencode/source/session-export.json");
    let output = Command::new(env!("CARGO_BIN_EXE_mukade"))
        .args(["hop", "opencode", "fixture", "opencode", "--from-file"])
        .arg(&fixture)
        .arg("--cwd")
        .arg(&scratch.0)
        .args([
            "--dest-version",
            "2.1.4",
            "--project-id",
            "global",
            "--model-provider",
            "tri-llm-gateway",
            "--model",
            "tritech-auto",
            "--opencode-exe",
        ])
        .arg(&launcher)
        .args(["--dest-id", reserved])
        .env("OPENCODE_DB", scratch.0.join("destination.db"))
        .env("MUKADE_DATA_DIR", scratch.0.join("data"))
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(5),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("outcome unknown"));
    assert!(!scratch.0.join("destination.db").exists());
}

#[test]
fn a_changed_source_is_refused_before_the_destination_is_written() {
    let scratch = Scratch::new();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/claude_code/source/c45db7bc-542c-4201-89de-b590c6fdb797.jsonl");
    let original = std::fs::read(&fixture).unwrap();
    let run = |expected: &str| {
        Command::new(env!("CARGO_BIN_EXE_mukade"))
            .args(["hop", "claude_code", "fixture", "pi", "--from-file"])
            .arg(&fixture)
            .arg("--cwd")
            .arg(&scratch.0)
            .args(["--dest-version", "1.4.0", "--expect-sha256", expected])
            .env("PI_CODING_AGENT_DIR", &scratch.0)
            .env("MUKADE_DATA_DIR", scratch.0.join("data"))
            .output()
            .unwrap()
    };
    let changed = run(&"0".repeat(64));
    assert_eq!(
        changed.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&changed.stderr)
    );
    assert!(String::from_utf8_lossy(&changed.stderr).contains("source changed"));
    assert!(!scratch.0.join("sessions").exists());
    let accepted = run(&mukade::sha256::sha256_hex(&original));
    assert!(
        accepted.status.success(),
        "{}",
        String::from_utf8_lossy(&accepted.stderr)
    );
    assert!(scratch.0.join("sessions").exists());
    assert_eq!(std::fs::read(&fixture).unwrap(), original);
}

#[test]
fn a_jsonl_capture_keeps_the_exact_bytes_that_were_parsed_even_if_the_file_changes() {
    let scratch = Scratch::new();
    let source = scratch.0.join("session.jsonl");
    let original = "{\"type\":\"user\",\"text\":\"before\"}\n";
    std::fs::write(&source, original).unwrap();
    let capture = mukade::reader::json::read_jsonl(&source).unwrap();
    std::fs::write(&source, "{\"type\":\"user\",\"text\":\"after\"}\n").unwrap();
    assert_eq!(capture.source_bytes.as_bytes(), original.as_bytes());
    assert_eq!(
        capture.sha256.wait(),
        mukade::sha256::sha256_hex(original.as_bytes())
    );
    assert_eq!(capture.rows[0]["text"], "before");
}

#[test]
fn a_claude_branch_with_out_of_order_rows_follows_its_parent_chain() {
    use mukade::model::Kind;
    use serde_json::json;
    let user = |uuid: &str, parent: serde_json::Value, text: &str| {
        json!({
            "type":"user", "uuid":uuid, "parentUuid":parent, "isSidechain":false,
            "message":{"role":"user", "content":text}
        })
    };
    let assistant = |uuid: &str, parent: &str, text: &str| {
        json!({
            "type":"assistant", "uuid":uuid, "parentUuid":parent, "isSidechain":false,
            "message":{"id":format!("msg-{uuid}"), "role":"assistant", "model":"synthetic",
                       "content":[{"type":"text", "text":text}], "stop_reason":"end_turn"}
        })
    };
    let rows = vec![
        user("u2", json!("a1"), "SECOND"),
        user("u1", serde_json::Value::Null, "FIRST"),
        assistant("a1", "u1", "ANSWER1"),
        assistant("a2", "u2", "ANSWER2"),
    ];
    let session = mukade::reader::claude_code::parse(&rows, None).unwrap();
    let turns: Vec<String> = session
        .conversation()
        .filter_map(|entry| match &entry.kind {
            Kind::User { text, .. } => Some(text.clone()),
            Kind::Step { text, .. } => Some(text.join("")),
            _ => None,
        })
        .collect();
    assert_eq!(turns, ["FIRST", "ANSWER1", "SECOND", "ANSWER2"]);
}

#[test]
fn out_of_order_claude_tool_history_follows_the_parent_chain() {
    use serde_json::json;
    let rows = vec![
        json!({"type":"user", "uuid":"u2", "parentUuid":"r1", "message":{"role":"user", "content":"next"}}),
        json!({"type":"user", "uuid":"u1", "parentUuid":null, "message":{"role":"user", "content":"first"}}),
        json!({"type":"assistant", "uuid":"a1", "parentUuid":"u1", "message":{"id":"m1", "role":"assistant", "content":[{"type":"tool_use", "id":"call1", "name":"Read", "input":{}}]}}),
        json!({"type":"user", "uuid":"r1", "parentUuid":"a1", "message":{"role":"user", "content":[{"type":"tool_result", "tool_use_id":"call1", "content":"result"}]}}),
        json!({"type":"assistant", "uuid":"a2", "parentUuid":"u2", "message":{"id":"m2", "role":"assistant", "content":[{"type":"text", "text":"done"}]}}),
    ];
    let session = mukade::reader::claude_code::parse(&rows, None).unwrap();
    let entries: Vec<_> = session.conversation().collect();
    assert_eq!(entries.len(), 5);
    assert!(matches!(&entries[0].kind, mukade::model::Kind::User { text, .. } if text == "first"));
    assert!(
        matches!(&entries[1].kind, mukade::model::Kind::Step { calls, .. } if calls.len() == 1 && calls[0].id == "call1")
    );
    assert!(
        matches!(&entries[2].kind, mukade::model::Kind::Results { results } if results.len() == 1 && results[0].call_id == "call1")
    );
    assert!(matches!(&entries[3].kind, mukade::model::Kind::User { text, .. } if text == "next"));
    assert!(
        matches!(&entries[4].kind, mukade::model::Kind::Step { text, .. } if text == &["done"])
    );
}

#[test]
fn a_dangling_or_cyclic_claude_chain_is_refused() {
    use serde_json::json;
    let missing = vec![
        json!({"type":"user", "uuid":"u1", "parentUuid":"missing", "message":{"role":"user", "content":"hello"}}),
    ];
    assert!(
        mukade::reader::claude_code::parse(&missing, None)
            .unwrap_err()
            .to_string()
            .contains("missing Claude Code parent")
    );
    let cycle = vec![
        json!({"type":"user", "uuid":"u1", "parentUuid":"a1", "message":{"role":"user", "content":"hello"}}),
        json!({"type":"assistant", "uuid":"a1", "parentUuid":"u1", "message":{"id":"m1", "role":"assistant", "content":[{"type":"text", "text":"answer"}]}}),
    ];
    assert!(
        mukade::reader::claude_code::parse(&cycle, None)
            .unwrap_err()
            .to_string()
            .contains("cycle")
    );
}

#[test]
fn an_out_of_order_chain_with_a_malformed_tool_is_not_silently_accepted() {
    use serde_json::json;
    let rows = vec![
        json!({"type":"user", "uuid":"u2", "parentUuid":"r1", "message":{"role":"user", "content":"next"}}),
        json!({"type":"user", "uuid":"u1", "parentUuid":null, "message":{"role":"user", "content":"first"}}),
        json!({"type":"assistant", "uuid":"a1", "parentUuid":"u1", "message":{"id":"m1", "role":"assistant", "content":[{"type":"tool_use", "id":"call1", "input":{}}]}}),
        json!({"type":"user", "uuid":"r1", "parentUuid":"a1", "message":{"role":"user", "content":[{"type":"tool_result", "tool_use_id":"call1", "content":"result"}]}}),
        json!({"type":"assistant", "uuid":"a2", "parentUuid":"u2", "message":{"id":"m2", "role":"assistant", "content":[{"type":"text", "text":"done"}]}}),
    ];
    assert!(mukade::reader::claude_code::parse(&rows, None).is_err());
}

#[test]
fn document_opencode_returns_a_native_import_document_without_starting_a_write() {
    let scratch = Scratch::new();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/opencode/source/session-export.json");
    let output = Command::new(env!("CARGO_BIN_EXE_mukade"))
        .args(["document", "opencode", "opencode", "fixture", "--from-file"]).arg(&fixture)
        .arg("--cwd").arg(&scratch.0)
        .args(["--dest-id", "ses_preview", "--dest-agent", "analyst", "--project-id", "global", "--model-provider", "tri-llm-gateway", "--model", "tritech-auto", "--without-tools"])
        .env("MUKADE_DATA_DIR", scratch.0.join("data"))
        .output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["document"]["info"]["id"], "ses_preview");
    assert_eq!(result["document"]["info"]["agent"], "analyst");
    assert!(result["counts"]["tools_omitted"].as_u64().unwrap() > 0);
    assert!(!scratch.0.join("destination.db").exists());
}
