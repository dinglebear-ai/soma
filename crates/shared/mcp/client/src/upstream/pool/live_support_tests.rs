use super::{
    bearer_token_from_env, capability_is_absent, normalize_bearer_value, websocket_authorization,
};
use crate::config::UpstreamConfig;

#[test]
fn bearer_value_normalization_accepts_raw_or_prefixed_tokens() {
    assert_eq!(normalize_bearer_value("secret"), "secret");
    assert_eq!(normalize_bearer_value(" Bearer secret "), "secret");
}

#[test]
fn bearer_token_env_supports_plain_http_and_websocket_auth() {
    let var = "SOMA_MCP_CLIENT_TEST_BEARER";
    // FIXME: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var(var, "Bearer secret") };
    let config = UpstreamConfig {
        name: "bearer".to_owned(),
        bearer_token_env: Some(var.to_owned()),
        ..UpstreamConfig::default()
    };

    assert_eq!(bearer_token_from_env(&config).as_deref(), Some("secret"));
    assert_eq!(
        websocket_authorization(&config).as_deref(),
        Some("Bearer secret")
    );

    // FIXME: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var(var) };
}

#[test]
fn capability_absence_matches_json_rpc_method_not_found() {
    assert!(capability_is_absent(
        "JSON-RPC error -32601: Method not found"
    ));
    assert!(capability_is_absent("method not found"));
    assert!(!capability_is_absent("connection refused"));
}
