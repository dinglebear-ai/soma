//! Reusable OpenAPI fragment builders for `paths.rs`: request/response
//! body wrappers and the shared path/query parameter definitions. Split
//! out so `paths.rs` keeps only the per-route operation assembly and
//! stays under the PATTERNS.md module size hard limit.

use serde_json::{Value, json};

use super::super::json::{obj, schema_ref};

/// A JSON `application/json` request body wrapper.
pub(super) fn json_request_body(description: &str, schema_name: &str, required: bool) -> Value {
    obj(vec![
        ("description", json!(description)),
        ("required", json!(required)),
        (
            "content",
            obj(vec![(
                "application/json",
                obj(vec![("schema", schema_ref(schema_name))]),
            )]),
        ),
    ])
}

/// A `200` (or other success) `application/json` response.
pub(super) fn json_response(description: &str, schema_name: &str) -> (&'static str, Value) {
    (
        "200",
        obj(vec![
            ("description", json!(description)),
            (
                "content",
                obj(vec![(
                    "application/json",
                    obj(vec![("schema", schema_ref(schema_name))]),
                )]),
            ),
        ]),
    )
}

/// A `text/event-stream` success response for the one SSE route.
pub(super) fn sse_response(description: &str) -> (&'static str, Value) {
    (
        "200",
        obj(vec![
            ("description", json!(description)),
            (
                "content",
                obj(vec![(
                    "text/event-stream",
                    obj(vec![
                        ("schema", schema_ref("RestEventResponse")),
                        (
                            "description",
                            json!(
                                "One SSE `data:` frame per event, `event:` set to the payload's \
                                 own `event` discriminant (`notification` | `request` | \
                                 `closed` | `timeout`). See `RestEventResponse` - the frame body \
                                 is that exact JSON shape, not wrapped further."
                            ),
                        ),
                    ]),
                )]),
            ),
        ]),
    )
}

/// A non-2xx `application/json` error response using [`RestErrorResponse`](crate::rest::types::RestErrorResponse).
pub(super) fn error_response(status: &'static str, description: &str) -> (&'static str, Value) {
    (status, json_response(description, "RestErrorResponse").1)
}

/// The `401` response documented on every operation except the two health
/// routes. Not one of the codes [`rest_error_response`](crate::rest::routes::rest_error_response)
/// emits - it comes from the *optional* [`bearer_auth`](crate::rest::auth::bearer_auth) layer,
/// which is not mounted by default. Listed here (rather than omitted, since
/// strictly nothing in `router_with_options` alone can 401) because a spec
/// consumer integrating against a real deployment needs to know 401 is
/// possible the moment an operator opts into `bearer_auth` - see this
/// document's top-level `info.description`.
pub(super) fn unauthorized_response() -> (&'static str, Value) {
    (
        "401",
        obj(vec![
            (
                "description",
                json!(
                    "Missing or invalid `Authorization: Bearer <token>` header. Only returned \
                     when the router is wrapped in `rest::bearer_auth(...)` - the base router \
                     has no built-in auth and never returns this on its own."
                ),
            ),
            (
                "content",
                obj(vec![(
                    "application/json",
                    obj(vec![("schema", schema_ref("RestErrorResponse"))]),
                )]),
            ),
        ]),
    )
}

pub(super) fn session_id_param() -> Value {
    obj(vec![
        ("name", json!("sessionId")),
        ("in", json!("path")),
        ("required", json!(true)),
        ("schema", obj(vec![("type", json!("string"))])),
        (
            "description",
            json!(
                "REST bridge session identifier returned as `sessionId` by `POST /v1/sessions` \
                 (the built-in `CodexRestBackend` mints values shaped like \
                 `session-<uuid-v4-simple>`, but that format is not a contract - callers must \
                 treat it as an opaque token)."
            ),
        ),
    ])
}

/// The `{method}` path parameter. Deliberately documented as *not* a
/// conventional single-segment OpenAPI path parameter: the underlying axum
/// route uses a `{*method}` catch-all (see `routes.rs`), because a real
/// `codex app-server` JSON-RPC method name is namespaced with a literal `/`
/// (`thread/start`, `config/read`, ...). Most OpenAPI tooling assumes
/// `{param}` matches exactly one path segment with no `/`; that assumption
/// is false for this parameter, which is exactly the "represent that
/// honestly" case called out in this crate's REST adapter notes. There is
/// no strictly-correct OpenAPI 3.1 way to express "one path parameter that
/// itself may contain literal slashes" - this documents the true behavior
/// in prose rather than picking a technically-valid-but-misleading schema.
pub(super) fn method_param() -> Value {
    obj(vec![
        ("name", json!("method")),
        ("in", json!("path")),
        ("required", json!(true)),
        ("schema", obj(vec![("type", json!("string"))])),
        (
            "description",
            json!(
                "Full `codex app-server` JSON-RPC method name, e.g. `thread/start` or \
                 `config/read`. IMPORTANT: this is captured by an axum `{*method}` wildcard, \
                 not a conventional single-segment path parameter - the value legitimately \
                 contains literal `/` characters, so naive path-templating clients that escape \
                 `/` in path parameters will build the wrong URL. A leading/trailing `/` on the \
                 captured value is trimmed server-side before use."
            ),
        ),
    ])
}

pub(super) fn request_key_param() -> Value {
    obj(vec![
        ("name", json!("requestKey")),
        ("in", json!("path")),
        ("required", json!(true)),
        ("schema", obj(vec![("type", json!("string"))])),
        (
            "description",
            json!(
                "Opaque key returned as `requestKey` on a `\"event\": \"request\"` payload from \
                 `GET .../events` or `GET .../events/stream`. Answers exactly one pending \
                 server-originated request and is single-use - a second reply attempt with the \
                 same key returns `404`."
            ),
        ),
    ])
}

/// The `timeoutMs` query parameter for the long-poll events route.
///
/// Split from [`stream_timeout_ms_param`] because the two routes clamp it
/// differently: only the streaming route enforces a lower bound. `minimum` is
/// `0` on both - a zero is *accepted* on both, it is simply raised on the
/// streaming one - so the difference is expressible only in prose, and a
/// single shared description would necessarily be wrong for one of them.
pub(super) fn timeout_ms_param() -> Value {
    obj(vec![
        ("name", json!("timeoutMs")),
        ("in", json!("query")),
        ("required", json!(false)),
        (
            "schema",
            obj(vec![("type", json!("integer")), ("minimum", json!(0))]),
        ),
        (
            "description",
            json!(
                "Long-poll budget in milliseconds. Defaults to, and is clamped down to, the \
                 server's configured `RestLimits::max_poll_timeout` (default 30000ms, overridable \
                 via `CODEX_APP_SERVER_REST_MAX_POLL_TIMEOUT_MS`) - a caller-requested value \
                 above that ceiling is silently lowered to it, never rejected. There is no lower \
                 bound: `0` means `report an event only if one is already waiting`, which is a \
                 supported non-blocking poll."
            ),
        ),
    ])
}

/// The `timeoutMs` query parameter for the SSE events route. See
/// [`timeout_ms_param`] for why this is a separate parameter.
pub(super) fn stream_timeout_ms_param() -> Value {
    obj(vec![
        ("name", json!("timeoutMs")),
        ("in", json!("query")),
        ("required", json!(false)),
        (
            "schema",
            obj(vec![("type", json!("integer")), ("minimum", json!(0))]),
        ),
        (
            "description",
            json!(
                "How long the server waits for the next event before emitting a `timeout` frame \
                 and waiting again, in milliseconds. Clamped into \
                 `[RestLimits::min_stream_poll_timeout, RestLimits::max_poll_timeout]` (default \
                 250ms to 30000ms, overridable via \
                 `CODEX_APP_SERVER_REST_MIN_STREAM_POLL_TIMEOUT_MS` and \
                 `CODEX_APP_SERVER_REST_MAX_POLL_TIMEOUT_MS`) - a value outside that range is \
                 silently moved into it, never rejected. Unlike the long-poll route, this one \
                 enforces a floor: a stream has no per-event HTTP round trip to pace it, so a \
                 zero timeout would make one request loop the backend without bound. The floor \
                 does not delay real events - it only caps how often an idle stream reports that \
                 nothing happened."
            ),
        ),
    ])
}
