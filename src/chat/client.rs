//! SDK client with explicit identity pinning; credentials are never included in errors.
use super::{Health, server::error};
use crate::bus::ChatError;
use rmcp::{
    RoleClient, ServiceExt,
    model::{
        CallToolRequest, CallToolRequestParams, CancelledNotificationParam, ClientInfo,
        ClientRequest, RequestId, ServerResult,
    },
    service::{PeerRequestOptions, RunningService},
    transport::streamable_http_client::{
        StreamableHttpClientTransport, StreamableHttpClientTransportConfig,
    },
};
use serde_json::Value;
use std::{collections::HashMap, time::Duration};

/// Deliberately does not implement Debug because it contains a bearer credential.
#[derive(Clone)]
pub struct ClientConfig {
    pub endpoint: String,
    pub token: String,
    pub service_id: String,
    pub instance_id: String,
}
pub struct ChatClient {
    running: RunningService<RoleClient, ClientInfo>,
    service_id: String,
    /// Requests sent and not answered yet; [`ChatClient::close`] cancels them.
    in_flight: std::sync::Mutex<Vec<RequestId>>,
}
impl ClientConfig {
    fn http(&self) -> Result<reqwest::Client, ChatError> {
        // Reuse the server's literal-loopback URL checks, without opening a data directory.
        super::ServiceConfig::new(
            std::env::temp_dir(),
            self.service_id.clone(),
            self.instance_id.clone(),
            self.endpoint.clone(),
            self.token.clone(),
        )?;
        reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .build()
            .map_err(|_| error("transport_failure", "Could not create local HTTP client"))
    }
    pub async fn health(&self) -> Result<Health, ChatError> {
        self.health_with(&self.http()?).await
    }
    async fn health_with(&self, http: &reqwest::Client) -> Result<Health, ChatError> {
        let response = http
            .get(format!("{}/health", self.endpoint.trim_end_matches("/mcp")))
            .bearer_auth(&self.token)
            .header("x-mukade-service-id", &self.service_id)
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .map_err(|_| error("transport_failure", "Could not contact local chat service"))?;
        if !response.status().is_success() {
            return Err(error(
                "service_mismatch",
                "Chat health or authentication check failed",
            ));
        }
        let health: Health = response
            .json()
            .await
            .map_err(|_| error("protocol_failure", "Invalid health response"))?;
        if health.service_id != self.service_id || health.instance_id != self.instance_id {
            return Err(error("service_mismatch", "Chat service identity changed"));
        }
        Ok(health)
    }
    pub async fn shutdown(&self) -> Result<Health, ChatError> {
        let http = self.http()?;
        self.health_with(&http).await?;
        let response = http
            .post(format!(
                "{}/shutdown",
                self.endpoint.trim_end_matches("/mcp")
            ))
            .bearer_auth(&self.token)
            .header("x-mukade-service-id", &self.service_id)
            .json(&serde_json::json!({"instance_id":self.instance_id}))
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .map_err(|_| error("transport_failure", "Could not request shutdown"))?;
        if !response.status().is_success() {
            return Err(error("service_mismatch", "Shutdown refused by service"));
        }
        let health: Health = response
            .json()
            .await
            .map_err(|_| error("protocol_failure", "Invalid shutdown response"))?;
        if health.service_id != self.service_id || health.instance_id != self.instance_id {
            return Err(error("service_mismatch", "Chat service identity changed"));
        }
        Ok(health)
    }
    /// Also used by embedding clients. Call health first; every request retains the service pin.
    pub fn transport_config(&self) -> Result<StreamableHttpClientTransportConfig, ChatError> {
        let mut headers = HashMap::new();
        headers.insert(
            reqwest::header::HeaderName::from_static("x-mukade-service-id"),
            self.service_id
                .parse()
                .map_err(|_| error("invalid_configuration", "Invalid service identity"))?,
        );
        Ok(
            StreamableHttpClientTransportConfig::with_uri(self.endpoint.clone())
                .auth_header(self.token.clone())
                .reinit_on_expired_session(false)
                .custom_headers(headers),
        )
    }
}
impl ChatClient {
    pub async fn connect(config: &ClientConfig) -> Result<Self, ChatError> {
        let http = config.http()?;
        let health = config.health_with(&http).await?;
        if health.status != "running" {
            return Err(error("server_stopping", "The service is stopping"));
        }
        let running = tokio::time::timeout(
            Duration::from_secs(10),
            ClientInfo::default().serve(StreamableHttpClientTransport::with_client(
                http,
                config.transport_config()?,
            )),
        )
        .await
        .map_err(|_| error("transport_failure", "Chat initialization timed out"))?
        .map_err(|_| error("protocol_failure", "Chat initialization failed"))?;
        Ok(Self::from_pinned_running(
            running,
            config.service_id.clone(),
        ))
    }
    /// For embedding/testing: caller has authenticated health and pinned this identity in the transport headers.
    pub fn from_pinned_running(
        running: RunningService<RoleClient, ClientInfo>,
        service_id: String,
    ) -> Self {
        Self {
            running,
            service_id,
            in_flight: std::sync::Mutex::new(Vec::new()),
        }
    }
    pub fn running(&self) -> &RunningService<RoleClient, ClientInfo> {
        &self.running
    }
    pub async fn call(&self, operation: &str, args: Value) -> Result<Value, ChatError> {
        let args = args
            .as_object()
            .cloned()
            .ok_or_else(|| error("invalid_arguments", "Tool arguments must be an object"))?;
        let name = if operation.starts_with("chat_") {
            operation.to_owned()
        } else {
            format!("chat_{operation}")
        };
        let failed = || error("transport_failure", "Chat tool request failed");
        let handle = self
            .running
            .send_cancellable_request(
                ClientRequest::CallToolRequest(CallToolRequest::new(
                    CallToolRequestParams::new(name).with_arguments(args),
                )),
                PeerRequestOptions::no_options(),
            )
            .await
            .map_err(|_| failed())?;
        let id = handle.id.clone();
        self.in_flight.lock().unwrap().push(id.clone());
        let response = handle.await_response().await;
        self.in_flight.lock().unwrap().retain(|other| *other != id);
        let result = match response {
            Ok(ServerResult::CallToolResult(result)) => result,
            _ => return Err(failed()),
        };
        let value = result
            .structured_content
            .ok_or_else(|| error("protocol_failure", "Missing structured tool response"))?;
        if value["service_id"].as_str() != Some(&self.service_id) {
            return Err(error(
                "service_mismatch",
                "Tool response service identity changed",
            ));
        }
        if result.is_error == Some(true) {
            return Err(serde_json::from_value(value)
                .map_err(|_| error("protocol_failure", "Invalid structured tool error"))?);
        }
        Ok(value)
    }
    /// Close the session. A request still running (a `wait` whose caller was stopped) is
    /// cancelled first: closing alone leaves the service to finish it, holding the member's
    /// stream until then.
    pub async fn close(self) {
        let pending = std::mem::take(&mut *self.in_flight.lock().unwrap());
        for id in pending {
            let cancelled = CancelledNotificationParam::new(Some(id), Some("closed".into()));
            let _ = tokio::time::timeout(
                Duration::from_secs(2),
                self.running.notify_cancelled(cancelled),
            )
            .await;
        }
        let _ = self.running.cancel().await;
    }
}
