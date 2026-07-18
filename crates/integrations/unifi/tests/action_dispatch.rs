//! End-to-end tests for the dynamic action-dispatch pipeline: capability
//! lookup -> hybrid resolution -> path substitution -> URL construction ->
//! HTTP call. `tests/client.rs` and the inline unit tests elsewhere in this
//! crate cover each of those steps in isolation; this file proves they
//! compose correctly through `ActionDispatcher::execute`, the crate's main
//! entry point for anything not covered by `UnifiClient`'s named methods.
//! Copy this pattern (not just `tests/client.rs`'s) when testing another
//! integration crate's dynamic dispatcher.

use serde_json::json;
use unifi::{ActionDispatcher, ActionRequest, UnifiClient, UnifiConfig, UnifiError};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn config(url: String) -> UnifiConfig {
    UnifiConfig {
        url,
        api_key: "test-key".to_string(),
        site: "default".to_string(),
        skip_tls_verify: true,
        legacy: false,
        ..UnifiConfig::default()
    }
}

#[tokio::test]
async fn dispatches_a_dynamic_official_action_with_a_path_parameter() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/proxy/network/integration/v1/sites/site-1/clients"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": [] })))
        .mount(&server)
        .await;

    let client = UnifiClient::new(&config(server.uri())).unwrap();
    let dispatcher = ActionDispatcher::new(client);

    let result = dispatcher
        .execute(ActionRequest {
            action: "official_list_clients".to_string(),
            params: json!({ "siteId": "site-1" }),
        })
        .await
        .unwrap();

    assert_eq!(result, json!({ "data": [] }));
}

#[tokio::test]
async fn dispatches_a_dynamic_internal_action_with_a_path_parameter() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/proxy/network/api/s/default/stat/device/aa-bb-cc"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "data": [{ "mac": "aa-bb-cc" }] })),
        )
        .mount(&server)
        .await;

    let client = UnifiClient::new(&config(server.uri())).unwrap();
    let dispatcher = ActionDispatcher::new(client);

    let result = dispatcher
        .execute(ActionRequest {
            action: "unifi_get_lldp_neighbors".to_string(),
            params: json!({ "device_mac": "aa-bb-cc" }),
        })
        .await
        .unwrap();

    assert_eq!(result, json!({ "data": [{ "mac": "aa-bb-cc" }] }));
}

#[tokio::test]
async fn hybrid_action_resolves_to_official_when_a_site_id_is_supplied() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/proxy/network/integration/v1/sites/site-1/clients"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "source": "official" })))
        .mount(&server)
        .await;

    let client = UnifiClient::new(&config(server.uri())).unwrap();
    let dispatcher = ActionDispatcher::new(client);

    let result = dispatcher
        .execute(ActionRequest {
            action: "list_clients".to_string(),
            params: json!({ "siteId": "site-1" }),
        })
        .await
        .unwrap();

    assert_eq!(result, json!({ "source": "official" }));
}

#[tokio::test]
async fn hybrid_action_resolves_to_internal_by_default() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/proxy/network/api/s/default/stat/sta"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "source": "internal" })))
        .mount(&server)
        .await;

    let client = UnifiClient::new(&config(server.uri())).unwrap();
    let dispatcher = ActionDispatcher::new(client);

    let result = dispatcher
        .execute(ActionRequest {
            action: "list_clients".to_string(),
            params: json!({}),
        })
        .await
        .unwrap();

    assert_eq!(result, json!({ "source": "internal" }));
}

#[tokio::test]
async fn an_unknown_action_is_rejected_before_any_request_is_sent() {
    // No mocks registered — wiremock fails loudly (panics) if a request
    // goes out unexpectedly, so this also proves the dispatcher fails fast
    // at capability lookup instead of building a doomed request.
    let server = MockServer::start().await;
    let client = UnifiClient::new(&config(server.uri())).unwrap();
    let dispatcher = ActionDispatcher::new(client);

    let err = dispatcher
        .execute(ActionRequest {
            action: "totally_bogus_action".to_string(),
            params: json!({}),
        })
        .await
        .unwrap_err();

    assert!(matches!(err, UnifiError::UnknownAction(action) if action == "totally_bogus_action"));
}
