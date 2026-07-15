use super::*;

#[test]
fn params_must_be_objects() {
    assert_eq!(
        object_params(&serde_json::json!("bad")),
        Err(ParamsError::MustBeObject)
    );
}

#[test]
fn proxy_omission_defaults_true_and_explicit_false_is_valid() {
    let omitted = upstream_config_from_params(&serde_json::json!({"name": "one"})).unwrap();
    let explicit = upstream_config_from_params(
        &serde_json::json!({"name": "two", "proxy_resources": false, "proxy_prompts": false}),
    )
    .unwrap();

    assert!(omitted.proxy_resources);
    assert!(omitted.proxy_prompts);
    assert!(!explicit.proxy_resources);
    assert!(!explicit.proxy_prompts);
}
