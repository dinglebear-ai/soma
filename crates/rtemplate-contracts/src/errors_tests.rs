use anyhow::anyhow;
use serde_json::json;

use super::{classify_execution_error, ServiceErrorKind, ToolError};
use crate::actions::ActionValidationError;

#[test]
fn service_error_kind_maps_status_and_retryability() {
    assert_eq!(ServiceErrorKind::Validation.http_status_code(), 400);
    assert_eq!(ServiceErrorKind::AuthRejected.http_status_code(), 403);
    assert_eq!(ServiceErrorKind::RateLimited.http_status_code(), 429);
    assert_eq!(ServiceErrorKind::Timeout.http_status_code(), 503);
    assert_eq!(
        ServiceErrorKind::UpstreamUnavailable.http_status_code(),
        503
    );
    assert_eq!(ServiceErrorKind::Execution.http_status_code(), 500);
    assert_eq!(ServiceErrorKind::Internal.http_status_code(), 500);

    assert!(ServiceErrorKind::Validation.retryable());
    assert!(ServiceErrorKind::Timeout.retryable());
    assert!(ServiceErrorKind::RateLimited.retryable());
    assert!(ServiceErrorKind::UpstreamUnavailable.retryable());
    assert!(!ServiceErrorKind::AuthRejected.retryable());
    assert!(!ServiceErrorKind::Execution.retryable());
    assert!(!ServiceErrorKind::Internal.retryable());
}

#[test]
fn validation_payloads_include_optional_fields_when_present() {
    let error = ToolError::validation("bad_field", "Bad field", "Use a better value.")
        .with_field("name")
        .with_bad_value("")
        .with_expected_pattern("^.+$")
        .with_available_actions(vec!["help", "status"]);

    assert_eq!(
        error.to_rest_payload(),
        json!({
            "error": "Bad field",
            "kind": "validation",
            "schema_version": 1,
            "code": "bad_field",
            "message": "Bad field",
            "retryable": true,
            "remediation": "Use a better value.",
            "field": "name",
            "bad_value": "",
            "expected_pattern": "^.+$",
            "available_actions": ["help", "status"],
        })
    );
}

#[test]
fn action_validation_error_preserves_action_context() {
    let error = ToolError::from_action_validation(&ActionValidationError::UnknownAction {
        action: "bogus".to_owned(),
    });

    assert_eq!(error.kind, ServiceErrorKind::Validation);
    assert_eq!(error.code, "unknown_action");
    assert!(error.available_actions.contains(&"help"));
    assert_eq!(error.bad_value.as_deref(), Some("bogus"));
}

#[test]
fn mcp_payload_includes_tool_action_and_service_kind() {
    let payload = ToolError::validation("missing_action", "Missing action", "Pass action.")
        .to_mcp_payload("example", Some("status"));

    assert_eq!(payload["kind"], "mcp_tool_error");
    assert_eq!(payload["tool"], "example");
    assert_eq!(payload["action"], "status");
    assert_eq!(payload["service_error_kind"], "validation");
}

#[test]
fn execution_errors_are_classified_from_text() {
    assert_eq!(
        classify_execution_error(&anyhow!("request timed out")),
        ServiceErrorKind::Timeout
    );
    assert_eq!(
        classify_execution_error(&anyhow!("HTTP 429 rate limit")),
        ServiceErrorKind::RateLimited
    );
    assert_eq!(
        classify_execution_error(&anyhow!("401 unauthorized")),
        ServiceErrorKind::AuthRejected
    );
    assert_eq!(
        classify_execution_error(&anyhow!("connection refused")),
        ServiceErrorKind::UpstreamUnavailable
    );
    assert_eq!(
        classify_execution_error(&anyhow!("unexpected response")),
        ServiceErrorKind::Execution
    );
}
