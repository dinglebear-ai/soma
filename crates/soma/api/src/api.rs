//! REST API handlers — direct `/v1/*` routes plus public health/status docs.
//!
//! All handlers are thin: parse the request, call the service, return JSON.
//! Business logic lives in `app.rs`.

#[path = "openapi.rs"]
mod openapi;
#[path = "probes.rs"]
mod probes;
#[path = "route_inventory.rs"]
mod route_inventory;

use anyhow::Result;
use axum::{
    extract::{rejection::JsonRejection, Extension, Path, State},
    http::{Method, StatusCode},
    response::{IntoResponse, Json},
};
#[cfg(feature = "auth")]
use soma_auth::AuthContext;
#[cfg(not(feature = "auth"))]
pub struct AuthContext {
    sub: String,
    scopes: Vec<String>,
}
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use soma_contracts::actions::SomaAction;
use soma_runtime::server::{AppState, AuthPolicy};
use soma_service::{
    ProviderAuthMode, ProviderCall, ProviderPrincipal, ProviderRequestLimits, ProviderSurface,
};

use crate::responses::{
    cap_rest_response, provider_rest_error_response, rest_error_response,
    rest_json_rejection_response,
};
pub use probes::{health, readyz, status};
pub use route_inventory::{CapabilitiesResponse, RestRoute, REST_ROUTES};

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
    Json(route_inventory::capabilities_response())
}

pub async fn v1_providers(State(state): State<AppState>) -> axum::response::Response {
    if let Some(response) = refresh_file_providers(&state) {
        return response;
    }
    Json(state.provider_registry.snapshot().inspection_report()).into_response()
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
        SomaAction::from_rest("greet", &optional_name_params(body.name)),
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
        SomaAction::from_rest("echo", &json!({ "message": body.message })),
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
        Ok(SomaAction::Status),
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
        Ok(SomaAction::Help),
        "help",
    )
    .await
}

pub async fn v1_provider_tool_action(
    State(state): State<AppState>,
    auth: Option<Extension<AuthContext>>,
    Path(action): Path<String>,
    body: Result<Json<Value>, JsonRejection>,
) -> axum::response::Response {
    let params = match json_body_or_empty(body, true) {
        JsonBodyOutcome::Params(params) => params,
        JsonBodyOutcome::Response(response) => return response,
    };

    run_provider_rest_action(
        state,
        auth.as_ref().map(|Extension(auth)| auth),
        action,
        params,
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
    if let Some(response) = refresh_file_providers(&state) {
        return response;
    }
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

    let params = match json_body_or_empty(body, method == "GET" || method == "DELETE") {
        JsonBodyOutcome::Params(params) => params,
        JsonBodyOutcome::Response(response) => return response,
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
    action: Result<SomaAction>,
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
    if let Some(response) = refresh_file_providers(&state) {
        return response;
    }
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

fn rest_params(action: &SomaAction) -> Value {
    match action {
        SomaAction::Greet { name } => optional_name_params(name.clone()),
        SomaAction::Echo { message } => json!({ "message": message }),
        SomaAction::Status
        | SomaAction::Help
        | SomaAction::ElicitName
        | SomaAction::ScaffoldIntent => json!({}),
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

fn optional_name_params(name: Option<String>) -> Value {
    match name {
        Some(name) => json!({ "name": name }),
        None => json!({}),
    }
}

enum JsonBodyOutcome {
    Params(Value),
    Response(axum::response::Response),
}

fn json_body_or_empty(
    body: Result<Json<Value>, JsonRejection>,
    allow_missing: bool,
) -> JsonBodyOutcome {
    match body {
        Ok(Json(value)) => JsonBodyOutcome::Params(value),
        Err(JsonRejection::MissingJsonContentType(_)) if allow_missing => {
            JsonBodyOutcome::Params(json!({}))
        }
        Err(error) => JsonBodyOutcome::Response(rest_json_rejection_response(error)),
    }
}

/// `GET /openapi.json` — generated OpenAPI schema for the REST surface.
pub async fn openapi_json(State(state): State<AppState>) -> axum::response::Response {
    if let Some(response) = refresh_file_providers(&state) {
        return response;
    }
    match serde_json::from_slice::<Value>(&state.provider_registry.snapshot().cached_openapi_bytes)
    {
        Ok(mut value) => {
            openapi::augment_with_gateway_route(&mut value);
            Json(value).into_response()
        }
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

fn refresh_file_providers(state: &AppState) -> Option<axum::response::Response> {
    match state.provider_registry.refresh_file_providers() {
        Ok(_) => None,
        Err(error) => {
            tracing::error!(%error, "provider refresh failed");
            Some(
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({
                        "error": "provider_refresh_failed",
                        "message": error.to_string(),
                    })),
                )
                    .into_response(),
            )
        }
    }
}

#[cfg(test)]
#[path = "api_tests.rs"]
mod tests;
