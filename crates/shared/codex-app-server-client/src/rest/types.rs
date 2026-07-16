use std::{collections::HashMap, future::Future, pin::Pin, time::Duration};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{CompatibilityReport, Error, SessionOptions, TextTurnResult};

pub type RestResult<T> = std::result::Result<T, RestError>;
pub type RestFuture<T> = Pin<Box<dyn Future<Output = RestResult<T>> + Send + 'static>>;

/// REST router behavior knobs.
///
/// [`Default`] is intentionally non-executing: only health and compatibility are
/// mounted. Enable the text-turn helper or raw bridge routes explicitly when the
/// caller mounts the router behind its own authz boundary.
#[derive(Clone, Debug, Default)]
pub struct RestRouterOptions {
    pub enable_text_turn_route: bool,
    pub enable_bridge_routes: bool,
    pub allow_unsafe_client_options: bool,
    pub limits: RestLimits,
}

impl RestRouterOptions {
    /// Enables the one-shot text-turn helper without raw callable/session routes.
    pub fn text_turn() -> Self {
        Self {
            enable_text_turn_route: true,
            enable_bridge_routes: false,
            allow_unsafe_client_options: false,
            limits: RestLimits::default(),
        }
    }

    /// Enables the full raw callable bridge for trusted deployments.
    pub fn trusted_bridge() -> Self {
        Self {
            enable_text_turn_route: true,
            enable_bridge_routes: true,
            allow_unsafe_client_options: false,
            limits: RestLimits::default(),
        }
    }

    /// Allows request bodies to override the Codex command, extra arguments, and
    /// app-server config. This is intentionally separate from
    /// [`Self::trusted_bridge`]: admitting a caller to the bridge is not the same
    /// as allowing that caller to choose host executables or weaken sandboxing.
    pub fn with_unsafe_client_options(mut self, allow: bool) -> Self {
        self.allow_unsafe_client_options = allow;
        self
    }

    pub fn with_max_sessions(mut self, max_sessions: usize) -> Self {
        self.limits.max_sessions = max_sessions;
        self
    }

    pub fn with_max_poll_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.limits.max_poll_timeout = Duration::from_millis(timeout_ms);
        self
    }

    pub fn with_max_text_turn_duration_ms(mut self, timeout_ms: u64) -> Self {
        self.limits.max_text_turn_duration = Duration::from_millis(timeout_ms);
        self
    }

    pub fn with_max_text_turn_output_bytes(mut self, max_bytes: usize) -> Self {
        self.limits.max_text_turn_output_bytes = max_bytes;
        self
    }
}

/// Resource limits used by the default REST backend and route layer.
#[derive(Clone, Debug)]
pub struct RestLimits {
    pub max_sessions: usize,
    pub max_one_shot_concurrency: usize,
    pub max_session_call_concurrency: usize,
    pub max_session_call_concurrency_per_session: usize,
    pub max_poll_timeout: Duration,
    pub max_text_turn_duration: Duration,
    pub max_text_turn_output_bytes: usize,
    pub pending_request_ttl: Duration,
    pub max_pending_requests_per_session: usize,
    pub idle_session_ttl: Duration,
    pub compatibility_ttl: Duration,
}

impl Default for RestLimits {
    fn default() -> Self {
        Self {
            max_sessions: 16,
            max_one_shot_concurrency: 4,
            max_session_call_concurrency: 64,
            max_session_call_concurrency_per_session: 8,
            max_poll_timeout: Duration::from_secs(30),
            max_text_turn_duration: Duration::from_secs(10 * 60),
            max_text_turn_output_bytes: 1024 * 1024,
            pending_request_ttl: Duration::from_secs(600),
            max_pending_requests_per_session: 64,
            idle_session_ttl: Duration::from_secs(30 * 60),
            compatibility_ttl: Duration::from_secs(30),
        }
    }
}

/// Errors surfaced by the optional REST adapter.
#[derive(Debug, thiserror::Error)]
pub enum RestError {
    #[error("{0}")]
    NotFound(String),

    #[error("{0}")]
    Gone(String),

    #[error("{0}")]
    Forbidden(String),

    #[error("{0}")]
    InvalidRequest(String),

    #[error("{0}")]
    RateLimited(String),

    #[error("{0}")]
    Conflict(String),

    #[error("{0}")]
    TimedOut(String),

    #[error("{0}")]
    PayloadTooLarge(String),

    #[error("{0}")]
    Internal(String),

    #[error(transparent)]
    Client(#[from] Error),
}

impl From<serde_json::Error> for RestError {
    fn from(error: serde_json::Error) -> Self {
        Self::Client(error.into())
    }
}

/// Backend used by the REST router.
///
/// The default [`crate::rest::CodexRestBackend`] talks to real `codex app-server`
/// processes. Tests and host applications can inject their own backend with
/// [`crate::rest::router_with_backend`] to control process lifecycle, pooling, or policy.
pub trait RestBackend: Send + Sync + 'static {
    fn compatibility_report(&self) -> RestFuture<CompatibilityReport>;
    fn run_text_turn(&self, request: RestTextTurnRequest) -> RestFuture<RestTextTurnResponse>;
    fn create_session(
        &self,
        _request: RestSessionCreateRequest,
    ) -> RestFuture<RestSessionCreateResponse> {
        Box::pin(async move {
            Err(RestError::NotFound(
                "REST session bridge is not implemented by this backend".to_owned(),
            ))
        })
    }
    fn list_sessions(&self) -> RestFuture<Vec<RestSessionSummary>> {
        Box::pin(async move { Ok(Vec::new()) })
    }
    fn delete_session(&self, session_id: String) -> RestFuture<RestStatusResponse> {
        Box::pin(async move {
            Err(RestError::NotFound(format!(
                "session `{session_id}` was not found"
            )))
        })
    }
    fn call_method(&self, request: RestCallRequest) -> RestFuture<RestCallResponse> {
        Box::pin(async move {
            Err(RestError::NotFound(format!(
                "REST raw call `{}` is not implemented by this backend",
                request.method
            )))
        })
    }
    fn poll_event(
        &self,
        session_id: String,
        timeout_ms: Option<u64>,
    ) -> RestFuture<RestEventResponse> {
        let _ = timeout_ms;
        Box::pin(async move {
            Err(RestError::NotFound(format!(
                "session `{session_id}` was not found"
            )))
        })
    }
    fn reply_request_result(
        &self,
        session_id: String,
        request_key: String,
        body: RestRequestReplyResultRequest,
    ) -> RestFuture<RestRequestReplyResponse> {
        let _ = (request_key, body);
        Box::pin(async move {
            Err(RestError::NotFound(format!(
                "session `{session_id}` was not found"
            )))
        })
    }
    fn reply_request_error(
        &self,
        session_id: String,
        request_key: String,
        body: RestErrorReplyRequest,
    ) -> RestFuture<RestRequestReplyResponse> {
        let _ = (request_key, body);
        Box::pin(async move {
            Err(RestError::NotFound(format!(
                "session `{session_id}` was not found"
            )))
        })
    }
}

/// Health response returned by `GET /health` and `GET /v1/health`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RestHealthResponse {
    pub status: String,
}

/// REST approval policy preset used while collecting turn events.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RestApprovalPolicy {
    #[default]
    DenyAll,
    ReadOnly,
    AllowAll,
}

/// Optional client/session overrides for REST requests that spawn Codex.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestClientOptions {
    pub name: Option<String>,
    pub version: Option<String>,
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_args: Vec<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub config: HashMap<String, String>,
    pub call_timeout_ms: Option<u64>,
}

impl RestClientOptions {
    pub(super) fn into_session_options(self, default_name: &str) -> SessionOptions {
        let mut options = SessionOptions::new(
            self.name.unwrap_or_else(|| default_name.to_owned()),
            self.version
                .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_owned()),
        );
        if let Some(command) = self.command {
            options = options.with_command(command);
        }
        for arg in self.extra_args {
            options = options.with_extra_arg(arg);
        }
        for (key, value) in self.config {
            options = options.with_config(key, value);
        }
        if let Some(timeout_ms) = self.call_timeout_ms {
            options = options.with_call_timeout(Duration::from_millis(timeout_ms));
        }
        options
    }
}

/// Request body for `POST /v1/text-turn`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestTextTurnRequest {
    pub prompt: String,
    pub model: Option<String>,
    pub approval_policy: Option<RestApprovalPolicy>,
    pub client: Option<RestClientOptions>,
}

impl RestTextTurnRequest {
    pub fn session_options(&self) -> SessionOptions {
        session_options_from(self.client.clone(), "codex_app_server_rest")
    }
}

/// Response body for `POST /v1/text-turn`.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestTextTurnResponse {
    pub thread_id: String,
    pub turn_id: String,
    pub turn_status: Option<String>,
    pub agent_message: String,
    pub latest_diff: Option<String>,
    pub errors: Vec<Value>,
}

impl From<TextTurnResult> for RestTextTurnResponse {
    fn from(result: TextTurnResult) -> Self {
        let agent_message = result.agent_message().to_owned();
        let latest_diff = result.latest_diff().map(str::to_owned);
        let turn_status = result.events.terminal_status().and_then(|status| {
            serde_json::to_value(status)
                .ok()?
                .as_str()
                .map(str::to_owned)
        });
        let errors = result
            .errors()
            .iter()
            .map(|error| {
                serde_json::to_value(error).unwrap_or_else(|_| Value::String(error.message.clone()))
            })
            .collect();
        Self {
            thread_id: result.thread.thread.id,
            turn_id: result.turn.turn.id,
            turn_status,
            agent_message,
            latest_diff,
            errors,
        }
    }
}

/// Request body for raw method bridge calls.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestCallBody {
    #[serde(default)]
    pub params: Value,
    pub client: Option<RestClientOptions>,
}

/// Backend-facing raw method call request.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestCallRequest {
    pub session_id: Option<String>,
    pub method: String,
    pub params: Value,
    pub client: Option<RestClientOptions>,
}

/// Response body for raw method bridge calls.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestCallResponse {
    pub method: String,
    pub result: Value,
}

/// Request body for `POST /v1/sessions`.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestSessionCreateRequest {
    pub client: Option<RestClientOptions>,
}

/// Response body for `POST /v1/sessions`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestSessionCreateResponse {
    pub session_id: String,
    pub initialize_response: Value,
}

/// One session entry in `GET /v1/sessions`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestSessionSummary {
    pub session_id: String,
}

/// Response body for `GET /v1/sessions`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RestListSessionsResponse {
    pub sessions: Vec<RestSessionSummary>,
}

/// Simple status response used by mutating bridge endpoints.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RestStatusResponse {
    pub status: String,
}

/// Event returned by `GET /v1/sessions/{sessionId}/events`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(
    tag = "event",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum RestEventResponse {
    Notification {
        notification: Value,
    },
    Request {
        request_key: String,
        request_id: Value,
        method: String,
        request: Value,
    },
    Closed,
    Timeout,
}

impl RestEventResponse {
    pub fn notification(notification: Value) -> Self {
        Self::Notification { notification }
    }

    pub fn request(
        request_key: impl Into<String>,
        request_id: Value,
        method: impl Into<String>,
        request: Value,
    ) -> Self {
        Self::Request {
            request_key: request_key.into(),
            request_id,
            method: method.into(),
            request,
        }
    }

    pub fn closed() -> Self {
        Self::Closed
    }

    pub fn timeout() -> Self {
        Self::Timeout
    }
}

/// Request body for replying successfully to a server-originated request.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestRequestReplyResultRequest {
    pub result: Value,
}

/// Request body for replying with an error to a server-originated request.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestErrorReplyRequest {
    pub code: i64,
    pub message: String,
    pub data: Option<Value>,
}

/// Response body for request-reply endpoints.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RestRequestReplyResponse {
    pub status: String,
}

/// Structured REST error payload.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RestErrorResponse {
    pub error: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

pub(super) fn session_options_from(
    client: Option<RestClientOptions>,
    default_name: &str,
) -> SessionOptions {
    client
        .unwrap_or_default()
        .into_session_options(default_name)
}
