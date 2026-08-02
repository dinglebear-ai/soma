use serde_json::json;
use soma_fleet::{HostEndpoint, SshEndpoint};

use super::*;

#[test]
fn local_connector_rejects_remote_targets_before_socket_access() {
    let remote = HostRecord::new(
        HostId::new("remote").unwrap(),
        HostEndpoint::Ssh(SshEndpoint::new("remote").unwrap()),
    );
    assert!(matches!(
        BollardReadClient::connect_local(&remote),
        Err(InfraError::UnsupportedTarget { .. })
    ));
}

#[test]
fn docker_list_responses_are_bounded() {
    assert!(ensure_list_bound("containers", MAX_LIST_ITEMS).is_ok());
    assert!(ensure_list_bound("containers", MAX_LIST_ITEMS + 1).is_err());
    assert!(bounded_json_value("row", json!({"value": "ok"})).is_ok());
    assert!(
        bounded_json_value(
            "row",
            json!({"value": "x".repeat(MAX_LIST_ITEM_JSON_BYTES)})
        )
        .is_err()
    );
}

#[test]
fn filters_and_identifiers_are_bounded() {
    assert!(validate_filter("").is_err());
    assert!(validate_filter("app=soma").is_ok());
    assert!(validate_identifier("container", "").is_err());
    assert!(validate_identifier("container", "api").is_ok());
}
