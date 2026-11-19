#[path = "support/chat_http.rs"]
mod chat_http;
mod support;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use mukade::{
    HarnessId,
    bus::Bus,
    chat::{ChatService, ServiceConfig},
    roots::StoreRoots,
};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;

fn fixture() -> ChatService {
    fixture_with_http(mukade::chat::server::HttpLimits::default())
}
fn fixture_with_http(limits: mukade::chat::server::HttpLimits) -> ChatService {
    let dir = support::scratch(HarnessId::Pi, "chat-http");
    let bus = Bus::open_in(&dir).unwrap();
    let mut config = ServiceConfig::new(
        dir.canonicalize().unwrap(),
        bus.service_id().unwrap(),
        uuid::Uuid::new_v4().to_string(),
        "http://127.0.0.1:39171/mcp".into(),
        "test-secret".into(),
    )
    .unwrap();
    config.http_limits = limits;
    ChatService::new(bus, StoreRoots::default(), config).unwrap()
}

#[tokio::test]
async fn wait_periodically_rechecks_commits_without_a_notification() {
    let service = fixture();
    let room = call(&service, "open", mutation(json!({"name":"periodic"}))).await;
    let reader = call(
        &service,
        "join",
        mutation(json!({"room_id":room["room_id"],"harness":"pi","role":"peer"})),
    )
    .await;
    let sender = call(
        &service,
        "join",
        mutation(json!({"room_id":room["room_id"],"harness":"pi","role":"peer"})),
    )
    .await;
    let args = json!({"room_id":room["room_id"],"member_handle":reader["member_handle"]});
    let batch = call(&service, "read", args.clone()).await;
    let mut wait = args;
    wait["ack_batch_id"] = batch["batch_id"].clone();
    wait["timeout_seconds"] = json!(3);
    let task = tokio::spawn({
        let s = service.clone();
        async move { s.call("wait", wait, CancellationToken::new()).await }
    });
    while service.active_streams() == 0 {
        tokio::task::yield_now().await;
    }
    let dir = service.config().data_dir.clone();
    let send = mutation(
        json!({"room_id":room["room_id"],"member_handle":sender["member_handle"],"text":"no notifier"}),
    );
    // Independent domain writer deliberately omits ChatService's internal notification.
    tokio::task::spawn_blocking(move || {
        Bus::open_in(&dir)
            .unwrap()
            .chat_call("send", send, &StoreRoots::default())
            .unwrap()
    })
    .await
    .unwrap();
    let result = tokio::time::timeout(std::time::Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(result["status"], "ok");
    assert_eq!(result["ack_batch_id"], batch["batch_id"]);
}

#[tokio::test]
async fn stalled_upload_and_unpolled_response_release_http_capacity() {
    use mukade::chat::server::HttpLimits;
    let service = fixture_with_http(HttpLimits {
        max_concurrent_requests: 1,
        upload_timeout: std::time::Duration::from_millis(100),
        response_idle_timeout: std::time::Duration::from_millis(100),
        response_lifetime: std::time::Duration::from_millis(300),
    });
    let upload = tokio::spawn({
        let app = service.router();
        async move {
            http(
                app,
                "/mcp",
                AUTH,
                Body::from_stream(futures_util::stream::pending::<
                    Result<Vec<u8>, std::io::Error>,
                >()),
            )
            .await
        }
    });
    while service.active_http_requests() == 0 {
        tokio::task::yield_now().await;
    }
    assert_eq!(
        http(service.router(), "/health", AUTH, Body::empty())
            .await
            .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(upload.await.unwrap().status(), StatusCode::REQUEST_TIMEOUT);
    assert_eq!(service.active_http_requests(), 0);
    let response = http(service.router(), "/health", AUTH, Body::empty()).await;
    assert_eq!(response.status(), StatusCode::OK);
    // Keep an unpolled body alive: expiry must free its queued bytes and permit independently.
    tokio::time::sleep(std::time::Duration::from_millis(350)).await;
    assert_eq!(service.active_http_requests(), 0);
    assert!(
        to_bytes(response.into_body(), 10000)
            .await
            .unwrap()
            .is_empty()
    );
    let response = http(service.router(), "/health", AUTH, Body::empty()).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        !to_bytes(response.into_body(), 10000)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn quiet_wait_outlives_write_idle_budget() {
    let service = fixture_with_http(mukade::chat::server::HttpLimits {
        response_idle_timeout: std::time::Duration::from_millis(50),
        response_lifetime: std::time::Duration::from_secs(3),
        ..Default::default()
    });
    let client = chat_http::client(&service).await;
    let room = client
        .call("open", mutation(json!({"name":"quiet"})))
        .await
        .unwrap();
    let member = client
        .call(
            "join",
            mutation(json!({"room_id":room["room_id"],"harness":"pi","role":"peer"})),
        )
        .await
        .unwrap();
    let result=client.call("wait",json!({"room_id":room["room_id"],"member_handle":member["member_handle"],"timeout_seconds":1})).await.unwrap();
    assert_eq!(result["status"], "timeout");
    client.close().await;
}

#[tokio::test]
async fn expired_unpolled_sse_preserves_wait_guard_and_staged_receipt() {
    use rmcp::transport::streamable_http_client::StreamableHttpClient;
    use std::collections::HashMap;
    let service = fixture_with_http(mukade::chat::server::HttpLimits {
        response_idle_timeout: std::time::Duration::from_millis(70),
        response_lifetime: std::time::Duration::from_millis(180),
        ..Default::default()
    });
    let room = call(&service, "open", mutation(json!({"name":"expired"}))).await;
    let member = call(
        &service,
        "join",
        mutation(json!({"room_id":room["room_id"],"harness":"pi","role":"peer"})),
    )
    .await;
    let sender = call(
        &service,
        "join",
        mutation(json!({"room_id":room["room_id"],"harness":"pi","role":"peer"})),
    )
    .await;
    let (adapter, uri, session) = chat_http::raw_session(&service).await;
    let args = json!({"room_id":room["room_id"],"member_handle":member["member_handle"],"timeout_seconds":3});
    let request=serde_json::from_value(json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"chat_wait","arguments":args}})).unwrap();
    let response = adapter
        .post_message(
            uri,
            request,
            Some(session),
            Some("test-secret".into()),
            HashMap::new(),
        )
        .await
        .unwrap();
    while service.active_streams() == 0 {
        tokio::task::yield_now().await;
    }
    tokio::time::sleep(std::time::Duration::from_millis(240)).await;
    assert_eq!(service.active_http_requests(), 0);
    assert_eq!(service.active_streams(), 1);
    let read = json!({"room_id":room["room_id"],"member_handle":member["member_handle"]});
    assert_eq!(
        service
            .call("read", read.clone(), CancellationToken::new())
            .await
            .unwrap_err()
            .code,
        "stream_busy"
    );
    call(&service,"send",mutation(json!({"room_id":room["room_id"],"member_handle":sender["member_handle"],"text":"after HTTP expiry"}))).await;
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while service.active_streams() != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let path = Bus::open_in(&service.config().data_dir)
        .unwrap()
        .path()
        .to_owned();
    let conn = rusqlite::Connection::open(path).unwrap();
    let staged: String = conn
        .query_row(
            "SELECT id FROM chat_room_batches WHERE member_id=?1 AND acknowledged=0",
            [member["member_id"].as_str().unwrap()],
            |r| r.get(0),
        )
        .unwrap();
    let batch = call(&service, "read", read).await;
    assert_eq!(batch["batch_id"], staged);
    assert_eq!(
        http(service.router(), "/health", AUTH, Body::empty())
            .await
            .status(),
        StatusCode::OK
    );
    drop(response);
}

#[tokio::test]
async fn locked_native_discovery_does_not_block_unrelated_send() {
    let dir = support::scratch(HarnessId::Pi, "native-catalog-lock");
    let native = dir.join("native.db");
    let lock = rusqlite::Connection::open(&native).unwrap();
    lock.execute_batch("CREATE TABLE marker(id INTEGER); BEGIN EXCLUSIVE")
        .unwrap();
    let bus = Bus::open_in(&dir.join("bus")).unwrap();
    let config = ServiceConfig::new(
        dir.join("bus").canonicalize().unwrap(),
        bus.service_id().unwrap(),
        uuid::Uuid::new_v4().to_string(),
        "http://127.0.0.1:39171/mcp".into(),
        "test-secret".into(),
    )
    .unwrap();
    let service = ChatService::new(
        bus,
        StoreRoots {
            opencode_db: Some(native),
            ..Default::default()
        },
        config,
    )
    .unwrap();
    let room = call(&service, "open", mutation(json!({"name":"locked"}))).await;
    let peer = call(
        &service,
        "join",
        mutation(json!({"room_id":room["room_id"],"harness":"pi","role":"peer"})),
    )
    .await;
    for (op, args) in [
        ("candidates", json!({"harness":"opencode"})),
        (
            "join",
            mutation(json!({"room_id":room["room_id"],"harness":"opencode","role":"peer"})),
        ),
    ] {
        let task = tokio::spawn({
            let s = service.clone();
            async move { s.call(op, args, CancellationToken::new()).await }
        });
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        assert!(!task.is_finished());
        let sent=tokio::time::timeout(std::time::Duration::from_millis(200),call(&service,"send",mutation(json!({"room_id":room["room_id"],"member_handle":peer["member_handle"],"text":"independent"})))).await;
        if sent.is_err() {
            lock.execute_batch("ROLLBACK").unwrap();
            panic!("Native discovery held the shared database lock");
        }
        assert_eq!(sent.unwrap()["status"], "ok");
        // Each discovery has a finite native SQLite busy timeout; keep its lock for both cases.
        let _ = task.await.unwrap();
    }
    lock.execute_batch("ROLLBACK").unwrap();
}
fn mutation(mut args: Value) -> Value {
    args["request_id"] = json!(uuid::Uuid::new_v4().to_string());
    args
}
async fn call(service: &ChatService, op: &str, args: Value) -> Value {
    service
        .call(op, args, CancellationToken::new())
        .await
        .unwrap()
}
async fn http(
    app: Router,
    path: &str,
    headers: &[(&str, &str)],
    body: Body,
) -> axum::response::Response {
    let mut req =
        Request::builder()
            .uri(path)
            .method(if path == "/health" { "GET" } else { "POST" });
    for (key, value) in headers {
        req = req.header(*key, *value);
    }
    app.oneshot(req.body(body).unwrap()).await.unwrap()
}
const AUTH: &[(&str, &str)] = &[
    ("host", "127.0.0.1:39171"),
    ("authorization", "Bearer test-secret"),
];

#[tokio::test]
async fn every_endpoint_requires_auth_host_origin_and_identity() {
    let service = fixture();
    for path in ["/health", "/shutdown", "/mcp"] {
        let response = http(service.router(), path, &[], Body::empty()).await;
        assert!(!response.status().is_success());
        for auth in ["", "Bearer wrong-token"] {
            assert_eq!(
                http(
                    service.router(),
                    path,
                    &[("host", "127.0.0.1:39171"), ("authorization", auth)],
                    Body::empty()
                )
                .await
                .status(),
                StatusCode::UNAUTHORIZED
            );
        }
        assert_eq!(
            http(
                service.router(),
                path,
                &[
                    ("host", "evil.test"),
                    ("authorization", "Bearer test-secret")
                ],
                Body::empty()
            )
            .await
            .status(),
            StatusCode::FORBIDDEN
        );
        let mut duplicate = AUTH.to_vec();
        duplicate.push(("authorization", "Bearer test-secret"));
        assert_eq!(
            http(service.router(), path, &duplicate, Body::empty())
                .await
                .status(),
            StatusCode::UNAUTHORIZED
        );
        for extra in [
            ("origin", "http://evil.test"),
            ("x-mukade-service-id", "wrong"),
        ] {
            let mut headers = AUTH.to_vec();
            headers.push(extra);
            assert!(
                !http(service.router(), path, &headers, Body::empty())
                    .await
                    .status()
                    .is_success()
            );
        }
    }
    let response = http(service.router(), "/health", AUTH, Body::empty()).await;
    assert_eq!(response.status(), StatusCode::OK);
    let value: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 10000).await.unwrap()).unwrap();
    assert_eq!(value["service_id"], service.config().service_id);
    assert!(!value.to_string().contains("test-secret"));
    let response = http(
        service.router(),
        "/mcp",
        AUTH,
        Body::from_stream(futures_util::stream::iter([
            Ok::<_, std::io::Error>(vec![b'x'; 524_289]),
            Ok(vec![b'x'; 524_289]),
        ])),
    )
    .await;
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn waiting_releases_database_and_guards_cancel_and_shutdown() {
    let service = fixture();
    let room = call(&service, "open", mutation(json!({"name":"a"}))).await;
    let args = mutation(json!({"room_id":room["room_id"],"harness":"pi","role":"peer"}));
    let member = call(&service, "join", args).await;
    assert_eq!(member["endpoint"], service.config().endpoint);
    let args = json!({"room_id":room["room_id"],"member_handle":member["member_handle"]});
    let cancel = CancellationToken::new();
    let task = tokio::spawn({
        let s = service.clone();
        let a = args.clone();
        let c = cancel.clone();
        async move { s.call("wait", a, c).await }
    });
    while service.active_waits() == 0 {
        tokio::task::yield_now().await;
    }
    while service.active_streams() == 0 {
        tokio::task::yield_now().await;
    }
    assert_eq!(
        service
            .call("read", args.clone(), CancellationToken::new())
            .await
            .unwrap_err()
            .code,
        "stream_busy"
    );
    call(&service, "rooms", json!({})).await;
    cancel.cancel();
    assert_eq!(task.await.unwrap().unwrap_err().code, "cancelled");
    let mut zero = args.clone();
    zero["timeout_seconds"] = json!(0);
    assert_eq!(call(&service, "wait", zero).await["status"], "timeout");
    let task = tokio::spawn({
        let s = service.clone();
        async move { s.call("wait", args, CancellationToken::new()).await }
    });
    while service.active_waits() == 0 {
        tokio::task::yield_now().await;
    }
    service.begin_shutdown();
    assert_eq!(task.await.unwrap().unwrap_err().code, "server_stopping");
    service.drain().await;
    assert_eq!(
        service
            .call("rooms", json!({}), CancellationToken::new())
            .await
            .unwrap_err()
            .code,
        "server_stopping"
    );
}

#[tokio::test]
async fn two_sdk_clients_discover_manifest_replay_and_wake() {
    let service = fixture();
    let (a, b) = tokio::join!(chat_http::client(&service), chat_http::client(&service));
    let tools = a.running().list_all_tools().await.unwrap();
    assert_eq!(tools.len(), 21);
    let expected = mukade::bus::chat_tool_definitions();
    for tool in tools {
        let value = serde_json::to_value(tool).unwrap();
        assert!(expected.iter().any(|e| e["name"] == value["name"]
            && e["inputSchema"] == value["inputSchema"]
            && e["outputSchema"] == value["outputSchema"]));
    }
    assert_eq!(
        a.running()
            .peer_info()
            .unwrap()
            .protocol_version
            .to_string(),
        "2025-11-25"
    );
    let room = a
        .call("open", mutation(json!({"name":"sdk"})))
        .await
        .unwrap();
    let peer = a
        .call(
            "join",
            mutation(json!({"room_id":room["room_id"],"harness":"pi","role":"peer"})),
        )
        .await
        .unwrap();
    let worker = b
        .call(
            "join",
            mutation(json!({"room_id":room["room_id"],"harness":"codex","role":"peer"})),
        )
        .await
        .unwrap();
    let wait_args = json!({"room_id":room["room_id"],"member_handle":worker["member_handle"],"timeout_seconds":5});
    let wait = tokio::spawn(async move {
        let r = b.call("wait", wait_args).await;
        b.close().await;
        r
    });
    while service.active_streams() == 0 {
        tokio::task::yield_now().await;
    }
    let send = mutation(
        json!({"room_id":room["room_id"],"member_handle":peer["member_handle"],"text":"hello"}),
    );
    let sent = a.call("send", send.clone()).await.unwrap();
    let result = wait.await.unwrap().unwrap();
    assert_eq!(result["status"], "ok");
    assert!(!result["batch_id"].is_null());
    a.call("close", mutation(json!({"room_id":room["room_id"]})))
        .await
        .unwrap();
    assert_eq!(sent, a.call("send", send).await.unwrap());
    a.close().await;
    service.finish_shutdown().await;
}

#[tokio::test]
async fn sdk_domain_errors_preserve_operation_request_and_committed_ack() {
    use rmcp::model::CallToolRequestParams;
    let service = fixture();
    let a = chat_http::client(&service).await;
    let req = mutation(json!({"room_id":"missing"}));
    let response = a
        .running()
        .call_tool(
            CallToolRequestParams::new("chat_close")
                .with_arguments(req.as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_eq!(response.is_error, Some(true));
    let e = response.structured_content.unwrap();
    assert_eq!(e["operation"], "chat_close");
    assert_eq!(e["request_id"], req["request_id"]);
    let room = a
        .call("open", mutation(json!({"name":"ack"})))
        .await
        .unwrap();
    let member = a
        .call(
            "join",
            mutation(json!({"room_id":room["room_id"],"harness":"pi","role":"peer"})),
        )
        .await
        .unwrap();
    let args = json!({"room_id":room["room_id"],"member_handle":member["member_handle"]});
    let batch = a.call("read", args.clone()).await.unwrap();
    let mut wait = args.clone();
    wait["ack_batch_id"] = batch["batch_id"].clone();
    wait["timeout_seconds"] = json!(0);
    let timeout = a.call("wait", wait).await.unwrap();
    assert_eq!(timeout["status"], "timeout");
    assert_eq!(timeout["ack_batch_id"], batch["batch_id"]);
    let observer = a
        .call(
            "join",
            mutation(json!({"room_id":room["room_id"],"harness":"pi","role":"observer"})),
        )
        .await
        .unwrap();
    assert_eq!(a.call("send",mutation(json!({"room_id":room["room_id"],"member_handle":observer["member_handle"],"text":"forbidden"}))).await.unwrap_err().code,"role_refused");
    let leave = mutation(args.clone());
    a.call("leave", leave.clone()).await.unwrap();
    a.call("leave", leave).await.unwrap();
    assert_eq!(a.call("read", args).await.unwrap_err().code, "stale_member");
    a.close().await;
}

#[tokio::test]
async fn supported_legacy_protocols_negotiate_through_sdk_http() {
    use rmcp::transport::streamable_http_client::StreamableHttpClient;
    use std::{collections::HashMap, sync::Arc};
    let service = fixture();
    for version in ["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"] {
        let adapter = chat_http::RouterHttp(service.router());
        let uri: Arc<str> = service.config().endpoint.clone().into();
        let initialize=serde_json::from_value(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":version,"capabilities":{},"clientInfo":{"name":"version-test","version":"1"}}})).unwrap();
        let (result, _) = adapter
            .post_message(
                uri,
                initialize,
                None,
                Some("test-secret".into()),
                HashMap::new(),
            )
            .await
            .unwrap()
            .expect_initialized::<std::io::Error>()
            .await
            .unwrap();
        assert_eq!(
            serde_json::to_value(result).unwrap()["result"]["protocolVersion"],
            version
        );
    }
    service.finish_shutdown().await;
}

#[tokio::test]
async fn unsupported_protocol_never_initializes() {
    let service = fixture();
    let mut headers = AUTH.to_vec();
    headers.extend([
        ("content-type", "application/json"),
        ("accept", "application/json, text/event-stream"),
    ]);
    let initialize = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2026-07-28","capabilities":{},"clientInfo":{"name":"test","version":"1"}}});
    let response = http(
        service.router(),
        "/mcp",
        &headers,
        Body::from(initialize.to_string()),
    )
    .await;
    let body = to_bytes(response.into_body(), 10000).await.unwrap();
    let text = String::from_utf8_lossy(&body);
    assert!(text.contains("not supported"), "{text}");
    assert!(!text.contains("serverInfo"));
    headers.push(("mcp-protocol-version", "2026-07-28"));
    assert_eq!(
        http(service.router(), "/mcp", &headers, Body::empty())
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn legacy_disconnected_response_keeps_wait_until_explicit_sdk_cancellation() {
    use rmcp::{
        model::ClientJsonRpcMessage,
        transport::streamable_http_client::{StreamableHttpClient, StreamableHttpPostResponse},
    };
    use std::{collections::HashMap, sync::Arc};
    let service = fixture();
    let a = chat_http::client(&service).await;
    let room = a
        .call("open", mutation(json!({"name":"disconnect"})))
        .await
        .unwrap();
    let member = a
        .call(
            "join",
            mutation(json!({"room_id":room["room_id"],"harness":"pi","role":"peer"})),
        )
        .await
        .unwrap();
    let adapter = chat_http::RouterHttp(service.router());
    let uri: Arc<str> = service.config().endpoint.clone().into();
    let message:ClientJsonRpcMessage=serde_json::from_value(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"disconnect-test","version":"1"}}})).unwrap();
    let (_, session) = adapter
        .post_message(
            uri.clone(),
            message,
            None,
            Some("test-secret".into()),
            HashMap::new(),
        )
        .await
        .unwrap()
        .expect_initialized::<std::io::Error>()
        .await
        .unwrap();
    let session: Arc<str> = session.unwrap().into();
    let initialized =
        serde_json::from_value(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
            .unwrap();
    adapter
        .post_message(
            uri.clone(),
            initialized,
            Some(session.clone()),
            Some("test-secret".into()),
            HashMap::new(),
        )
        .await
        .unwrap();
    let args = json!({"room_id":room["room_id"],"member_handle":member["member_handle"],"timeout_seconds":10});
    let request=serde_json::from_value(json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"chat_wait","arguments":args}})).unwrap();
    let response = adapter
        .post_message(
            uri.clone(),
            request,
            Some(session.clone()),
            Some("test-secret".into()),
            HashMap::new(),
        )
        .await
        .unwrap();
    assert!(matches!(response, StreamableHttpPostResponse::Sse(..)));
    drop(response);
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while service.active_streams() == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_eq!(
        a.call(
            "read",
            args.as_object()
                .unwrap()
                .iter()
                .filter(|(k, _)| k.as_str() != "timeout_seconds")
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<serde_json::Map<_, _>>()
                .into()
        )
        .await
        .unwrap_err()
        .code,
        "stream_busy"
    );
    let cancel=serde_json::from_value(json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":2,"reason":"explicit cancellation"}})).unwrap();
    adapter
        .post_message(
            uri,
            cancel,
            Some(session),
            Some("test-secret".into()),
            HashMap::new(),
        )
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while service.active_streams() != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let mut args = args;
    args["timeout_seconds"] = json!(0);
    assert_eq!(a.call("wait", args).await.unwrap()["status"], "timeout");
    a.close().await;
    service.finish_shutdown().await;
}

#[tokio::test]
async fn wait_limit_and_stale_shutdown_are_explicit() {
    let service = fixture();
    let room = call(&service, "open", mutation(json!({"name":"limits"}))).await;
    let mut members = Vec::new();
    for _ in 0..65 {
        members.push(
            call(
                &service,
                "join",
                mutation(json!({"room_id":room["room_id"],"harness":"pi","role":"peer"})),
            )
            .await,
        );
    }
    let cancel = CancellationToken::new();
    let mut tasks = Vec::new();
    for (i, member) in members.iter().take(64).enumerate() {
        let s = service.clone();
        let c = cancel.clone();
        let args = json!({"room_id":room["room_id"],"member_handle":member["member_handle"],"timeout_seconds":10});
        tasks.push(tokio::spawn(async move { s.call("wait", args, c).await }));
        while service.active_streams() < i + 1 {
            tokio::task::yield_now().await;
        }
    }
    let args = json!({"room_id":room["room_id"],"member_handle":members[64]["member_handle"]});
    assert_eq!(
        service
            .call("wait", args, CancellationToken::new())
            .await
            .unwrap_err()
            .code,
        "overloaded"
    );
    let mut headers = AUTH.to_vec();
    headers.push(("content-type", "application/json"));
    assert_eq!(
        http(
            service.router(),
            "/shutdown",
            &headers,
            Body::from(json!({"instance_id":"stale"}).to_string())
        )
        .await
        .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(service.health().status, "running");
    cancel.cancel();
    for task in tasks {
        assert_eq!(task.await.unwrap().unwrap_err().code, "cancelled");
    }
    assert_eq!(service.active_streams(), 0);
    assert_eq!(service.active_waits(), 0);
    let response = http(
        service.router(),
        "/shutdown",
        &headers,
        Body::from(json!({"instance_id":service.config().instance_id}).to_string()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(service.health().status, "stopping");
}

#[tokio::test]
async fn graceful_shutdown_delivers_wait_error_before_sdk_transport_closes() {
    let service = fixture();
    let client = chat_http::client(&service).await;
    let room = client
        .call("open", mutation(json!({"name":"drain"})))
        .await
        .unwrap();
    let member = client
        .call(
            "join",
            mutation(json!({"room_id":room["room_id"],"harness":"pi","role":"peer"})),
        )
        .await
        .unwrap();
    let wait = tokio::spawn(async move {
        client
            .call(
                "wait",
                json!({"room_id":room["room_id"],"member_handle":member["member_handle"]}),
            )
            .await
    });
    while service.active_streams() == 0 {
        tokio::task::yield_now().await;
    }
    let mut headers = AUTH.to_vec();
    headers.push(("content-type", "application/json"));
    let response = http(
        service.router(),
        "/shutdown",
        &headers,
        Body::from(json!({"instance_id":service.config().instance_id}).to_string()),
    )
    .await;
    let finish = tokio::spawn({
        let s = service.clone();
        async move { s.finish_shutdown().await }
    });
    assert_eq!(response.status(), StatusCode::OK);
    to_bytes(response.into_body(), 10000).await.unwrap();
    assert_eq!(wait.await.unwrap().unwrap_err().code, "server_stopping");
    finish.await.unwrap();
    assert!(service.transport_token().is_cancelled());
}

#[tokio::test]
async fn source_retry_does_not_block_another_stream() {
    let dir = support::scratch(HarnessId::Pi, "chat-http-source");
    let roots = support::roots(&dir);
    let native = dir.join("home/sessions/project").join(format!(
        "source_{}.jsonl",
        support::source_id(HarnessId::Pi)
    ));
    std::fs::create_dir_all(native.parent().unwrap()).unwrap();
    std::fs::copy(support::source_path(HarnessId::Pi), &native).unwrap();
    let mut bus = Bus::open_in(&dir.join("bus")).unwrap();
    let room = bus
        .chat_call("open", mutation(json!({"name":"source"})), &roots)
        .unwrap();
    let target=bus.chat_call("join",mutation(json!({"room_id":room["room_id"],"harness":"pi","role":"peer","session_id":support::source_id(HarnessId::Pi)})),&roots).unwrap();
    let observer = bus
        .chat_call(
            "join",
            mutation(json!({"room_id":room["room_id"],"harness":"pi","role":"observer"})),
            &roots,
        )
        .unwrap();
    let config = ServiceConfig::new(
        dir.canonicalize().unwrap(),
        bus.service_id().unwrap(),
        uuid::Uuid::new_v4().to_string(),
        "http://127.0.0.1:39171/mcp".into(),
        "test-secret".into(),
    )
    .unwrap();
    let service = ChatService::new(bus, roots, config).unwrap();
    let mut args = json!({"room_id":room["room_id"],"member_handle":observer["member_handle"],"target_member_id":target["member_id"]});
    let batch = call(&service, "observe", args.clone()).await;
    assert!(!batch["batch_id"].is_null());
    args["ack_batch_id"] = batch["batch_id"].clone();
    std::fs::remove_file(&native).unwrap();
    let task = tokio::spawn({
        let s = service.clone();
        async move { s.call("observe", args, CancellationToken::new()).await }
    });
    while service.active_streams() == 0 {
        tokio::task::yield_now().await;
    }
    tokio::time::sleep(std::time::Duration::from_millis(60)).await;
    assert!(!task.is_finished());
    let sent=tokio::time::timeout(std::time::Duration::from_millis(200),call(&service,"send",mutation(json!({"room_id":room["room_id"],"member_handle":target["member_handle"],"text":"other stream"})))).await.unwrap();
    assert_eq!(sent["status"], "ok");
    assert!(!task.is_finished());
    let error = task.await.unwrap().unwrap_err();
    assert_eq!(error.code, "source_unavailable");
    assert_eq!(error.ack_batch_id.as_deref(), batch["batch_id"].as_str());
}

#[tokio::test]
#[ignore = "requires loopback TCP permission; managed sandbox denies connect with WinError 10013"]
async fn live_network_sdk_clients() {
    use mukade::chat::client::{ChatClient, ClientConfig};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let dir = support::scratch(HarnessId::Pi, "chat-live");
    let bus = Bus::open_in(&dir).unwrap();
    let id = bus.service_id().unwrap();
    let instance = uuid::Uuid::new_v4().to_string();
    let endpoint = format!("http://{address}/mcp");
    let service = ChatService::new(
        bus,
        StoreRoots::default(),
        ServiceConfig::new(
            dir.canonicalize().unwrap(),
            id.clone(),
            instance.clone(),
            endpoint.clone(),
            "test-secret".into(),
        )
        .unwrap(),
    )
    .unwrap();
    let serve = tokio::spawn({
        let s = service.clone();
        async move { s.serve(listener).await }
    });
    // Explicitly diagnose host loopback restrictions before credential-redacted client errors.
    drop(
        tokio::net::TcpStream::connect(address)
            .await
            .expect("live loopback TCP connection is required"),
    );
    let config = ClientConfig {
        endpoint,
        token: "test-secret".into(),
        service_id: id,
        instance_id: instance,
    };
    let (a, b) = tokio::join!(ChatClient::connect(&config), ChatClient::connect(&config));
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.running().list_all_tools().await.unwrap().len(), 21);
    assert_eq!(b.call("rooms", json!({})).await.unwrap()["status"], "ok");
    a.close().await;
    b.close().await;
    service.finish_shutdown().await;
    serve.await.unwrap().unwrap();
}
