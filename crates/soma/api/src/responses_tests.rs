use axum::{body::to_bytes, http::StatusCode};
use soma_application::ApplicationError;

use super::application_error_response;

#[tokio::test]
async fn application_errors_map_to_stable_http_status_and_json() {
    let response = application_error_response(ApplicationError::new(
        "response_too_large",
        "too large",
        false,
        "request less data",
    ));

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["code"], "response_too_large");
    assert_eq!(body["remediation"], "request less data");
}
