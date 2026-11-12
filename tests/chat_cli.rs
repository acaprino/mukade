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
