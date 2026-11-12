#[path = "support/chat_http.rs"]
mod chat_http;
mod support;
use mukade::{
    chat::{lifecycle, security},
    cli::chat,
    roots::StoreRoots,
};
use serde_json::json;
use std::{
    io::{self, Write},
    path::PathBuf,
};

fn scratch() -> PathBuf {
    // A folder outside any project, as the chat service requires: the scratch root the suite is
    // given, else the system's temporary folder.
    let p = std::env::var_os("MUKADE_TEST_SCRATCH_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("mukade-tests-cli-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&p).unwrap();
    p
}
#[test]
fn status_and_bad_arguments_create_nothing() {
    let root = scratch().join("missing");
    let paths = lifecycle::Paths::resolve(&root, &StoreRoots::default()).unwrap();
    assert!(!lifecycle::owner_active(&paths).unwrap());
    assert!(!root.exists());
    assert!(chat::parse(&["send".into(), "--unknown".into()]).is_err());
    assert!(
        chat::parse(&[
            "open".into(),
            "--name".into(),
            "room".into(),
            "--request-id".into(),
            "bad".into()
        ])
        .is_err()
    );
}
#[test]
fn private_storage_lock_and_root_guards() {
    let root = scratch();
    let paths = lifecycle::Paths::resolve(&root.join("data"), &StoreRoots::default()).unwrap();
    let owner = lifecycle::Owner::acquire(&paths).unwrap();
    assert!(lifecycle::owner_active(&paths).unwrap());
    assert!(lifecycle::Owner::acquire(&paths).is_err());
    security::verify_directory(&paths.bus_dir).unwrap();
    drop(owner);
    assert!(!lifecycle::owner_active(&paths).unwrap());
    std::fs::create_dir(root.join(".git")).unwrap();
    assert!(lifecycle::Paths::resolve(&root.join("project-data"), &StoreRoots::default()).is_err());
    assert!(!root.join("project-data").exists());
}
struct Broken;
impl Write for Broken {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::ErrorKind::BrokenPipe.into())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[tokio::test]
async fn real_sdk_delivery_flushes_before_ack_and_escapes_controls() {
    use mukade::{
        bus::Bus,
        chat::{ChatService, ServiceConfig},
    };
    let dir = scratch();
    let bus = Bus::open_in(&dir).unwrap();
    let svc = ChatService::new(
        bus,
        StoreRoots::default(),
        ServiceConfig::new(
            dir.canonicalize().unwrap(),
            "unused".into(),
            uuid::Uuid::new_v4().to_string(),
            "http://127.0.0.1:39171/mcp".into(),
            "test-secret".into(),
        )
        .unwrap(),
    );
    // Fixture identity is taken from the durable ledger.
    assert!(svc.is_err());
    let bus = Bus::open_in(&dir).unwrap();
    let sid = bus.service_id().unwrap();
    let svc = ChatService::new(
        bus,
        StoreRoots::default(),
        ServiceConfig::new(
            dir.canonicalize().unwrap(),
            sid,
            uuid::Uuid::new_v4().to_string(),
            "http://127.0.0.1:39171/mcp".into(),
            "test-secret".into(),
        )
        .unwrap(),
    )
    .unwrap();
    let client = chat_http::client(&svc).await;
    let room = client
        .call(
            "open",
            json!({"name":"cli","request_id":uuid::Uuid::new_v4().to_string()}),
        )
        .await
        .unwrap();
    let member=client.call("join",json!({"room_id":room["room_id"],"harness":"pi","role":"peer","request_id":uuid::Uuid::new_v4().to_string()})).await.unwrap();
    let args = json!({"room_id":room["room_id"],"member_handle":member["member_handle"]});
    let cmd = chat::DomainCommand {
        operation: "read".into(),
        arguments: args.clone(),
        json: true,
        output: None,
    };
    assert!(
        chat::execute(&client, &cmd, &mut Broken, &mut Vec::new())
            .await
            .is_err()
    );
    let pending = client.call("read", args.clone()).await.unwrap();
    let mut out = Vec::new();
    chat::execute(&client, &cmd, &mut out, &mut Vec::new())
        .await
        .unwrap();
    let rendered: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(rendered["batch_id"], pending["batch_id"]);
    let after = client.call("read", args).await.unwrap();
    assert_ne!(after["batch_id"], pending["batch_id"]);
    let mut escaped = Vec::new();
    chat::write_value(&mut escaped, &json!({"text":"a\u{1b}\u{202e}b"}), true).unwrap();
    assert!(!escaped.contains(&0x1b));
    assert!(!String::from_utf8_lossy(&escaped).contains('\u{202e}'));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&escaped).unwrap()["text"],
        "a\u{1b}\u{202e}b"
    );
    client.close().await;
    svc.finish_shutdown().await;
}

/// A service that cannot start says why: the parent reports the child's own error, not a
/// generic "failed before readiness".
#[test]
fn a_service_that_fails_to_start_reports_its_own_reason() {
    let root = scratch().join("data");
    let paths = lifecycle::Paths::resolve(&root, &StoreRoots::default()).unwrap();
    security::create_private_directory(&paths.bus_dir).unwrap();
    security::atomic_write(&paths.bus_dir.join("config.json"), br#"{"port":0}"#).unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_mukade"))
        .args(["chat", "start"])
        .env("MUKADE_DATA_DIR", &root)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(4), "{stderr}");
    assert!(stderr.contains("Invalid persisted chat port"), "{stderr}");
    assert!(
        !lifecycle::owner_active(&paths).unwrap(),
        "no service left running"
    );
}

/// A hook of an instance that was not launched into a room does nothing at all: no output, no
/// file, exit 0, so installing the hooks can never disturb a harness.
#[test]
fn a_hook_outside_a_room_prints_nothing_and_writes_nothing() {
    use std::io::Write as _;
    let root = scratch().join("never-made");
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_mukade"))
        .args(["chat", "hook", "claude_code", "stop"])
        .env("MUKADE_DATA_DIR", &root)
        .env_remove("MUKADE_CHAT_ROOM_ID")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(br#"{"session_id":"s1"}"#)
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stdout.is_empty() && out.stderr.is_empty());
    assert!(!root.exists());
}

/// A hook that cannot reach the service still exits 0 and says why on stderr.
#[test]
fn a_hook_without_a_service_says_why_and_lets_the_harness_go_on() {
    use std::io::Write as _;
    let root = scratch().join("data");
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_mukade"))
        .args(["chat", "hook", "claude_code", "prompt"])
        .env("MUKADE_DATA_DIR", &root)
        .env("MUKADE_CHAT_ROOM_ID", "r1")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(br#"{"session_id":"s1"}"#)
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("mukade chat hook"));
}

#[test]
fn launch_takes_the_room_and_passes_the_rest_to_the_harness() {
    let parsed = chat::parse(
        &[
            "launch",
            "claude_code",
            "--room",
            "design",
            "--role",
            "lead",
            "--listen",
            "120",
            "--",
            "-p",
            "--model",
            "haiku",
        ]
        .map(String::from),
    )
    .unwrap();
    let chat::Command::Launch(launch) = parsed else {
        panic!("{parsed:?}")
    };
    assert_eq!(launch.room, "design");
    assert_eq!(launch.role, "lead");
    assert_eq!(launch.listen, 120);
    assert_eq!(launch.harness_args, ["-p", "--model", "haiku"]);
    for bad in [
        &["launch", "claude_code"][..],
        &["launch", "claude_code", "--room", "r", "--role", "boss"],
        &["launch", "claude_code", "--room", "r", "--listen", "99999"],
        &["launch", "claude_code", "--room", "r", "stray"],
        &["hook", "claude_code", "lunch"],
    ] {
        let bad: Vec<String> = bad.iter().map(|a| a.to_string()).collect();
        assert!(chat::parse(&bad).is_err(), "{bad:?}");
    }
}

/// Only a harness with an adapter is launched; another is refused before the service starts.
#[test]
fn launching_a_harness_without_an_adapter_starts_nothing() {
    let root = scratch().join("data");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_mukade"))
        .args(["chat", "launch", "pi", "--room", "r"])
        .env("MUKADE_DATA_DIR", &root)
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("no chat adapter"));
    assert!(!root.join("bus").join("instance.json").exists());
}

/// Run mukade and collect its output, failing (not hanging) when the output never closes.
fn run_capturing(root: &std::path::Path, args: &[&str]) -> std::process::Output {
    let child = std::process::Command::new(env!("CARGO_BIN_EXE_mukade"))
        .args(args)
        .env("MUKADE_DATA_DIR", root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    match rx.recv_timeout(std::time::Duration::from_secs(30)) {
        Ok(output) => output.unwrap(),
        Err(_) => {
            // Whatever holds the pipe is the service: stop it so the suite does not leak it.
            let _ = std::process::Command::new(env!("CARGO_BIN_EXE_mukade"))
                .args(["chat", "stop"])
                .env("MUKADE_DATA_DIR", root)
                .output();
            panic!("mukade {args:?}: its output never closed");
        }
    }
}

/// A script that captures `chat start` gets its answer: the background service does not hold
/// the caller's output open. And `chat stop` answers once the service's process is gone, so the
/// data folder can be removed right after it.
#[test]
fn start_can_be_captured_and_after_stop_the_folder_can_be_removed() {
    let root = scratch().join("data");
    let started = run_capturing(&root, &["chat", "start"]);
    assert!(
        started.status.success(),
        "{}",
        String::from_utf8_lossy(&started.stderr)
    );
    assert!(String::from_utf8_lossy(&started.stdout).contains("\"running\""));
    let opened = run_capturing(&root, &["chat", "open", "--name", "r"]);
    assert!(
        opened.status.success(),
        "{}",
        String::from_utf8_lossy(&opened.stderr)
    );
    let stopped = run_capturing(&root, &["chat", "stop"]);
    assert!(
        stopped.status.success(),
        "{}",
        String::from_utf8_lossy(&stopped.stderr)
    );
    std::fs::remove_dir_all(&root).expect("nothing of the service holds its files any more");
}
