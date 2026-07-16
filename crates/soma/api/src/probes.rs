use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};
use soma_runtime::server::AppState;

/// `GET /health` — liveness probe (unauthenticated).
pub async fn health() -> impl IntoResponse {
    tracing::debug!("health probe");
    Json(json!({ "status": "ok" }))
}

/// `GET /readyz` — readiness probe (unauthenticated).
///
/// Unlike `/health` (pure liveness: "the process is up"), this probes the
/// upstream dependency and returns `503 Service Unavailable` when it is
/// unreachable, so orchestrators only route traffic once the server can serve it.
pub async fn readyz(State(state): State<AppState>) -> Response {
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

/// `GET /status` — local runtime status (unauthenticated, redacts secrets).
pub async fn status(State(state): State<AppState>) -> Response {
    Json(status_body(&state)).into_response()
}

fn status_body(state: &AppState) -> Value {
    json!({
        "status": "ok",
        "server": state.config.server_name,
        "version": env!("CARGO_PKG_VERSION"),
        "transport": "http",
    })
}

#[cfg(test)]
#[path = "probes_tests.rs"]
mod tests;
