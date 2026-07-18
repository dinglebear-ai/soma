use axum::{body::to_bytes, http::StatusCode};
use soma_application::ApplicationError;

use super::{launcher_not_found, palette_error_status};

#[test]
fn maps_not_found_codes() {
    let error = ApplicationError::new("unknown_action", "nope", false, "retry");
    assert_eq!(palette_error_status(&error), StatusCode::NOT_FOUND);
}

#[test]
fn maps_forbidden_codes() {
    let error = ApplicationError::new("admin_required", "nope", false, "retry");
    assert_eq!(palette_error_status(&error), StatusCode::FORBIDDEN);
}

#[test]
fn maps_bad_request_codes() {
    let error = ApplicationError::new("confirmation_required", "nope", false, "confirm");
    assert_eq!(palette_error_status(&error), StatusCode::BAD_REQUEST);
}

#[test]
fn defaults_unknown_codes_to_internal_error() {
    let error = ApplicationError::new("something_else", "nope", false, "retry");
    assert_eq!(
        palette_error_status(&error),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn launcher_not_found_is_404() {
    let response = launcher_not_found("mystery:tool");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn launcher_not_found_body_matches_application_error_shape() {
    let response = launcher_not_found("mystery:tool");
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    // Same wire shape as `palette_error_response` (an `ApplicationError`
    // body), not a one-off `json!` literal with a different key set.
    assert_eq!(value["code"], "launcher_not_found");
    assert_eq!(
        value["message"],
        "no palette-exposed launcher entry `mystery:tool`"
    );
    assert_eq!(value["retryable"], false);
    assert_eq!(
        value["remediation"],
        "Refresh the catalog and use a known launcher id."
    );
}
