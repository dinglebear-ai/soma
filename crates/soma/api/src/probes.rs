use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};
use soma_http_api::probe::{liveness_response, readiness_response};

use crate::ApiState;

/// `GET /health` — liveness probe (unauthenticated).
pub async fn health() -> impl IntoResponse {
    tracing::debug!("health probe");
    liveness_response()
}

/// `GET /readyz` — readiness probe (unauthenticated).
///
/// Unlike `/health` (pure liveness: "the process is up"), this probes the
/// upstream dependency and returns `503 Service Unavailable` when it is
/// unreachable, so orchestrators only route traffic once the server can serve it.
pub async fn readyz(State(state): State<ApiState>) -> Response {
    let result = state.application().readiness().await;
    if let Err(error) = &result {
        tracing::warn!(%error, "readiness probe failed");
    }
    readiness_response(result)
}

/// `GET /status` — local runtime status (unauthenticated, redacts secrets).
pub async fn status(State(state): State<ApiState>) -> Response {
    Json(status_body(state.server_name())).into_response()
}

fn status_body(server_name: &str) -> Value {
    json!({
        "status": "ok",
        "server": server_name,
        "version": env!("CARGO_PKG_VERSION"),
        "transport": "http",
    })
}

#[cfg(test)]
#[path = "probes_tests.rs"]
mod tests;
