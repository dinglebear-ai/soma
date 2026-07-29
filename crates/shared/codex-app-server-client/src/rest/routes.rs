use std::{
    collections::HashSet,
    convert::Infallible,
    pin::Pin,
    sync::{Arc, Mutex as StdMutex},
    task::{Context, Poll},
};

use axum::{
    Router,
    extract::{DefaultBodyLimit, Path, Query, State, rejection::JsonRejection},
    http::StatusCode,
    response::{
        IntoResponse, Json, Response,
        sse::{Event as SseEvent, KeepAlive, Sse},
    },
    routing::{delete, get, post},
};
use futures_core::Stream;
use serde::Deserialize;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::Error;

use super::{
    backend::CodexRestBackend,
    types::{
        RestApprovalPolicy, RestBackend, RestCallBody, RestCallRequest, RestClientOptions,
        RestError, RestErrorReplyRequest, RestErrorResponse, RestEventResponse, RestFuture,
        RestHealthResponse, RestListSessionsResponse, RestRequestReplyResultRequest, RestResult,
        RestRouterOptions, RestSessionCreateRequest, RestTextTurnRequest,
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
/// The default router exposes only non-executing health and compatibility
/// routes. Use [`text_turn_router`], [`trusted_bridge_router`], or
/// [`router_with_options`] when the router is mounted behind a trusted authz
/// boundary and should execute Codex work.
pub fn router() -> Router {
    router_with_options(RestRouterOptions::default())
}

/// Builds a router with the one-shot text-turn helper enabled.
pub fn text_turn_router() -> Router {
    router_with_options(RestRouterOptions::text_turn())
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
/// - `GET /v1/sessions/{sessionId}/events/stream` (Server-Sent Events
///   counterpart to `.../events`: same payloads, one per `data:` frame,
///   streamed instead of long-polled one at a time)
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
        .route("/v1/compatibility", get(compatibility));

    let router = if options.enable_text_turn_route {
        router.route("/v1/text-turn", post(text_turn))
    } else {
        router
    };

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
                "/v1/sessions/{session_id}/events/stream",
                get(poll_event_stream),
            )
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

    // Cap request bodies before any handler runs. axum applies a silent 2 MiB
    // default otherwise; this makes the bound explicit, tunable
    // (`RestLimits::max_request_body_bytes`), and consistent with every other
    // documented limit. Applied to the whole router, but only the body-reading
    // POST routes can actually trip it - the health/compat/event GETs have no
    // body to measure.
    router
        .layer(DefaultBodyLimit::max(options.limits.max_request_body_bytes))
        .with_state(state)
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
    match state.backend.compatibility_report().await {
        Ok(response) => Json(response).into_response(),
        Err(error) => rest_error(error),
    }
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
    if body.client.is_some() {
        return rest_error(RestError::InvalidRequest(
            "`client` options are only accepted when creating a session or making one-shot calls"
                .to_owned(),
        ));
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

/// Server-Sent Events counterpart to [`poll_event`].
///
/// Where `GET .../events` returns exactly one [`RestEventResponse`] per
/// request and requires the caller to poll again, this repeatedly calls
/// [`RestBackend::poll_event`] and streams every response - including
/// [`RestEventResponse::Timeout`] - as its own `data:` frame, tagged with an
/// `event:` field matching the JSON payload's own `event` discriminant
/// (`notification`, `request`, `closed`, or `timeout`). Forwarding
/// `Timeout` rather than swallowing it is the deliberate choice for "what
/// happens on a poll timeout" here: it gives a browser `EventSource`
/// listener an application-level heartbeat with the exact same shape it
/// would see from one long-poll cycle, on top of (not instead of) the
/// wire-level `KeepAlive` comments axum injects if the stream is ever
/// `Pending` for longer than [`RestLimits::sse_keep_alive_interval`].
///
/// The stream ends after forwarding [`RestEventResponse::Closed`] or a
/// backend error (surfaced as a terminal `event: error` frame carrying the
/// same [`RestErrorResponse`] shape the non-streaming routes return, minus
/// the HTTP status code, since `200 OK` is already committed by the time
/// any frame can be written). It never ends on `Timeout` - that's the
/// "long-poll but as a stream" contract this route exists to provide.
///
/// [`ActivePollGuard`] is held for the entire lifetime of the stream (moved
/// into the returned [`EventPollStream`], not the handler's local scope),
/// so a session can have at most one active consumer whether that's a
/// long-poll or an SSE stream, never both at once. Dropping the response
/// body - which happens when the client disconnects - drops the
/// [`EventPollStream`] and, with it, the guard, exactly as if the
/// long-poll caller had stopped polling.
///
/// [`RestLimits`]: super::types::RestLimits
async fn poll_event_stream(
    State(state): State<RestState>,
    Path(session_id): Path<String>,
    Query(query): Query<EventQuery>,
) -> Response {
    let guard = match acquire_poll_guard(&state, &session_id) {
        Ok(guard) => guard,
        Err(error) => return rest_error(error),
    };
    let timeout_ms = clamp_stream_poll_timeout_ms(
        &state.options,
        query
            .timeout_ms
            .unwrap_or(state.options.limits.max_poll_timeout.as_millis() as u64),
    );
    let stream = EventPollStream {
        backend: state.backend.clone(),
        session_id,
        timeout_ms,
        pending: None,
        guard: Some(guard),
        done: false,
        synchronous_polls: 0,
    };
    Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(state.options.limits.sse_keep_alive_interval))
        .into_response()
}

/// [`Stream`] backing [`poll_event_stream`]: repeatedly drives
/// [`RestBackend::poll_event`], turning each resolved [`RestEventResponse`]
/// (or terminal error) into one SSE frame, and holds the session's
/// [`ActivePollGuard`] for as long as the stream itself is alive.
///
/// Manually implemented (rather than built from `futures_util::stream`
/// combinators or an `async_stream::stream!` block) to avoid adding either
/// dependency - see README.md on this crate's minimal-dependency-graph
/// rule. All fields are `Unpin` (an `Arc`, a `String`, a `u64`, an
/// `Option<ActivePollGuard>`, and an `Option<Pin<Box<dyn Future + Send>>>`
/// are all `Unpin` regardless of what's inside the box), so `poll_next` can
/// use a plain `&mut Self` via `Pin::get_mut` instead of `pin_project`.
struct EventPollStream {
    backend: Arc<dyn RestBackend>,
    session_id: String,
    timeout_ms: u64,
    /// The in-flight `poll_event` call, if one has been started and hasn't
    /// resolved yet. Replaced with a fresh call after every resolution
    /// until the stream reaches a terminal state.
    pending: Option<RestFuture<RestEventResponse>>,
    /// Held until a terminal event (`Closed` or an error) is reached, or
    /// the stream itself is dropped (client disconnect) - whichever comes
    /// first. `take()`n explicitly on the terminal-event path so the guard
    /// is released the moment the session becomes pollable again, rather
    /// than waiting for hyper to finish tearing down the now-finished body.
    guard: Option<ActivePollGuard>,
    done: bool,
    /// Consecutive `poll_next` calls that resolved a `poll_event` future
    /// without the executor ever getting control back. Reset to 0 whenever
    /// this stream parks (returns `Poll::Pending`). See
    /// [`YIELD_AFTER_SYNCHRONOUS_POLLS`].
    synchronous_polls: u32,
}

/// How many back-to-back synchronously-resolving `poll_event` calls this
/// stream will service before forcing itself to yield to the executor.
///
/// [`RestBackend`] is a public trait that host applications implement, and a
/// `poll_event` future is free to resolve on its first poll - a backend with
/// an already-buffered event does exactly that, and it is the natural shape
/// for one. When that happens this stream never returns `Poll::Pending`, so
/// the task driving it never hands control back to the runtime: on a
/// current-thread runtime (which `codex-app-server-rest` uses) one such
/// stream starves every other task in the process, and there is no upper
/// bound on how long it continues.
///
/// The default [`crate::rest::CodexRestBackend`] happens not to trigger the
/// unbounded case, because its `poll_event` bottoms out in tokio's mpsc and
/// timer primitives, which park at least once. That is incidental, not a
/// contract this stream can rely on: a bursty session (events already queued)
/// and any third-party backend both break the assumption. So the bound is
/// enforced here, where the loop actually lives, rather than being left to
/// the backend's good behavior.
///
/// The value only needs to be small enough to bound starvation and large
/// enough not to add a park to every event on a busy-but-well-behaved
/// session; 32 is comfortably both.
const YIELD_AFTER_SYNCHRONOUS_POLLS: u32 = 32;

impl Stream for EventPollStream {
    type Item = Result<SseEvent, Infallible>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.done {
            return Poll::Ready(None);
        }
        if this.synchronous_polls >= YIELD_AFTER_SYNCHRONOUS_POLLS {
            // Hand control back to the executor, then ask to be polled again
            // immediately. This is what `tokio::task::yield_now()` does; it is
            // open-coded because `poll_next` is a manual `poll` fn and cannot
            // `.await`.
            this.synchronous_polls = 0;
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        if this.pending.is_none() {
            this.pending = Some(
                this.backend
                    .poll_event(this.session_id.clone(), Some(this.timeout_ms)),
            );
        }
        let pending = this
            .pending
            .as_mut()
            .expect("pending future was just populated above");
        match pending.as_mut().poll(cx) {
            Poll::Pending => {
                // Parked on the backend: the executor has control back, so the
                // starvation budget is spent and resets.
                this.synchronous_polls = 0;
                Poll::Pending
            }
            Poll::Ready(result) => {
                this.pending = None;
                this.synchronous_polls = this.synchronous_polls.saturating_add(1);
                match result {
                    Ok(response) => {
                        if matches!(response, RestEventResponse::Closed) {
                            this.done = true;
                            this.guard = None;
                        }
                        Poll::Ready(Some(Ok(sse_event_from_response(&response))))
                    }
                    Err(error) => {
                        this.done = true;
                        this.guard = None;
                        Poll::Ready(Some(Ok(sse_error_event(error))))
                    }
                }
            }
        }
    }
}

/// Renders a [`RestEventResponse`] as the SSE frame [`poll_event_stream`]
/// sends for it: `event:` set to the JSON payload's own `event`
/// discriminant, `data:` set to that same JSON payload serialized exactly
/// as the long-poll route would return it.
fn sse_event_from_response(response: &RestEventResponse) -> SseEvent {
    let event_name = match response {
        RestEventResponse::Notification { .. } => "notification",
        RestEventResponse::Request { .. } => "request",
        RestEventResponse::Closed => "closed",
        RestEventResponse::Timeout => "timeout",
    };
    let payload = serde_json::to_string(response)
        .unwrap_or_else(|_| r#"{"event":"internal_error"}"#.to_owned());
    SseEvent::default().event(event_name).data(payload)
}

/// Renders a terminal backend error as an `event: error` SSE frame. Reuses
/// [`rest_error_response`] so the JSON body matches what the non-streaming
/// routes would return in `Err`, minus the HTTP status code - the response
/// has already committed to `200 OK` by the time any frame can be written.
fn sse_error_event(error: RestError) -> SseEvent {
    let (_status, body) = rest_error_response(error);
    let payload =
        serde_json::to_string(&body).unwrap_or_else(|_| r#"{"error":"internal"}"#.to_owned());
    SseEvent::default().event("error").data(payload)
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
    // A body that blew the `DefaultBodyLimit` (see
    // `router_with_backend_arc_and_options`) surfaces here as a `JsonRejection`
    // too, but it is a `413`, not a malformed-request `400` - reporting it as
    // "invalid_json" would tell a caller their JSON was wrong when it was
    // simply too big. The rejection knows its own status; trust it for the
    // too-large case and keep the crate's `payload_too_large` error shape so it
    // matches `RestError::PayloadTooLarge` responses from elsewhere.
    if error.status() == StatusCode::PAYLOAD_TOO_LARGE {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(RestErrorResponse {
                error: "payload_too_large".to_owned(),
                message: error.body_text(),
                code: None,
                data: None,
            }),
        )
            .into_response();
    }
    (
        StatusCode::BAD_REQUEST,
        Json(RestErrorResponse {
            error: "invalid_json".to_owned(),
            message: error.body_text(),
            code: None,
            data: None,
        }),
    )
        .into_response()
}

/// Maps a [`RestError`] to the `(status, body)` pair the non-streaming
/// routes respond with. Split out from [`rest_error`] so
/// [`sse_error_event`] can reuse the exact same body construction for its
/// terminal SSE frame without duplicating every match arm - the SSE case
/// just has nowhere to put the status code, since `200 OK` and the SSE
/// content-type are already committed by the time a frame can be written.
fn rest_error_response(error: RestError) -> (StatusCode, RestErrorResponse) {
    /// The shape every variant below shares except the two `Client` ones:
    /// a status, a stable machine-readable `error` kind, and the message the
    /// variant already carries. Factored out so adding a variant is one line
    /// and cannot accidentally disagree with its neighbours about the
    /// `code`/`data` fields, which only the JSON-RPC passthrough populates.
    fn simple(status: StatusCode, kind: &str, message: String) -> (StatusCode, RestErrorResponse) {
        (
            status,
            RestErrorResponse {
                error: kind.to_owned(),
                message,
                code: None,
                data: None,
            },
        )
    }

    match error {
        RestError::NotFound(message) => simple(StatusCode::NOT_FOUND, "not_found", message),
        RestError::Gone(message) => simple(StatusCode::GONE, "gone", message),
        RestError::Forbidden(message) => simple(StatusCode::FORBIDDEN, "forbidden", message),
        RestError::InvalidRequest(message) => {
            simple(StatusCode::BAD_REQUEST, "invalid_request", message)
        }
        RestError::RateLimited(message) => {
            simple(StatusCode::TOO_MANY_REQUESTS, "rate_limited", message)
        }
        RestError::Conflict(message) => simple(StatusCode::CONFLICT, "conflict", message),
        RestError::TimedOut(message) => simple(StatusCode::GATEWAY_TIMEOUT, "timeout", message),
        RestError::PayloadTooLarge(message) => {
            simple(StatusCode::PAYLOAD_TOO_LARGE, "payload_too_large", message)
        }
        RestError::Internal(message) => {
            simple(StatusCode::INTERNAL_SERVER_ERROR, "internal", message)
        }
        // The only variants that don't fit `simple`: a JSON-RPC error from the
        // app-server is passed through with its own `code`/`data` intact.
        RestError::Client(Error::Rpc {
            code,
            message,
            data,
        }) => (
            StatusCode::BAD_GATEWAY,
            RestErrorResponse {
                error: "json_rpc_error".to_owned(),
                message,
                code: Some(code),
                data,
            },
        ),
        RestError::Client(error) => simple(
            StatusCode::BAD_GATEWAY,
            "codex_app_server_error",
            error.to_string(),
        ),
    }
}

fn rest_error(error: RestError) -> Response {
    let (status, body) = rest_error_response(error);
    (status, Json(body)).into_response()
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

/// Like [`clamp_poll_timeout_ms`], but also enforces
/// [`RestLimits::min_stream_poll_timeout`] as a floor.
///
/// The long-poll route deliberately has no floor: there, `timeoutMs=0` means
/// "tell me if an event is already waiting, otherwise return immediately",
/// which is a legitimate non-blocking-poll idiom, and each repeat costs the
/// caller a full HTTP round trip that paces it.
///
/// The streaming route has no such pacing - it is one request that loops
/// server-side for as long as the client reads - and no equivalent use for a
/// zero timeout, since a stream by definition wants to wait for the next
/// event. Without a floor, `?timeoutMs=0` turns one request into an unbounded
/// run of back-to-back `poll_event` calls, each costing a session-map lock, an
/// idle-session scan, a heap-allocated future, and a serialized `timeout`
/// frame - all to report that nothing happened.
///
/// This costs real events nothing: `poll_event` resolves as soon as an event
/// arrives, so the timeout only bounds the *idle* wait. The floor therefore
/// only limits how often an idle stream emits `timeout` frames.
fn clamp_stream_poll_timeout_ms(options: &RestRouterOptions, timeout_ms: u64) -> u64 {
    let min = options.limits.min_stream_poll_timeout.as_millis() as u64;
    let max = options.limits.max_poll_timeout.as_millis() as u64;
    // `min` wins a misconfigured `min > max`: the floor is a resource-abuse
    // backstop, and clamping into an empty range has to fail toward "poll less
    // often", never toward the unbounded spin the floor exists to prevent.
    timeout_ms.min(max).max(min)
}

fn normalize_method(method: String) -> Option<String> {
    let method = method.trim_matches('/').trim();
    (!method.is_empty()).then(|| method.to_owned())
}
