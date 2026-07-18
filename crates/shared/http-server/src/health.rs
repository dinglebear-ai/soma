//! Generic health-check route wiring.
//!
//! `soma_http_api::probe` owns the liveness/readiness response *bodies*.
//! This module owns mounting them as ready-to-merge routers so a product
//! doesn't have to hand-roll the same two routes itself. Both are generic
//! over the router's state type so a product can `.merge()` them into a
//! bigger stateful router without a state-shape mismatch.
//!
//! Not yet adopted: `apps/soma`'s `/health` and `/readyz` routes are still
//! hand-implemented in `crates/soma/api` rather than mounting these
//! routers. This module is available scaffolding for that future
//! consolidation, not something already wired into a live Soma surface.

use std::future::Future;

use axum::{routing::get, Router};
use soma_http_api::probe::{liveness_response, readiness_response};

/// Mount `GET /health`, an always-`200 OK` liveness probe.
pub fn liveness_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new().route("/health", get(|| async { liveness_response() }))
}

/// Mount `GET /readyz`, a readiness probe backed by `check`.
///
/// `check` runs on every request. `Ok(())` renders `200 OK`; `Err(e)`
/// renders `503 Service Unavailable` with `e.to_string()` as the reason —
/// see [`soma_http_api::probe::readiness_response`].
pub fn readiness_router<S, F, Fut, E>(check: F) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    F: Fn() -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<(), E>> + Send,
    E: std::fmt::Display,
{
    Router::new().route(
        "/readyz",
        get(move || {
            let check = check.clone();
            async move { readiness_response(check().await) }
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::to_bytes, body::Body, http::Request, http::StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn liveness_router_reports_ok() {
        let app: Router<()> = liveness_router();
        let request = Request::builder()
            .uri("/health")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["status"], "ok");
    }

    #[tokio::test]
    async fn readiness_router_reports_ready_when_check_succeeds() {
        let app: Router<()> = readiness_router(|| async { Ok::<(), &'static str>(()) });
        let request = Request::builder()
            .uri("/readyz")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn readiness_router_reports_unavailable_when_check_fails() {
        let app: Router<()> =
            readiness_router(|| async { Err::<(), &'static str>("upstream down") });
        let request = Request::builder()
            .uri("/readyz")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["reason"], "upstream down");
    }
}
