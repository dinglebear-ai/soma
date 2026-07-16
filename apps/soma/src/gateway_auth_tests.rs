use super::to_auth_upstream;
use mcp_client::config::{
    GatewayUpstreamOauthConfig, GatewayUpstreamOauthMode, GatewayUpstreamOauthRegistration,
    UpstreamConfig,
};

#[test]
fn converts_generic_gateway_oauth_config_to_soma_auth() {
    let upstream = UpstreamConfig {
        name: "secured".to_owned(),
        url: Some("https://example.com/mcp".to_owned()),
        oauth: Some(GatewayUpstreamOauthConfig {
            mode: GatewayUpstreamOauthMode::AuthorizationCodePkce,
            registration: GatewayUpstreamOauthRegistration::Dynamic,
            scopes: Some(vec!["tools.read".to_owned()]),
            prefer_client_metadata_document: Some(true),
        }),
        ..UpstreamConfig::default()
    };

    let converted = to_auth_upstream(&upstream).expect("convert OAuth config");
    assert_eq!(converted.name, "secured");
    assert_eq!(converted.url.as_deref(), Some("https://example.com/mcp"));
    assert_eq!(
        converted.oauth.expect("oauth").scopes,
        Some(vec!["tools.read".to_owned()])
    );
}
