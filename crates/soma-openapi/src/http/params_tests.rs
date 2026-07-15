use serde_json::json;

use crate::registry::OperationHandle;

fn op(base: &str, method: reqwest::Method, template: &str) -> OperationHandle {
    OperationHandle {
        operation_id: "getUser".into(),
        method,
        path_template: template.into(),
        base_url: base.parse().unwrap(),
        credential: None,
    }
}

#[test]
fn path_params_are_percent_encoded_and_cannot_escape_base_path() {
    let op = op(
        "https://api.example.com/tenant/v1",
        reqwest::Method::GET,
        "/users/{id}",
    );
    let (_, url) = super::params::build_url_with_params(&op, &json!({ "id": "a/b.%?#" })).unwrap();
    assert_eq!(
        url.as_str(),
        "https://api.example.com/tenant/v1/users/a%2Fb%2E%25%3F%23"
    );
}

#[test]
fn traversal_path_param_is_rejected() {
    let op = op(
        "https://api.example.com/tenant/v1",
        reqwest::Method::GET,
        "/users/{id}",
    );
    let err = super::params::build_url_with_params(&op, &json!({ "id": ".." })).unwrap_err();
    assert_eq!(err.kind(), "invalid_param");
}

#[test]
fn joined_path_cannot_escape_to_base_path_sibling() {
    let op = op(
        "https://api.example.com/tenant/v1",
        reqwest::Method::GET,
        "../v1-evil/users/{id}",
    );
    let err = super::params::build_url_with_params(&op, &json!({ "id": "7" })).unwrap_err();
    assert_eq!(err.kind(), "forbidden");
}

#[test]
fn joined_path_allows_exact_base_component_descendants() {
    let op = op(
        "https://api.example.com/tenant/v1",
        reqwest::Method::GET,
        "users/{id}",
    );
    let (_, url) = super::params::build_url_with_params(&op, &json!({ "id": "7" })).unwrap();
    assert_eq!(url.as_str(), "https://api.example.com/tenant/v1/users/7");
}

#[test]
fn missing_and_non_scalar_path_params_are_rejected() {
    let op = op(
        "https://api.example.com",
        reqwest::Method::GET,
        "/users/{id}",
    );
    let missing = super::params::build_url_with_params(&op, &json!({})).unwrap_err();
    assert_eq!(missing.kind(), "invalid_param");
    let object = super::params::build_url_with_params(&op, &json!({ "id": { "nested": true } }))
        .unwrap_err();
    assert_eq!(object.kind(), "invalid_param");
}

#[test]
fn safe_methods_put_remaining_params_in_query() {
    let op = op(
        "https://api.example.com",
        reqwest::Method::GET,
        "/users/{id}",
    );
    let (used, url) =
        super::params::build_url_with_params(&op, &json!({ "id": 7, "verbose": true })).unwrap();
    let url = super::params::apply_query(url, &op, &json!({ "id": 7, "verbose": true }), &used);
    assert_eq!(url.as_str(), "https://api.example.com/users/7?verbose=true");
}
