use axum::{
    extract::{rejection::JsonRejection, Extension, Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
#[cfg(feature = "auth")]
use soma_auth::AuthContext;
#[cfg(not(feature = "auth"))]
pub struct AuthContext {
    pub sub: String,
    pub scopes: Vec<String>,
}
use serde_json::{json, Value};
use soma_application::{ApplicationError, GatewayExecuteRequest};

use crate::{responses::application_error_status, ApiState};

pub async fn v1_gateway_action(
    State(state): State<ApiState>,
    auth: Option<Extension<AuthContext>>,
    Path(action): Path<String>,
    body: Result<Json<Value>, JsonRejection>,
) -> axum::response::Response {
    let params = match body {
        Ok(Json(value)) => value,
        Err(JsonRejection::MissingJsonContentType(_)) => json!({}),
        Err(error) => return json_rejection_response(error),
    };
    let auth = auth.as_ref().map(|Extension(auth)| auth);
    let scopes = auth.map(|auth| auth.scopes.as_slice()).unwrap_or_default();
    let context = state.execution_context(auth.map(|auth| auth.sub.as_str()), scopes);

    match state
        .application()
        .gateway_execute(
            GatewayExecuteRequest {
                action: action.clone(),
                params,
            },
            context,
        )
        .await
    {
        Ok(response) => Json(response.output).into_response(),
        Err(error) => gateway_error_response(&action, error),
    }
}

fn gateway_error_response(action: &str, error: ApplicationError) -> axum::response::Response {
    let status = application_error_status(&error);
    let kind = match error.code.as_str() {
        "admin_required" | "not_exposed" => "authorization",
        "invalid_param"
        | "unknown_action"
        | "spawn_validation_failed"
        | "upstream_exists"
        | "upstream_missing"
        | "invalid_config"
        | "unknown_upstream" => "validation",
        "unsupported_transport" => "unsupported",
        "response_too_large" => "limits",
        "store_not_mounted" => "configuration",
        _ => "runtime",
    };
    (
        status,
        Json(json!({
            "isError": true,
            "schema_version": "mcp.gateway.error.v1",
            "code": error.code,
            "kind": kind,
            "tool": "gateway",
            "action": action,
            "remediation": error.remediation,
        })),
    )
        .into_response()
}

fn json_rejection_response(error: JsonRejection) -> axum::response::Response {
    let status = if error.status() == StatusCode::PAYLOAD_TOO_LARGE {
        StatusCode::PAYLOAD_TOO_LARGE
    } else {
        StatusCode::BAD_REQUEST
    };
    (status, Json(json!({"error": error.to_string()}))).into_response()
}

#[cfg(test)]
#[path = "gateway_tests.rs"]
mod tests;
