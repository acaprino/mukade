use std::{
    collections::HashSet,
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::{
    bus::{Bus, ChatError, chat_tool_definitions},
    roots::StoreRoots,
};
use axum::{
    Json, Router,
    body::{Body, to_bytes},
    extract::{Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    model::*,
    service::RequestContext,
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use subtle::ConstantTimeEq;
use tokio::sync::{Notify, Semaphore};
use tokio_util::sync::CancellationToken;

pub const MAX_BODY_BYTES: usize = 1_048_576;
pub const MAX_WAITS: usize = 64;
pub const MAX_BLOCKING_WORK: usize = 64;
pub const RESPONSE_DRAIN_TIMEOUT: Duration = Duration::from_secs(5);
pub const WAIT_RECHECK_INTERVAL: Duration = Duration::from_millis(500);
#[derive(Clone, Copy, Debug)]
pub struct HttpLimits {
    pub max_concurrent_requests: usize,
    pub upload_timeout: Duration,
    /// Maximum time a ready response chunk may remain undelivered. Producer silence
    /// (including a quiet wait) is bounded only by response_lifetime.
    pub response_idle_timeout: Duration,
    pub response_lifetime: Duration,
}
impl Default for HttpLimits {
    fn default() -> Self {
        Self {
            max_concurrent_requests: 256,
            upload_timeout: Duration::from_secs(10),
            response_idle_timeout: Duration::from_secs(30),
            response_lifetime: Duration::from_secs(330),
        }
    }
}

/// Lifecycle supplies the protected token, canonical directory and process instance identity.
/// Deliberately does not implement Debug: configuration contains a credential.
#[derive(Clone)]
pub struct ServiceConfig {
    pub data_dir: PathBuf,
    pub service_id: String,
    pub instance_id: String,
    pub endpoint: String,
    pub http_limits: HttpLimits,
    token: String,
    authority: String,
    listen_addr: SocketAddr,
}
impl ServiceConfig {
    pub fn new(
        data_dir: PathBuf,
        service_id: String,
        instance_id: String,
        endpoint: String,
        token: String,
    ) -> Result<Self, ChatError> {
        let url = reqwest::Url::parse(&endpoint)
            .map_err(|_| error("invalid_configuration", "Invalid service endpoint"))?;
        let address = url
            .host_str()
            .and_then(|s| s.trim_matches(['[', ']']).parse::<std::net::IpAddr>().ok());
        if url.scheme() != "http"
            || !address.is_some_and(|ip| ip.is_loopback())
            || url.path() != "/mcp"
            || url.query().is_some()
            || url.fragment().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
            || token.is_empty()
            || service_id.is_empty()
            || instance_id.is_empty()
            || !data_dir.is_absolute()
            || url.port_or_known_default() == Some(0)
            || axum::http::HeaderValue::from_str(&format!("Bearer {token}")).is_err()
        {
            return Err(error(
                "invalid_configuration",
                "Expected absolute data directory, identities, token and literal loopback HTTP /mcp endpoint",
            ));
        }
        let authority = match url.port() {
            Some(port) => format!("{}:{port}", url.host_str().unwrap()),
            None => url.host_str().unwrap().to_owned(),
        };
        let listen_addr = SocketAddr::new(address.unwrap(), url.port_or_known_default().unwrap());
        Ok(Self {
            data_dir,
            service_id,
            instance_id,
            endpoint,
            http_limits: HttpLimits::default(),
            token,
            authority,
            listen_addr,
        })
    }
    pub fn authority(&self) -> &str {
        &self.authority
    }
    pub fn listen_addr(&self) -> SocketAddr {
        self.listen_addr
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Health {
    pub service_id: String,
    pub instance_id: String,
    pub status: String,
    pub protocol_version: String,
}
#[derive(Default)]
struct Admission {
    stopping: bool,
    active: usize,
}
struct Inner {
    bus: Mutex<Bus>,
    roots: StoreRoots,
    config: ServiceConfig,
    admission: Mutex<Admission>,
    streams: Mutex<HashSet<String>>,
    changed: Notify,
    drained: Notify,
    responses: Mutex<usize>,
    responses_drained: Notify,
    work: Arc<Semaphore>,
    waits: Arc<Semaphore>,
    http: Arc<Semaphore>,
    stopping: CancellationToken,
    transport: CancellationToken,
}
#[derive(Clone)]
pub struct ChatService(Arc<Inner>);
struct AdmissionGuard(ChatService);
impl Drop for AdmissionGuard {
    fn drop(&mut self) {
        let mut gate = self.0.0.admission.lock().unwrap();
        gate.active -= 1;
        if gate.active == 0 {
            self.0.0.drained.notify_waiters();
        }
    }
}
struct StreamGuard {
    service: ChatService,
    key: String,
}
struct ResponseGuard(ChatService);
impl Drop for ResponseGuard {
    fn drop(&mut self) {
        let mut count = self.0.0.responses.lock().unwrap();
        *count -= 1;
        if *count == 0 {
            self.0.0.responses_drained.notify_waiters();
        }
    }
}
impl Drop for StreamGuard {
    fn drop(&mut self) {
        self.service.0.streams.lock().unwrap().remove(&self.key);
    }
}
pub(crate) fn error(code: &str, message: &str) -> ChatError {
    let mut error = ChatError::new(code, message);
    error.retryable = matches!(
        code,
        "overloaded" | "stream_busy" | "transport_failure" | "server_stopping"
    );
    error
}

impl ChatService {
    pub fn new(bus: Bus, roots: StoreRoots, config: ServiceConfig) -> Result<Self, ChatError> {
        let limits = config.http_limits;
        if limits.max_concurrent_requests == 0
            || limits.max_concurrent_requests > 1024
            || limits.upload_timeout.is_zero()
            || limits.response_idle_timeout.is_zero()
            || limits.response_lifetime.is_zero()
        {
            return Err(error(
                "invalid_configuration",
                "HTTP limits must be nonzero and concurrency at most 1024",
            ));
        }
        if bus.service_id()? != config.service_id {
            return Err(error(
                "service_mismatch",
                "Database service identity differs from configuration",
            ));
        }
        Ok(Self(Arc::new(Inner {
            bus: Mutex::new(bus),
            roots,
            config,
            admission: Mutex::default(),
            streams: Mutex::default(),
            changed: Notify::new(),
            drained: Notify::new(),
            responses: Mutex::new(0),
            responses_drained: Notify::new(),
            work: Arc::new(Semaphore::new(MAX_BLOCKING_WORK)),
            waits: Arc::new(Semaphore::new(MAX_WAITS)),
            http: Arc::new(Semaphore::new(limits.max_concurrent_requests)),
            stopping: CancellationToken::new(),
            transport: CancellationToken::new(),
        })))
    }
    pub fn config(&self) -> &ServiceConfig {
        &self.0.config
    }
    pub fn active_waits(&self) -> usize {
        MAX_WAITS - self.0.waits.available_permits()
    }
    pub fn active_http_requests(&self) -> usize {
        self.config().http_limits.max_concurrent_requests - self.0.http.available_permits()
    }
    pub fn active_streams(&self) -> usize {
        self.0.streams.lock().unwrap().len()
    }
    pub fn health(&self) -> Health {
        Health {
            service_id: self.config().service_id.clone(),
            instance_id: self.config().instance_id.clone(),
            status: if self.0.admission.lock().unwrap().stopping {
                "stopping"
            } else {
                "running"
            }
            .into(),
            protocol_version: "2025-11-25".into(),
        }
    }
    /// Close admission before signalling waits. Accepted blocking mutations are drained.
    pub fn begin_shutdown(&self) {
        self.0.admission.lock().unwrap().stopping = true;
        self.0.stopping.cancel();
    }
    pub fn stopping_token(&self) -> CancellationToken {
        self.0.stopping.clone()
    }
    pub fn transport_token(&self) -> CancellationToken {
        self.0.transport.clone()
    }
    pub async fn drain(&self) {
        loop {
            let notified = self.0.drained.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.0.admission.lock().unwrap().active == 0 {
                break;
            }
            notified.await;
        }
    }
    /// Drain admitted domain work, then allow up to five seconds for POST responses to
    /// finish before cancelling transport sessions. Slow/disconnected peers cannot hold
    /// the response phase indefinitely. Blocking database work is never forcibly killed.
    pub async fn finish_shutdown(&self) {
        self.begin_shutdown();
        self.drain().await;
        let _ = tokio::time::timeout(RESPONSE_DRAIN_TIMEOUT, async {
            loop {
                let changed = self.0.responses_drained.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                if *self.0.responses.lock().unwrap() == 0 {
                    break;
                }
                changed.await;
            }
        })
        .await;
        self.0.transport.cancel();
    }
    pub async fn serve(&self, listener: tokio::net::TcpListener) -> std::io::Result<()> {
        let addr: SocketAddr = listener.local_addr()?;
        if !addr.ip().is_loopback() || addr != self.config().listen_addr {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Listener must match configured loopback endpoint",
            ));
        }
        axum::serve(listener, self.router())
            .with_graceful_shutdown(self.transport_token().cancelled_owned())
            .await
    }
    pub async fn call(
        &self,
        operation: &str,
        arguments: Value,
        cancel: CancellationToken,
    ) -> Result<Value, ChatError> {
        let guard = {
            let mut gate = self.0.admission.lock().unwrap();
            if gate.stopping {
                return Err(error("server_stopping", "The service is stopping"));
            }
            // Bound admitted requests as well as running blocking jobs and waiting streams.
            if gate.active >= MAX_WAITS + MAX_BLOCKING_WORK {
                return Err(error("overloaded", "Too many active requests"));
            }
            gate.active += 1;
            AdmissionGuard(self.clone())
        };
        let service = self.clone();
        let operation = operation
            .strip_prefix("chat_")
            .unwrap_or(operation)
            .to_owned();
        // A lost HTTP response must not drop a legacy operation or release its consuming guard.
        tokio::spawn(async move {
            let _guard = guard;
            service.run(operation, arguments, cancel).await
        })
        .await
        .map_err(|_| error("storage_failure", "The service worker failed"))?
    }
    async fn blocking<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Bus, &StoreRoots) -> Result<T, ChatError> + Send + 'static,
    ) -> Result<T, ChatError> {
        let permit = self
            .0
            .work
            .clone()
            .try_acquire_owned()
            .map_err(|_| error("overloaded", "Blocking work limit reached"))?;
        let service = self.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let mut bus = service
                .0
                .bus
                .lock()
                .map_err(|_| error("storage_failure", "Database lock unavailable"))?;
            f(&mut bus, &service.0.roots)
        })
        .await
        .map_err(|_| error("storage_failure", "The database worker failed"))?
    }
    async fn native<T: Send + 'static>(
        &self,
        f: impl FnOnce(&StoreRoots) -> Result<T, ChatError> + Send + 'static,
    ) -> Result<T, ChatError> {
        let permit = self
            .0
            .work
            .clone()
            .try_acquire_owned()
            .map_err(|_| error("overloaded", "Blocking work limit reached"))?;
        let roots = self.0.roots.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            f(&roots)
        })
        .await
        .map_err(|_| error("storage_failure", "Native discovery worker failed"))?
    }
    async fn native_mutation(&self, op: String, args: Value) -> Result<Value, ChatError> {
        use crate::bus::chat_mutation::{self, Prepared};
        match self
            .blocking(move |bus, _| chat_mutation::prepare(bus, &op, args))
            .await?
        {
            Prepared::Replay(value) => Ok(value),
            Prepared::Native(work) => {
                let loaded = self.native(move |roots| work.load(roots)).await?;
                self.blocking(move |bus, _| chat_mutation::finish(bus, loaded))
                    .await
            }
        }
    }
    async fn run(
        &self,
        operation: String,
        mut args: Value,
        cancel: CancellationToken,
    ) -> Result<Value, ChatError> {
        let is_wait = operation == "wait";
        let _wait = if is_wait {
            Some(
                self.0
                    .waits
                    .clone()
                    .try_acquire_owned()
                    .map_err(|_| error("overloaded", "Maximum 64 concurrent waits reached"))?,
            )
        } else {
            None
        };
        let service = self.clone();
        let op = operation.clone();
        let input = args.clone();
        let (_stream, observation) = self
            .blocking(move |bus, _| {
                let Some(key) = bus.chat_stream_key(&op, &input)? else {
                    return Ok((None, None));
                };
                if !service.0.streams.lock().unwrap().insert(key.clone()) {
                    return Err(error(
                        "stream_busy",
                        "A consuming request is already active on this stream",
                    ));
                }
                let guard = StreamGuard { service, key };
                // Select the observation generation and acquire its guard under the same
                // database lock. Rebinding cannot move preparation to an unguarded stream.
                let prepared = if op == "observe" {
                    Some(crate::bus::chat_observe::prepare(bus, &input)?)
                } else {
                    None
                };
                Ok((Some(guard), prepared))
            })
            .await?;
        if let Some(prepared) = observation {
            return self
                .observe(prepared, args["ack_batch_id"].as_str().map(str::to_owned))
                .await;
        }
        let deadline = tokio::time::Instant::now()
            + Duration::from_secs(args["timeout_seconds"].as_u64().unwrap_or(30).min(300));
        let mut committed_ack = None;
        loop {
            // Register BEFORE reading committed state: mutations between read and sleep cannot be lost.
            let changed = self.0.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if is_wait && (cancel.is_cancelled() || self.0.stopping.is_cancelled()) {
                let mut e = if self.0.stopping.is_cancelled() {
                    error("server_stopping", "The service is stopping")
                } else {
                    error("cancelled", "The wait was cancelled")
                };
                e.ack_batch_id = committed_ack;
                return Err(e);
            }
            let op = operation.clone();
            let input = args.clone();
            let result = if matches!(op.as_str(), "join" | "rejoin" | "bind") {
                self.native_mutation(op, input).await
            } else if op == "candidates" && input.get("cursor").is_none_or(Value::is_null) {
                let query = input.clone();
                match self
                    .native(move |roots| {
                        crate::bus::chat_query::load_candidates("candidates", &query, roots)
                    })
                    .await
                {
                    Ok(rows) => {
                        self.blocking(move |bus, _| {
                            crate::bus::chat_query::finish(bus, "candidates", &input, rows)
                        })
                        .await
                    }
                    Err(e) => Err(e),
                }
            } else {
                self.blocking(move |bus, roots| bus.chat_call(&op, input, roots))
                    .await
            };
            let mut result = match result {
                Ok(value) => value,
                Err(mut e) => {
                    if e.ack_batch_id.is_none() {
                        e.ack_batch_id = committed_ack;
                    }
                    return Err(e);
                }
            };
            if let Some(ack) = result["ack_batch_id"].as_str() {
                committed_ack = Some(ack.to_owned());
            }
            if let Some(ack) = &committed_ack {
                result["ack_batch_id"] = json!(ack);
            }
            if matches!(operation.as_str(), "join" | "rejoin") {
                result["data_dir"] = json!(self.config().data_dir);
                result["endpoint"] = json!(self.config().endpoint);
            }
            if !is_wait {
                self.0.changed.notify_waiters();
                return Ok(result);
            }
            if result["status"] != "waiting" {
                return Ok(result);
            }
            args.as_object_mut().unwrap().remove("ack_batch_id");
            if tokio::time::Instant::now() >= deadline {
                result["status"] = json!("timeout");
                return Ok(result);
            }
            let recheck = deadline.min(tokio::time::Instant::now() + WAIT_RECHECK_INTERVAL);
            tokio::select! {
                _=changed => {},
                _=cancel.cancelled()=>{},
                _=self.0.stopping.cancelled()=>{},
                _=tokio::time::sleep_until(recheck)=>{},
            }
        }
    }
    async fn observe(
        &self,
        prepared: crate::bus::chat_observe::Prepared,
        ack: Option<String>,
    ) -> Result<Value, ChatError> {
        use crate::bus::chat_observe::{self, Prepared};
        match prepared {
            Prepared::Cached(value) => Ok(value),
            Prepared::Read(work) => {
                let result = async {
                    let permit = self
                        .0
                        .work
                        .clone()
                        .try_acquire_owned()
                        .map_err(|_| error("overloaded", "Blocking work limit reached"))?;
                    let roots = self.0.roots.clone();
                    let loaded = tokio::task::spawn_blocking(move || {
                        let _permit = permit;
                        work.load(&roots)
                    })
                    .await
                    .map_err(|_| error("storage_failure", "Native source worker failed"))??;
                    self.blocking(move |bus, _| chat_observe::finish(bus, loaded))
                        .await
                }
                .await;
                result.map_err(|mut e: ChatError| {
                    if e.ack_batch_id.is_none() {
                        e.ack_batch_id = ack;
                    }
                    e
                })
            }
        }
    }
    pub fn router(&self) -> Router {
        let handler = self.clone();
        let sdk = StreamableHttpService::new(
            move || Ok(handler.clone()),
            Arc::new(LocalSessionManager::default()),
            StreamableHttpServerConfig::default()
                .with_allowed_hosts([self.config().authority.clone()])
                .with_cancellation_token(self.transport_token()),
        );
        Router::new()
            .nest_service("/mcp", sdk)
            .route("/health", get(health))
            .route("/shutdown", post(shutdown))
            .with_state(self.clone())
            .layer(middleware::from_fn_with_state(self.clone(), boundary))
    }
}
async fn health(State(service): State<ChatService>) -> Json<Health> {
    Json(service.health())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Shutdown {
    instance_id: String,
}
async fn shutdown(State(service): State<ChatService>, Json(request): Json<Shutdown>) -> Response {
    if request.instance_id != service.config().instance_id {
        return (StatusCode::CONFLICT, "Instance identity mismatch").into_response();
    }
    service.begin_shutdown();
    Json(service.health()).into_response()
}
async fn boundary(State(service): State<ChatService>, request: Request, next: Next) -> Response {
    let headers = request.headers();
    let single = |name: &str| headers.get_all(name).iter().count() == 1;
    if !single("host")
        || headers.get("host").and_then(|v| v.to_str().ok()) != Some(service.config().authority())
        || request
            .uri()
            .authority()
            .is_some_and(|v| v.as_str() != service.config().authority())
        || headers.contains_key("origin")
    {
        return (
            StatusCode::FORBIDDEN,
            "Untrusted request authority or origin",
        )
            .into_response();
    }
    let expected = format!("Bearer {}", service.config().token);
    let auth = headers
        .get("authorization")
        .map(|v| v.as_bytes())
        .unwrap_or_default();
    if !single("authorization") || !bool::from(auth.ct_eq(expected.as_bytes())) {
        return (StatusCode::UNAUTHORIZED, "Authentication required").into_response();
    }
    if headers.contains_key("x-mukade-service-id")
        && (!single("x-mukade-service-id")
            || headers
                .get("x-mukade-service-id")
                .and_then(|v| v.to_str().ok())
                != Some(&service.config().service_id))
    {
        return (StatusCode::CONFLICT, "Service identity mismatch").into_response();
    }
    if let Some(version) = headers.get("mcp-protocol-version")
        && (!single("mcp-protocol-version")
            || !matches!(
                version.to_str().unwrap_or(""),
                "2024-11-05" | "2025-03-26" | "2025-06-18" | "2025-11-25"
            ))
    {
        return (StatusCode::BAD_REQUEST, "Unsupported MCP protocol version").into_response();
    }
    let http_permit = match service.0.http.clone().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "Concurrent HTTP request limit reached",
            )
                .into_response();
        }
    };
    let limits = service.config().http_limits;
    let (parts, body) = request.into_parts();
    let bytes = match tokio::time::timeout(limits.upload_timeout, to_bytes(body, MAX_BODY_BYTES))
        .await
    {
        Ok(Ok(bytes)) => bytes,
        Err(_) => {
            return (StatusCode::REQUEST_TIMEOUT, "HTTP upload deadline exceeded").into_response();
        }
        Ok(Err(_)) => {
            return (StatusCode::PAYLOAD_TOO_LARGE, "Request body exceeds 1 MiB").into_response();
        }
    };
    let response_guard = if parts.method == axum::http::Method::POST {
        *service.0.responses.lock().unwrap() += 1;
        Some(ResponseGuard(service.clone()))
    } else {
        None
    };
    let deadline = tokio::time::Instant::now() + limits.response_lifetime;
    let response = match tokio::time::timeout_at(
        deadline,
        next.run(Request::from_parts(parts, Body::from(bytes))),
    )
    .await
    {
        Ok(response) => response,
        Err(_) => {
            return (
                StatusCode::GATEWAY_TIMEOUT,
                "HTTP dispatch deadline exceeded",
            )
                .into_response();
        }
    };
    let (parts, body) = response.into_parts();
    let mut response = Response::from_parts(
        parts,
        super::http_body::bounded(
            body,
            limits,
            deadline,
            service.transport_token(),
            (http_permit, response_guard),
        ),
    );
    if let Ok(value) = service.config().service_id.parse() {
        response.headers_mut().insert("x-mukade-service-id", value);
    }
    response
}
impl ServerHandler for ChatService {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("mukade", env!("CARGO_PKG_VERSION")))
    }
    async fn initialize(
        &self,
        request: InitializeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, ErrorData> {
        if request.protocol_version.to_string().as_str() >= "2026-07-28" {
            return Err(ErrorData::invalid_params(
                "MCP 2026-07-28 is not supported",
                None,
            ));
        }
        context.peer.set_peer_info(request);
        Ok(self.get_info())
    }
    async fn list_tools(
        &self,
        _params: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let tools = chat_tool_definitions()
            .into_iter()
            .map(serde_json::from_value::<Tool>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ErrorData::internal_error("Invalid domain tool manifest", None))?;
        Ok(ListToolsResult::with_all_items(tools))
    }
    async fn call_tool(
        &self,
        params: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let operation = params.name.to_string();
        let args = Value::Object(params.arguments.unwrap_or_default());
        let request_id = args.get("request_id").cloned();
        Ok(match self.call(&operation, args, context.ct).await {
            Ok(value) => CallToolResult::structured(value),
            Err(error) => {
                let mut value = serde_json::to_value(error).unwrap();
                value["operation"] = json!(operation);
                value["service_id"] = json!(self.config().service_id);
                if let Some(id) = request_id {
                    value["request_id"] = id;
                }
                CallToolResult::structured_error(value)
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn admitted_mutation_survives_caller_drop_and_shutdown_drains_commit() {
        let base = std::env::var_os("MUKADE_TEST_SCRATCH_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let dir = base.join(format!("mukade-tests-drain-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let bus = Bus::open_in(&dir).unwrap();
        let id = bus.service_id().unwrap();
        let service = ChatService::new(
            bus,
            StoreRoots::default(),
            ServiceConfig::new(
                dir.canonicalize().unwrap(),
                id,
                uuid::Uuid::new_v4().to_string(),
                "http://127.0.0.1:39171/mcp".into(),
                "test".into(),
            )
            .unwrap(),
        )
        .unwrap();
        let (locked_tx, locked_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let blocker = std::thread::spawn({
            let s = service.clone();
            move || {
                let _lock = s.0.bus.lock().unwrap();
                locked_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            }
        });
        locked_rx.await.unwrap();
        let request = json!({"name":"admitted","request_id":uuid::Uuid::new_v4().to_string()});
        let replay = request.clone();
        let caller = tokio::spawn({
            let s = service.clone();
            async move { s.call("open", request, CancellationToken::new()).await }
        });
        while service.0.admission.lock().unwrap().active == 0 {
            tokio::task::yield_now().await;
        }
        caller.abort();
        service.begin_shutdown();
        let drain = tokio::spawn({
            let s = service.clone();
            async move { s.finish_shutdown().await }
        });
        tokio::task::yield_now().await;
        assert!(!drain.is_finished());
        assert!(!service.transport_token().is_cancelled());
        release_tx.send(()).unwrap();
        blocker.join().unwrap();
        drain.await.unwrap();
        assert!(service.transport_token().is_cancelled());
        let mut bus = service.0.bus.lock().unwrap();
        let result = bus
            .chat_call("open", replay, &StoreRoots::default())
            .unwrap();
        assert_eq!(result["name"], "admitted");
    }
}
