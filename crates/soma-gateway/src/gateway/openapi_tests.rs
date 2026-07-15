use super::*;

#[test]
fn openapi_adapter_delegates_spec_url_ssrf_policy() {
    assert!(validate_spec_url("https://api.example.com/openapi.json").is_ok());
    assert_eq!(
        validate_spec_url("http://127.0.0.1/openapi.json"),
        Err(OpenApiAdapterError::SpecUrlDenied)
    );
}

#[test]
fn openapi_params_must_be_objects() {
    assert_eq!(
        validate_operation_params(&serde_json::json!("bad")),
        Err(OpenApiAdapterError::ParamsMustBeObject)
    );
}
