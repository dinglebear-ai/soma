use http::HeaderName;

use super::{account_event_bytes, extract_scope, validate_custom_header, CappedStreamError};

#[test]
fn reserved_headers_reject_client_overrides() {
    assert!(validate_custom_header(&HeaderName::from_static("accept")).is_err());
    assert!(validate_custom_header(&HeaderName::from_static("mcp-session-id")).is_err());
    assert!(validate_custom_header(&HeaderName::from_static("last-event-id")).is_err());
    assert!(validate_custom_header(&HeaderName::from_static("x-safe")).is_ok());
}

#[test]
fn protocol_version_header_is_allowed_for_worker_injection() {
    assert!(validate_custom_header(&HeaderName::from_static("mcp-protocol-version")).is_ok());
}

#[test]
fn scope_extraction_accepts_quoted_and_bare_forms() {
    assert_eq!(
        extract_scope(r#"Bearer error="insufficient_scope", scope="files:read files:write""#),
        Some("files:read files:write".to_owned())
    );
    assert_eq!(
        extract_scope(r#"Bearer scope=files:read,error="insufficient_scope""#),
        Some("files:read".to_owned())
    );
}

#[test]
fn sse_event_counter_resets_on_boundaries() {
    let mut state = (0usize, false);
    account_event_bytes(b"data: one\n\n", &mut state, 10).expect("first event under cap");
    assert_eq!(state.0, 0);
    account_event_bytes(b"data: two", &mut state, 10).expect("second event under cap");
    assert_eq!(state.0, 9);
}

#[test]
fn sse_event_counter_detects_cross_chunk_boundaries() {
    let mut state = (0usize, false);
    account_event_bytes(b"data: a\n", &mut state, 8).expect("partial event under cap");
    account_event_bytes(b"\ndata: b", &mut state, 8).expect("boundary resets next event");
    assert_eq!(state.0, 7);
}

#[test]
fn sse_event_counter_rejects_single_oversized_event() {
    let mut state = (0usize, false);
    let error = account_event_bytes(b"data: too-big", &mut state, 6)
        .expect_err("single event should exceed cap");
    assert!(matches!(error, CappedStreamError::TooLarge { .. }));
    assert!(error.to_string().contains("response_too_large"));
}
