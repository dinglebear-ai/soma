use crate::error::{OpenApiError, SsrfError};

#[test]
fn openapi_error_kinds_are_stable() {
    let cases = [
        (
            OpenApiError::SpecParse { label: "x".into() },
            "config_error",
        ),
        (
            OpenApiError::RequestBlockedPrivateAddr { label: "x".into() },
            "forbidden",
        ),
        (
            OpenApiError::InvalidPathParam {
                label: "op".into(),
                param: "id".into(),
            },
            "invalid_param",
        ),
        (
            OpenApiError::UnknownInstance {
                label: "x".into(),
                valid: vec![],
            },
            "unknown_instance",
        ),
        (
            OpenApiError::UnknownOperation {
                label: "x".into(),
                operation_id: "op".into(),
            },
            "unknown_action",
        ),
        (OpenApiError::ClientBuildFailed, "internal_error"),
        (
            OpenApiError::UpstreamTimeout { label: "x".into() },
            "timeout",
        ),
    ];

    for (error, kind) in cases {
        assert_eq!(error.kind(), kind, "{error:?}");
    }
}

#[test]
fn error_display_is_scrubbed() {
    let error = OpenApiError::UpstreamRequest {
        label: "vendor".into(),
    };
    let display = error.to_string();
    assert!(!display.contains("reqwest"));
    assert!(!display.contains("token"));
    assert!(!display.contains("body"));
}

#[test]
fn ssrf_error_kinds_are_stable() {
    assert_eq!(SsrfError::InvalidUrl("bad".into()).kind(), "invalid_param");
    assert_eq!(SsrfError::Blocked("blocked".into()).kind(), "ssrf_blocked");
}
