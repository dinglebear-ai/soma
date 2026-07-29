use rmcp::model::ServerResult;
use rmcp::service::ClientInitializeError;

use super::{LifecycleAttempt, compatibility_retry};

#[test]
fn retries_when_an_unexpected_result_carries_discovery_server_info() {
    let result = serde_json::from_value::<ServerResult>(serde_json::json!({
        "resultType": "complete",
        "supportedVersions": ["2026-07-28", "2025-11-25"],
        "capabilities": {"tools": {}},
        "ttlMs": 0,
        "cacheScope": "private",
        "_meta": {
            "io.modelcontextprotocol/serverInfo": {
                "name": "modern-server",
                "version": "1.0.0"
            }
        }
    }))
    .expect("unexpected result should deserialize through the SDK union");
    let error = ClientInitializeError::ExpectedInitResult(Some(result));

    assert_eq!(
        compatibility_retry(&error),
        Some(LifecycleAttempt::LegacyInitialize)
    );
}

#[test]
fn does_not_retry_an_unexpected_result_without_discovery_server_info() {
    let result = serde_json::from_value::<ServerResult>(serde_json::json!({
        "resultType": "complete",
        "_meta": {"traceId": "not-discovery"}
    }))
    .expect("tool-shaped result should deserialize");
    let error = ClientInitializeError::ExpectedInitResult(Some(result));

    assert_eq!(compatibility_retry(&error), None);
}

#[test]
fn retries_only_for_explicit_lifecycle_incompatibility() {
    let cases = [
        ClientInitializeError::ConnectionClosed("discover response".to_owned()),
        ClientInitializeError::NoCompatibleProtocolVersion {
            client_supported: vec![],
            server_supported: vec![],
        },
    ];

    for error in cases {
        assert_eq!(
            compatibility_retry(&error),
            Some(LifecycleAttempt::LegacyInitialize),
            "{error}"
        );
    }
}

#[test]
fn does_not_downgrade_operational_failures() {
    for error in [
        ClientInitializeError::ConnectionClosed("connection timed out".to_owned()),
        ClientInitializeError::Cancelled,
    ] {
        assert_eq!(compatibility_retry(&error), None, "{error}");
    }
}
