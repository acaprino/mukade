//! The host is the program that runs the room for the user. A member it creates is marked, and
//! only a request carrying the host's own credential may mark one: the member credential, which
//! every agent receives, cannot.
#[path = "support/chat_http.rs"]
mod chat_http;
mod support;

use mukade::{
    HarnessId,
    bus::{Bus, chat_host_tool_definitions, chat_tool_definitions},
    chat::{ChatService, server::ServiceConfig},
    roots::StoreRoots,
};
use serde_json::{Value, json};

const MEMBER_TOKEN: &str = "test-secret";
const HOST_TOKEN: &str = "test-host-secret";

fn fixture() -> ChatService {
    let dir = support::scratch(HarnessId::Pi, "chat-host");
    let bus = Bus::open_in(&dir).unwrap();
    let config = ServiceConfig::new(
        dir.canonicalize().unwrap(),
        bus.service_id().unwrap(),
        uuid::Uuid::new_v4().to_string(),
        "http://127.0.0.1:39171/mcp".into(),
        MEMBER_TOKEN.into(),
    )
    .unwrap()
    .with_host_token(HOST_TOKEN.into())
    .unwrap();
    ChatService::new(bus, StoreRoots::default(), config).unwrap()
}

fn mutation(mut args: Value) -> Value {
    args["request_id"] = json!(uuid::Uuid::new_v4().to_string());
    args
}

async fn members(client: &mukade::chat::client::ChatClient, room: &Value) -> Vec<Value> {
    let listed = client
        .call("members", json!({"room_id": room["room_id"]}))
        .await
        .unwrap();
    listed["items"].as_array().unwrap().clone()
}

#[tokio::test]
async fn host_flag_marks_the_member() {
    let service = fixture();
    let host = chat_http::client_as(&service, HOST_TOKEN).await;
    let room = host
        .call("open", mutation(json!({"name": "marked"})))
        .await
        .unwrap();
    let marked = host
        .call(
            "join",
            mutation(json!({"room_id": room["room_id"], "harness": "shinatama", "role": "peer", "name": "shinatama", "host": true})),
        )
        .await
        .unwrap();
    let plain = host
        .call(
            "join",
            mutation(json!({"room_id": room["room_id"], "harness": "pi", "role": "peer"})),
        )
        .await
        .unwrap();
    let listed = members(&host, &room).await;
    let flag = |id: &Value| listed.iter().find(|m| m["member_id"] == *id).unwrap()["host"].clone();
    assert_eq!(flag(&marked["member_id"]), json!(true));
    assert_eq!(flag(&plain["member_id"]), json!(false));
    let again = host
        .call(
            "rejoin",
            mutation(json!({"room_id": room["room_id"], "member_id": marked["member_id"]})),
        )
        .await
        .unwrap();
    assert_eq!(again["member_id"], marked["member_id"]);
    let listed = members(&host, &room).await;
    assert_eq!(
        listed
            .iter()
            .find(|m| m["member_id"] == marked["member_id"])
            .unwrap()["host"],
        json!(true),
        "a rejoin keeps the mark"
    );
}

#[tokio::test]
async fn a_member_credential_cannot_make_a_host() {
    let service = fixture();
    let agent = chat_http::client_as(&service, MEMBER_TOKEN).await;
    let room = agent
        .call("open", mutation(json!({"name": "forged"})))
        .await
        .unwrap();
    let refused = agent
        .call(
            "join",
            mutation(
                json!({"room_id": room["room_id"], "harness": "pi", "role": "peer", "host": true}),
            ),
        )
        .await
        .expect_err("host: true needs the host credential");
    assert_eq!(refused.code, "invalid_arguments");
    assert_eq!(
        members(&agent, &room).await.len(),
        0,
        "no member was created"
    );
    for op in ["set_role", "retire"] {
        let refused = agent
            .call(op, mutation(json!({"room_id": room["room_id"], "member_handle": uuid::Uuid::new_v4().to_string(), "member_id": "x", "role": "observer"})))
            .await
            .expect_err("a host operation is unknown to a member credential");
        assert_eq!(refused.code, "unknown_operation");
    }
}

#[test]
fn the_tools_an_agent_lists_carry_no_host_surface() {
    let agent = chat_tool_definitions();
    let names: Vec<_> = agent.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(!names.contains(&"chat_set_role") && !names.contains(&"chat_retire"));
    let join = agent.iter().find(|t| t["name"] == "chat_join").unwrap();
    assert!(join["inputSchema"]["properties"].get("host").is_none());
    let host = chat_host_tool_definitions();
    let names: Vec<_> = host.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"chat_set_role") && names.contains(&"chat_retire"));
    let join = host.iter().find(|t| t["name"] == "chat_join").unwrap();
    assert!(join["inputSchema"]["properties"].get("host").is_some());
}

async fn host_in(client: &mukade::chat::client::ChatClient, room: &Value) -> Value {
    client
        .call(
            "join",
            mutation(json!({"room_id": room["room_id"], "harness": "shinatama", "role": "peer", "name": "shinatama", "host": true})),
        )
        .await
        .unwrap()
}

async fn peer_in(client: &mukade::chat::client::ChatClient, room: &Value) -> Value {
    client
        .call(
            "join",
            mutation(json!({"room_id": room["room_id"], "harness": "pi", "role": "peer"})),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn only_a_host_sets_a_role() {
    let service = fixture();
    let host = chat_http::client_as(&service, HOST_TOKEN).await;
    let room = host
        .call("open", mutation(json!({"name": "roles"})))
        .await
        .unwrap();
    let boss = host_in(&host, &room).await;
    let peer = peer_in(&host, &room).await;
    let other = peer_in(&host, &room).await;
    let set = |handle: &Value, target: &Value, role: &str| {
        mutation(
            json!({"room_id": room["room_id"], "member_handle": handle, "member_id": target["member_id"], "role": role}),
        )
    };
    // Holding the host credential is not enough: the handle must be a host member's.
    let refused = host
        .call("set_role", set(&peer["member_handle"], &other, "lead"))
        .await
        .unwrap_err();
    assert_eq!(refused.code, "not_host");
    let refused = host
        .call("set_role", set(&boss["member_handle"], &boss, "lead"))
        .await
        .unwrap_err();
    assert_eq!(refused.code, "host_member");
    let done = host
        .call("set_role", set(&boss["member_handle"], &peer, "lead"))
        .await
        .unwrap();
    assert_eq!(done["role"], json!("lead"));
    assert_eq!(done["member_id"], peer["member_id"]);
    let listed = members(&host, &room).await;
    let role = |m: &Value| {
        listed
            .iter()
            .find(|x| x["member_id"] == m["member_id"])
            .unwrap()["role"]
            .clone()
    };
    assert_eq!(role(&peer), json!("lead"));
    assert_eq!(role(&other), json!("peer"));
}

#[tokio::test]
async fn set_role_changes_what_the_member_may_do() {
    let service = fixture();
    let host = chat_http::client_as(&service, HOST_TOKEN).await;
    let agent = chat_http::client_as(&service, MEMBER_TOKEN).await;
    let room = host
        .call("open", mutation(json!({"name": "may"})))
        .await
        .unwrap();
    let boss = host_in(&host, &room).await;
    let peer = peer_in(&agent, &room).await;
    host.call(
        "set_role",
        mutation(json!({"room_id": room["room_id"], "member_handle": boss["member_handle"], "member_id": peer["member_id"], "role": "observer"})),
    )
    .await
    .unwrap();
    let refused = agent
        .call(
            "send",
            mutation(json!({"room_id": room["room_id"], "member_handle": peer["member_handle"], "text": "hello"})),
        )
        .await
        .unwrap_err();
    assert_eq!(refused.code, "role_refused");
}

#[tokio::test]
async fn retire_marks_the_member_left_and_stales_its_handle() {
    let service = fixture();
    let host = chat_http::client_as(&service, HOST_TOKEN).await;
    let agent = chat_http::client_as(&service, MEMBER_TOKEN).await;
    let room = host
        .call("open", mutation(json!({"name": "retiring"})))
        .await
        .unwrap();
    let boss = host_in(&host, &room).await;
    let peer = peer_in(&agent, &room).await;
    let gone = host
        .call(
            "retire",
            mutation(json!({"room_id": room["room_id"], "member_handle": boss["member_handle"], "member_id": peer["member_id"]})),
        )
        .await
        .unwrap();
    assert_eq!(gone["member_id"], peer["member_id"]);
    let listed = members(&host, &room).await;
    let left = &listed
        .iter()
        .find(|m| m["member_id"] == peer["member_id"])
        .unwrap()["left_at"];
    assert!(left.is_string(), "{left}");
    let stale = agent
        .call(
            "send",
            mutation(json!({"room_id": room["room_id"], "member_handle": peer["member_handle"], "text": "still here"})),
        )
        .await
        .unwrap_err();
    assert_eq!(stale.code, "stale_member");
    let again = host
        .call(
            "retire",
            mutation(json!({"room_id": room["room_id"], "member_handle": boss["member_handle"], "member_id": peer["member_id"]})),
        )
        .await
        .unwrap_err();
    assert_eq!(again.code, "member_not_found");
}

#[test]
fn the_command_line_has_the_host_operations() {
    use mukade::cli::chat::{Command, parse};
    let args = |list: &[&str]| list.iter().map(|a| a.to_string()).collect::<Vec<_>>();
    let Command::Domain(join) = parse(&args(&[
        "join",
        "--room-id",
        "r",
        "--harness",
        "shinatama",
        "--role",
        "peer",
        "--host",
    ]))
    .unwrap() else {
        panic!("join is a domain command")
    };
    assert_eq!(join.arguments["host"], json!(true));
    let Command::Domain(role) = parse(&args(&[
        "set-role",
        "--room-id",
        "r",
        "--member-handle",
        "7a1b2c3d-0000-4000-8000-000000000000",
        "--member-id",
        "m",
        "--role",
        "lead",
    ]))
    .unwrap() else {
        panic!("set-role is a domain command")
    };
    assert_eq!(role.operation, "set_role");
    assert_eq!(role.arguments["role"], json!("lead"));
    let Command::Domain(retire) = parse(&args(&[
        "retire",
        "--room-id",
        "r",
        "--member-handle",
        "7a1b2c3d-0000-4000-8000-000000000000",
        "--member-id",
        "m",
    ]))
    .unwrap() else {
        panic!("retire is a domain command")
    };
    assert_eq!(retire.operation, "retire");
}

async fn rejoin_events(client: &mukade::chat::client::ChatClient, room: &Value) -> usize {
    client
        .call(
            "ledger",
            json!({"room_id": room["room_id"], "kind": "rejoin"}),
        )
        .await
        .unwrap()["items"]
        .as_array()
        .unwrap()
        .len()
}

async fn peeks(client: &mukade::chat::client::ChatClient, room: &Value, member: &Value) -> bool {
    client
        .call(
            "peek",
            json!({"room_id": room["room_id"], "member_handle": member["member_handle"]}),
        )
        .await
        .is_ok()
}

#[tokio::test]
async fn an_agent_cannot_recover_a_host_member() {
    let service = fixture();
    let host = chat_http::client_as(&service, HOST_TOKEN).await;
    let agent = chat_http::client_as(&service, MEMBER_TOKEN).await;
    let room = host
        .call("open", mutation(json!({"name": "stolen"})))
        .await
        .unwrap();
    let boss = host_in(&host, &room).await;
    let before = rejoin_events(&agent, &room).await;
    let refused = agent
        .call(
            "rejoin",
            mutation(json!({"room_id": room["room_id"], "member_id": boss["member_id"]})),
        )
        .await
        .expect_err("a host member is recovered with the host credential");
    assert_eq!(refused.code, "not_host");
    assert!(
        peeks(&host, &room, &boss).await,
        "the host member's handle is untouched"
    );
    assert_eq!(rejoin_events(&agent, &room).await, before, "no event");
    // An unknown member answers as it always did.
    let unknown = agent
        .call(
            "rejoin",
            mutation(json!({"room_id": room["room_id"], "member_id": "0".repeat(32)})),
        )
        .await
        .unwrap_err();
    assert_eq!(unknown.code, "member_not_found");
}

#[tokio::test]
async fn the_host_recovers_its_own_member() {
    let service = fixture();
    let host = chat_http::client_as(&service, HOST_TOKEN).await;
    let room = host
        .call("open", mutation(json!({"name": "recover"})))
        .await
        .unwrap();
    let boss = host_in(&host, &room).await;
    let again = host
        .call(
            "rejoin",
            mutation(
                json!({"room_id": room["room_id"], "member_id": boss["member_id"], "host": true}),
            ),
        )
        .await
        .unwrap();
    assert_eq!(again["member_id"], boss["member_id"]);
    assert_ne!(again["member_handle"], boss["member_handle"]);
    assert_eq!(again["host"], json!(true));
    assert!(!peeks(&host, &room, &boss).await, "the old handle is stale");
    assert!(peeks(&host, &room, &again).await);
    let listed = members(&host, &room).await;
    assert_eq!(
        listed
            .iter()
            .find(|m| m["member_id"] == boss["member_id"])
            .unwrap()["host"],
        json!(true)
    );
}

#[tokio::test]
async fn an_agent_still_recovers_its_own_member() {
    let service = fixture();
    let host = chat_http::client_as(&service, HOST_TOKEN).await;
    let agent = chat_http::client_as(&service, MEMBER_TOKEN).await;
    let room = host
        .call("open", mutation(json!({"name": "own"})))
        .await
        .unwrap();
    let peer = peer_in(&agent, &room).await;
    let again = agent
        .call(
            "rejoin",
            mutation(json!({"room_id": room["room_id"], "member_id": peer["member_id"]})),
        )
        .await
        .unwrap();
    assert_eq!(again["member_id"], peer["member_id"]);
    assert_ne!(again["member_handle"], peer["member_handle"]);
    assert_eq!(again["host"], json!(false));
    let refused = agent
        .call(
            "rejoin",
            mutation(
                json!({"room_id": room["room_id"], "member_id": peer["member_id"], "host": true}),
            ),
        )
        .await
        .unwrap_err();
    assert_eq!(refused.code, "invalid_arguments");
    // A host flag on the host's rejoin of a plain member changes nothing.
    let again = host
        .call(
            "rejoin",
            mutation(
                json!({"room_id": room["room_id"], "member_id": peer["member_id"], "host": true}),
            ),
        )
        .await
        .unwrap();
    assert_eq!(again["host"], json!(false));
}

#[test]
fn rejoin_host_field_is_hidden_from_agents() {
    let rejoin = |tools: Vec<Value>| {
        tools
            .into_iter()
            .find(|t| t["name"] == "chat_rejoin")
            .unwrap()
    };
    assert!(
        rejoin(chat_tool_definitions())["inputSchema"]["properties"]
            .get("host")
            .is_none()
    );
    assert!(
        rejoin(chat_host_tool_definitions())["inputSchema"]["properties"]
            .get("host")
            .is_some()
    );
}

#[test]
fn rejoin_takes_host_on_the_command_line() {
    use mukade::cli::chat::{Command, parse};
    let args = |list: &[&str]| list.iter().map(|a| a.to_string()).collect::<Vec<_>>();
    let Command::Domain(rejoin) = parse(&args(&[
        "rejoin",
        "--room-id",
        "r",
        "--member-id",
        "m",
        "--host",
    ]))
    .unwrap() else {
        panic!("rejoin is a domain command")
    };
    assert_eq!(rejoin.operation, "rejoin");
    assert_eq!(rejoin.arguments["host"], json!(true));
}
