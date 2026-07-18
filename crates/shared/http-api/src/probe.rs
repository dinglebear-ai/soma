//! Reusable liveness/readiness probe DTOs and response helpers.
//!
//! Products wire these into their own `/health` and `/readyz` handlers,
//! supplying whatever async check represents "ready" for them.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

/// The one liveness value this crate emits. An enum (rather than a bare
/// `&'static str`) so a second construction site can't drift to an
/// unintended value — mirrors `ReadinessStatus` below.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LivenessStatus {
    Ok,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LivenessBody {
    pub status: LivenessStatus,
}

impl Default for LivenessBody {
    fn default() -> Self {
        Self {
            status: LivenessStatus::Ok,
        }
    }
}

/// The two readiness values this crate emits. An enum (rather than a bare
/// `&'static str`) so callers can't construct a third, unintended status
/// value that this module's response builders never produce.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReadinessStatus {
    Ready,
    NotReady,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReadinessBody {
    pub status: ReadinessStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// `200 OK` liveness response — "the process is up".
pub fn liveness_response() -> Response {
    Json(LivenessBody::default()).into_response()
}

/// Readiness response from a dependency check: `200 OK` with `{"status":
/// "ready"}` on success, `503 Service Unavailable` with the failure reason
/// on error. Unlike liveness, readiness probes an upstream dependency so
/// orchestrators only route traffic once the service can actually serve it.
pub fn readiness_response<E: std::fmt::Display>(result: Result<(), E>) -> Response {
    match result {
        Ok(()) => (
            StatusCode::OK,
            Json(ReadinessBody {
                status: ReadinessStatus::Ready,
                reason: None,
            }),
        )
            .into_response(),
        Err(error) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ReadinessBody {
                status: ReadinessStatus::NotReady,
                reason: Some(error.to_string()),
            }),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use axum::body::to_bytes;

    use super::*;

    #[tokio::test]
    async fn liveness_response_reports_ok() {
        let response = liveness_response();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["status"], "ok");
    }

    #[tokio::test]
    async fn readiness_response_ok_reports_ready() {
        let response = readiness_response::<std::convert::Infallible>(Ok(()));
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["status"], "ready");
        assert!(value.get("reason").is_none());
    }

    #[tokio::test]
    async fn readiness_response_err_reports_reason_and_503() {
        let response = readiness_response(Err("upstream unreachable"));
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["status"], "not_ready");
        assert_eq!(value["reason"], "upstream unreachable");
    }
}
