//! The harness adapter: hooks make an instance a room member and hand it what the room wrote.
//! Everything runs against an in-process service; no harness and no network.

#[path = "support/chat_http.rs"]
mod chat_http;

use mukade::bus::Bus;
use mukade::chat::adapter::{self, Delivery, Event, HookInput, Settings};
use mukade::chat::client::ChatClient;
use mukade::chat::{ChatService, ServiceConfig};
use mukade::roots::StoreRoots;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::Duration;

const HARNESS: &str = "claude_code";

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::var_os("MUKADE_TEST_SCRATCH_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!(
            "mukade-tests-adapter-{tag}-{}",
            uuid::Uuid::new_v4()
        ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

struct Fixture {
    service: ChatService,
    client: ChatClient,
    bus_dir: PathBuf,
    room: String,
}

async fn fixture(tag: &str) -> Fixture {
    let bus_dir = scratch(tag);
    let bus = Bus::open_in(&bus_dir).unwrap();
    let service_id = bus.service_id().unwrap();
    let service = ChatService::new(
        bus,
        StoreRoots::default(),
        ServiceConfig::new(
            bus_dir.canonicalize().unwrap(),
            service_id,
            uuid::Uuid::new_v4().to_string(),
            "http://127.0.0.1:39171/mcp".into(),
            "test-secret".into(),
        )
        .unwrap(),
    )
    .unwrap();
    let client = chat_http::client(&service).await;
    let room = client
        .call(
            "open",
            json!({"name": "room", "request_id": uuid::Uuid::new_v4().to_string()}),
        )
        .await
        .unwrap()["room_id"]
        .as_str()
        .unwrap()
        .to_string();
    Fixture {
        service,
        client,
        bus_dir,
        room,
    }
}

impl Fixture {
    fn settings(&self, role: &str, listen: u64) -> Settings {
        Settings {
            room_id: self.room.clone(),
            role: role.into(),
            listen: Duration::from_secs(listen),
            max_wakes: 10,
        }
    }

    async fn hook(&self, settings: &Settings, event: Event, session: &str) -> Option<Delivery> {
        let input = HookInput {
            session_id: session.into(),
            transcript_path: None,
        };
        adapter::handle(
            &self.client,
            &self.bus_dir,
            HARNESS,
            settings,
            event,
            &input,
        )
        .await
        .unwrap()
    }

    /// Another member, joined directly: its name and handle.
    async fn other(&self, role: &str) -> (String, String) {
        let joined = self
            .client
            .call(
                "join",
                json!({"room_id": self.room, "harness": "codex", "role": role,
                       "request_id": uuid::Uuid::new_v4().to_string()}),
            )
            .await
            .unwrap();
        (
            joined["name"].as_str().unwrap().into(),
            joined["member_handle"].as_str().unwrap().into(),
        )
    }

    async fn send(&self, handle: &str, text: &str) -> Value {
        self.client
            .call(
                "send",
                json!({"room_id": self.room, "member_handle": handle, "text": text,
                       "request_id": uuid::Uuid::new_v4().to_string()}),
            )
            .await
            .unwrap()
    }

    fn state(&self, session: &str) -> adapter::State {
        adapter::state_of(&self.bus_dir, HARNESS, session)
            .unwrap()
            .unwrap()
    }

    async fn unread(&self, session: &str) -> usize {
        let state = self.state(session);
        self.client
            .call(
                "peek",
                json!({"room_id": self.room, "member_handle": state.member_handle}),
            )
            .await
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["kind"] == "message" || e["kind"] == "signal")
            .count()
    }

    async fn done(self) {
        self.client.close().await;
        self.service.finish_shutdown().await;
    }
}

fn text(delivery: Option<Delivery>) -> (bool, String) {
    match delivery {
        Some(Delivery::Context(text)) => (false, text),
        Some(Delivery::Continue(text)) => (true, text),
        None => (false, String::new()),
    }
}

#[test]
fn settings_come_from_the_environment_the_launcher_set() {
    let vars = |pairs: &'static [(&'static str, &'static str)]| {
        move |name: &str| {
            pairs
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| v.to_string())
        }
    };
    assert_eq!(
        Settings::from_vars(vars(&[])).unwrap(),
        None,
        "no room: the hooks do nothing"
    );
    let s = Settings::from_vars(vars(&[("MUKADE_CHAT_ROOM_ID", "r1")]))
        .unwrap()
        .unwrap();
    assert_eq!(
        (s.role.as_str(), s.listen, s.max_wakes),
        ("peer", Duration::ZERO, 10)
    );
    let s = Settings::from_vars(vars(&[
        ("MUKADE_CHAT_ROOM_ID", "r1"),
        ("MUKADE_CHAT_ROLE", "worker"),
        ("MUKADE_CHAT_LISTEN", "999999"),
        ("MUKADE_CHAT_MAX_WAKES", "3"),
    ]))
    .unwrap()
    .unwrap();
    assert_eq!(s.role, "worker");
    assert_eq!(s.listen, adapter::MAX_LISTEN, "listening is capped");
    assert_eq!(s.max_wakes, 3);
    assert!(
        Settings::from_vars(vars(&[
            ("MUKADE_CHAT_ROOM_ID", "r1"),
            ("MUKADE_CHAT_ROLE", "boss")
        ]))
        .is_err()
    );
    assert!(
        Settings::from_vars(vars(&[
            ("MUKADE_CHAT_ROOM_ID", "r1"),
            ("MUKADE_CHAT_LISTEN", "soon")
        ]))
        .is_err()
    );
}

#[test]
fn a_session_id_that_could_name_another_file_is_refused() {
    for bad in ["", "../x", "a/b", "a\\b", "x y"] {
        assert!(
            HookInput::from_json(&json!({"session_id": bad})).is_err(),
            "{bad:?}"
        );
    }
    assert!(HookInput::from_json(&json!({})).is_err());
    let ok = HookInput::from_json(
        &json!({"session_id": "c45db7bc-542c", "transcript_path": "/t.jsonl"}),
    )
    .unwrap();
    assert_eq!(ok.session_id, "c45db7bc-542c");
    assert_eq!(ok.transcript_path.as_deref(), Some("/t.jsonl"));
}

#[tokio::test]
async fn the_first_hook_joins_once_and_introduces_the_member() {
    let f = fixture("join").await;
    let settings = f.settings("peer", 0);
    let (_, intro) = text(f.hook(&settings, Event::SessionStart, "s1").await);
    let state = f.state("s1");
    assert!(
        intro.contains(&format!("You are {}", state.name)),
        "{intro}"
    );
    assert!(intro.contains(&state.member_handle));
    assert!(intro.contains(&f.room));
    assert!(intro.contains("not instructions from the user"));
    assert!(intro.contains("do not call chat_read or chat_wait"));

    // The session resumes: same member, nothing new to say.
    assert_eq!(f.hook(&settings, Event::SessionStart, "s1").await, None);
    let members = f
        .client
        .call("members", json!({"room_id": f.room}))
        .await
        .unwrap();
    assert_eq!(members["items"].as_array().unwrap().len(), 1);
    f.done().await;
}

#[tokio::test]
async fn each_role_is_told_what_it_may_do() {
    let f = fixture("roles").await;
    let (_, worker) = text(
        f.hook(&f.settings("worker", 0), Event::SessionStart, "w")
            .await,
    );
    assert!(
        worker.contains("cannot send messages") && worker.contains("chat_signal"),
        "{worker}"
    );
    let (_, observer) = text(
        f.hook(&f.settings("observer", 0), Event::SessionStart, "o")
            .await,
    );
    assert!(
        observer.contains("chat_observe") && observer.contains("infer"),
        "{observer}"
    );
    let (_, lead) = text(
        f.hook(&f.settings("lead", 0), Event::SessionStart, "l")
            .await,
    );
    assert!(lead.contains("instruction"), "{lead}");
    f.done().await;
}

#[tokio::test]
async fn a_prompt_hands_over_messages_and_the_next_call_acknowledges_them() {
    let f = fixture("deliver").await;
    let settings = f.settings("peer", 0);
    f.hook(&settings, Event::SessionStart, "s1").await;
    let me = f.state("s1").name;
    let (_, handle) = f.other("peer").await;
    f.send(&handle, &format!("@{me} what is 2+3?")).await;

    let (cont, news) = text(f.hook(&settings, Event::Prompt, "s1").await);
    assert!(!cont);
    assert!(news.contains("(to you): @"), "{news}");
    assert!(news.contains("what is 2+3?"), "{news}");
    assert_eq!(f.unread("s1").await, 1, "handed over, not yet acknowledged");

    assert_eq!(f.hook(&settings, Event::Prompt, "s1").await, None);
    assert_eq!(f.unread("s1").await, 0, "the next call acknowledged it");
    f.done().await;
}

#[tokio::test]
async fn a_batch_the_harness_did_not_take_is_handed_over_again() {
    let f = fixture("again").await;
    let settings = f.settings("peer", 0);
    f.hook(&settings, Event::SessionStart, "s1").await;
    let (_, handle) = f.other("peer").await;
    f.send(&handle, "hello room").await;
    let (_, first) = text(f.hook(&settings, Event::Prompt, "s1").await);
    assert!(first.contains("hello room"));

    adapter::forget_delivery(&f.bus_dir, HARNESS, "s1").unwrap();
    let (_, again) = text(f.hook(&settings, Event::Prompt, "s1").await);
    assert!(again.contains("hello room"), "{again}");
    f.done().await;
}

#[tokio::test]
async fn without_listening_an_instance_stops_and_its_messages_wait() {
    let f = fixture("nolisten").await;
    let settings = f.settings("peer", 0);
    f.hook(&settings, Event::SessionStart, "s1").await;
    let me = f.state("s1").name;
    let (_, handle) = f.other("peer").await;
    f.send(&handle, &format!("@{me} later")).await;
    assert_eq!(f.hook(&settings, Event::Stop, "s1").await, None);
    let (_, news) = text(f.hook(&settings, Event::Prompt, "s1").await);
    assert!(news.contains("later"), "{news}");
    f.done().await;
}

#[tokio::test]
async fn a_listening_instance_goes_on_when_a_message_for_it_arrives() {
    let f = fixture("listen").await;
    let settings = f.settings("peer", 20);
    f.hook(&settings, Event::SessionStart, "s1").await;
    let me = f.state("s1").name;
    let (_, handle) = f.other("peer").await;
    // A message to the whole room does not wake it; one to it does.
    f.send(&handle, "general note").await;
    f.hook(&settings, Event::Prompt, "s1").await;
    let sender = async {
        tokio::time::sleep(Duration::from_millis(400)).await;
        f.send(&handle, "nobody in particular").await;
        tokio::time::sleep(Duration::from_millis(400)).await;
        f.send(&handle, &format!("@{me} ping")).await;
    };
    let (delivery, _) = tokio::join!(f.hook(&settings, Event::Stop, "s1"), sender);
    let (cont, news) = text(delivery);
    assert!(cont, "the instance goes on");
    assert!(
        news.contains("ping") && news.contains("nobody in particular"),
        "{news}"
    );
    assert_eq!(f.state("s1").wakes, 1);
    f.done().await;
}

#[tokio::test]
async fn listening_ends_after_its_time_and_after_its_wake_ups() {
    let f = fixture("limits").await;
    let mut settings = f.settings("peer", 1);
    settings.max_wakes = 1;
    f.hook(&settings, Event::SessionStart, "s1").await;
    let me = f.state("s1").name;
    let started = std::time::Instant::now();
    assert_eq!(
        f.hook(&settings, Event::Stop, "s1").await,
        None,
        "nothing came"
    );
    assert!(started.elapsed() >= Duration::from_millis(900));

    let (_, handle) = f.other("peer").await;
    f.send(&handle, &format!("@{me} one")).await;
    assert!(
        text(f.hook(&settings, Event::Stop, "s1").await).0,
        "first wake-up"
    );
    f.send(&handle, &format!("@{me} two")).await;
    assert_eq!(
        f.hook(&settings, Event::Stop, "s1").await,
        None,
        "no wake-up left"
    );
    let (_, news) = text(f.hook(&settings, Event::Prompt, "s1").await);
    assert!(
        news.contains("two"),
        "the message waits for the next prompt: {news}"
    );
    f.done().await;
}

#[tokio::test]
async fn a_session_launched_into_another_room_joins_that_room() {
    let f = fixture("move").await;
    f.hook(&f.settings("peer", 0), Event::SessionStart, "s1")
        .await;
    let other = f
        .client
        .call(
            "open",
            json!({"name": "other", "request_id": uuid::Uuid::new_v4().to_string()}),
        )
        .await
        .unwrap()["room_id"]
        .as_str()
        .unwrap()
        .to_string();
    let settings = Settings {
        room_id: other.clone(),
        ..f.settings("peer", 0)
    };
    let (_, intro) = text(f.hook(&settings, Event::SessionStart, "s1").await);
    assert!(intro.contains(&other));
    assert_eq!(f.state("s1").room_id, other);
    f.done().await;
}

#[tokio::test]
async fn a_worker_signal_reaches_its_lead_as_one_line() {
    let f = fixture("signal").await;
    let settings = f.settings("lead", 0);
    f.hook(&settings, Event::SessionStart, "lead").await;
    let lead = f.state("lead");
    let (worker, handle) = f.other("worker").await;
    let instruction = f
        .send(&lead.member_handle, &format!("@{worker} build it"))
        .await;
    let event = instruction["event_id"].as_str().unwrap().to_string();
    f.client
        .call(
            "signal",
            json!({"room_id": f.room, "member_handle": handle, "instruction_event_id": event,
                   "signal": "blocked", "reason": "no compiler",
                   "request_id": uuid::Uuid::new_v4().to_string()}),
        )
        .await
        .unwrap();
    let (_, news) = text(f.hook(&settings, Event::Prompt, "lead").await);
    assert!(
        news.contains(&format!(
            "{worker} signalled blocked on #{event}: no compiler"
        )),
        "{news}"
    );
    f.done().await;
}

#[test]
fn claude_code_reads_the_hook_answers_in_its_own_format() {
    let claude = mukade::harnesses::get(mukade::HarnessId::ClaudeCode);
    let context: Value = serde_json::from_str(
        &claude
            .chat_hook_output(Event::Prompt, &Delivery::Context("hi".into()))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        context["hookSpecificOutput"]["hookEventName"],
        "UserPromptSubmit"
    );
    assert_eq!(context["hookSpecificOutput"]["additionalContext"], "hi");
    let start: Value = serde_json::from_str(
        &claude
            .chat_hook_output(Event::SessionStart, &Delivery::Context("hi".into()))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(start["hookSpecificOutput"]["hookEventName"], "SessionStart");
    let stop: Value = serde_json::from_str(
        &claude
            .chat_hook_output(Event::Stop, &Delivery::Continue("go on".into()))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(stop, json!({"decision": "block", "reason": "go on"}));
    let codex = mukade::harnesses::get(mukade::HarnessId::Codex)
        .chat_hook_output(Event::Stop, &Delivery::Continue("go on".into()))
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&codex).unwrap(),
        stop,
        "Codex reads the same"
    );
    for other in [mukade::HarnessId::OpenCode, mukade::HarnessId::Pi] {
        assert!(
            mukade::harnesses::get(other)
                .chat_hook_output(Event::Stop, &Delivery::Continue("x".into()))
                .is_none(),
            "{other} has no adapter yet"
        );
    }
}

#[test]
fn the_claude_code_kit_is_a_plugin_whose_hooks_call_mukade() {
    let folder = scratch("kit").join("launch");
    let kit = adapter::Kit {
        exe: PathBuf::from(r"C:\Program Files\mukade\mukade.exe"),
        folder: folder.clone(),
        server: mukade::chat::connect::Server {
            endpoint: "http://127.0.0.1:4000/mcp".into(),
            token: "t0k3n".into(),
            service_id: "svc".into(),
        },
        stop_timeout: 330,
    };
    let command = mukade::harnesses::get(mukade::HarnessId::ClaudeCode)
        .chat_launch(&kit)
        .unwrap()
        .unwrap();
    let args: Vec<String> = command
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        args,
        [
            "--plugin-dir".to_string(),
            folder.to_string_lossy().into_owned()
        ]
    );
    let read = |p: &str| -> Value {
        serde_json::from_str(&std::fs::read_to_string(folder.join(p)).unwrap()).unwrap()
    };
    assert_eq!(read(".claude-plugin/plugin.json")["name"], "mukade");
    let hooks = read("hooks/hooks.json");
    let stop = &hooks["hooks"]["Stop"][0]["hooks"][0];
    assert_eq!(
        stop["command"],
        r#""C:/Program Files/mukade/mukade.exe" chat hook claude_code stop"#
    );
    assert_eq!(stop["timeout"], 330);
    assert!(
        hooks["hooks"]["SessionStart"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .ends_with("session-start")
    );
    assert!(
        hooks["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .ends_with(" prompt")
    );
    let mcp = read(".mcp.json");
    assert_eq!(
        mcp["mcpServers"]["mukade"]["url"],
        "http://127.0.0.1:4000/mcp"
    );
    assert_eq!(
        mcp["mcpServers"]["mukade"]["headers"]["Authorization"],
        "Bearer t0k3n"
    );
    mukade::chat::security::verify_file(&folder.join(".mcp.json")).unwrap();
}

/// What a member wrote itself is never handed back to it, and does not wake it.
#[tokio::test]
async fn a_member_is_not_handed_its_own_messages() {
    let f = fixture("own").await;
    let settings = f.settings("peer", 1);
    f.hook(&settings, Event::SessionStart, "s1").await;
    let me = f.state("s1");
    f.send(&me.member_handle, "hello from me").await;
    assert_eq!(f.hook(&settings, Event::Prompt, "s1").await, None);
    assert_eq!(
        f.hook(&settings, Event::Stop, "s1").await,
        None,
        "its own message does not wake it"
    );
    assert_eq!(f.state("s1").wakes, 0);
    f.done().await;
}

/// Someone joining is news at the next prompt, but no reason to wake a listening instance.
#[tokio::test]
async fn a_join_alone_does_not_wake_a_listening_instance() {
    let f = fixture("joinwake").await;
    let settings = f.settings("peer", 1);
    f.hook(&settings, Event::SessionStart, "s1").await;
    let (other, _) = f.other("peer").await;
    assert_eq!(f.hook(&settings, Event::Stop, "s1").await, None);
    assert_eq!(f.state("s1").wakes, 0);
    let (_, news) = text(f.hook(&settings, Event::Prompt, "s1").await);
    assert!(
        news.contains(&format!("{other} joined")) || news.is_empty(),
        "{news}"
    );
    f.done().await;
}

/// A message to the whole room is no reason to restart a listening instance: it waits, unread,
/// for a message addressed to the instance or for the next prompt.
#[tokio::test]
async fn a_message_to_the_room_does_not_wake_a_listening_instance() {
    let f = fixture("roomwide").await;
    let settings = f.settings("peer", 1);
    f.hook(&settings, Event::SessionStart, "s1").await;
    let (_, handle) = f.other("peer").await;
    f.send(&handle, "hello everybody").await;
    assert_eq!(f.hook(&settings, Event::Stop, "s1").await, None);
    assert_eq!(f.state("s1").wakes, 0);
    let (_, news) = text(f.hook(&settings, Event::Prompt, "s1").await);
    assert!(news.contains("hello everybody"), "not lost: {news}");
    f.done().await;
}

#[test]
fn the_codex_launch_is_configuration_overrides_and_keeps_the_credential_off_the_command_line() {
    let kit = adapter::Kit {
        exe: PathBuf::from(r"C:\Program Files\it's\mukade.exe"),
        folder: scratch("codex-kit").join("launch"),
        server: mukade::chat::connect::Server {
            endpoint: "http://127.0.0.1:4000/mcp".into(),
            token: "s3cr3t-t0k3n".into(),
            service_id: "svc".into(),
        },
        stop_timeout: adapter::STOP_HOOK_TIMEOUT,
    };
    let command = mukade::harnesses::get(mukade::HarnessId::Codex)
        .chat_launch(&kit)
        .unwrap()
        .unwrap();
    let args: Vec<String> = command
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert!(args.iter().all(|a| !a.contains("s3cr3t")), "{args:?}");
    assert!(args.iter().step_by(2).all(|a| a == "-c"), "{args:?}");
    let overrides: Vec<&String> = args.iter().skip(1).step_by(2).collect();
    let get = |key: &str| {
        overrides
            .iter()
            .find_map(|o| o.strip_prefix(&format!("{key}=")))
            .unwrap_or_else(|| panic!("no {key} in {overrides:?}"))
            .to_string()
    };
    assert_eq!(
        get("mcp_servers.mukade.url"),
        "\"http://127.0.0.1:4000/mcp\""
    );
    assert!(get("mcp_servers.mukade.env_http_headers").contains("MUKADE_CHAT_AUTHORIZATION"));
    let stop: toml_edit::Value = get("hooks.Stop").parse().unwrap();
    let group = stop
        .as_array()
        .unwrap()
        .get(0)
        .unwrap()
        .as_inline_table()
        .unwrap();
    let hook = group["hooks"].as_array().unwrap().get(0).unwrap();
    let hook = hook.as_inline_table().unwrap();
    let line = hook["command"].as_str().unwrap();
    if cfg!(windows) {
        assert_eq!(
            line,
            "& 'C:/Program Files/it''s/mukade.exe' chat hook codex stop"
        );
    } else {
        assert!(line.ends_with(" chat hook codex stop"), "{line}");
    }
    assert_eq!(
        hook["timeout"].as_integer(),
        Some(adapter::STOP_HOOK_TIMEOUT as i64)
    );
    let envs: Vec<(String, String)> = command
        .get_envs()
        .filter_map(|(k, v)| {
            Some((
                k.to_string_lossy().into_owned(),
                v?.to_string_lossy().into_owned(),
            ))
        })
        .collect();
    assert!(envs.contains(&(
        "MUKADE_CHAT_AUTHORIZATION".into(),
        "Bearer s3cr3t-t0k3n".into()
    )));
    assert!(envs.contains(&("MUKADE_CHAT_SERVICE_ID".into(), "svc".into())));
}
