use super::*;
use soma_config::{AuthConfig, SomaConfig, TraceHeaderMode};
use soma_gateway::config::{GatewayConfig, GatewayPaths, UpstreamConfig};
use soma_gateway::gateway::config_store::FsGatewayConfigStore;

fn config(host: &str) -> Config {
    Config {
        mcp: McpConfig {
            host: host.into(),
            ..McpConfig::default()
        },
        soma: SomaConfig::default(),
        python: Default::default(),
    }
}

#[test]
fn loopback_bind_is_loopback_dev_without_credentials() {
    let config = config("127.0.0.1");
    assert_eq!(
        resolve_auth_policy_kind(&config, false).unwrap(),
        AuthPolicyKind::LoopbackDev
    );
}

#[test]
fn non_loopback_no_auth_without_gateway_is_rejected() {
    let mut config = config("0.0.0.0");
    config.mcp.no_auth = true;
    let error = resolve_auth_policy_kind(&config, false).unwrap_err();
    assert!(error.to_string().contains("SOMA_MCP_NO_AUTH=true"));
}

#[test]
fn non_loopback_no_auth_with_gateway_is_trusted_gateway_unscoped() {
    let mut config = config("0.0.0.0");
    config.mcp.no_auth = true;
    assert_eq!(
        resolve_auth_policy_kind(&config, true).unwrap(),
        AuthPolicyKind::TrustedGatewayUnscoped
    );
}

#[test]
fn non_loopback_gateway_without_credentials_is_trusted_gateway_unscoped() {
    let config = config("0.0.0.0");
    assert_eq!(
        resolve_auth_policy_kind(&config, true).unwrap(),
        AuthPolicyKind::TrustedGatewayUnscoped
    );
}

#[test]
fn non_loopback_bearer_token_mounts_bearer_policy() {
    let mut config = config("0.0.0.0");
    config.mcp.api_token = Some("secret".into());
    assert_eq!(
        resolve_auth_policy_kind(&config, false).unwrap(),
        AuthPolicyKind::MountedBearer
    );
}

#[test]
fn off_trace_headers_do_not_require_a_trust_boundary() {
    let mut config = config("0.0.0.0");
    config.mcp.api_token = Some("secret".into());
    assert_eq!(
        resolve_auth_policy_kind(&config, false).unwrap(),
        AuthPolicyKind::MountedBearer
    );
}

#[test]
fn trusted_trace_headers_reject_mounted_bearer() {
    let mut config = config("0.0.0.0");
    config.mcp.api_token = Some("secret".into());
    config.mcp.trace_headers = TraceHeaderMode::Trusted;
    let error = resolve_auth_policy_kind(&config, false).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("not a trace-header trust boundary"),
        "error was: {error}"
    );
}

#[cfg(feature = "auth")]
#[test]
fn non_loopback_oauth_mounts_oauth_policy() {
    let mut config = config("0.0.0.0");
    config.mcp.auth = AuthConfig {
        mode: AuthMode::OAuth,
        ..AuthConfig::default()
    };
    assert_eq!(
        resolve_auth_policy_kind(&config, false).unwrap(),
        AuthPolicyKind::MountedOAuth
    );
}

#[cfg(feature = "auth")]
#[test]
fn trusted_with_baggage_rejects_mounted_oauth() {
    let mut config = config("0.0.0.0");
    config.mcp.auth = AuthConfig {
        mode: AuthMode::OAuth,
        ..AuthConfig::default()
    };
    config.mcp.trace_headers = TraceHeaderMode::TrustedWithBaggage;
    let error = resolve_auth_policy_kind(&config, false).unwrap_err();
    let message = error.to_string();
    assert!(message.contains("not a trace-header trust boundary"));
    assert!(message.contains("SOMA_MCP_TRACE_HEADERS=trusted-with-baggage"));
    assert!(!message.contains("TrustedWithBaggage"));
}

#[test]
fn trusted_trace_headers_allowed_on_loopback() {
    let mut config = config("127.0.0.1");
    config.mcp.trace_headers = TraceHeaderMode::TrustedWithBaggage;
    assert_eq!(
        resolve_auth_policy_kind(&config, false).unwrap(),
        AuthPolicyKind::LoopbackDev
    );
}

#[test]
fn trusted_trace_headers_allowed_on_trusted_gateway_unscoped() {
    let mut config = config("0.0.0.0");
    config.mcp.no_auth = true;
    config.mcp.trace_headers = TraceHeaderMode::Trusted;
    assert_eq!(
        resolve_auth_policy_kind(&config, true).unwrap(),
        AuthPolicyKind::TrustedGatewayUnscoped
    );
}

#[test]
fn trusted_trace_headers_allowed_on_trusted_gateway_with_bearer_auth() {
    let mut config = config("0.0.0.0");
    config.mcp.api_token = Some("secret".into());
    config.mcp.trace_headers = TraceHeaderMode::Trusted;
    assert_eq!(
        resolve_auth_policy_kind(&config, true).unwrap(),
        AuthPolicyKind::MountedBearer
    );
}

#[cfg(not(feature = "auth"))]
#[test]
fn non_loopback_oauth_requires_auth_feature() {
    let mut config = config("0.0.0.0");
    config.mcp.auth = AuthConfig {
        mode: AuthMode::OAuth,
        ..AuthConfig::default()
    };
    let error = resolve_auth_policy_kind(&config, false).unwrap_err();
    assert!(error.to_string().contains("requires compiling"));
}

#[test]
fn non_loopback_without_auth_or_gateway_is_rejected() {
    let config = config("0.0.0.0");
    let error = resolve_auth_policy_kind(&config, false).unwrap_err();
    assert!(error.to_string().contains("without authentication"));
}

#[test]
fn invalid_public_url_is_rejected() {
    let mut config = config("0.0.0.0");
    config.mcp.auth.public_url = Some("not a url".into());
    let error = resolve_auth_policy_kind(&config, true).unwrap_err();
    assert!(error.to_string().contains("SOMA_MCP_PUBLIC_URL is invalid"));
}

#[test]
fn wildcard_public_url_is_rejected() {
    let mut config = config("0.0.0.0");
    config.mcp.auth.public_url = Some("https://*.example.com".into());
    let error = resolve_auth_policy_kind(&config, true).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("SOMA_MCP_PUBLIC_URL must not contain wildcard hosts")
    );
}

#[tokio::test]
async fn gateway_product_state_loads_filesystem_config() {
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path().join(".mcp-gateway");
    let paths = GatewayPaths::new(home).expect("paths");
    let store = FsGatewayConfigStore::from_paths(paths);
    store
        .save(&GatewayConfig {
            upstream: vec![UpstreamConfig {
                name: "persisted".to_owned(),
                url: Some("https://example.com/mcp".to_owned()),
                ..UpstreamConfig::default()
            }],
            ..GatewayConfig::default()
        })
        .expect("save gateway config");

    let state = gateway_product_state_from_store(store).expect("gateway state");

    assert_eq!(
        state.discover().await.expect("discover")[0].name,
        "persisted"
    );
}
