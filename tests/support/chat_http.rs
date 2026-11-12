//! Actual Axum HTTP requests + SDK-owned Streamable HTTP framing, without a TCP connection.
#![allow(dead_code)]
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{HeaderName, HeaderValue, Request, StatusCode},
    response::Response,
};
use futures_util::{StreamExt, stream::BoxStream};
use mukade::chat::{
    ChatService,
    client::{ChatClient, ClientConfig},
};
use rmcp::{
    ServiceExt,
    model::{ClientInfo, ClientJsonRpcMessage},
    transport::streamable_http_client::{
        SseError, StreamableHttpClient, StreamableHttpClientTransport, StreamableHttpError,
        StreamableHttpPostResponse,
    },
};
use sse_stream::{Sse, SseStream};
use std::{collections::HashMap, sync::Arc};
use tower::ServiceExt as _;

#[derive(Clone)]
pub struct RouterHttp(pub Router);
pub async fn raw_session(service: &ChatService) -> (RouterHttp, Arc<str>, Arc<str>) {
    let adapter = RouterHttp(service.router());
    let uri: Arc<str> = service.config().endpoint.clone().into();
    let initialize=serde_json::from_value(serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"raw-test","version":"1"}}})).unwrap();
    let (_, session) = adapter
        .post_message(
            uri.clone(),
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
    let session: Arc<str> = session.unwrap().into();
    let initialized = serde_json::from_value(
        serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    )
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
    (adapter, uri, session)
}
type Error = StreamableHttpError<std::io::Error>;
impl RouterHttp {
    async fn request(
        &self,
        method: &str,
        uri: &str,
        session: Option<&str>,
        auth: Option<String>,
        headers: HashMap<HeaderName, HeaderValue>,
        body: Body,
    ) -> Result<Response, Error> {
        let uri: axum::http::Uri = uri.parse().unwrap();
        let mut request = Request::builder()
            .method(method)
            .uri(uri.clone())
            .header("host", uri.authority().unwrap().as_str())
            .header("accept", "application/json, text/event-stream")
            .header("content-type", "application/json");
        if let Some(s) = session {
            request = request.header("mcp-session-id", s);
        }
        if let Some(token) = auth {
            request = request.header("authorization", format!("Bearer {token}"));
        }
        for (k, v) in headers {
            request = request.header(k, v);
        }
        let response = self
            .0
            .clone()
            .oneshot(request.body(body).unwrap())
            .await
            .unwrap();
        if response.status() == StatusCode::NOT_FOUND && session.is_some() {
            return Err(Error::SessionExpired);
        }
        if !response.status().is_success() {
            return Err(Error::UnexpectedServerResponse(
                format!("HTTP {}", response.status()).into(),
            ));
        }
        Ok(response)
    }
}
impl StreamableHttpClient for RouterHttp {
    type Error = std::io::Error;
    async fn post_message(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session_id: Option<Arc<str>>,
        auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<StreamableHttpPostResponse, Error> {
        let response = self
            .request(
                "POST",
                &uri,
                session_id.as_deref(),
                auth_header,
                custom_headers,
                Body::from(serde_json::to_vec(&message).unwrap()),
            )
            .await?;
        if response.status() == StatusCode::ACCEPTED {
            return Ok(StreamableHttpPostResponse::Accepted);
        }
        let session = response
            .headers()
            .get("mcp-session-id")
            .map(|s| s.to_str().unwrap().to_owned());
        if response
            .headers()
            .get("content-type")
            .is_some_and(|v| v.as_bytes().starts_with(b"text/event-stream"))
        {
            Ok(StreamableHttpPostResponse::Sse(
                SseStream::new(response.into_body()).boxed(),
                session,
            ))
        } else {
            let bytes = to_bytes(response.into_body(), 2_000_000).await.unwrap();
            Ok(StreamableHttpPostResponse::Json(
                serde_json::from_slice(&bytes)?,
                session,
            ))
        }
    }
    async fn delete_session(
        &self,
        uri: Arc<str>,
        session_id: Arc<str>,
        auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<(), Error> {
        self.request(
            "DELETE",
            &uri,
            Some(&session_id),
            auth_header,
            custom_headers,
            Body::empty(),
        )
        .await?;
        Ok(())
    }
    async fn get_stream(
        &self,
        uri: Arc<str>,
        session_id: Arc<str>,
        last_event_id: Option<String>,
        auth_header: Option<String>,
        mut custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<BoxStream<'static, Result<Sse, SseError>>, Error> {
        if let Some(id) = last_event_id {
            custom_headers.insert(
                HeaderName::from_static("last-event-id"),
                id.parse().unwrap(),
            );
        }
        let response = self
            .request(
                "GET",
                &uri,
                Some(&session_id),
                auth_header,
                custom_headers,
                Body::empty(),
            )
            .await?;
        Ok(SseStream::new(response.into_body()).boxed())
    }
}
pub async fn client(service: &ChatService) -> ChatClient {
    let config = ClientConfig {
        endpoint: service.config().endpoint.clone(),
        token: "test-secret".into(),
        service_id: service.config().service_id.clone(),
        instance_id: service.config().instance_id.clone(),
    };
    let response = RouterHttp(service.router())
        .request(
            "GET",
            &config.endpoint.replace("/mcp", "/health"),
            None,
            Some(config.token.clone()),
            config.transport_config().unwrap().custom_headers,
            Body::empty(),
        )
        .await
        .unwrap();
    let health: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 10000).await.unwrap()).unwrap();
    assert_eq!(health["service_id"], config.service_id);
    assert_eq!(health["instance_id"], config.instance_id);
    let running = ClientInfo::default()
        .serve(StreamableHttpClientTransport::with_client(
            RouterHttp(service.router()),
            config.transport_config().unwrap(),
        ))
        .await
        .unwrap();
    ChatClient::from_pinned_running(running, config.service_id)
}
