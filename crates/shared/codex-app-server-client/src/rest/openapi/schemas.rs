//! Builds `components.schemas` for every request/response body type in
//! [`super::types`](crate::rest::types) plus [`crate::compat`]. Field names
//! and required-ness are transcribed straight from each type's `serde`
//! attributes - see the parent module's docs for why this is hand-written
//! rather than derived, and `super::tests` (in `openapi.rs`) for how that
//! transcription is checked.

use serde_json::{Value, json};

use super::json::{
    any_value_schema, array_schema, integer_schema, nonneg_integer_schema, nullable_string_schema,
    obj, object_schema, schema_ref, string_schema,
};

/// Builds `components.schemas` for every request/response body type in
/// [`super::types`](crate::rest::types) plus [`crate::compat`]. Field names
/// and required-ness are transcribed straight from each type's `serde`
/// attributes - see the module docs for why this is hand-written rather
/// than derived, and this file's `tests` module for how that transcription
/// is checked.
pub(super) fn build_schemas() -> Value {
    let mut entries: Vec<(&'static str, Value)> = vec![
        (
            "RestHealthResponse",
            object_schema(
                vec![("status", string_schema("Always `\"ok\"`."))],
                &["status"],
                None,
            ),
        ),
        (
            "RestApprovalPolicy",
            obj(vec![
                ("type", json!("string")),
                ("enum", json!(["deny_all", "read_only", "allow_all"])),
                ("default", json!("deny_all")),
                (
                    "description",
                    json!(
                        "Approval policy preset applied while collecting turn events for \
                         `POST /v1/text-turn`. `allow_all` requires \
                         `RestRouterOptions::with_unsafe_client_options(true)` and is rejected \
                         with `403` otherwise."
                    ),
                ),
            ]),
        ),
        (
            "RestClientOptions",
            object_schema(
                vec![
                    (
                        "name",
                        nullable_string_schema(
                            "Client name reported to the app-server's `initialize` call. Defaults to a per-route value (e.g. `codex_app_server_rest`) when omitted.",
                        ),
                    ),
                    (
                        "version",
                        nullable_string_schema(
                            "Client version reported to `initialize`. Defaults to this crate's own version when omitted.",
                        ),
                    ),
                    (
                        "command",
                        nullable_string_schema(
                            "Override the `codex` executable path/name to spawn. Requires \
                             `RestRouterOptions::with_unsafe_client_options(true)`; rejected with \
                             `403` otherwise, since it lets the caller choose an arbitrary host executable.",
                        ),
                    ),
                    (
                        "extraArgs",
                        array_schema(
                            string_schema("One extra CLI argument."),
                            "Extra arguments passed to the spawned `codex app-server` process. \
                             Requires `RestRouterOptions::with_unsafe_client_options(true)`; \
                             rejected with `403` otherwise. Omitted from the wire format when empty.",
                        ),
                    ),
                    (
                        "config",
                        obj(vec![
                            ("type", json!("object")),
                            ("additionalProperties", json!({"type": "string"})),
                            (
                                "description",
                                json!(
                                    "Extra `-c key=value` app-server config overrides. Requires \
                                     `RestRouterOptions::with_unsafe_client_options(true)`; rejected \
                                     with `403` otherwise. Omitted from the wire format when empty."
                                ),
                            ),
                        ]),
                    ),
                    (
                        "callTimeoutMs",
                        integer_schema(
                            "Per-call JSON-RPC timeout override in milliseconds. Defaults to `codex_app_server_client::DEFAULT_CALL_TIMEOUT` (120s) when omitted.",
                        ),
                    ),
                ],
                &[],
                Some(false),
            ),
        ),
        (
            "RestTextTurnRequest",
            object_schema(
                vec![
                    (
                        "prompt",
                        string_schema(
                            "The text prompt to send. Rejected with `400` if empty or whitespace-only.",
                        ),
                    ),
                    (
                        "model",
                        nullable_string_schema(
                            "Model override for the ephemeral thread. Uses the app-server's own default when omitted.",
                        ),
                    ),
                    ("approvalPolicy", schema_ref("RestApprovalPolicy")),
                    ("client", schema_ref("RestClientOptions")),
                ],
                &["prompt"],
                Some(false),
            ),
        ),
        (
            "RestTextTurnResponse",
            object_schema(
                vec![
                    ("threadId", string_schema("The ephemeral thread's id.")),
                    ("turnId", string_schema("The turn's id.")),
                    (
                        "turnStatus",
                        nullable_string_schema(
                            "The turn's terminal status (e.g. `\"completed\"`), or `null` if it could not be determined.",
                        ),
                    ),
                    (
                        "agentMessage",
                        string_schema(
                            "Concatenated assistant message text observed for the turn. Empty string if none.",
                        ),
                    ),
                    (
                        "latestDiff",
                        nullable_string_schema(
                            "The most recent unified diff observed for the turn, or `null` if none.",
                        ),
                    ),
                    (
                        "errors",
                        array_schema(
                            any_value_schema(
                                "One turn-level error event, in the app-server's own shape.",
                            ),
                            "Turn error events observed while collecting the turn. Empty array if none.",
                        ),
                    ),
                ],
                &[
                    "threadId",
                    "turnId",
                    "turnStatus",
                    "agentMessage",
                    "latestDiff",
                    "errors",
                ],
                None,
            ),
        ),
        (
            "RestCallBody",
            object_schema(
                vec![
                    (
                        "params",
                        any_value_schema(
                            "JSON-RPC params for the target method. Defaults to `null` when omitted.",
                        ),
                    ),
                    ("client", schema_ref("RestClientOptions")),
                ],
                &[],
                Some(false),
            ),
        ),
        (
            "RestCallResponse",
            object_schema(
                vec![
                    (
                        "method",
                        string_schema("The method that was called (echoes the path parameter)."),
                    ),
                    ("result", any_value_schema("The raw JSON-RPC result.")),
                ],
                &["method", "result"],
                None,
            ),
        ),
        (
            "RestSessionCreateRequest",
            object_schema(
                vec![("client", schema_ref("RestClientOptions"))],
                &[],
                Some(false),
            ),
        ),
        (
            "RestSessionCreateResponse",
            object_schema(
                vec![
                    (
                        "sessionId",
                        string_schema(
                            "Opaque session identifier for use in subsequent bridge calls.",
                        ),
                    ),
                    (
                        "initializeResponse",
                        any_value_schema("The raw app-server `initialize` response."),
                    ),
                ],
                &["sessionId", "initializeResponse"],
                None,
            ),
        ),
        (
            "RestSessionSummary",
            object_schema(
                vec![("sessionId", string_schema("Opaque session identifier."))],
                &["sessionId"],
                None,
            ),
        ),
        (
            "RestListSessionsResponse",
            object_schema(
                vec![(
                    "sessions",
                    array_schema(
                        schema_ref("RestSessionSummary"),
                        "Currently open bridge sessions.",
                    ),
                )],
                &["sessions"],
                None,
            ),
        ),
        (
            "RestStatusResponse",
            object_schema(
                vec![(
                    "status",
                    string_schema("A short status word, e.g. `\"deleted\"`."),
                )],
                &["status"],
                None,
            ),
        ),
        ("RestEventResponse", build_rest_event_response_schema()),
        (
            "RestRequestReplyResultRequest",
            object_schema(
                vec![(
                    "result",
                    any_value_schema("The JSON-RPC result to send back to the app-server."),
                )],
                &["result"],
                Some(false),
            ),
        ),
        (
            "RestErrorReplyRequest",
            object_schema(
                vec![
                    (
                        "code",
                        integer_schema("JSON-RPC error code sent back to the app-server."),
                    ),
                    (
                        "message",
                        string_schema("JSON-RPC error message sent back to the app-server."),
                    ),
                    (
                        "data",
                        any_value_schema(
                            "Optional JSON-RPC error data. Omitted (defaults to `null`) when not supplied.",
                        ),
                    ),
                ],
                &["code", "message"],
                Some(false),
            ),
        ),
        (
            "RestRequestReplyResponse",
            object_schema(
                vec![("status", string_schema("Always `\"ok\"` on success."))],
                &["status"],
                None,
            ),
        ),
        (
            "RestErrorResponse",
            object_schema(
                vec![
                    (
                        "error",
                        string_schema(
                            "Short machine-readable error kind, e.g. `\"not_found\"`, `\"rate_limited\"`, `\"json_rpc_error\"`.",
                        ),
                    ),
                    ("message", string_schema("Human-readable error message.")),
                    (
                        "code",
                        integer_schema(
                            "JSON-RPC error code, present only for `error: \"json_rpc_error\"` (a JSON-RPC error propagated from the app-server). Omitted otherwise.",
                        ),
                    ),
                    (
                        "data",
                        any_value_schema(
                            "JSON-RPC error data, present only for `error: \"json_rpc_error\"` when the app-server supplied one. Omitted otherwise.",
                        ),
                    ),
                ],
                &["error", "message"],
                None,
            ),
        ),
        (
            "CompatibilityReport",
            // NOTE: unlike every REST-specific type above, `CompatibilityReport` and
            // `SurfaceSummary` (in `src/compat.rs`) carry no `#[serde(rename_all = "camelCase")]`
            // - they predate the REST adapter and serialize with their literal Rust
            // (snake_case) field names. Getting this right is the entire point of this file.
            object_schema(
                vec![
                    (
                        "schema_codex_version",
                        string_schema(
                            "The Codex version this crate's vendored protocol schema was generated from (`schema/CODEX_VERSION.txt`).",
                        ),
                    ),
                    (
                        "installed_codex_version",
                        nullable_string_schema(
                            "Output of the local `codex --version`, or `null` if `codex` is not on `PATH` or the check failed.",
                        ),
                    ),
                    ("surface", schema_ref("SurfaceSummary")),
                ],
                &["schema_codex_version", "installed_codex_version", "surface"],
                None,
            ),
        ),
        (
            "SurfaceSummary",
            object_schema(
                vec![
                    (
                        "client_requests",
                        nonneg_integer_schema(
                            "Number of client->server request methods in the vendored schema.",
                        ),
                    ),
                    (
                        "server_requests",
                        nonneg_integer_schema(
                            "Number of server->client request methods in the vendored schema.",
                        ),
                    ),
                    (
                        "server_notifications",
                        nonneg_integer_schema(
                            "Number of server->client notification methods in the vendored schema.",
                        ),
                    ),
                    (
                        "client_notifications",
                        nonneg_integer_schema(
                            "Number of client->server notification methods in the vendored schema.",
                        ),
                    ),
                ],
                &[
                    "client_requests",
                    "server_requests",
                    "server_notifications",
                    "client_notifications",
                ],
                None,
            ),
        ),
    ];
    // The `RestEventResponse` union's variants are registered as real named
    // schemas so its `discriminator.mapping` refs resolve; see
    // `rest_event_response_variant_schemas`. `obj` sorts by key, so appending
    // here (rather than interleaving them alphabetically above) doesn't affect
    // the emitted key order.
    entries.extend(rest_event_response_variant_schemas());
    obj(entries)
}

/// The four `RestEventResponse` variants, as `(component schema name, `event`
/// tag value)` pairs.
///
/// Single source of truth for both the `components/schemas` entries
/// ([`rest_event_response_variant_schemas`]) and the parent union's
/// `oneOf`/`discriminator.mapping` ([`build_rest_event_response_schema`]).
/// Those three lists have to agree exactly - OpenAPI requires every
/// `discriminator.mapping` target to resolve to a real schema - and deriving
/// all of them from one table is what keeps a new variant from being added to
/// the `oneOf` while its mapping silently dangles.
const REST_EVENT_RESPONSE_VARIANTS: &[(&str, &str)] = &[
    ("RestEventResponseNotification", "notification"),
    ("RestEventResponseRequest", "request"),
    ("RestEventResponseClosed", "closed"),
    ("RestEventResponseTimeout", "timeout"),
];

/// The per-variant `components/schemas` entries backing
/// [`build_rest_event_response_schema`]'s `oneOf` refs.
///
/// These are registered as real named schemas rather than inlined into the
/// `oneOf` array specifically so `discriminator.mapping` has something to
/// point at: a mapping whose `$ref` names a schema that exists only inline is
/// unresolvable, and spec-compliant generators reject the whole document
/// rather than the one keyword. Naming them also gives generated clients real
/// per-variant types instead of an anonymous union member.
fn rest_event_response_variant_schemas() -> Vec<(&'static str, Value)> {
    vec![
        (
            "RestEventResponseNotification",
            object_schema(
                vec![
                    ("event", obj(vec![("const", json!("notification"))])),
                    (
                        "notification",
                        any_value_schema(
                            "The raw app-server JSON-RPC notification (has its own `method`/`params`).",
                        ),
                    ),
                ],
                &["event", "notification"],
                None,
            ),
        ),
        (
            "RestEventResponseRequest",
            object_schema(
                vec![
                    ("event", obj(vec![("const", json!("request"))])),
                    (
                        "requestKey",
                        string_schema(
                            "Opaque key for replying via the `.../requests/{requestKey}/result` or `/error` routes.",
                        ),
                    ),
                    (
                        "requestId",
                        any_value_schema(
                            "The JSON-RPC request id (typically a number or string) as sent by the app-server.",
                        ),
                    ),
                    (
                        "method",
                        string_schema(
                            "The server-originated JSON-RPC method name, e.g. `currentTime/read`.",
                        ),
                    ),
                    (
                        "request",
                        any_value_schema(
                            "The raw server-originated JSON-RPC request (`id`, `method`, `params`).",
                        ),
                    ),
                ],
                &["event", "requestKey", "requestId", "method", "request"],
                None,
            ),
        ),
        (
            "RestEventResponseClosed",
            object_schema(
                vec![("event", obj(vec![("const", json!("closed"))]))],
                &["event"],
                None,
            ),
        ),
        (
            "RestEventResponseTimeout",
            object_schema(
                vec![("event", obj(vec![("const", json!("timeout"))]))],
                &["event"],
                None,
            ),
        ),
    ]
}

/// `RestEventResponse` is an internally-tagged enum
/// (`#[serde(tag = "event", rename_all = "snake_case", rename_all_fields = "camelCase")]`),
/// modeled here as `oneOf` over one `$ref` per variant, discriminated by the
/// shared `event` property, per OpenAPI 3.1's `discriminator` keyword.
fn build_rest_event_response_schema() -> Value {
    obj(vec![
        (
            "description",
            json!(
                "One session event. Internally tagged on `event`: `notification` (a server \
                 notification arrived), `request` (a server-originated request arrived and \
                 awaits a reply), `closed` (the session's transport closed), or `timeout` (no \
                 event arrived within the poll budget - a normal outcome, not an error)."
            ),
        ),
        (
            "oneOf",
            Value::Array(
                REST_EVENT_RESPONSE_VARIANTS
                    .iter()
                    .map(|(schema_name, _)| schema_ref(schema_name))
                    .collect(),
            ),
        ),
        (
            "discriminator",
            obj(vec![
                ("propertyName", json!("event")),
                (
                    "mapping",
                    obj(REST_EVENT_RESPONSE_VARIANTS
                        .iter()
                        .map(|(schema_name, tag)| {
                            (*tag, json!(format!("#/components/schemas/{schema_name}")))
                        })
                        .collect()),
                ),
            ]),
        ),
    ])
}
