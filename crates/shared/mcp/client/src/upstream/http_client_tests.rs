use super::*;

#[test]
fn caps_reject_oversized_http_json_before_parse() {
    let caps = ResponseCaps {
        http_json_bytes: 2,
        ..ResponseCaps::default()
    };

    let error = parse_capped_json(br#"{"ok":true}"#, &caps).unwrap_err();

    assert!(matches!(
        error,
        UpstreamError::ResponseTooLarge {
            scope: CapScope::HttpJson,
            observed_bytes: 11,
            limit: 2
        }
    ));
}

#[test]
fn caps_reject_oversized_sse_events() {
    let caps = ResponseCaps {
        http_sse_event_bytes: 3,
        ..ResponseCaps::default()
    };

    let error = capped_sse_event("data: too much", &caps).unwrap_err();

    assert!(matches!(
        error,
        UpstreamError::ResponseTooLarge {
            scope: CapScope::HttpSseEvent,
            ..
        }
    ));
}

#[test]
fn websocket_urls_select_websocket_transport() {
    let decision = decide_http_transport("wss://example.test/mcp");

    assert_eq!(
        transport_kind_for_decision(&decision),
        TransportKind::WebSocket
    );
    assert!(matches!(decision, HttpTransportDecision::WebSocket));
}
