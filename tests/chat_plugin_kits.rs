//! The OpenCode and Pi chat adapters: a plugin kit per launch, whose JavaScript runs the hooks.
//! The kit's files and command are checked here; its JavaScript runs under Node against a real
//! `mukade chat` service, with Node playing the harness (`support/plugin_kit_driver.mjs`).

use mukade::HarnessId;
use mukade::chat::adapter::{self, Delivery, Event, Kit};
use mukade::chat::lifecycle;
use mukade::roots::StoreRoots;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::var_os("MUKADE_TEST_SCRATCH_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("mukade-tests-kits-{tag}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn kit(folder: PathBuf, exe: PathBuf, server: mukade::chat::connect::Server, args: &[&str]) -> Kit {
    Kit {
        exe,
        folder,
        server,
        stop_timeout: adapter::STOP_HOOK_TIMEOUT,
        args: args.iter().map(|a| a.to_string()).collect(),
    }
}

fn server(token: &str) -> mukade::chat::connect::Server {
    mukade::chat::connect::Server {
        endpoint: "http://127.0.0.1:4000/mcp".into(),
        token: token.into(),
        service_id: "svc".into(),
    }
}

fn args_of(command: &Command) -> Vec<String> {
    command
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect()
}

fn env_of(command: &Command, name: &str) -> Option<String> {
    command
        .get_envs()
        .find(|(k, _)| *k == name)
        .and_then(|(_, v)| v.map(|v| v.to_string_lossy().into_owned()))
}

#[test]
fn opencode_and_pi_read_the_hook_answers_in_the_plugin_format() {
    for harness in [HarnessId::OpenCode, HarnessId::Pi] {
        let adapter = mukade::harnesses::get(harness);
        let answer = |event, delivery: &Delivery| -> Value {
            serde_json::from_str(&adapter.chat_hook_output(event, delivery).unwrap()).unwrap()
        };
        let context = Delivery::Context("hi".into());
        assert_eq!(
            answer(Event::Prompt, &context),
            serde_json::json!({"context": "hi"})
        );
        assert_eq!(
            answer(Event::SessionStart, &context),
            serde_json::json!({"context": "hi"})
        );
        assert_eq!(
            answer(Event::Stop, &Delivery::Continue("go on".into())),
            serde_json::json!({"continue": "go on"})
        );
        assert!(adapter.chat_hook_output(Event::Stop, &context).is_none());
    }
    for other in [HarnessId::Hermes, HarnessId::DeepSeekHarness] {
        assert!(
            mukade::harnesses::get(other)
                .chat_hook_output(Event::Stop, &Delivery::Continue("x".into()))
                .is_none(),
            "{other} has no adapter"
        );
    }
}

/// OpenCode: a server plugin named in `OPENCODE_CONFIG_CONTENT` with the MCP entry, a private
/// server so the plugin has this environment, and no credential in a file or argument.
#[test]
fn the_opencode_kit_is_a_server_plugin_and_a_configuration_document() {
    let folder = scratch("opencode").join("launch");
    let exe = PathBuf::from("/opt/mukade/mukade");
    let launch = |args: &[&str]| {
        mukade::harnesses::get(HarnessId::OpenCode).chat_launch(&kit(
            folder.clone(),
            exe.clone(),
            server("s3cr3t"),
            args,
        ))
    };
    let command = launch(&[]).unwrap().unwrap();
    assert_eq!(args_of(&command), ["--standalone"]);
    let content: Value =
        serde_json::from_str(&env_of(&command, "OPENCODE_CONFIG_CONTENT").unwrap()).unwrap();
    let plugin = folder.join("opencode-plugin");
    assert_eq!(
        content["plugins"],
        serde_json::json!([plugin.to_string_lossy()])
    );
    let mcp = &content["mcp"]["servers"]["mukade"];
    assert_eq!(mcp["type"], "remote");
    assert_eq!(mcp["codemode"], false, "the chat tools are direct tools");
    assert_eq!(mcp["url"], "http://127.0.0.1:4000/mcp");
    assert_eq!(
        mcp["headers"]["Authorization"],
        "{env:MUKADE_CHAT_AUTHORIZATION}"
    );
    assert_eq!(
        env_of(&command, "MUKADE_CHAT_AUTHORIZATION").as_deref(),
        Some("Bearer s3cr3t")
    );
    let source = std::fs::read_to_string(plugin.join("server.js")).unwrap();
    assert!(source.starts_with("const MUKADE = {"), "{source}");
    assert!(source.contains(r#""harness":"opencode""#));
    assert!(source.contains(r#""exe":"/opt/mukade/mukade""#));
    assert!(source.contains("export default"));
    assert!(!source.contains("s3cr3t"));
    let package: Value =
        serde_json::from_str(&std::fs::read_to_string(plugin.join("package.json")).unwrap())
            .unwrap();
    assert_eq!(package["type"], "module");

    // `--standalone` goes where OpenCode reads it.
    let args = |given: &[&str]| args_of(&launch(given).unwrap().unwrap());
    assert_eq!(args(&["run", "hi"]), ["run", "--standalone", "hi"]);
    assert_eq!(args(&["mini", "-c"]), ["mini", "--standalone", "-c"]);
    assert_eq!(args(&["/work/app"]), ["--standalone", "/work/app"]);
    assert_eq!(args(&["--standalone", "run"]), ["--standalone", "run"]);
    let refused = launch(&["--server", "http://x"]).unwrap_err();
    assert_eq!(refused.code, "invalid_arguments");
}

/// Pi: one extension file given with `-e`, the harness arguments after it, and the credential in
/// the environment only.
#[test]
fn the_pi_kit_is_one_extension_given_on_the_command_line() {
    let folder = scratch("pi").join("launch");
    let command = mukade::harnesses::get(HarnessId::Pi)
        .chat_launch(&kit(
            folder.clone(),
            PathBuf::from(r"C:\Program Files\mukade\mukade.exe"),
            server("s3cr3t"),
            &["-p", "hello"],
        ))
        .unwrap()
        .unwrap();
    let extension = folder.join("pi-extension").join("mukade-chat.ts");
    assert_eq!(
        args_of(&command),
        [
            "-e".to_string(),
            extension.to_string_lossy().into_owned(),
            "-p".into(),
            "hello".into()
        ]
    );
    assert_eq!(
        env_of(&command, "MUKADE_CHAT_AUTHORIZATION").as_deref(),
        Some("Bearer s3cr3t")
    );
    let source = std::fs::read_to_string(&extension).unwrap();
    assert!(
        source.contains(r#""exe":"C:\\Program Files\\mukade\\mukade.exe""#),
        "{source}"
    );
    assert!(source.contains(r#""endpoint":"http://127.0.0.1:4000/mcp""#));
    assert!(source.contains("registerMcpServer"));
    assert!(!source.contains("s3cr3t"));
    mukade::chat::security::verify_file(&extension).unwrap();
}

/// The service a kit talks to, stopped when dropped.
struct Service {
    root: PathBuf,
}

impl Service {
    fn start(tag: &str) -> Service {
        let root = scratch(tag).join("data");
        let out = mukade(&root, &["chat", "start"]);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        Service { root }
    }

    fn call(&self, args: &[&str]) -> Value {
        let out = mukade(&self.root, args);
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        let _ = mukade(&self.root, &["chat", "stop"]);
    }
}

fn mukade(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_mukade"))
        .args(args)
        .env("MUKADE_DATA_DIR", root)
        .output()
        .unwrap()
}

fn node() -> Option<&'static str> {
    Command::new("node")
        .arg("--version")
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|_| "node")
}

/// The kit's JavaScript, run as the harness runs it: Node plays OpenCode or Pi and calls the
/// plugin's hooks; the hooks run the real mukade against a real service; another member writes
/// to the room. Joining, news with a prompt, going on with an addressed message, and a wait that
/// ends when the user acts are all seen through the plugin.
fn run_kit(harness: HarnessId, kit_file: &str) {
    let Some(node) = node() else {
        eprintln!("skipped: no node on PATH to run the {harness} kit");
        return;
    };
    let service = Service::start(harness.as_str());
    let room = service.call(&["chat", "open", "--name", "kits"])["room_id"]
        .as_str()
        .unwrap()
        .to_string();
    let peer = service.call(&[
        "chat",
        "join",
        "--room-id",
        &room,
        "--harness",
        "codex",
        "--role",
        "peer",
    ])["member_handle"]
        .as_str()
        .unwrap()
        .to_string();
    let paths = lifecycle::Paths::resolve(&service.root, &StoreRoots::default()).unwrap();
    let config = lifecycle::client_config(&paths).unwrap();
    let folder = paths.bus_dir.join("adapters").join("kit-test");
    let exe = PathBuf::from(env!("CARGO_BIN_EXE_mukade"));
    let server = mukade::chat::connect::Server {
        endpoint: config.endpoint.clone(),
        token: config.token.clone(),
        service_id: config.service_id.clone(),
    };
    let command = mukade::harnesses::get(harness)
        .chat_launch(&kit(folder.clone(), exe.clone(), server, &[]))
        .unwrap()
        .unwrap();
    let driver = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("support")
        .join("plugin_kit_driver.mjs");
    let mut run = Command::new(node);
    run.arg(driver)
        .env("KIT", folder.join(kit_file))
        .env("MODE", harness.as_str())
        .env("MUKADE_EXE", &exe)
        .env("PEER_HANDLE", &peer)
        .env("MUKADE_ENDPOINT", &config.endpoint)
        .env("MUKADE_DATA_DIR", &service.root)
        .env(adapter::ENV_ROOM, &room)
        .env(adapter::ENV_ROLE, "peer")
        .env(adapter::ENV_LISTEN, "20");
    for name in ["MUKADE_CHAT_AUTHORIZATION", "MUKADE_CHAT_SERVICE_ID"] {
        run.env(name, env_of(&command, name).unwrap());
    }
    let out = run.output().unwrap();
    assert!(
        out.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "ok");
}

#[test]
fn the_pi_extension_keeps_a_session_in_the_room() {
    run_kit(HarnessId::Pi, "pi-extension/mukade-chat.ts");
}

#[test]
fn the_opencode_plugin_keeps_a_session_in_the_room() {
    run_kit(HarnessId::OpenCode, "opencode-plugin/server.js");
}
