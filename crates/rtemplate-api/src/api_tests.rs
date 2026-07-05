use rtemplate_contracts::token_limit::MAX_RESPONSE_BYTES;
use serde_json::json;

use super::{cap_rest_response, rest_principal};

#[test]
fn cap_rest_response_leaves_small_json_unchanged() {
    let value = json!({"echo": "hello"});
    assert_eq!(cap_rest_response(value.clone()).unwrap(), value);
}

#[test]
fn cap_rest_response_returns_json_safe_truncation_envelope() {
    let value = json!({"payload": "x".repeat(MAX_RESPONSE_BYTES + 1)});
    let capped = cap_rest_response(value).unwrap();

    assert_eq!(capped["truncated"], true);
    assert_eq!(
        capped["error"],
        "response exceeded REST response size limit"
    );
    assert_eq!(capped["max_response_bytes"], MAX_RESPONSE_BYTES);
    assert!(capped["hint"]
        .as_str()
        .unwrap_or_default()
        .contains("limit"));
    assert!(
        serde_json::to_vec(&capped).unwrap().len() < MAX_RESPONSE_BYTES,
        "{capped}"
    );
}

#[test]
fn missing_rest_auth_context_uses_anonymous_principal() {
    let principal = rest_principal(None);

    assert_eq!(principal.subject, "anonymous");
    assert!(principal.scopes.is_empty());
}
