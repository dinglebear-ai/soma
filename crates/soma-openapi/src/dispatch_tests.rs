use std::collections::HashMap;

use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::config::OpenApiCredential;
use crate::dispatch::dispatch_openapi_call_no_ssrf as dispatch_openapi_call;
use crate::registry::{OpenApiRegistry, OperationHandle, SpecEntry};

fn registry_from_handle(label: &str, op: OperationHandle) -> OpenApiRegistry {
    let mut operations = HashMap::new();
    operations.insert(op.operation_id.clone(), op);
    let mut inner = HashMap::new();
    inner.insert(label.to_string(), SpecEntry { operations });
    OpenApiRegistry::from_map_for_test(inner)
}

fn get_user_handle(base: &str, credential: Option<OpenApiCredential>) -> OperationHandle {
    OperationHandle {
        operation_id: "getUser".into(),
        method: reqwest::Method::GET,
        path_template: "/users/{id}".into(),
        base_url: base.parse().unwrap(),
        credential,
    }
}

#[tokio::test]
async fn happy_path_calls_allowed_operation() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/users/7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "id": "7" })))
        .mount(&server)
        .await;

    let reg = registry_from_handle("vendor", get_user_handle(&server.uri(), None));
    let out = dispatch_openapi_call(
        &reg,
        &crate::http::client::build_loopback_test_client(),
        "vendor",
        "getUser",
        json!({ "id": "7" }),
    )
    .await
    .unwrap();
    assert_eq!(out["id"], "7");
}

#[tokio::test]
async fn credential_injected_server_side() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/users/7"))
        .and(header("authorization", "Bearer redaction-canary-bearer"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "id": "7" })))
        .mount(&server)
        .await;

    let handle = get_user_handle(
        &server.uri(),
        Some(OpenApiCredential::bearer_token("redaction-canary-bearer")),
    );
    let reg = registry_from_handle("vendor", handle);
    let out = dispatch_openapi_call(
        &reg,
        &crate::http::client::build_loopback_test_client(),
        "vendor",
        "getUser",
        json!({ "id": "7" }),
    )
    .await
    .unwrap();
    assert_eq!(out["id"], "7");
}

#[tokio::test]
async fn unknown_operation_returns_unknown_action() {
    let server = MockServer::start().await;
    let reg = registry_from_handle("vendor", get_user_handle(&server.uri(), None));
    let err = dispatch_openapi_call(
        &reg,
        &crate::http::client::build_loopback_test_client(),
        "vendor",
        "deleteUser",
        json!({ "id": "7" }),
    )
    .await
    .unwrap_err();
    assert_eq!(err.kind(), "unknown_action");
}

#[tokio::test]
async fn unknown_label_returns_unknown_instance() {
    let err = dispatch_openapi_call(
        &OpenApiRegistry::default(),
        &crate::http::client::build_loopback_test_client(),
        "nope",
        "getUser",
        json!({}),
    )
    .await
    .unwrap_err();
    assert_eq!(err.kind(), "unknown_instance");
}

#[tokio::test]
async fn dispatch_error_does_not_include_upstream_body_or_reqwest_display() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/users/7"))
        .respond_with(ResponseTemplate::new(500).set_body_string("CANARY-9f3b-SECRET"))
        .mount(&server)
        .await;

    let reg = registry_from_handle("vendor", get_user_handle(&server.uri(), None));
    let err = dispatch_openapi_call(
        &reg,
        &crate::http::client::build_loopback_test_client(),
        "vendor",
        "getUser",
        json!({ "id": "7" }),
    )
    .await
    .unwrap_err();
    for text in [format!("{err}"), format!("{err:?}")] {
        assert!(!text.contains("CANARY-9f3b-SECRET"), "{text}");
        assert!(!text.contains("reqwest"), "{text}");
    }
}

#[tokio::test]
async fn path_param_traversal_token_is_rejected() {
    let server = MockServer::start().await;
    let reg = registry_from_handle("vendor", get_user_handle(&server.uri(), None));
    let err = dispatch_openapi_call(
        &reg,
        &crate::http::client::build_loopback_test_client(),
        "vendor",
        "getUser",
        json!({ "id": ".." }),
    )
    .await
    .unwrap_err();
    assert_eq!(err.kind(), "invalid_param");
}

#[tokio::test]
async fn base_path_prefix_is_preserved_in_request() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tenant-A/v1/users/7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "id": "7" })))
        .mount(&server)
        .await;

    let handle = OperationHandle {
        operation_id: "getUser".into(),
        method: reqwest::Method::GET,
        path_template: "/users/{id}".into(),
        base_url: format!("{}/tenant-A/v1", server.uri()).parse().unwrap(),
        credential: None,
    };
    let reg = registry_from_handle("vendor", handle);
    let out = dispatch_openapi_call(
        &reg,
        &crate::http::client::build_loopback_test_client(),
        "vendor",
        "getUser",
        json!({ "id": "7" }),
    )
    .await
    .unwrap();
    assert_eq!(out["id"], "7");
}
