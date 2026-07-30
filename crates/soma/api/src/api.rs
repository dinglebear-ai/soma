//! REST API handlers — direct `/v1/*` routes plus public health/status docs.
//!
//! All handlers are thin: parse HTTP input, call `SomaApplication`, return JSON.

#[path = "openapi.rs"]
mod openapi;
#[path = "probes.rs"]
mod probes;
#[path = "route_inventory.rs"]
mod route_inventory;

use anyhow::Result;
use axum::{
    extract::{Extension, Path, State, rejection::JsonRejection},
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
use serde_json::{Value, json};

use soma_application::ExecuteActionRequest;
use soma_domain::{
    Confirmation,
    actions::{SomaAction, action_spec},
};
use soma_http_api::json::{JsonBodyOutcome, json_body_or_else};

use crate::ApiState;
use crate::responses::{
    application_error_response, rest_error_response, rest_json_rejection_response,
};
pub use probes::{health, readyz, status};
pub use route_inventory::{CapabilitiesResponse, REST_ROUTES, RestRoute};

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

pub async fn v1_providers(State(state): State<ApiState>) -> axum::response::Response {
    if let Some(response) = refresh_file_providers(&state).await {
        return response;
    }
    Json(state.application().provider_inspection_report()).into_response()
}

pub async fn v1_greet(
    State(state): State<ApiState>,
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
    State(state): State<ApiState>,
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
    State(state): State<ApiState>,
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
    State(state): State<ApiState>,
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
    State(state): State<ApiState>,
    auth: Option<Extension<AuthContext>>,
    Path(action): Path<String>,
    body: Result<Json<Value>, JsonRejection>,
) -> axum::response::Response {
    let params = match json_body_or_else(body, true, || json!({})) {
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
    State(state): State<ApiState>,
    auth: Option<Extension<AuthContext>>,
    method: Method,
    Path(path): Path<String>,
    body: Result<Json<Value>, JsonRejection>,
) -> axum::response::Response {
    let route_path = format!("/v1/{path}");
    let method = method.as_str().to_ascii_uppercase();
    if let Some(response) = refresh_file_providers(&state).await {
        return response;
    }
    let action = match state.application().resolve_rest_route(&method, &route_path) {
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

    let params = match json_body_or_else(body, method == "GET" || method == "DELETE", || json!({}))
    {
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
    state: ApiState,
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
    state: ApiState,
    auth: Option<&AuthContext>,
    action_name: String,
    params: Value,
) -> axum::response::Response {
    if let Some(response) = refresh_file_providers(&state).await {
        return response;
    }
    let (params, confirmation) = split_rest_transport_metadata(&action_name, params);
    let request = ExecuteActionRequest {
        action: action_name.clone(),
        params,
    };
    let mut context = rest_execution_context(&state, auth);
    context.destructive_confirmation = confirmation;

    match state.application().execute_action(request, context).await {
        Ok(output) => Json(output.output).into_response(),
        Err(error) => application_error_response(error),
    }
}

fn rest_params(action: &SomaAction) -> Value {
    match action {
        SomaAction::Greet { name } => optional_name_params(name.clone()),
        SomaAction::Echo { message } => json!({ "message": message }),
        SomaAction::PythonEnvironmentPrunePlan {
            stale_before_unix_seconds,
            max_entries,
        }
        | SomaAction::PythonEnvironmentPrune {
            stale_before_unix_seconds,
            max_entries,
        } => json!({
            "stale_before_unix_seconds": stale_before_unix_seconds,
            "max_entries": max_entries,
        }),
        SomaAction::PythonEnvironmentRepair { provider_path }
        | SomaAction::PythonEnvironmentUpdate { provider_path } => {
            json!({"provider_path": provider_path})
        }
        SomaAction::PythonWorkerCancel { provider }
        | SomaAction::PythonWorkerReset { provider } => json!({"provider": provider}),
        SomaAction::PythonGenerationRollback { generation_id } => {
            json!({"generation_id": generation_id})
        }
        SomaAction::Status
        | SomaAction::PythonEnvironmentStatus
        | SomaAction::PythonWorkerStatus
        | SomaAction::PythonGenerationStatus
        | SomaAction::Help
        | SomaAction::ElicitName
        | SomaAction::ScaffoldIntent => json!({}),
    }
}

fn rest_execution_context(
    state: &ApiState,
    auth: Option<&AuthContext>,
) -> soma_application::ExecutionContext {
    let scopes = auth.map(|auth| auth.scopes.as_slice()).unwrap_or_default();
    state.execution_context(auth.map(|auth| auth.sub.as_str()), scopes)
}

fn split_rest_transport_metadata(action_name: &str, mut params: Value) -> (Value, Confirmation) {
    if !action_spec(action_name).is_some_and(|spec| spec.destructive) {
        return (params, Confirmation::Missing);
    }
    let confirm = params
        .as_object_mut()
        .and_then(|params| params.remove("confirm"));
    let confirmation = if confirm.as_ref().and_then(Value::as_bool) == Some(true) {
        Confirmation::Confirmed
    } else {
        Confirmation::Missing
    };
    (params, confirmation)
}

fn optional_name_params(name: Option<String>) -> Value {
    match name {
        Some(name) => json!({ "name": name }),
        None => json!({}),
    }
}

/// `GET /openapi.json` — generated OpenAPI schema for the REST surface.
pub async fn openapi_json(State(state): State<ApiState>) -> axum::response::Response {
    match build_openapi_document(&state).await {
        Ok(value) => Json(value).into_response(),
        Err(response) => response,
    }
}

/// Refresh, build, and gateway-augment the OpenAPI document, returning the
/// raw `Value` rather than a `Response`. `openapi_json` wraps this directly;
/// the composition root (`apps/soma`) also calls it so it can layer its own
/// route augmentation (e.g. Palette's `/v1/palette/*`) on top without
/// `soma-api` depending on a peer product-surface crate.
pub async fn build_openapi_document(state: &ApiState) -> Result<Value, axum::response::Response> {
    if let Some(response) = refresh_file_providers(state).await {
        return Err(response);
    }
    match state.application().openapi_document() {
        Ok(mut value) => {
            openapi::augment_with_static_action_routes(&mut value);
            openapi::augment_with_gateway_route(&mut value);
            Ok(value)
        }
        Err(error) => Err(application_error_response(error)),
    }
}

async fn refresh_file_providers(state: &ApiState) -> Option<axum::response::Response> {
    match state.application().refresh_providers_async().await {
        Ok(_) => None,
        Err(error) => {
            tracing::error!(%error, "provider refresh failed");
            Some(application_error_response(error))
        }
    }
}

#[cfg(test)]
#[path = "api_tests.rs"]
mod tests;
