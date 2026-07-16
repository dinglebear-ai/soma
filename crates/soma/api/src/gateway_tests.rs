use axum::{body::to_bytes, http::StatusCode};
use soma_application::ApplicationError;

use super::gateway_error_response;

#[tokio::test]
async fn gateway_errors_keep_the_gateway_http_contract() {
    let response = gateway_error_response(
        "gateway.test",
        ApplicationError::new(
            "admin_required",
            "gateway admin access required",
            false,
            "use an admin principal",
        ),
    );

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["schema_version"], "mcp.gateway.error.v1");
    assert_eq!(body["code"], "admin_required");
    assert_eq!(body["kind"], "authorization");
    assert_eq!(body["action"], "gateway.test");
}
