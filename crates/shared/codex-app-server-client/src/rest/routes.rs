use std::{
    collections::HashSet,
    sync::{Arc, Mutex as StdMutex},
};

use axum::{
    extract::{rejection::JsonRejection, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::{delete, get, post},
    Router,
};
use serde::Deserialize;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::Error;

use super::{
    backend::CodexRestBackend,
    types::{
        RestApprovalPolicy, RestBackend, RestCallBody, RestCallRequest, RestClientOptions,
        RestError, RestErrorReplyRequest, RestErrorResponse, RestHealthResponse,
        RestListSessionsResponse, RestRequestReplyResultRequest, RestResult, RestRouterOptions,
        RestSessionCreateRequest, RestTextTurnRequest,
    },
};

#[derive(Clone)]
struct RestState {
    backend: Arc<dyn RestBackend>,
    options: RestRouterOptions,
    one_shot_gate: Arc<Semaphore>,
    active_polls: Arc<StdMutex<HashSet<String>>>,
}

struct ActivePollGuard {
    active_polls: Arc<StdMutex<HashSet<String>>>,
    session_id: String,
}

impl Drop for ActivePollGuard {
    fn drop(&mut self) {
        let mut active = self
            .active_polls
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        active.remove(&self.session_id);
    }
}

/// Builds a conservative REST router backed by real `codex app-server` processes.
///
/// The default router exposes health, compatibility, and the text-turn helper.
/// It does not mount the raw callable bridge/session routes. Use
/// [`trusted_bridge_router`] or [`router_with_options`] when the router is
/// mounted behind a trusted authz boundary and should expose the full bridge.
pub fn router() -> Router {
    router_with_options(RestRouterOptions::default())
}

/// Builds a trusted full bridge router backed by real `codex app-server` processes.
///
/// Routes:
/// - `GET /health`
/// - `GET /v1/health`
/// - `GET /v1/compatibility`
/// - `POST /v1/text-turn`
/// - `POST /v1/call/{method}`
/// - `GET|POST /v1/sessions`
/// - `DELETE /v1/sessions/{sessionId}`
/// - `POST /v1/sessions/{sessionId}/call/{method}`
/// - `GET /v1/sessions/{sessionId}/events`
/// - `POST /v1/sessions/{sessionId}/requests/{requestKey}/result`
/// - `POST /v1/sessions/{sessionId}/requests/{requestKey}/error`
pub fn trusted_bridge_router() -> Router {
    router_with_options(RestRouterOptions::trusted_bridge())
}

/// Builds a REST router backed by real `codex app-server` processes and options.
pub fn router_with_options(options: RestRouterOptions) -> Router {
    router_with_backend_and_options(
        CodexRestBackend::with_limits(options.limits.clone()),
        options,
    )
}

/// Builds a REST router with a caller-provided backend.
pub fn router_with_backend<B>(backend: B) -> Router
where
    B: RestBackend,
{
    router_with_backend_and_options(backend, RestRouterOptions::default())
}

/// Builds a REST router with a caller-provided backend and options.
pub fn router_with_backend_and_options<B>(backend: B, options: RestRouterOptions) -> Router
where
    B: RestBackend,
{
    router_with_backend_arc_and_options(Arc::new(backend), options)
}

/// Builds a REST router from a shared backend trait object.
pub fn router_with_backend_arc(backend: Arc<dyn RestBackend>) -> Router {
    router_with_backend_arc_and_options(backend, RestRouterOptions::default())
}

/// Builds a REST router from a shared backend trait object and options.
pub fn router_with_backend_arc_and_options(
    backend: Arc<dyn RestBackend>,
    options: RestRouterOptions,
) -> Router {
    let state = RestState {
        backend,
        one_shot_gate: Arc::new(Semaphore::new(options.limits.max_one_shot_concurrency)),
        active_polls: Arc::default(),
        options: options.clone(),
    };

    let router = Router::new()
        .route("/health", get(health))
        .route("/v1/health", get(health))
        .route("/v1/compatibility", get(compatibility))
        .route("/v1/text-turn", post(text_turn));

    let router = if options.enable_bridge_routes {
        router
            .route("/v1/call/{*method}", post(call_method))
            .route("/v1/sessions", get(list_sessions).post(create_session))
            .route("/v1/sessions/{session_id}", delete(delete_session))
            .route(
                "/v1/sessions/{session_id}/call/{*method}",
                post(call_session_method),
            )
            .route("/v1/sessions/{session_id}/events", get(poll_event))
            .route(
                "/v1/sessions/{session_id}/requests/{request_key}/result",
                post(reply_request_result),
            )
            .route(
                "/v1/sessions/{session_id}/requests/{request_key}/error",
                post(reply_request_error),
            )
    } else {
        router
    };

    router.with_state(state)
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EventQuery {
    timeout_ms: Option<u64>,
}

async fn health() -> impl IntoResponse {
    Json(RestHealthResponse {
        status: "ok".to_owned(),
    })
}

async fn compatibility(State(state): State<RestState>) -> impl IntoResponse {
    Json(state.backend.compatibility_report())
}

async fn text_turn(
    State(state): State<RestState>,
    body: std::result::Result<Json<RestTextTurnRequest>, JsonRejection>,
) -> Response {
    let Json(request) = match body {
        Ok(body) => body,
        Err(error) => return invalid_json(error),
    };
    if request.prompt.trim().is_empty() {
        return invalid_request("prompt must not be empty");
    }
    if let Err(error) = validate_text_turn_request(&state.options, &request) {
        return rest_error(error);
    }

    let _permit = match acquire_one_shot_permit(&state) {
        Ok(permit) => permit,
        Err(error) => return rest_error(error),
    };
    match state.backend.run_text_turn(request).await {
        Ok(response) => Json(response).into_response(),
        Err(error) => rest_error(error),
    }
}

async fn call_method(
    State(state): State<RestState>,
    Path(method): Path<String>,
    body: std::result::Result<Json<RestCallBody>, JsonRejection>,
) -> Response {
    let method = match normalize_method(method) {
        Some(method) => method,
        None => return invalid_request("method path must not be empty"),
    };
    let Json(body) = match body {
        Ok(body) => body,
        Err(error) => return invalid_json(error),
    };
    if let Err(error) = validate_client_options(&state.options, body.client.as_ref()) {
        return rest_error(error);
    }
    let _permit = match acquire_one_shot_permit(&state) {
        Ok(permit) => permit,
        Err(error) => return rest_error(error),
    };
    let request = RestCallRequest {
        session_id: None,
        method,
        params: body.params,
        client: body.client,
    };
    match state.backend.call_method(request).await {
        Ok(response) => Json(response).into_response(),
        Err(error) => rest_error(error),
    }
}

async fn create_session(
    State(state): State<RestState>,
    body: std::result::Result<Json<RestSessionCreateRequest>, JsonRejection>,
) -> Response {
    let Json(request) = match body {
        Ok(body) => body,
        Err(error) => return invalid_json(error),
    };
    if let Err(error) = validate_client_options(&state.options, request.client.as_ref()) {
        return rest_error(error);
    }
    match state.backend.list_sessions().await {
        Ok(sessions) if sessions.len() >= state.options.limits.max_sessions => {
            return rest_error(RestError::RateLimited(format!(
                "maximum REST session count ({}) reached",
                state.options.limits.max_sessions
            )));
        }
        Ok(_) => {}
        Err(error) => return rest_error(error),
    }
    match state.backend.create_session(request).await {
        Ok(response) => Json(response).into_response(),
        Err(error) => rest_error(error),
    }
}

async fn list_sessions(State(state): State<RestState>) -> Response {
    match state.backend.list_sessions().await {
        Ok(sessions) => Json(RestListSessionsResponse { sessions }).into_response(),
        Err(error) => rest_error(error),
    }
}

async fn delete_session(
    State(state): State<RestState>,
    Path(session_id): Path<String>,
) -> Response {
    match state.backend.delete_session(session_id).await {
        Ok(response) => Json(response).into_response(),
        Err(error) => rest_error(error),
    }
}

async fn call_session_method(
    State(state): State<RestState>,
    Path((session_id, method)): Path<(String, String)>,
    body: std::result::Result<Json<RestCallBody>, JsonRejection>,
) -> Response {
    let method = match normalize_method(method) {
        Some(method) => method,
        None => return invalid_request("method path must not be empty"),
    };
    let Json(body) = match body {
        Ok(body) => body,
        Err(error) => return invalid_json(error),
    };
    if let Err(error) = validate_client_options(&state.options, body.client.as_ref()) {
        return rest_error(error);
    }
    let request = RestCallRequest {
        session_id: Some(session_id),
        method,
        params: body.params,
        client: body.client,
    };
    match state.backend.call_method(request).await {
        Ok(response) => Json(response).into_response(),
        Err(error) => rest_error(error),
    }
}

async fn poll_event(
    State(state): State<RestState>,
    Path(session_id): Path<String>,
    Query(query): Query<EventQuery>,
) -> Response {
    let _guard = match acquire_poll_guard(&state, &session_id) {
        Ok(guard) => guard,
        Err(error) => return rest_error(error),
    };
    let timeout_ms = Some(clamp_poll_timeout_ms(
        &state.options,
        query
            .timeout_ms
            .unwrap_or(state.options.limits.max_poll_timeout.as_millis() as u64),
    ));
    match state.backend.poll_event(session_id, timeout_ms).await {
        Ok(response) => Json(response).into_response(),
        Err(error) => rest_error(error),
    }
}

async fn reply_request_result(
    State(state): State<RestState>,
    Path((session_id, request_key)): Path<(String, String)>,
    body: std::result::Result<Json<RestRequestReplyResultRequest>, JsonRejection>,
) -> Response {
    let Json(body) = match body {
        Ok(body) => body,
        Err(error) => return invalid_json(error),
    };
    match state
        .backend
        .reply_request_result(session_id, request_key, body)
        .await
    {
        Ok(response) => Json(response).into_response(),
        Err(error) => rest_error(error),
    }
}

async fn reply_request_error(
    State(state): State<RestState>,
    Path((session_id, request_key)): Path<(String, String)>,
    body: std::result::Result<Json<RestErrorReplyRequest>, JsonRejection>,
) -> Response {
    let Json(body) = match body {
        Ok(body) => body,
        Err(error) => return invalid_json(error),
    };
    match state
        .backend
        .reply_request_error(session_id, request_key, body)
        .await
    {
        Ok(response) => Json(response).into_response(),
        Err(error) => rest_error(error),
    }
}

fn invalid_request(message: impl Into<String>) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(RestErrorResponse {
            error: "invalid_request".to_owned(),
            message: message.into(),
            code: None,
            data: None,
        }),
    )
        .into_response()
}

fn invalid_json(error: JsonRejection) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(RestErrorResponse {
            error: "invalid_json".to_owned(),
            message: error.to_string(),
            code: None,
            data: None,
        }),
    )
        .into_response()
}

fn rest_error(error: RestError) -> Response {
    match error {
        RestError::NotFound(message) => (
            StatusCode::NOT_FOUND,
            Json(RestErrorResponse {
                error: "not_found".to_owned(),
                message,
                code: None,
                data: None,
            }),
        )
            .into_response(),
        RestError::Gone(message) => (
            StatusCode::GONE,
            Json(RestErrorResponse {
                error: "gone".to_owned(),
                message,
                code: None,
                data: None,
            }),
        )
            .into_response(),
        RestError::Forbidden(message) => (
            StatusCode::FORBIDDEN,
            Json(RestErrorResponse {
                error: "forbidden".to_owned(),
                message,
                code: None,
                data: None,
            }),
        )
            .into_response(),
        RestError::RateLimited(message) => (
            StatusCode::TOO_MANY_REQUESTS,
            Json(RestErrorResponse {
                error: "rate_limited".to_owned(),
                message,
                code: None,
                data: None,
            }),
        )
            .into_response(),
        RestError::Conflict(message) => (
            StatusCode::CONFLICT,
            Json(RestErrorResponse {
                error: "conflict".to_owned(),
                message,
                code: None,
                data: None,
            }),
        )
            .into_response(),
        RestError::Client(Error::Rpc {
            code,
            message,
            data,
        }) => (
            StatusCode::BAD_GATEWAY,
            Json(RestErrorResponse {
                error: "json_rpc_error".to_owned(),
                message,
                code: Some(code),
                data,
            }),
        )
            .into_response(),
        RestError::Client(error) => (
            StatusCode::BAD_GATEWAY,
            Json(RestErrorResponse {
                error: "codex_app_server_error".to_owned(),
                message: error.to_string(),
                code: None,
                data: None,
            }),
        )
            .into_response(),
    }
}

fn validate_text_turn_request(
    options: &RestRouterOptions,
    request: &RestTextTurnRequest,
) -> RestResult<()> {
    if !options.allow_unsafe_client_options
        && matches!(request.approval_policy, Some(RestApprovalPolicy::AllowAll))
    {
        return Err(RestError::Forbidden(
            "`approvalPolicy: allow_all` requires a trusted REST bridge".to_owned(),
        ));
    }
    validate_client_options(options, request.client.as_ref())
}

fn validate_client_options(
    options: &RestRouterOptions,
    client: Option<&RestClientOptions>,
) -> RestResult<()> {
    if options.allow_unsafe_client_options {
        return Ok(());
    }
    let Some(client) = client else {
        return Ok(());
    };
    if client.command.is_some() || !client.extra_args.is_empty() || !client.config.is_empty() {
        return Err(RestError::Forbidden(
            "client command, extraArgs, and config overrides require a trusted REST bridge"
                .to_owned(),
        ));
    }
    Ok(())
}

fn acquire_one_shot_permit(state: &RestState) -> RestResult<OwnedSemaphorePermit> {
    state
        .one_shot_gate
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            RestError::RateLimited(format!(
                "maximum one-shot REST call concurrency ({}) reached",
                state.options.limits.max_one_shot_concurrency
            ))
        })
}

fn acquire_poll_guard(state: &RestState, session_id: &str) -> RestResult<ActivePollGuard> {
    let mut active = state
        .active_polls
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !active.insert(session_id.to_owned()) {
        return Err(RestError::Conflict(format!(
            "an event poll is already active for session `{session_id}`"
        )));
    }
    Ok(ActivePollGuard {
        active_polls: state.active_polls.clone(),
        session_id: session_id.to_owned(),
    })
}

fn clamp_poll_timeout_ms(options: &RestRouterOptions, timeout_ms: u64) -> u64 {
    let max = options.limits.max_poll_timeout.as_millis() as u64;
    timeout_ms.min(max)
}

fn normalize_method(method: String) -> Option<String> {
    let method = method.trim_matches('/').trim();
    (!method.is_empty()).then(|| method.to_owned())
}
