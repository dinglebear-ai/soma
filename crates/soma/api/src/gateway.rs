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
use soma_gateway::gateway::manager::GatewayManagerError;
use soma_gateway::upstream::UpstreamError;
use soma_runtime::server::{AppState, AuthPolicy};

use crate::responses::cap_json_response;

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

    match dispatch_gateway_action(&state.gateway, access, &action, params).await {
        Ok(value) => Json(cap_gateway_response(value)).into_response(),
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
    let status = match &error {
        GatewayDispatchError::AdminRequired => StatusCode::FORBIDDEN,
        GatewayDispatchError::Params(_) | GatewayDispatchError::SpawnValidation => {
            StatusCode::BAD_REQUEST
        }
        GatewayDispatchError::UnknownAction => StatusCode::NOT_FOUND,
        GatewayDispatchError::Manager(error) => manager_error_status(error),
    };
    (status, Json(error.structured(action).to_json())).into_response()
}

fn manager_error_status(error: &GatewayManagerError) -> StatusCode {
    match error {
        GatewayManagerError::Config(_)
        | GatewayManagerError::UpstreamExists(_)
        | GatewayManagerError::Upstream(UpstreamError::ParamsMustBeObject) => {
            StatusCode::BAD_REQUEST
        }
        GatewayManagerError::UpstreamMissing(_)
        | GatewayManagerError::Upstream(UpstreamError::UnknownUpstream { .. }) => {
            StatusCode::NOT_FOUND
        }
        GatewayManagerError::Upstream(UpstreamError::NotExposed { .. }) => StatusCode::FORBIDDEN,
        GatewayManagerError::Upstream(UpstreamError::Unsupported { .. }) => {
            StatusCode::NOT_IMPLEMENTED
        }
        GatewayManagerError::Upstream(UpstreamError::LiveConnect { .. })
        | GatewayManagerError::Upstream(UpstreamError::LiveCall { .. })
        | GatewayManagerError::OAuth(_) => StatusCode::SERVICE_UNAVAILABLE,
        GatewayManagerError::Upstream(UpstreamError::ResponseTooLarge { .. }) => {
            StatusCode::PAYLOAD_TOO_LARGE
        }
        GatewayManagerError::GatewayReloading
        | GatewayManagerError::StoreNotMounted
        | GatewayManagerError::Upstream(UpstreamError::NotRoutable { .. }) => {
            StatusCode::SERVICE_UNAVAILABLE
        }
    }
}

fn cap_gateway_response(value: Value) -> Value {
    cap_json_response(value, "Use a narrower gateway action or filter.")
        .unwrap_or_else(|_| json!({"error": "internal server error"}))
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
