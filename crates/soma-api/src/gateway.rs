use axum::{
    extract::{rejection::JsonRejection, Extension, Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
#[cfg(feature = "auth")]
use soma_auth::AuthContext;
#[cfg(not(feature = "auth"))]
pub struct AuthContext {
    pub scopes: Vec<String>,
}
use serde_json::{json, Value};

use soma_contracts::actions::{scopes_satisfy, READ_SCOPE};
use soma_contracts::scopes::has_admin_scope;
use soma_gateway::gateway::dispatch::{
    dispatch_gateway_action, GatewayAccess, GatewayDispatchError,
};
use soma_runtime::server::{AppState, AuthPolicy};

pub async fn v1_gateway_action(
    State(state): State<AppState>,
    auth: Option<Extension<AuthContext>>,
    Path(action): Path<String>,
    body: Result<Json<Value>, JsonRejection>,
) -> axum::response::Response {
    let params = match body {
        Ok(Json(value)) => value,
        Err(JsonRejection::MissingJsonContentType(_)) => json!({}),
        Err(error) => return json_rejection_response(error),
    };
    let access = gateway_access_from_scopes(
        &state.auth_policy,
        auth.as_ref()
            .map(|Extension(auth)| auth.scopes.as_slice())
            .unwrap_or_default(),
    );

    match dispatch_gateway_action(&state.gateway, access, &action, params) {
        Ok(value) => Json(value).into_response(),
        Err(error) => gateway_error_response(&action, error),
    }
}

#[must_use]
pub fn gateway_access_from_scopes(policy: &AuthPolicy, scopes: &[String]) -> GatewayAccess {
    match policy {
        AuthPolicy::LoopbackDev | AuthPolicy::TrustedGatewayUnscoped => GatewayAccess {
            read: true,
            admin: true,
        },
        AuthPolicy::Mounted { .. } => {
            let admin = has_admin_scope(scopes);
            GatewayAccess {
                read: admin || scopes_satisfy(scopes, READ_SCOPE),
                admin,
            }
        }
    }
}

fn gateway_error_response(action: &str, error: GatewayDispatchError) -> axum::response::Response {
    let status = match error {
        GatewayDispatchError::AdminRequired => StatusCode::FORBIDDEN,
        GatewayDispatchError::Params(_) | GatewayDispatchError::SpawnValidation => {
            StatusCode::BAD_REQUEST
        }
        GatewayDispatchError::Manager(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, Json(error.structured(action).to_json())).into_response()
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
