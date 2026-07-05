//! REST API handlers — direct `/v1/*` routes plus public health/status docs.
//!
//! All handlers are thin: parse the request, call the service, return JSON.
//! Business logic lives in `app.rs`.

use anyhow::Result;
use axum::{
    extract::{rejection::JsonRejection, Extension, Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Json},
};
#[cfg(feature = "auth")]
use rtemplate_auth::AuthContext;
#[cfg(not(feature = "auth"))]
struct AuthContext {
    sub: String,
    scopes: Vec<String>,
}
use serde::Serialize;
use serde_json::{json, Value};

use rtemplate_contracts::{actions::ActionSpec, token_limit::MAX_RESPONSE_BYTES};
use rtemplate_runtime::server::{AppState, AuthPolicy};
use rtemplate_service::{classify_service_error, dispatch_action, validate_params};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RestRoute {
    pub method: String,
    pub path: String,
    pub action: Option<String>,
    pub auth: String,
    pub description: String,
}

pub const INFRA_REST_ROUTES: &[(&str, &str, &str, &str)] = &[
    ("GET", "/health", "public", "Fast liveness probe."),
    (
        "GET",
        "/readyz",
        "public",
        "Readiness probe; 503 when the upstream dependency is unreachable.",
    ),
    (
        "GET",
        "/metrics",
        "public",
        "Prometheus metrics (text exposition format; requires the observability feature).",
    ),
    ("GET", "/status", "public", "Local redacted runtime status."),
    (
        "GET",
        "/openapi.json",
        "public",
        "Generated OpenAPI schema.",
    ),
    (
        "GET",
        "/v1/capabilities",
        "mounted auth policy",
        "Direct REST route inventory and server metadata.",
    ),
];

pub fn rest_routes() -> Vec<RestRoute> {
    let mut routes: Vec<RestRoute> = INFRA_REST_ROUTES
        .iter()
        .map(|(method, path, auth, description)| RestRoute {
            method: (*method).to_owned(),
            path: (*path).to_owned(),
            action: None,
            auth: (*auth).to_owned(),
            description: (*description).to_owned(),
        })
        .collect();
    routes.extend(
        rtemplate_service::action_specs()
            .iter()
            .filter(|spec| spec.transport.rest())
            .map(rest_route_from_action),
    );
    routes
}

fn rest_route_from_action(spec: &ActionSpec) -> RestRoute {
    RestRoute {
        method: spec.rest_method.unwrap_or("POST").to_owned(),
        path: spec.rest_path.unwrap_or("/v1/{action}").to_owned(),
        action: Some(spec.name.to_owned()),
        auth: rest_auth_description(spec).to_owned(),
        description: spec.description.to_owned(),
    }
}

fn rest_auth_description(spec: &ActionSpec) -> &'static str {
    match spec.required_scope {
        Some(scope) if scope == rtemplate_contracts::actions::READ_SCOPE => {
            "mounted auth policy; requires example:read when scoped"
        }
        Some(scope) if scope == rtemplate_contracts::actions::WRITE_SCOPE => {
            "mounted auth policy; requires example:write when scoped"
        }
        Some(_) => "mounted auth policy; requires configured action scope when scoped",
        None => "mounted auth policy",
    }
}

#[derive(Debug, Serialize)]
pub struct CapabilitiesResponse {
    pub server: &'static str,
    pub version: &'static str,
    pub preferred_rest_style: &'static str,
    pub supported_routes: Vec<String>,
    pub routes: Vec<RestRoute>,
}

pub async fn v1_capabilities() -> impl IntoResponse {
    let routes = rest_routes();
    Json(CapabilitiesResponse {
        server: "rtemplate-mcp",
        version: env!("CARGO_PKG_VERSION"),
        preferred_rest_style: "direct_routes",
        supported_routes: routes
            .iter()
            .map(|route| format!("{} {}", route.method, route.path))
            .collect(),
        routes,
    })
}

pub async fn v1_action_post(
    State(state): State<AppState>,
    auth: Option<Extension<AuthContext>>,
    Path(action): Path<String>,
    body: Result<Json<Value>, JsonRejection>,
) -> axum::response::Response {
    let Json(params) = match body {
        Ok(body) => body,
        Err(error) => return rest_json_rejection_response(error),
    };
    let Some(spec) = rtemplate_service::action_registry().rest_post(&action) else {
        return (StatusCode::NOT_FOUND, Json(json!({"error": "not_found"}))).into_response();
    };
    run_rest_action_request(
        state,
        auth.as_ref().map(|Extension(auth)| auth),
        spec.name,
        params,
    )
    .await
}

pub async fn v1_service_status(
    State(state): State<AppState>,
    auth: Option<Extension<AuthContext>>,
) -> axum::response::Response {
    run_rest_action_request(
        state,
        auth.as_ref().map(|Extension(auth)| auth),
        "status",
        json!({}),
    )
    .await
}

pub async fn v1_help(
    State(state): State<AppState>,
    auth: Option<Extension<AuthContext>>,
) -> axum::response::Response {
    run_rest_action_request(
        state,
        auth.as_ref().map(|Extension(auth)| auth),
        "help",
        json!({}),
    )
    .await
}

async fn run_rest_action_request(
    state: AppState,
    auth: Option<&AuthContext>,
    action_name: &str,
    params: Value,
) -> axum::response::Response {
    let Some(spec) = rtemplate_service::action_registry().action(action_name) else {
        return rest_error_response(
            rtemplate_contracts::actions::action_error(
                rtemplate_contracts::actions::ValidationError::UnknownAction {
                    action: action_name.to_owned(),
                },
            ),
            action_name,
        );
    };
    if !spec.transport.rest() {
        return (StatusCode::NOT_FOUND, Json(json!({"error": "not_found"}))).into_response();
    }
    if let Some(response) = enforce_rest_scope(&state, auth, action_name) {
        return response;
    }
    if spec.requires_admin {
        let Some(auth) = auth else {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({"error": "forbidden: missing auth context"})),
            )
                .into_response();
        };
        if !auth.scopes.iter().any(|scope| scope == "admin") {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({"error": "forbidden: requires admin"})),
            )
                .into_response();
        }
    }
    if let Err(error) = rtemplate_contracts::actions::require_confirmation_if_destructive_from(
        rtemplate_service::action_specs(),
        action_name,
        &params,
    ) {
        return (
            StatusCode::from_u16(error.http_status_code()).unwrap_or(StatusCode::BAD_REQUEST),
            Json(error.to_rest_payload()),
        )
            .into_response();
    }
    if let Err(error) = validate_params(spec, &params) {
        return rest_error_response(error, action_name);
    }

    match dispatch_action(&state.service, action_name, &params, "rest").await {
        Ok(value) => match cap_rest_response(value) {
            Ok(value) => Json(value).into_response(),
            Err(e) => {
                tracing::error!(error = %e, action = %action_name, "REST response serialization failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "internal server error"})),
                )
                    .into_response()
            }
        },
        Err(e) => rest_error_response(e, action_name),
    }
}

fn rest_error_response(error: anyhow::Error, action: &str) -> axum::response::Response {
    let tool_error = classify_service_error(&error);
    if tool_error.kind == rtemplate_contracts::errors::ServiceErrorKind::Validation {
        tracing::warn!(
            action = %action,
            code = %tool_error.code,
            "REST action rejected invalid params"
        );
    } else {
        tracing::error!(
            error = %error,
            action = %action,
            service_error_kind = %tool_error.kind.as_str(),
            "REST action execution failed"
        );
    }
    (
        StatusCode::from_u16(tool_error.http_status_code())
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
        Json(tool_error.to_rest_payload()),
    )
        .into_response()
}

fn rest_json_rejection_response(error: JsonRejection) -> axum::response::Response {
    let status = if error.status() == StatusCode::PAYLOAD_TOO_LARGE {
        StatusCode::PAYLOAD_TOO_LARGE
    } else {
        StatusCode::BAD_REQUEST
    };
    (status, Json(json!({"error": error.to_string()}))).into_response()
}

fn cap_rest_response(value: Value) -> Result<Value> {
    let serialized = serde_json::to_vec(&value)?;
    if serialized.len() <= MAX_RESPONSE_BYTES {
        return Ok(value);
    }
    Ok(json!({
        "truncated": true,
        "error": "response exceeded REST response size limit",
        "max_response_bytes": MAX_RESPONSE_BYTES,
        "hint": "Use limit/offset parameters or more specific filters to get a smaller result.",
    }))
}

fn enforce_rest_scope(
    state: &AppState,
    auth: Option<&AuthContext>,
    action: &str,
) -> Option<axum::response::Response> {
    if !matches!(&state.auth_policy, AuthPolicy::Mounted { .. }) {
        return None;
    }
    let required_scope = rtemplate_contracts::actions::required_scope_for_action_from(
        rtemplate_service::action_specs(),
        action,
    )?;
    let Some(auth) = auth else {
        tracing::warn!(action = %action, "REST action denied: missing auth context");
        return Some(
            (
                StatusCode::FORBIDDEN,
                Json(json!({"error": "forbidden: missing auth context"})),
            )
                .into_response(),
        );
    };
    let satisfied = rtemplate_contracts::actions::scopes_satisfy(&auth.scopes, required_scope);
    if satisfied {
        return None;
    }
    tracing::warn!(
        subject = %auth.sub,
        action = %action,
        required_scope = %required_scope,
        "REST action denied: insufficient scope"
    );
    Some(
        (
            StatusCode::FORBIDDEN,
            Json(json!({"error": format!("forbidden: requires scope: {required_scope}")})),
        )
            .into_response(),
    )
}

/// `GET /health` — liveness probe (unauthenticated).
pub async fn health() -> impl IntoResponse {
    tracing::debug!("health probe");
    Json(json!({ "status": "ok" }))
}

/// `GET /readyz` — readiness probe (unauthenticated).
///
/// Unlike `/health` (pure liveness: "the process is up"), this probes the
/// upstream dependency and returns `503 Service Unavailable` when it is
/// unreachable, so orchestrators (Kubernetes, compose healthchecks, load
/// balancers) only route traffic once the server can actually serve it.
pub async fn readyz(State(state): State<AppState>) -> impl IntoResponse {
    match state.service.ready().await {
        Ok(()) => (StatusCode::OK, Json(json!({ "status": "ready" }))).into_response(),
        Err(error) => {
            tracing::warn!(%error, "readiness probe failed");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "status": "not_ready", "reason": error.to_string() })),
            )
                .into_response()
        }
    }
}

/// `GET /openapi.json` — generated OpenAPI schema for the REST surface.
pub async fn openapi_json() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
        include_str!("../../../docs/generated/openapi.json"),
    )
}

/// `GET /status` — local runtime status (unauthenticated, redacts secrets).
pub async fn status(State(state): State<AppState>) -> impl IntoResponse {
    Json(json!({
        "status": "ok",
        "server": state.config.server_name,
        "version": env!("CARGO_PKG_VERSION"),
        "transport": "http",
    }))
    .into_response()
}

#[cfg(test)]
#[path = "api_tests.rs"]
mod tests;
