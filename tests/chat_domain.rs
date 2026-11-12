mod support;

use mukade::{HarnessId, bus::Bus, roots::StoreRoots};
use serde_json::{Value, json};
use uuid::Uuid;

fn mutate(bus: &mut Bus, operation: &str, mut args: Value) -> Value {
    args["request_id"] = json!(Uuid::new_v4().to_string());
    bus.chat_call(operation, args, &StoreRoots::default())
        .unwrap()
}

fn setup(tag: &str) -> Bus {
    Bus::open_in(&support::scratch(HarnessId::Pi, tag).join("bus")).unwrap()
}

#[test]
fn send_replays_after_close_and_leave_replays_with_revoked_handle() {
    let mut bus = setup("chat-idempotency");
    let room = mutate(&mut bus, "open", json!({"name":"room"}));
    let room_id = &room["room_id"];
    let member = mutate(
        &mut bus,
        "join",
        json!({"room_id":room_id,"harness":"pi","role":"peer"}),
    );
    let handle = &member["member_handle"];
    let roots = StoreRoots::default();
    let send = json!({"room_id":room_id,"member_handle":handle,"text":"hello","request_id":Uuid::new_v4().to_string()});
    let message = bus.chat_call("send", send.clone(), &roots).unwrap();
    mutate(&mut bus, "close", json!({"room_id":room_id}));
    assert_eq!(bus.chat_call("send", send, &roots).unwrap(), message);
    let leave =
        json!({"room_id":room_id,"member_handle":handle,"request_id":Uuid::new_v4().to_string()});
    let response = bus.chat_call("leave", leave.clone(), &roots).unwrap();
    assert_eq!(bus.chat_call("leave", leave, &roots).unwrap(), response);
    assert_eq!(
        bus.chat_call(
            "read",
            json!({"room_id":room_id,"member_handle":handle}),
            &roots
        )
        .unwrap_err()
        .code,
        "stale_member"
    );
}

#[test]
fn addressed_workers_have_independent_signal_state() {
    let mut bus = setup("chat-workers");
    let room = mutate(&mut bus, "open", json!({"name":"room"}));
    let room_id = &room["room_id"];
    let lead = mutate(
        &mut bus,
        "join",
        json!({"room_id":room_id,"harness":"pi","role":"lead","name":"lead"}),
    );
    let a = mutate(
        &mut bus,
        "join",
        json!({"room_id":room_id,"harness":"pi","role":"worker","name":"worker-a"}),
    );
    let b = mutate(
        &mut bus,
        "join",
        json!({"room_id":room_id,"harness":"pi","role":"worker","name":"worker-b"}),
    );
    let message = mutate(
        &mut bus,
        "send",
        json!({"room_id":room_id,"member_handle":lead["member_handle"],"text":"@worker-a and @worker-b please check"}),
    );
    let signal = |bus: &mut Bus, worker: &Value, signal: &str, reason: Value| {
        mutate(
            bus,
            "signal",
            json!({"room_id":room_id,"member_handle":worker["member_handle"],"instruction_event_id":message["event_id"],"signal":signal,"reason":reason}),
        )
    };
    let done = signal(&mut bus, &a, "done", Value::Null);
    let blocked = signal(&mut bus, &b, "blocked", json!("awaiting input"));
    assert_ne!(done["event_id"], blocked["event_id"]);
    assert_eq!(done, signal(&mut bus, &a, "done", Value::Null));
    let latest_signals = |bus: &mut Bus| {
        let page = call(bus, "members", json!({"room_id":room_id}));
        let summary = |name| {
            page["items"]
                .as_array()
                .unwrap()
                .iter()
                .find(|member| member["name"] == name)
                .unwrap()
                .as_object()
                .unwrap()
                .get("last_signal")
                .unwrap()
                .clone()
        };
        assert_eq!(summary("lead"), Value::Null);
        (summary("worker-a"), summary("worker-b"))
    };
    let (a_before, b_before) = latest_signals(&mut bus);
    assert_eq!(
        a_before,
        json!({"instruction_event_id":message["event_id"],"event_id":done["event_id"],"state":"done","reason":null})
    );
    assert_eq!(
        b_before,
        json!({"instruction_event_id":message["event_id"],"event_id":blocked["event_id"],"state":"blocked","reason":"awaiting input"})
    );
    let b_done = signal(&mut bus, &b, "done", Value::Null);
    let (a_after, b_after) = latest_signals(&mut bus);
    assert_eq!(a_after, a_before);
    assert_eq!(
        b_after,
        json!({"instruction_event_id":message["event_id"],"event_id":b_done["event_id"],"state":"done","reason":null})
    );
    let waiting = bus
        .chat_call(
            "wait",
            json!({"room_id":room_id,"member_handle":lead["member_handle"],"addressed_only":true}),
            &StoreRoots::default(),
        )
        .unwrap();
    assert_eq!(waiting["trigger_event_id"], done["event_id"]);
    let error = bus.chat_call("send",json!({"room_id":room_id,"member_handle":a["member_handle"],"request_id":Uuid::new_v4().to_string(),"text":"refuse"}),&StoreRoots::default()).unwrap_err();
    assert_eq!(error.code, "role_refused");
}

fn join(bus: &mut Bus, room_id: &Value, role: &str, name: &str) -> Value {
    mutate(
        bus,
        "join",
        json!({"room_id":room_id,"harness":"pi","role":role,"name":name}),
    )
}

fn call(bus: &mut Bus, op: &str, args: Value) -> Value {
    bus.chat_call(op, args, &StoreRoots::default()).unwrap()
}

#[test]
fn pending_first_wait_and_idempotent_ack_survive_restart_and_rejoin() {
    let root = support::scratch(HarnessId::Pi, "chat-batches").join("bus");
    let mut bus = Bus::open_in(&root).unwrap();
    let service = bus.service_id().unwrap();
    let room = mutate(&mut bus, "open", json!({"name":"batches"}));
    let room_id = &room["room_id"];
    let peer = join(&mut bus, room_id, "peer", "peer");
    let observer = join(&mut bus, room_id, "observer", "observer");
    let old_handle = &observer["member_handle"];
    let stream = bus
        .chat_stream_key(
            "wait",
            &json!({"room_id":room_id,"member_handle":old_handle}),
        )
        .unwrap();
    let first = call(
        &mut bus,
        "read",
        json!({"room_id":room_id,"member_handle":old_handle,"budget_chars":1}),
    );
    assert_eq!(first["items"].as_array().unwrap().len(), 1);
    assert_eq!(first["items"][0]["kind"], "open");

    drop(bus);
    let mut bus = Bus::open_in(&root).unwrap();
    assert_eq!(bus.service_id().unwrap(), service);
    assert_eq!(
        call(
            &mut bus,
            "wait",
            json!({"room_id":room_id,"member_handle":old_handle,"addressed_only":true})
        ),
        first
    );

    let rejoined = mutate(
        &mut bus,
        "rejoin",
        json!({"room_id":room_id,"member_id":observer["member_id"]}),
    );
    let handle = &rejoined["member_handle"];
    assert_ne!(handle, old_handle);
    assert_eq!(rejoined["member_id"], observer["member_id"]);
    assert_eq!(rejoined["role"], "observer");
    assert_eq!(
        bus.chat_stream_key("wait", &json!({"room_id":room_id,"member_handle":handle}))
            .unwrap(),
        stream
    );
    assert_eq!(
        bus.chat_call(
            "ack",
            json!({"room_id":room_id,"member_handle":old_handle,"batch_id":first["batch_id"]}),
            &StoreRoots::default()
        )
        .unwrap_err()
        .code,
        "stale_member"
    );
    assert_eq!(
        call(
            &mut bus,
            "read",
            json!({"room_id":room_id,"member_handle":handle})
        ),
        first
    );
    let ack_a = json!({"room_id":room_id,"member_handle":handle,"batch_id":first["batch_id"]});
    let a = call(&mut bus, "ack", ack_a.clone());
    let b = call(
        &mut bus,
        "read",
        json!({"room_id":room_id,"member_handle":handle,"budget_chars":1}),
    );
    assert_ne!(b["batch_id"], first["batch_id"]);
    assert_eq!(call(&mut bus, "ack", ack_a), a);
    assert_eq!(
        call(
            &mut bus,
            "read",
            json!({"room_id":room_id,"member_handle":handle})
        ),
        b
    );
    let waiting = call(
        &mut bus,
        "wait",
        json!({"room_id":room_id,"member_handle":handle,"ack_batch_id":b["batch_id"],"addressed_only":true}),
    );
    assert_eq!(waiting["status"], "waiting");
    assert_eq!(waiting["ack_batch_id"], b["batch_id"]);
    let message = mutate(
        &mut bus,
        "send",
        json!({"room_id":room_id,"member_handle":peer["member_handle"],"text":"@observer update"}),
    );
    let trigger = call(
        &mut bus,
        "wait",
        json!({"room_id":room_id,"member_handle":handle,"addressed_only":true,"budget_chars":1}),
    );
    assert_eq!(trigger["trigger_event_id"], message["event_id"]);
    assert_eq!(trigger["items"][0]["kind"], "join");
}

#[test]
fn deletion_checks_revision_revokes_export_and_fences_old_request_ids() {
    let mut bus = setup("chat-delete");
    let roots = StoreRoots::default();
    let open = json!({"name":"project","request_id":Uuid::new_v4().to_string()});
    let room = call(&mut bus, "open", open.clone());
    let room_id = &room["room_id"];
    let join = json!({"room_id":room_id,"harness":"pi","role":"peer","request_id":Uuid::new_v4().to_string()});
    let member = call(&mut bus, "join", join.clone());
    let handle = &member["member_handle"];
    let send = json!({"room_id":room_id,"member_handle":handle,"text":"history","request_id":Uuid::new_v4().to_string()});
    call(&mut bus, "send", send.clone());
    let export = call(&mut bus, "export", json!({"room_id":room_id,"limit":1}));
    mutate(
        &mut bus,
        "join",
        json!({"room_id":room_id,"harness":"pi","role":"worker"}),
    );
    let closed = mutate(&mut bus, "close", json!({"room_id":room_id}));
    let leave =
        json!({"room_id":room_id,"member_handle":handle,"request_id":Uuid::new_v4().to_string()});
    call(&mut bus, "leave", leave.clone());
    let mut cursor = export["next_cursor"].clone();
    let mut items = export["items"].as_array().unwrap().clone();
    while !cursor.is_null() {
        let page = call(
            &mut bus,
            "export",
            json!({"room_id":room_id,"limit":1,"cursor":cursor}),
        );
        assert_eq!(page["high_water"], export["high_water"]);
        assert_eq!(page["snapshot_id"], export["snapshot_id"]);
        items.extend(page["items"].as_array().unwrap().iter().cloned());
        cursor = page["next_cursor"].clone();
    }
    assert_eq!(
        items.iter().filter(|row| row["type"] == "member").count(),
        1
    );
    assert!(
        items
            .iter()
            .all(|row| row["event"]["kind"] != "close" && row["event"]["kind"] != "leave")
    );
    let delete = json!({"room_id":room_id,"confirm":true,"expected_revision":closed["revision"],"request_id":Uuid::new_v4().to_string()});
    assert_eq!(
        bus.chat_call("delete", delete, &roots).unwrap_err().code,
        "revision_conflict"
    );
    let rooms = call(&mut bus, "rooms", json!({}));
    let delete = json!({"room_id":room_id,"confirm":true,"expected_revision":rooms["items"][0]["revision"],"request_id":Uuid::new_v4().to_string()});
    let deleted = call(&mut bus, "delete", delete.clone());
    assert_eq!(call(&mut bus, "delete", delete), deleted);
    let replacement = mutate(&mut bus, "open", json!({"name":"project"}));
    assert_ne!(replacement["room_id"], *room_id);
    for (op, args) in [
        ("open", open),
        ("join", join),
        ("send", send),
        ("leave", leave),
        (
            "export",
            json!({"room_id":room_id,"cursor":export["next_cursor"]}),
        ),
    ] {
        assert_eq!(
            bus.chat_call(op, args, &roots).unwrap_err().code,
            "resource_deleted",
            "{op}"
        );
    }
}

#[test]
fn binding_requires_exact_ids_and_can_be_revoked_in_a_closed_room() {
    let root = support::scratch(HarnessId::Pi, "chat-binding");
    let prefix = support::stage_fixture(HarnessId::Pi, &root);
    let native_id = support::source_id(HarnessId::Pi);
    let roots = support::roots(&root);
    let mut bus = Bus::open_in(&root.join("bus")).unwrap();
    let room = mutate(&mut bus, "open", json!({"name":"binding"}));
    let room_id = &room["room_id"];
    let member = join(&mut bus, room_id, "peer", "peer");
    let other = join(&mut bus, room_id, "peer", "other");
    let observer = join(&mut bus, room_id, "observer", "observer");
    let candidate = bus
        .chat_call("candidates", json!({"harness":"pi"}), &roots)
        .unwrap();
    let source = &candidate["items"][0]["source_ref"];
    let bind = |handle: &Value, id: &str, source: &Value| json!({"room_id":room_id,"member_handle":handle,"harness":"pi","session_id":id,"source_ref":source,"request_id":Uuid::new_v4().to_string()});
    assert_eq!(
        bus.chat_call(
            "bind",
            bind(&member["member_handle"], &prefix, source),
            &roots
        )
        .unwrap_err()
        .code,
        "session_not_found"
    );
    assert_eq!(
        bus.chat_call(
            "bind",
            bind(&member["member_handle"], native_id, &json!("not-a-source")),
            &roots
        )
        .unwrap_err()
        .code,
        "session_not_found"
    );
    let bind_request = bind(&member["member_handle"], native_id, source);
    let bound = bus.chat_call("bind", bind_request.clone(), &roots).unwrap();
    assert_eq!(bound["session"]["id"], native_id);
    assert_eq!(
        bus.chat_call(
            "bind",
            bind(&other["member_handle"], native_id, source),
            &roots
        )
        .unwrap_err()
        .code,
        "binding_conflict"
    );
    let args = json!({"room_id":room_id,"member_handle":observer["member_handle"],"target_member_id":member["member_id"]});
    let before = bus.chat_stream_key("observe", &args).unwrap().unwrap();
    mutate(&mut bus, "close", json!({"room_id":room_id}));
    assert_eq!(bus.chat_call("bind", bind_request, &roots).unwrap(), bound);
    let unbound = mutate(
        &mut bus,
        "unbind",
        json!({"room_id":room_id,"member_handle":member["member_handle"]}),
    );
    assert_eq!(
        unbound["binding_generation"],
        bound["binding_generation"].as_i64().unwrap() + 1
    );
    assert_eq!(
        bus.chat_stream_key("observe", &args).unwrap_err().code,
        "target_unbound"
    );
    mutate(&mut bus, "reopen", json!({"room_id":room_id}));
    bus.chat_call(
        "bind",
        bind(&member["member_handle"], native_id, source),
        &roots,
    )
    .unwrap();
    assert_ne!(
        bus.chat_stream_key("observe", &args).unwrap().unwrap(),
        before
    );
}

#[test]
fn mentions_use_token_boundaries_and_all_expands_at_send_time() {
    let mut bus = setup("chat-mentions");
    let room = mutate(&mut bus, "open", json!({"name":"mentions"}));
    let id = &room["room_id"];
    let lead = join(&mut bus, id, "lead", "lead");
    let worker = join(&mut bus, id, "worker", "worker");
    let departed = join(&mut bus, id, "worker", "departed");
    mutate(
        &mut bus,
        "leave",
        json!({"room_id":id,"member_handle":departed["member_handle"]}),
    );
    let msg = mutate(
        &mut bus,
        "send",
        json!({"room_id":id,"member_handle":lead["member_handle"],"text":"test@worker.com @worker/package @unknown (@departed)."}),
    );
    assert_eq!(msg["event"]["payload"]["mentions"], json!(["departed"]));
    assert_eq!(
        msg["event"]["payload"]["historical_mentions"],
        json!(["departed"])
    );
    assert_eq!(msg["event"]["recipient_ids"], json!([]));
    let msg = mutate(
        &mut bus,
        "send",
        json!({"room_id":id,"member_handle":lead["member_handle"],"text":"@all start"}),
    );
    let late = join(&mut bus, id, "worker", "late");
    let ids = msg["event"]["recipient_ids"].as_array().unwrap();
    assert!(ids.contains(&worker["member_id"]));
    assert!(!ids.contains(&late["member_id"]));
    let error = bus.chat_call("send",json!({"room_id":id,"member_handle":lead["member_handle"],"text":"bad recipient","recipients":["departed"],"request_id":Uuid::new_v4().to_string()}),&StoreRoots::default()).unwrap_err();
    assert_eq!(error.code, "recipient_not_found");
}

#[test]
fn validates_secret_unicode_and_unknown_arguments_before_any_event() {
    let mut bus = setup("chat-validation");
    let room = mutate(&mut bus, "open", json!({"name":"validation"}));
    let id = &room["room_id"];
    let peer = join(&mut bus, id, "peer", "peer");
    let request = |text: String| json!({"room_id":id,"member_handle":peer["member_handle"],"text":text,"request_id":Uuid::new_v4().to_string()});
    let roots = StoreRoots::default();
    assert_eq!(
        bus.chat_call("send", request("é".repeat(32_769)), &roots)
            .unwrap_err()
            .code,
        "invalid_arguments"
    );
    let secret = "Authorization: Bearer tok_12abcdefghijklmnopqrstuvwxyz1234567890";
    let error = bus
        .chat_call("send", request(format!("ordinary text\n{secret}")), &roots)
        .unwrap_err();
    assert_eq!(error.code, "secret_refused");
    assert!(error.message.contains("bearer token"));
    assert!(error.message.contains("line 2"));
    assert!(!error.message.contains(secret));
    assert!(!error.message.contains("tok_12"));
    let mut args = request("valid content".to_owned());
    args["unknown"] = json!(true);
    assert_eq!(
        bus.chat_call("send", args, &roots).unwrap_err().code,
        "invalid_arguments"
    );
    assert_eq!(
        call(&mut bus, "ledger", json!({"room_id":id,"kind":"message"}))["items"],
        json!([])
    );
}

#[test]
fn superseded_join_retries_do_not_restore_old_handles() {
    let mut bus = setup("chat-stale-join");
    let room = mutate(&mut bus, "open", json!({"name":"recover"}));
    let id = &room["room_id"];
    let join = json!({"room_id":id,"harness":"pi","role":"observer","request_id":Uuid::new_v4().to_string()});
    let member = call(&mut bus, "join", join.clone());
    assert_eq!(
        call(&mut bus, "join", join.clone())["member_handle"],
        member["member_handle"]
    );
    mutate(
        &mut bus,
        "rejoin",
        json!({"room_id":id,"member_id":member["member_id"]}),
    );
    assert_eq!(
        bus.chat_call("join", join, &StoreRoots::default())
            .unwrap_err()
            .code,
        "stale_member"
    );
}

#[test]
fn budget_measures_structured_unicode_and_keeps_oversized_events_whole() {
    let mut bus = setup("chat-page-budget");
    let room = mutate(&mut bus, "open", json!({"name":"budget"}));
    let id = &room["room_id"];
    let peer = join(&mut bus, id, "peer", "peer");
    let handle = &peer["member_handle"];
    let first = call(
        &mut bus,
        "read",
        json!({"room_id":id,"member_handle":handle}),
    );
    call(
        &mut bus,
        "ack",
        json!({"room_id":id,"member_handle":handle,"batch_id":first["batch_id"]}),
    );
    let text = "🔥".repeat(1000);
    mutate(
        &mut bus,
        "send",
        json!({"room_id":id,"member_handle":handle,"text":text}),
    );
    mutate(
        &mut bus,
        "send",
        json!({"room_id":id,"member_handle":handle,"text":"second"}),
    );
    let unbounded = call(
        &mut bus,
        "peek",
        json!({"room_id":id,"member_handle":handle}),
    );
    let size = unbounded.to_string().chars().count();
    assert!(size < unbounded.to_string().len());
    let at_budget = call(
        &mut bus,
        "peek",
        json!({"room_id":id,"member_handle":handle,"budget_chars":size}),
    );
    assert_eq!(at_budget, unbounded);
    let below = call(
        &mut bus,
        "peek",
        json!({"room_id":id,"member_handle":handle,"budget_chars":size-1}),
    );
    assert_eq!(below["items"].as_array().unwrap().len(), 1);
    assert_eq!(below["remaining"], 1);
    let tiny = call(
        &mut bus,
        "read",
        json!({"room_id":id,"member_handle":handle,"budget_chars":1}),
    );
    assert_eq!(tiny["items"][0]["payload"]["text"], text);
    assert_eq!(tiny["items"].as_array().unwrap().len(), 1);
}

#[test]
fn query_tokens_bind_filters_and_expire_without_substituting_new_snapshots() {
    let mut bus = setup("chat-query-cursor");
    let room = mutate(&mut bus, "open", json!({"name":"query"}));
    let id = &room["room_id"];
    let peer = join(&mut bus, id, "peer", "peer");
    for text in ["topic one", "topic two", "other"] {
        mutate(
            &mut bus,
            "send",
            json!({"room_id":id,"member_handle":peer["member_handle"],"text":text}),
        );
    }
    let first = call(
        &mut bus,
        "ledger",
        json!({"room_id":id,"kind":"message","text":"topic","limit":1}),
    );
    let cursor = &first["next_cursor"];
    let roots = StoreRoots::default();
    let mismatch = json!({"room_id":id,"kind":"message","text":"other","limit":1,"cursor":cursor});
    assert_eq!(
        bus.chat_call("ledger", mismatch, &roots).unwrap_err().code,
        "cursor_mismatch"
    );
    let valid = json!({"room_id":id,"kind":"message","text":"topic","limit":1,"cursor":cursor});
    assert_eq!(
        call(&mut bus, "ledger", valid.clone())["items"][0]["payload"]["text"],
        "topic two"
    );
    bus.set_chat_snapshot_lifetime(std::time::Duration::ZERO);
    assert_eq!(
        bus.chat_call("ledger", valid, &roots).unwrap_err().code,
        "snapshot_expired"
    );
}

#[test]
fn closed_room_with_no_active_members_keeps_administrative_history() {
    let mut bus = setup("chat-closed-history");
    let room = mutate(&mut bus, "open", json!({"name":"closed"}));
    let id = &room["room_id"];
    let peer = join(&mut bus, id, "peer", "peer");
    mutate(
        &mut bus,
        "send",
        json!({"room_id":id,"member_handle":peer["member_handle"],"text":"historical message"}),
    );
    mutate(&mut bus, "close", json!({"room_id":id}));
    let wait = call(
        &mut bus,
        "wait",
        json!({"room_id":id,"member_handle":peer["member_handle"],"addressed_only":true}),
    );
    assert_eq!(wait["status"], "room_closed");
    assert_eq!(wait["remaining"], 4);
    mutate(
        &mut bus,
        "leave",
        json!({"room_id":id,"member_handle":peer["member_handle"]}),
    );
    let ledger = call(&mut bus, "ledger", json!({"room_id":id,"kind":"message"}));
    assert_eq!(ledger["items"][0]["payload"]["text"], "historical message");
    let export = call(&mut bus, "export", json!({"room_id":id}));
    assert!(
        export["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["type"] == "member" && !row["member"]["left_at"].is_null())
    );
}

#[test]
fn mutation_requests_are_canonical_but_reject_changed_content() {
    let mut bus = setup("chat-canonical-request");
    let room = mutate(&mut bus, "open", json!({"name":"canonical"}));
    let id = &room["room_id"];
    let peer = join(&mut bus, id, "peer", "peer");
    let handle = &peer["member_handle"];
    let request = Uuid::new_v4();
    let args =
        json!({"room_id":id,"member_handle":handle,"text":"one","request_id":request.to_string()});
    let first = call(&mut bus, "send", args);
    let args = json!({"request_id":request.to_string().to_ascii_uppercase(),"text":"one","member_handle":handle,"room_id":id});
    assert_eq!(call(&mut bus, "send", args.clone()), first);
    let mut changed = args;
    changed["text"] = json!("two");
    assert_eq!(
        bus.chat_call("send", changed, &StoreRoots::default())
            .unwrap_err()
            .code,
        "request_conflict"
    );
}

#[test]
fn tool_definitions_expose_concrete_arguments_and_delivery_results() {
    let definitions = mukade::bus::chat_tool_definitions();
    assert_eq!(definitions.len(), 21);
    let tool = |name| {
        definitions
            .iter()
            .find(|tool| tool["name"] == name)
            .unwrap()
    };
    let export_member = &tool("chat_export")["outputSchema"]["$defs"]["export_member"];
    assert_eq!(export_member["additionalProperties"], false);
    assert!(export_member["properties"].get("read_cursor").is_none());
    let member = &tool("chat_members")["outputSchema"]["$defs"]["member"];
    assert!(
        member["required"]
            .as_array()
            .unwrap()
            .contains(&json!("read_cursor"))
    );
    assert!(
        member["required"]
            .as_array()
            .unwrap()
            .contains(&json!("last_signal"))
    );
    for definition in &definitions {
        assert_eq!(definition["inputSchema"]["additionalProperties"], false);
        assert_eq!(definition["outputSchema"]["type"], "object");
        assert!(
            definition["outputSchema"]["required"]
                .as_array()
                .unwrap()
                .contains(&json!("service_id"))
        );
    }
    assert_eq!(
        tool("chat_join")["outputSchema"]["properties"]["member_handle"]["format"],
        "uuid"
    );
    assert_eq!(
        tool("chat_read")["outputSchema"]["properties"]["items"]["items"]["$ref"],
        "#/$defs/event"
    );
    assert_eq!(
        tool("chat_export")["outputSchema"]["properties"]["items"]["items"]["oneOf"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
}

#[test]
fn duplicate_native_id_requires_a_configured_source_reference() {
    let root = support::scratch(HarnessId::Pi, "chat-source-choice");
    support::stage_fixture(HarnessId::Pi, &root);
    let roots = support::roots(&root);
    let sessions = roots.pi_sessions().unwrap();
    let source = std::fs::read_dir(sessions.join("--pi-project--"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let second_dir = sessions.join("--duplicate-project--");
    std::fs::create_dir_all(&second_dir).unwrap();
    std::fs::copy(&source, second_dir.join(source.file_name().unwrap())).unwrap();
    let mut bus = Bus::open_in(&root.join("bus")).unwrap();
    let room = mutate(&mut bus, "open", json!({"name":"duplicate"}));
    let id = &room["room_id"];
    let peer = join(&mut bus, id, "peer", "peer");
    let candidates = bus
        .chat_call("candidates", json!({"harness":"pi"}), &roots)
        .unwrap();
    let candidates = candidates["items"].as_array().unwrap();
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0]["session_id"], candidates[1]["session_id"]);
    assert_ne!(candidates[0]["source_ref"], candidates[1]["source_ref"]);
    let mut bind = json!({"room_id":id,"member_handle":peer["member_handle"],"harness":"pi",
        "session_id":candidates[0]["session_id"],"request_id":Uuid::new_v4().to_string()});
    assert_eq!(
        bus.chat_call("bind", bind.clone(), &roots)
            .unwrap_err()
            .code,
        "session_ambiguous"
    );
    bind["source_ref"] = candidates[1]["source_ref"].clone();
    let bound = bus.chat_call("bind", bind, &roots).unwrap();
    assert_eq!(bound["session"]["source_ref"], candidates[1]["source_ref"]);
}

#[test]
fn export_omits_delivery_state_across_every_page() {
    let mut bus = setup("chat-export-delivery-privacy");
    let room = mutate(&mut bus, "open", json!({"name":"export"}));
    let id = &room["room_id"];
    let peer = join(&mut bus, id, "peer", "peer");
    let handle = &peer["member_handle"];
    let first = call(
        &mut bus,
        "read",
        json!({"room_id":id,"member_handle":handle,"budget_chars":1}),
    );
    let ack = call(
        &mut bus,
        "ack",
        json!({"room_id":id,"member_handle":handle,"batch_id":first["batch_id"]}),
    );
    let pending = call(
        &mut bus,
        "read",
        json!({"room_id":id,"member_handle":handle,"budget_chars":1}),
    );
    assert!(pending["batch_id"].is_string());
    let members = call(&mut bus, "members", json!({"room_id":id}));
    assert_eq!(
        members["items"][0]["read_cursor"],
        ack["acknowledged_through"]
    );
    assert_ne!(members["items"][0]["read_cursor"], "0");

    fn assert_no_delivery_fields(value: &Value) {
        match value {
            Value::Object(fields) => {
                for (key, value) in fields {
                    assert!(
                        ![
                            "read_cursor",
                            "member_handle",
                            "batch_id",
                            "ack_batch_id",
                            "pending_batch_id",
                            "expected_cursor"
                        ]
                        .contains(&key.as_str()),
                        "export includes {key}"
                    );
                    assert_no_delivery_fields(value);
                }
            }
            Value::Array(values) => values.iter().for_each(assert_no_delivery_fields),
            _ => {}
        }
    }

    let mut cursor = Value::Null;
    let mut pages = 0;
    let mut member_seen = false;
    loop {
        let page = call(
            &mut bus,
            "export",
            json!({"room_id":id,"member_handle":handle,"limit":1,"cursor":cursor}),
        );
        assert_no_delivery_fields(&page);
        let text = page.to_string();
        assert!(!text.contains(handle.as_str().unwrap()));
        assert!(!text.contains(pending["batch_id"].as_str().unwrap()));
        member_seen |= page["items"][0]["type"] == "member";
        pages += 1;
        cursor = page["next_cursor"].clone();
        if cursor.is_null() {
            break;
        }
    }
    assert!(member_seen && pages > 1);
}
