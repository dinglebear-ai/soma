use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{Request, StatusCode, header::AUTHORIZATION};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use tower_http::trace::TraceLayer;

use crate::{ExecuteOptions, StandaloneError, StandaloneRuntime};

#[derive(Clone)]
struct HttpState {
    runtime: Arc<StandaloneRuntime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct OperationBody {
    parameters: Value,
    confirmed: bool,
    idempotency_key: Option<String>,
    actor: Option<String>,
}

impl Default for OperationBody {
    fn default() -> Self {
        Self {
            parameters: json!({}),
            confirmed: false,
            idempotency_key: None,
            actor: None,
        }
    }
}

pub async fn serve(runtime: Arc<StandaloneRuntime>) -> anyhow::Result<()> {
    let bind = runtime.config().server.bind_addr()?;
    let router = router(runtime);
    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!(%bind, "standalone Synapse listening");
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

pub fn router(runtime: Arc<StandaloneRuntime>) -> Router {
    let state = HttpState {
        runtime: Arc::clone(&runtime),
    };
    let api = Router::new()
        .route("/operations", get(operations))
        .route("/activity", get(activity))
        .route("/openapi.json", get(openapi))
        .route("/v1/operations/{operation}/plan", post(plan))
        .route("/v1/operations/{operation}/execute", post(execute))
        .with_state(state.clone())
        .nest_service("/mcp", crate::mcp::http_service(Arc::clone(&runtime)));
    let protected = if let Some(token) = runtime.config().server.api_token.as_deref() {
        let expected = Arc::<[u8]>::from(format!("Bearer {token}").into_bytes());
        api.layer(middleware::from_fn_with_state(expected, bearer_auth))
    } else {
        api
    };
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/status", get(status))
        .with_state(state)
        .merge(protected)
        .layer(TraceLayer::new_for_http())
}

async fn health() -> Json<Value> {
    Json(json!({"status":"ok"}))
}

async fn ready(State(state): State<HttpState>) -> Json<Value> {
    Json(json!({
        "status":"ready",
        "operations":state.runtime.catalog().operation_count(),
        "hosts":state.runtime.config().hosts.len()
    }))
}

async fn status(State(state): State<HttpState>) -> Json<Value> {
    Json(json!({
        "name":"synapse",
        "version":env!("CARGO_PKG_VERSION"),
        "canonical_operations":state.runtime.catalog().operation_count(),
        "reads":35,
        "mutations":21,
        "hosts":state.runtime.config().hosts.len(),
        "activity_events":state.runtime.activity().len(),
        "mutation_policy": if state.runtime.config().server.allow_mutations {
            "configured_auto_confirmation"
        } else {
            "explicit_confirmation_required"
        },
        "authentication": if state.runtime.config().server.api_token.is_some() {
            "bearer"
        } else {
            "none"
        }
    }))
}

async fn operations(State(state): State<HttpState>) -> Json<Value> {
    Json(state.runtime.operation_catalog_json())
}

async fn activity(State(state): State<HttpState>) -> Json<Value> {
    Json(serde_json::to_value(state.runtime.activity().snapshot()).expect("activity serializes"))
}

async fn openapi(State(state): State<HttpState>) -> Json<Value> {
    Json(crate::openapi::document(state.runtime.as_ref()))
}

async fn plan(
    State(state): State<HttpState>,
    Path(operation): Path<String>,
    Json(body): Json<OperationBody>,
) -> Result<Json<Value>, ApiError> {
    let options = body.options("http-plan");
    let plan = state
        .runtime
        .plan(&operation, &body.parameters, &options)
        .await?;
    Ok(Json(
        serde_json::to_value(plan).map_err(anyhow::Error::from)?,
    ))
}

async fn execute(
    State(state): State<HttpState>,
    Path(operation): Path<String>,
    Json(body): Json<OperationBody>,
) -> Result<Json<Value>, ApiError> {
    let options = body.options("http");
    let result = state
        .runtime
        .execute(
            &operation,
            &body.parameters,
            &options,
            &CancellationToken::new(),
        )
        .await?;
    Ok(Json(result))
}

impl OperationBody {
    fn options(&self, default_actor: &str) -> ExecuteOptions {
        ExecuteOptions {
            confirmed: self.confirmed,
            idempotency_key: self.idempotency_key.clone(),
            actor: self.actor.clone().or_else(|| Some(default_actor.into())),
        }
    }
}

struct ApiError(StandaloneError);

impl<E> From<E> for ApiError
where
    E: Into<StandaloneError>,
{
    fn from(error: E) -> Self {
        Self(error.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if let Some(plan) = self.0.plan() {
            return (
                StatusCode::PRECONDITION_REQUIRED,
                Json(json!({
                    "error":"confirmation_required",
                    "message":self.0.to_string(),
                    "plan":plan
                })),
            )
                .into_response();
        }
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"request_failed","message":self.0.to_string()})),
        )
            .into_response()
    }
}

async fn bearer_auth(
    State(expected): State<Arc<[u8]>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let actual = request
        .headers()
        .get(AUTHORIZATION)
        .map(|value| value.as_bytes())
        .unwrap_or_default();
    if constant_time_eq(actual, expected.as_ref()) {
        next.run(request).await
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"unauthorized"})),
        )
            .into_response()
    }
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    for index in 0..left.len().max(right.len()) {
        let left = left.get(index).copied().unwrap_or_default();
        let right = right.get(index).copied().unwrap_or_default();
        difference |= usize::from(left ^ right);
    }
    difference == 0
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

#[cfg(test)]
#[path = "http_tests.rs"]
mod tests;
