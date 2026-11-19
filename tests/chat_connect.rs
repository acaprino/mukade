//! `mukade chat connect` / `disconnect`: one entry in a harness's own MCP configuration,
//! everything else left as it was. Every file here is in a scratch folder: no real harness
//! configuration is in reach.

use mukade::chat::connect::{self, Format, Server, Target};
use mukade::chat::security;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn scratch(tag: &str) -> PathBuf {
    let root = std::env::var_os("MUKADE_TEST_SCRATCH_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = root.join(format!(
        "mukade-tests-connect-{tag}-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The chat service's own folder, private as the service makes it.
fn bus_dir(root: &Path) -> PathBuf {
    let dir = root.join("data").join("bus");
    security::create_private_directory(&dir).unwrap();
    dir
}

fn server(port: u16) -> Server {
    Server {
        endpoint: format!("http://127.0.0.1:{port}/mcp"),
        token: TOKEN.into(),
        service_id: "service-1".into(),
    }
}

fn json_target(path: PathBuf, port: u16) -> Target {
    let server = server(port);
    Target {
        path,
        format: Format::Json,
        tables: vec!["mcpServers"],
        entry: json!({"type": "http", "url": server.endpoint, "headers": server.headers()}),
    }
}

fn toml_target(path: PathBuf, port: u16) -> Target {
    let server = server(port);
    Target {
        path,
        format: Format::Toml,
        tables: vec!["mcp_servers"],
        entry: json!({"url": server.endpoint, "http_headers": server.headers(), "tool_timeout_sec": 330}),
    }
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn connect_adds_one_entry_and_keeps_everything_else() {
    let root = scratch("json");
    let bus = bus_dir(&root);
    let file = root.join(".claude.json");
    std::fs::write(
        &file,
        r#"{"numStartups": 7, "mcpServers": {"other": {"type": "stdio", "command": "x"}}, "projects": {"a": {"b": 1}}}"#,
    )
    .unwrap();
    let target = json_target(file.clone(), 4000);

    let report = connect::connect(&target, &bus, "claude_code", false).unwrap();
    assert_eq!(report["status"], "connected");
    assert_eq!(report["scope"], "user");
    assert!(
        !report.to_string().contains(TOKEN),
        "the credential is never printed"
    );
    assert!(report.to_string().contains(connect::REDACTED));

    let now = read_json(&file);
    assert_eq!(now["numStartups"], 7);
    assert_eq!(now["projects"]["a"]["b"], 1);
    assert_eq!(now["mcpServers"]["other"]["command"], "x");
    assert_eq!(now["mcpServers"]["mukade"], target.entry);
    let keys: Vec<&String> = now.as_object().unwrap().keys().collect();
    assert_eq!(
        keys,
        ["numStartups", "mcpServers", "projects"],
        "key order is kept"
    );

    // The earlier file is kept in the service's own folder, not beside the configuration.
    let copies: Vec<_> = std::fs::read_dir(bus.join("backups"))
        .unwrap()
        .flatten()
        .collect();
    assert_eq!(copies.len(), 1);
    assert!(
        std::fs::read_to_string(copies[0].path())
            .unwrap()
            .contains("numStartups")
    );
    let beside: Vec<String> = std::fs::read_dir(&root)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        beside.iter().all(|name| !name.contains(".tmp")),
        "{beside:?}"
    );
}

#[test]
fn connect_twice_changes_nothing_and_disconnect_removes_only_ours() {
    let root = scratch("repeat");
    let bus = bus_dir(&root);
    let file = root.join("mcp.json");
    std::fs::write(&file, r#"{"mcpServers": {"other": {"url": "http://x"}}}"#).unwrap();
    let target = json_target(file.clone(), 4000);
    connect::connect(&target, &bus, "pi", false).unwrap();
    let after_first = std::fs::read(&file).unwrap();

    let again = connect::connect(&target, &bus, "pi", false).unwrap();
    assert_eq!(again["status"], "unchanged");
    assert_eq!(std::fs::read(&file).unwrap(), after_first);

    let gone = connect::disconnect(&target, &bus, "pi", false).unwrap();
    assert_eq!(gone["status"], "disconnected");
    let now = read_json(&file);
    assert!(now["mcpServers"].get("mukade").is_none());
    assert_eq!(now["mcpServers"]["other"]["url"], "http://x");

    let nothing = connect::disconnect(&target, &bus, "pi", false).unwrap();
    assert_eq!(nothing["status"], "not_connected");
}

#[test]
fn a_missing_file_is_created_with_the_entry_alone() {
    let root = scratch("missing");
    let bus = bus_dir(&root);
    let file = root.join("agent").join("mcp.json");
    let target = json_target(file.clone(), 4000);
    assert_eq!(
        connect::connect(&target, &bus, "pi", false).unwrap()["status"],
        "connected"
    );
    assert_eq!(
        read_json(&file),
        json!({"mcpServers": {"mukade": target.entry}})
    );
    assert!(
        !bus.join("backups").exists(),
        "there was nothing to keep a copy of"
    );
}

#[test]
fn a_dry_run_writes_nothing() {
    let root = scratch("dry");
    let bus = bus_dir(&root);
    let file = root.join("mcp.json");
    let target = json_target(file.clone(), 4000);
    let report = connect::connect(&target, &bus, "pi", true).unwrap();
    assert_eq!(report["status"], "would_connect");
    assert!(!file.exists());
    assert!(!bus.join("connections.json").exists());

    connect::connect(&target, &bus, "pi", false).unwrap();
    let before = std::fs::read(&file).unwrap();
    assert_eq!(
        connect::disconnect(&target, &bus, "pi", true).unwrap()["status"],
        "would_disconnect"
    );
    assert_eq!(std::fs::read(&file).unwrap(), before);
}

#[test]
fn an_entry_mukade_did_not_write_is_a_conflict_and_the_file_stays() {
    let root = scratch("conflict");
    let bus = bus_dir(&root);
    let file = root.join("mcp.json");
    let theirs = r#"{"mcpServers": {"mukade": {"url": "http://someone-else"}}}"#;
    std::fs::write(&file, theirs).unwrap();
    let target = json_target(file.clone(), 4000);

    let error = connect::connect(&target, &bus, "pi", false).unwrap_err();
    assert_eq!(error.code, "configuration_conflict");
    assert_eq!(error.exit_code(), 2);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), theirs);
    let error = connect::disconnect(&target, &bus, "pi", false).unwrap_err();
    assert_eq!(error.code, "configuration_conflict");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), theirs);
}

#[test]
fn an_entry_the_user_changed_since_is_left_alone() {
    let root = scratch("edited");
    let bus = bus_dir(&root);
    let file = root.join("mcp.json");
    let target = json_target(file.clone(), 4000);
    connect::connect(&target, &bus, "pi", false).unwrap();
    let mut edited = read_json(&file);
    edited["mcpServers"]["mukade"]["url"] = json!("http://127.0.0.1:1/mine");
    std::fs::write(&file, edited.to_string()).unwrap();

    assert_eq!(
        connect::disconnect(&target, &bus, "pi", false)
            .unwrap_err()
            .code,
        "configuration_conflict"
    );
    assert_eq!(
        connect::connect(&target, &bus, "pi", false)
            .unwrap_err()
            .code,
        "configuration_conflict"
    );
    assert_eq!(
        read_json(&file)["mcpServers"]["mukade"]["url"],
        "http://127.0.0.1:1/mine"
    );
}

#[test]
fn our_own_entry_follows_the_service_to_a_new_port() {
    let root = scratch("port");
    let bus = bus_dir(&root);
    let file = root.join("mcp.json");
    connect::connect(&json_target(file.clone(), 4000), &bus, "pi", false).unwrap();
    let moved = json_target(file.clone(), 4001);
    assert_eq!(
        connect::connect(&moved, &bus, "pi", false).unwrap()["status"],
        "connected"
    );
    assert_eq!(
        read_json(&file)["mcpServers"]["mukade"]["url"],
        "http://127.0.0.1:4001/mcp"
    );
    assert_eq!(
        connect::disconnect(&moved, &bus, "pi", false).unwrap()["status"],
        "disconnected"
    );
}

#[test]
fn toml_keeps_comments_and_other_tables() {
    let root = scratch("toml");
    let bus = bus_dir(&root);
    let file = root.join("config.toml");
    let before = "# my settings\nmodel = \"gpt\"  # the one I like\n\n[mcp_servers.other]\ncommand = \"x\"\n\n[features]\nplugins = false\n";
    std::fs::write(&file, before).unwrap();
    let target = toml_target(file.clone(), 4000);

    let report = connect::connect(&target, &bus, "codex", false).unwrap();
    assert_eq!(report["status"], "connected");
    assert!(!report.to_string().contains(TOKEN));
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("# my settings"));
    assert!(text.contains("# the one I like"));
    assert!(text.contains("[mcp_servers.other]"));
    assert!(text.contains("[mcp_servers.mukade]"));
    assert!(text.contains("plugins = false"));
    let parsed: toml_edit::DocumentMut = text.parse().unwrap();
    assert_eq!(
        parsed["mcp_servers"]["mukade"]["url"].as_str(),
        Some("http://127.0.0.1:4000/mcp")
    );
    assert_eq!(
        parsed["mcp_servers"]["mukade"]["http_headers"]["Authorization"].as_str(),
        Some(format!("Bearer {TOKEN}").as_str())
    );
    assert_eq!(
        parsed["mcp_servers"]["mukade"]["tool_timeout_sec"].as_integer(),
        Some(330)
    );

    assert_eq!(
        connect::connect(&target, &bus, "codex", false).unwrap()["status"],
        "unchanged"
    );
    assert_eq!(
        connect::disconnect(&target, &bus, "codex", false).unwrap()["status"],
        "disconnected"
    );
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        before,
        "back to what it was"
    );
}

#[test]
fn a_file_with_comments_in_json_is_refused_untouched() {
    let root = scratch("jsonc");
    let bus = bus_dir(&root);
    let file = root.join("opencode.json");
    let text = "{\n  // my comment\n  \"mcp\": {}\n}\n";
    std::fs::write(&file, text).unwrap();
    let mut target = json_target(file.clone(), 4000);
    target.tables = vec!["mcp", "servers"];
    let error = connect::connect(&target, &bus, "opencode", false).unwrap_err();
    assert_eq!(error.code, "configuration_failure");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), text);
}

#[test]
fn only_five_copies_of_one_harness_are_kept() {
    let root = scratch("rotate");
    let bus = bus_dir(&root);
    let file = root.join("mcp.json");
    std::fs::write(&file, "{}").unwrap();
    for port in 4000..4008 {
        connect::connect(&json_target(file.clone(), port), &bus, "pi", false).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(3));
    }
    assert_eq!(std::fs::read_dir(bus.join("backups")).unwrap().count(), 5);
}

#[test]
fn each_harness_names_its_own_file_and_tables() {
    let home = scratch("targets");
    let vars = mukade::catalog::Vars {
        home: Some(home.clone()),
        app_data: None,
        local_app_data: None,
        xdg_data_home: None,
    };
    // The harnesses' own variables would move these files: this test reads the defaults.
    for name in [
        "CLAUDE_CONFIG_DIR",
        "CODEX_HOME",
        "XDG_CONFIG_HOME",
        "PI_CODING_AGENT_DIR",
    ] {
        if std::env::var_os(name).is_some() {
            return;
        }
    }
    let server = server(4000);
    let expect = [
        (
            "claude_code",
            home.join(".claude.json"),
            vec!["mcpServers"],
            Format::Json,
        ),
        (
            "codex",
            home.join(".codex").join("config.toml"),
            vec!["mcp_servers"],
            Format::Toml,
        ),
        (
            "opencode",
            home.join(".config").join("opencode").join("opencode.json"),
            vec!["mcp", "servers"],
            Format::Json,
        ),
        (
            "pi",
            home.join(".pi").join("agent").join("mcp.json"),
            vec!["mcpServers"],
            Format::Json,
        ),
    ];
    for (name, path, tables, format) in expect {
        let harness: mukade::HarnessId = name.parse().unwrap();
        let target = connect::target_of(harness, &vars, &server).unwrap();
        assert_eq!(target.path, path, "{name}");
        assert_eq!(target.tables, tables, "{name}");
        assert_eq!(target.format, format, "{name}");
        assert!(
            target
                .entry
                .to_string()
                .contains("http://127.0.0.1:4000/mcp"),
            "{name}"
        );
        assert!(target.entry.to_string().contains(TOKEN), "{name}");
    }
}
