//! REST API handlers — direct `/v1/*` routes plus public health/status docs.
//!
//! All handlers are thin: parse the request, call the service, return JSON.
//! Business logic lives in `app.rs`.

use anyhow::Result;
use axum::{
    extract::{rejection::JsonRejection, Extension, Path, State},
    http::{Method, StatusCode},
    response::{IntoResponse, Json},
};
#[cfg(feature = "auth")]
use rtemplate_auth::AuthContext;
#[cfg(not(feature = "auth"))]
struct AuthContext {
    sub: String,
    scopes: Vec<String>,
}
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use rtemplate_contracts::actions::ExampleAction;
use rtemplate_contracts::token_limit::MAX_RESPONSE_BYTES;
use rtemplate_runtime::server::{AppState, AuthPolicy};
use rtemplate_service::{
    classify_service_error, ProviderAuthMode, ProviderCall, ProviderError, ProviderPrincipal,
    ProviderRequestLimits, ProviderSurface,
};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct RestRoute {
    pub method: &'static str,
    pub path: &'static str,
    pub action: Option<&'static str>,
    pub auth: &'static str,
    pub description: &'static str,
}

pub const REST_ROUTES: &[RestRoute] = &[
    RestRoute {
        method: "GET",
        path: "/health",
        action: None,
        auth: "public",
        description: "Fast liveness probe.",
    },
    RestRoute {
        method: "GET",
        path: "/readyz",
        action: None,
        auth: "public",
        description: "Readiness probe; 503 when the upstream dependency is unreachable.",
    },
    RestRoute {
        method: "GET",
        path: "/metrics",
        action: None,
        auth: "public",
        description:
            "Prometheus metrics (text exposition format; requires the observability feature).",
    },
    RestRoute {
        method: "GET",
        path: "/status",
        action: None,
        auth: "public",
        description: "Local redacted runtime status.",
    },
    RestRoute {
        method: "GET",
        path: "/openapi.json",
        action: None,
        auth: "public",
        description: "Generated OpenAPI schema.",
    },
    RestRoute {
        method: "GET",
        path: "/v1/capabilities",
        action: None,
        auth: "mounted auth policy",
        description: "Direct REST route inventory and server metadata.",
    },
    RestRoute {
        method: "POST",
        path: "/v1/greet",
        action: Some("greet"),
        auth: "mounted auth policy; requires example:read when scoped",
        description: "Return a greeting.",
    },
    RestRoute {
        method: "POST",
        path: "/v1/echo",
        action: Some("echo"),
        auth: "mounted auth policy; requires example:read when scoped",
        description: "Echo a message back unchanged.",
    },
    RestRoute {
        method: "GET",
        path: "/v1/status",
        action: Some("status"),
        auth: "mounted auth policy; requires example:read when scoped",
        description: "Return authenticated service status.",
    },
    RestRoute {
        method: "GET",
        path: "/v1/help",
        action: Some("help"),
        auth: "mounted auth policy",
        description: "Return the action catalog and route help.",
    },
];

#[derive(Debug, Serialize)]
pub struct CapabilitiesResponse {
    pub server: &'static str,
    pub version: &'static str,
    pub preferred_rest_style: &'static str,
    pub supported_routes: Vec<String>,
    pub routes: &'static [RestRoute],
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GreetRequest {
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EchoRequest {
    pub message: String,
}

pub async fn v1_capabilities() -> impl IntoResponse {
    Json(CapabilitiesResponse {
        server: "rtemplate-mcp",
        version: env!("CARGO_PKG_VERSION"),
        preferred_rest_style: "direct_routes",
        supported_routes: REST_ROUTES
            .iter()
            .map(|route| format!("{} {}", route.method, route.path))
            .collect(),
        routes: REST_ROUTES,
    })
}

pub async fn v1_greet(
    State(state): State<AppState>,
    auth: Option<Extension<AuthContext>>,
    body: Result<Json<GreetRequest>, JsonRejection>,
) -> axum::response::Response {
    let Json(body) = match body {
        Ok(body) => body,
        Err(error) => return rest_json_rejection_response(error),
    };
    run_rest_action_request(
        state,
        auth.as_ref().map(|Extension(auth)| auth),
        ExampleAction::from_rest("greet", &optional_name_params(body.name)),
        "greet",
    )
    .await
}

pub async fn v1_echo(
    State(state): State<AppState>,
    auth: Option<Extension<AuthContext>>,
    body: Result<Json<EchoRequest>, JsonRejection>,
) -> axum::response::Response {
    let Json(body) = match body {
        Ok(body) => body,
        Err(error) => return rest_json_rejection_response(error),
    };
    run_rest_action_request(
        state,
        auth.as_ref().map(|Extension(auth)| auth),
        ExampleAction::from_rest("echo", &json!({ "message": body.message })),
        "echo",
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
        Ok(ExampleAction::Status),
        "status",
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
        Ok(ExampleAction::Help),
        "help",
    )
    .await
}

pub async fn v1_dynamic_provider_route(
    State(state): State<AppState>,
    auth: Option<Extension<AuthContext>>,
    method: Method,
    Path(path): Path<String>,
    body: Result<Json<Value>, JsonRejection>,
) -> axum::response::Response {
    let route_path = format!("/v1/{path}");
    let method = method.as_str().to_ascii_uppercase();
    let action = match state
        .provider_registry
        .snapshot()
        .route_action(&method, &route_path)
        .map(str::to_owned)
    {
        Some(action) => action,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "error": "not_found",
                    "message": format!("No provider route registered for {method} {route_path}"),
                })),
            )
                .into_response();
        }
    };

    let params = if method == "GET" || method == "DELETE" {
        match body {
            Ok(Json(value)) => value,
            Err(JsonRejection::MissingJsonContentType(_)) => json!({}),
            Err(error) => return rest_json_rejection_response(error),
        }
    } else {
        match body {
            Ok(Json(value)) => value,
            Err(error) => return rest_json_rejection_response(error),
        }
    };

    run_provider_rest_action(
        state,
        auth.as_ref().map(|Extension(auth)| auth),
        action,
        params,
    )
    .await
}

async fn run_rest_action_request(
    state: AppState,
    auth: Option<&AuthContext>,
    action: Result<ExampleAction>,
    action_name: &str,
) -> axum::response::Response {
    match action {
        Ok(action) => {
            let action_name = action.name().to_owned();
            run_provider_rest_action(state, auth, action_name, rest_params(&action)).await
        }
        Err(error) => rest_error_response(error, action_name),
    }
}

async fn run_provider_rest_action(
    state: AppState,
    auth: Option<&AuthContext>,
    action_name: String,
    params: Value,
) -> axum::response::Response {
    let call = ProviderCall {
        provider: String::new(),
        action: action_name.clone(),
        params,
        principal: rest_principal(auth),
        auth_mode: rest_auth_mode(&state),
        surface: ProviderSurface::Rest,
        destructive_confirmed: false,
        limits: ProviderRequestLimits::default(),
        snapshot_id: String::new(),
    };

    match state.provider_registry.dispatch(call).await {
        Ok(output) => match cap_rest_response(output.value) {
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
        Err(e) => provider_rest_error_response(e),
    }
}

fn rest_params(action: &ExampleAction) -> Value {
    match action {
        ExampleAction::Greet { name } => optional_name_params(name.clone()),
        ExampleAction::Echo { message } => json!({ "message": message }),
        ExampleAction::Status
        | ExampleAction::Help
        | ExampleAction::ElicitName
        | ExampleAction::ScaffoldIntent => json!({}),
    }
}

fn rest_principal(auth: Option<&AuthContext>) -> ProviderPrincipal {
    match auth {
        Some(auth) => ProviderPrincipal {
            subject: auth.sub.clone(),
            scopes: auth.scopes.clone(),
        },
        None => ProviderPrincipal::anonymous(),
    }
}

fn rest_auth_mode(state: &AppState) -> ProviderAuthMode {
    match &state.auth_policy {
        AuthPolicy::LoopbackDev => ProviderAuthMode::LoopbackDev,
        AuthPolicy::TrustedGatewayUnscoped => ProviderAuthMode::TrustedGateway,
        AuthPolicy::Mounted { .. } => ProviderAuthMode::Mounted,
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

fn provider_rest_error_response(error: ProviderError) -> axum::response::Response {
    let status = match &*error.code {
        "unknown_action" | "surface_not_exposed" => StatusCode::NOT_FOUND,
        "insufficient_scope" | "capability_denied" => StatusCode::FORBIDDEN,
        "input_too_large" | "response_too_large" => StatusCode::PAYLOAD_TOO_LARGE,
        "input_schema_failed" | "confirmation_required" => StatusCode::BAD_REQUEST,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    let (provider, action, code) = error.log_code();
    tracing::warn!(provider, action, code, "REST provider call failed");
    (
        status,
        Json(serde_json::to_value(error).unwrap_or_else(|_| json!({"error":"provider_error"}))),
    )
        .into_response()
}

fn optional_name_params(name: Option<String>) -> Value {
    match name {
        Some(name) => json!({ "name": name }),
        None => json!({}),
    }
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
pub async fn openapi_json(State(state): State<AppState>) -> axum::response::Response {
    match serde_json::from_slice::<Value>(&state.provider_registry.snapshot().cached_openapi_bytes)
    {
        Ok(value) => Json(value).into_response(),
        Err(error) => {
            tracing::error!(%error, "runtime OpenAPI snapshot failed to deserialize");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "openapi_unavailable"})),
            )
                .into_response()
        }
    }
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
