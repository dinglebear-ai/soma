//! Builds the OpenAPI `paths` object: per-route/method request and response
//! wrappers, path/query parameter definitions, and the per-[`RouteDef`]
//! operation bodies read from [`super::route_table::ROUTES`].

use serde_json::{Value, json};

use super::{
    json::obj,
    route_table::{ROUTES, RouteDef},
};

#[path = "paths_components.rs"]
mod paths_components;

use paths_components::{
    error_response, json_request_body, json_response, method_param, request_key_param,
    session_id_param, sse_response, stream_timeout_ms_param, timeout_ms_param,
    unauthorized_response,
};

/// Builds the full OpenAPI operation object for one [`RouteDef`]. Keyed on
/// `(method, path_template)` and panics on an unmapped combination -
/// deliberately, so adding a row to [`ROUTES`] without also writing its
/// operation body fails loudly at spec-build time (every call to
/// `openapi_spec()`, including in tests) instead of silently emitting a
/// route with no operation.
fn operation_for(route: &RouteDef) -> Value {
    let mut operation = operation_definition(route);
    ensure_request_body_limit_413(&mut operation);
    operation
}

/// Adds a `413` response to any operation that has a `requestBody`.
///
/// Every route that reads a body can be rejected by the router's
/// `DefaultBodyLimit` ([`RestLimits::max_request_body_bytes`](crate::rest::RestLimits::max_request_body_bytes))
/// before its handler runs, independently of any route-specific failure. Doing
/// it here, once, rather than hand-listing `413` on each POST route means a
/// route added later cannot forget it - `every_route_with_a_request_body_documents_413`
/// in `super::super` fails the build if this ever stops holding.
///
/// Skips an operation that already documents `413` (only `POST /v1/text-turn`,
/// whose `413` also covers its output-byte cap) so its more specific
/// description is kept.
fn ensure_request_body_limit_413(operation: &mut Value) {
    let Some(map) = operation.as_object_mut() else {
        return;
    };
    if !map.contains_key("requestBody") {
        return;
    }
    let Some(responses) = map.get_mut("responses").and_then(Value::as_object_mut) else {
        return;
    };
    if responses.contains_key("413") {
        return;
    }
    let (_, body) = error_response(
        "413",
        "Request body exceeded `RestLimits::max_request_body_bytes` (the router's \
         `DefaultBodyLimit`); rejected before the handler ran.",
    );
    responses.insert("413".to_owned(), body);
    // Re-sort so output stays byte-identical whether `serde_json::Map` is a
    // `BTreeMap` or an insertion-ordered `IndexMap` in this build - the same
    // determinism concern `json::obj` exists for (see the module docs). A bare
    // `insert` would append at the end under `IndexMap`.
    let mut sorted: Vec<(String, Value)> = responses
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    sorted.sort_by(|left, right| left.0.cmp(&right.0));
    *responses = sorted.into_iter().collect();
}

fn operation_definition(route: &RouteDef) -> Value {
    let auth_note = unauthorized_response();
    match (route.method, route.path_template) {
        ("get", "/health") | ("get", "/v1/health") => obj(vec![
            ("summary", json!("Liveness probe")),
            (
                "description",
                json!(
                    "Always returns `200`. Never requires authentication, even when `rest::bearer_auth` is layered on (see `BearerAuthLayer::allow_unauthenticated_health`, default `true`)."
                ),
            ),
            (
                "operationId",
                json!(format!(
                    "get{}",
                    route.path_template.replace(['/', '-'], "_")
                )),
            ),
            (
                "responses",
                obj(vec![json_response(
                    "The process is up.",
                    "RestHealthResponse",
                )]),
            ),
        ]),
        ("get", "/v1/compatibility") => obj(vec![
            (
                "summary",
                json!("Schema/installed-version compatibility report"),
            ),
            (
                "description",
                json!(
                    "Reports the vendored protocol schema's Codex version, the locally \
                     installed `codex --version` (if any), and a generated method-count \
                     summary. Requires authentication when `rest::bearer_auth` is layered on \
                     (unlike `/health`) since it reveals environment details."
                ),
            ),
            ("operationId", json!("getV1Compatibility")),
            (
                "responses",
                obj(vec![
                    json_response("Compatibility report.", "CompatibilityReport"),
                    error_response(
                        "500",
                        "The backend's compatibility check failed (e.g. the check task panicked or was cancelled).",
                    ),
                    auth_note.clone(),
                ]),
            ),
        ]),
        ("post", "/v1/text-turn") => obj(vec![
            ("summary", json!("One-shot text turn")),
            (
                "description",
                json!(format!(
                    "Starts a fresh, ephemeral Codex session, sends one text prompt, waits for \
                     turn completion (bounded by `RestLimits::max_text_turn_duration`), and \
                     returns the assistant's message plus the latest diff and any turn errors. \
                     Does not preserve session state across calls - use the stateful bridge \
                     routes (`POST /v1/sessions`, ...) for multi-turn conversations.{}",
                    route.gate.note()
                )),
            ),
            ("operationId", json!("postV1TextTurn")),
            (
                "requestBody",
                json_request_body(
                    "The prompt and optional model/approval/client overrides.",
                    "RestTextTurnRequest",
                    true,
                ),
            ),
            (
                "responses",
                obj(vec![
                    json_response("The turn reached a terminal state.", "RestTextTurnResponse"),
                    error_response(
                        "400",
                        "Malformed JSON body, or `prompt` is empty/whitespace-only.",
                    ),
                    error_response(
                        "403",
                        "`approvalPolicy: \"allow_all\"` or a `client.command`/`extraArgs`/`config` \
                         override was requested without `RestRouterOptions::with_unsafe_client_options(true)`.",
                    ),
                    error_response(
                        "429",
                        "`RestLimits::max_one_shot_concurrency` concurrent one-shot calls are already in flight.",
                    ),
                    error_response(
                        "413",
                        "The request body exceeded `RestLimits::max_request_body_bytes` (rejected before \
                         the turn started), or accumulated turn output exceeded \
                         `RestLimits::max_text_turn_output_bytes` (the turn was interrupted).",
                    ),
                    error_response(
                        "502",
                        "The underlying `codex app-server` process failed, disconnected, or returned a JSON-RPC error while starting the thread or turn.",
                    ),
                    error_response(
                        "504",
                        "The turn did not reach a terminal state within `RestLimits::max_text_turn_duration`; it was interrupted.",
                    ),
                    auth_note.clone(),
                ]),
            ),
        ]),
        ("post", "/v1/call/{method}") => obj(vec![
            ("summary", json!("One-shot raw JSON-RPC bridge call")),
            (
                "description",
                json!(format!(
                    "Starts a fresh, short-lived Codex session, calls the named app-server \
                     method once, and returns its raw JSON-RPC result. Does not preserve turn \
                     state, stream events, or let you answer server-originated requests \
                     afterward - use the stateful bridge (`POST /v1/sessions` + \
                     `POST /v1/sessions/{{sessionId}}/call/{{method}}`) for that.{}",
                    route.gate.note()
                )),
            ),
            ("operationId", json!("postV1CallMethod")),
            ("parameters", json!([method_param()])),
            (
                "requestBody",
                json_request_body(
                    "JSON-RPC params for `method`, plus optional client overrides.",
                    "RestCallBody",
                    true,
                ),
            ),
            (
                "responses",
                obj(vec![
                    json_response("The call returned a result.", "RestCallResponse"),
                    error_response(
                        "400",
                        "Malformed JSON body, or the `{method}` path segment was empty.",
                    ),
                    error_response(
                        "403",
                        "A `client.command`/`extraArgs`/`config` override was requested without \
                         `RestRouterOptions::with_unsafe_client_options(true)`.",
                    ),
                    error_response(
                        "429",
                        "`RestLimits::max_one_shot_concurrency` concurrent one-shot calls are already in flight.",
                    ),
                    error_response(
                        "502",
                        "The underlying `codex app-server` process failed, disconnected, or the call itself returned a JSON-RPC error.",
                    ),
                    auth_note.clone(),
                ]),
            ),
        ]),
        ("get", "/v1/sessions") => obj(vec![
            ("summary", json!("List active bridge sessions")),
            (
                "description",
                json!(format!(
                    "Lists session IDs for currently open stateful bridge sessions.{}",
                    route.gate.note()
                )),
            ),
            ("operationId", json!("getV1Sessions")),
            (
                "responses",
                obj(vec![
                    json_response("Active sessions.", "RestListSessionsResponse"),
                    auth_note.clone(),
                ]),
            ),
        ]),
        ("post", "/v1/sessions") => obj(vec![
            ("summary", json!("Start a stateful bridge session")),
            (
                "description",
                json!(format!(
                    "Spawns a persistent `codex app-server` process and returns a `sessionId` \
                     plus its raw `initialize` response. Use the returned `sessionId` with \
                     `POST /v1/sessions/{{sessionId}}/call/{{method}}` to drive it.{}",
                    route.gate.note()
                )),
            ),
            ("operationId", json!("postV1Sessions")),
            (
                "requestBody",
                json_request_body(
                    "Optional client overrides.",
                    "RestSessionCreateRequest",
                    false,
                ),
            ),
            (
                "responses",
                obj(vec![
                    json_response("Session created.", "RestSessionCreateResponse"),
                    error_response("400", "Malformed JSON body."),
                    error_response(
                        "403",
                        "A `client.command`/`extraArgs`/`config` override was requested without \
                         `RestRouterOptions::with_unsafe_client_options(true)`.",
                    ),
                    error_response(
                        "429",
                        "`RestLimits::max_sessions` concurrently open bridge sessions are already open.",
                    ),
                    error_response(
                        "502",
                        "The `codex app-server` process failed to spawn or initialize.",
                    ),
                    auth_note.clone(),
                ]),
            ),
        ]),
        ("delete", "/v1/sessions/{sessionId}") => obj(vec![
            ("summary", json!("Drop a bridge session")),
            (
                "description",
                json!(format!(
                    "Removes the session and terminates its owned `codex app-server` process \
                     once no client clones remain.{}",
                    route.gate.note()
                )),
            ),
            ("operationId", json!("deleteV1SessionsBySessionId")),
            ("parameters", json!([session_id_param()])),
            (
                "responses",
                obj(vec![
                    json_response("Session removed.", "RestStatusResponse"),
                    error_response("404", "No session with this `sessionId` is open."),
                    auth_note.clone(),
                ]),
            ),
        ]),
        ("post", "/v1/sessions/{sessionId}/call/{method}") => obj(vec![
            (
                "summary",
                json!("Call a method on an existing bridge session"),
            ),
            (
                "description",
                json!(format!(
                    "Calls any app-server method on an already-open session, preserving \
                     thread/turn state across calls (e.g. `thread/start` then `turn/start`). \
                     Unlike the one-shot `POST /v1/call/{{method}}`, the request body's `client` \
                     field is rejected (`400`) here - client overrides only apply at session \
                     creation or on one-shot calls, not per-call on an existing session.{}",
                    route.gate.note()
                )),
            ),
            ("operationId", json!("postV1SessionsBySessionIdCallMethod")),
            ("parameters", json!([session_id_param(), method_param()])),
            (
                "requestBody",
                json_request_body(
                    "JSON-RPC params for `method`. `client` must be omitted or null.",
                    "RestCallBody",
                    true,
                ),
            ),
            (
                "responses",
                obj(vec![
                    json_response("The call returned a result.", "RestCallResponse"),
                    error_response(
                        "400",
                        "Malformed JSON body, the `{method}` path segment was empty, or a non-null `client` was supplied.",
                    ),
                    error_response("404", "No session with this `sessionId` is open."),
                    error_response(
                        "429",
                        "`RestLimits::max_session_call_concurrency` (global) or \
                         `RestLimits::max_session_call_concurrency_per_session` (this session) is already saturated.",
                    ),
                    error_response(
                        "502",
                        "The underlying `codex app-server` process failed, disconnected, or the call itself returned a JSON-RPC error.",
                    ),
                    auth_note.clone(),
                ]),
            ),
        ]),
        ("get", "/v1/sessions/{sessionId}/events") => obj(vec![
            ("summary", json!("Long-poll the next session event")),
            (
                "description",
                json!(format!(
                    "Long-polls for the next server notification or server-originated request \
                     on this session, up to `timeoutMs` (clamped to \
                     `RestLimits::max_poll_timeout`). Returns `{{\"event\": \"timeout\"}}` if \
                     nothing arrived in time - that is a normal `200`, not an error, and callers \
                     are expected to poll again. At most one poll (via this route or its SSE \
                     counterpart below) may be active per session at a time.{}",
                    route.gate.note()
                )),
            ),
            ("operationId", json!("getV1SessionsBySessionIdEvents")),
            (
                "parameters",
                json!([session_id_param(), timeout_ms_param()]),
            ),
            (
                "responses",
                obj(vec![
                    json_response(
                        "One event (`notification` | `request` | `closed` | `timeout`).",
                        "RestEventResponse",
                    ),
                    error_response("404", "No session with this `sessionId` is open."),
                    error_response(
                        "409",
                        "An event poll (long-poll or SSE stream) is already active for this session.",
                    ),
                    error_response(
                        "410",
                        "A server-originated request arrived but its reply deadline had already \
                         passed before it could be surfaced; the app-server was sent an error reply automatically.",
                    ),
                    error_response(
                        "429",
                        "This session already holds `RestLimits::max_pending_requests_per_session` \
                         un-replied-to server requests.",
                    ),
                    auth_note.clone(),
                ]),
            ),
        ]),
        ("get", "/v1/sessions/{sessionId}/events/stream") => obj(vec![
            (
                "summary",
                json!("Server-Sent Events counterpart to the long-poll route"),
            ),
            (
                "description",
                json!(format!(
                    "Same events as `GET /v1/sessions/{{sessionId}}/events`, streamed as \
                     Server-Sent Events instead of returned one at a time: repeatedly polls the \
                     backend and forwards every resolved event (including `timeout`, as an \
                     application-level heartbeat) as its own `data:` frame until a `closed` \
                     event or a backend error ends the stream. IMPORTANT: because the `200` \
                     status and `text/event-stream` content type are committed the moment the \
                     stream opens, a backend error that would be `404`/`409`/`410`/`429` on the \
                     long-poll route instead ends this stream with a terminal `event: error` \
                     frame carrying the same `RestErrorResponse` body (minus the HTTP status, \
                     since none can be sent at that point) - the *only* HTTP-level error status \
                     this route can return is `409`, and only synchronously before the stream \
                     opens, when another poll is already active for this session. At most one \
                     poll (via this route or its long-poll counterpart above) may be active per \
                     session at a time. Keep-alive comment frames are sent every \
                     `RestLimits::sse_keep_alive_interval` while no real event is ready.{}",
                    route.gate.note()
                )),
            ),
            ("operationId", json!("getV1SessionsBySessionIdEventsStream")),
            (
                "parameters",
                json!([session_id_param(), stream_timeout_ms_param()]),
            ),
            (
                "responses",
                obj(vec![
                    sse_response("An SSE stream of events for this session."),
                    error_response(
                        "409",
                        "An event poll (long-poll or SSE stream) is already active for this session. \
                         This is the only backend-originated error this route can report as an HTTP \
                         status - see the operation description.",
                    ),
                    auth_note.clone(),
                ]),
            ),
        ]),
        ("post", "/v1/sessions/{sessionId}/requests/{requestKey}/result") => obj(vec![
            (
                "summary",
                json!("Reply to a pending server-originated request with a result"),
            ),
            (
                "description",
                json!(format!(
                    "Answers a pending server-originated request (surfaced via a `\"event\": \
                     \"request\"` payload from the events routes above) with a successful \
                     JSON-RPC result.{}",
                    route.gate.note()
                )),
            ),
            (
                "operationId",
                json!("postV1SessionsBySessionIdRequestsByRequestKeyResult"),
            ),
            (
                "parameters",
                json!([session_id_param(), request_key_param()]),
            ),
            (
                "requestBody",
                json_request_body(
                    "The JSON-RPC result value.",
                    "RestRequestReplyResultRequest",
                    true,
                ),
            ),
            (
                "responses",
                obj(vec![
                    json_response("The reply was delivered.", "RestRequestReplyResponse"),
                    error_response("400", "Malformed JSON body."),
                    error_response(
                        "404",
                        "No session with this `sessionId` is open, or no pending request matches `requestKey`.",
                    ),
                    error_response(
                        "410",
                        "`requestKey` matched a request that has since expired (past \
                         `RestLimits::pending_request_ttl` or the app-server's own reply deadline) \
                         or was already answered.",
                    ),
                    auth_note.clone(),
                ]),
            ),
        ]),
        ("post", "/v1/sessions/{sessionId}/requests/{requestKey}/error") => obj(vec![
            (
                "summary",
                json!("Reply to a pending server-originated request with an error"),
            ),
            (
                "description",
                json!(format!(
                    "Answers a pending server-originated request (surfaced via a `\"event\": \
                     \"request\"` payload from the events routes above) with a JSON-RPC error.{}",
                    route.gate.note()
                )),
            ),
            (
                "operationId",
                json!("postV1SessionsBySessionIdRequestsByRequestKeyError"),
            ),
            (
                "parameters",
                json!([session_id_param(), request_key_param()]),
            ),
            (
                "requestBody",
                json_request_body(
                    "The JSON-RPC error code/message/data.",
                    "RestErrorReplyRequest",
                    true,
                ),
            ),
            (
                "responses",
                obj(vec![
                    json_response("The reply was delivered.", "RestRequestReplyResponse"),
                    error_response("400", "Malformed JSON body."),
                    error_response(
                        "404",
                        "No session with this `sessionId` is open, or no pending request matches `requestKey`.",
                    ),
                    error_response(
                        "410",
                        "`requestKey` matched a request that has since expired (past \
                         `RestLimits::pending_request_ttl` or the app-server's own reply deadline) \
                         or was already answered.",
                    ),
                    auth_note.clone(),
                ]),
            ),
        ]),
        (method, path) => unreachable!(
            "openapi.rs::operation_definition has no operation body mapped for `{method} {path}` - \
             add one alongside the new ROUTES entry"
        ),
    }
}

/// Builds `paths`, grouping [`ROUTES`] entries that share a `path_template`
/// (`/v1/sessions` mounts both `GET` and `POST`) into a single path-item object.
pub(super) fn build_paths() -> Value {
    let mut by_path: Vec<(&'static str, Vec<&RouteDef>)> = Vec::new();
    for route in ROUTES {
        match by_path
            .iter_mut()
            .find(|(path, _)| *path == route.path_template)
        {
            Some((_, routes)) => routes.push(route),
            None => by_path.push((route.path_template, vec![route])),
        }
    }
    let entries = by_path
        .into_iter()
        .map(|(path, routes)| {
            let operations = routes
                .into_iter()
                .map(|route| (route.method, operation_for(route)))
                .collect();
            (path, obj(operations))
        })
        .collect();
    obj(entries)
}
