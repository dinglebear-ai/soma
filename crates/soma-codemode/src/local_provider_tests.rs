use serde_json::json;

use super::local_provider::{
    is_reserved_provider_namespace, parse_local_provider_call, LocalProviderName,
};

#[test]
fn local_provider_dispatch_is_explicit() {
    assert!(is_reserved_provider_namespace("state"));
    assert!(is_reserved_provider_namespace("git"));
    #[cfg(not(feature = "openapi"))]
    assert!(!is_reserved_provider_namespace("openapi"));
    #[cfg(feature = "openapi")]
    assert!(is_reserved_provider_namespace("openapi"));
    let call = parse_local_provider_call("state::status", json!({}))
        .unwrap()
        .unwrap();
    assert_eq!(call.provider, LocalProviderName::State);
    assert_eq!(call.method, "status");
}

#[test]
fn openapi_provider_is_only_parsed_with_openapi_feature() {
    let parsed = parse_local_provider_call("openapi::petstore.listPets", json!({})).unwrap();
    #[cfg(not(feature = "openapi"))]
    assert!(parsed.is_none());
    #[cfg(feature = "openapi")]
    {
        let call = parsed.unwrap();
        assert_eq!(call.provider, LocalProviderName::Openapi);
        assert_eq!(call.method, "petstore.listPets");
    }
}
