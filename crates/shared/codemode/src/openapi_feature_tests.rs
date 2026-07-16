#[cfg(feature = "openapi")]
use serde_json::json;

#[cfg(not(feature = "openapi"))]
use super::openapi_feature::openapi_provider_unavailable_error;
#[cfg(feature = "openapi")]
use super::openapi_feature::{
    dispatch_openapi_provider, openapi_error_to_tool_error, split_openapi_method,
};

#[cfg(not(feature = "openapi"))]
#[test]
fn no_feature_openapi_provider_has_normal_unknown_error() {
    let error = openapi_provider_unavailable_error();
    assert_eq!(error.kind(), "unknown_action");
    assert!(error.user_message().contains("openapi"));
}

#[cfg(feature = "openapi")]
#[test]
fn dotted_operation_ids_split_only_on_first_dot() {
    let (label, operation_id) = split_openapi_method("vendor.pets.list.by.id").unwrap();
    assert_eq!(label, "vendor");
    assert_eq!(operation_id, "pets.list.by.id");
    assert!(split_openapi_method("vendor").is_none());
    assert!(split_openapi_method(".operation").is_none());
    assert!(split_openapi_method("vendor.").is_none());
}

#[cfg(feature = "openapi")]
#[test]
fn openapi_error_mapping_is_feature_gated() {
    let error = openapi_error_to_tool_error(soma_openapi::OpenApiError::UnknownInstance {
        label: "petstore".to_string(),
        valid: vec!["orders".to_string()],
    });
    assert_eq!(error.kind(), "unknown_instance");
    assert_eq!(
        serde_json::to_value(error).unwrap()["valid"],
        json!(["orders"])
    );
}

#[cfg(feature = "openapi")]
#[tokio::test]
async fn dispatch_calls_soma_openapi_and_maps_empty_registry_error() {
    let registry = soma_openapi::OpenApiRegistry::default();
    let client = soma_openapi::http::build_dispatch_client().unwrap();
    let error = dispatch_openapi_provider(&registry, &client, "petstore.list.pets", json!({}))
        .await
        .unwrap_err();
    assert_eq!(error.kind(), "unknown_instance");
}
