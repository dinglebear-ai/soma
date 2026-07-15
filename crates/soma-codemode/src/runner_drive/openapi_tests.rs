use serde_json::json;

use super::openapi::{dispatch_openapi_call_outside_local_lock, is_openapi_provider_call};

#[test]
fn openapi_provider_call_is_explicit() {
    assert!(is_openapi_provider_call("openapi::petstore.list"));
    assert!(!is_openapi_provider_call("state::get"));
}

#[tokio::test]
async fn openapi_dispatch_empty_registry_is_not_serialized_by_local_state() {
    let registry = soma_openapi::OpenApiRegistry::default();
    let client = soma_openapi::http::build_dispatch_client().unwrap();
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        dispatch_openapi_call_outside_local_lock(&registry, &client, "vendor.getUser", json!({})),
    )
    .await;
    assert!(
        result.is_ok(),
        "OpenAPI dispatch should not wait on state/git"
    );
    assert_eq!(result.unwrap().unwrap_err().kind(), "unknown_instance");
}
