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

#[test]
fn rejects_upstream_missing_oauth_config() {
    let upstream = UpstreamConfig {
        name: "unsecured".to_owned(),
        url: Some("https://example.com/mcp".to_owned()),
        oauth: None,
        ..UpstreamConfig::default()
    };

    let error = to_auth_upstream(&upstream).expect_err("missing oauth config is rejected");
    assert_eq!(error.kind(), "internal_error");
}

#[test]
fn rejects_upstream_missing_url() {
    let upstream = UpstreamConfig {
        name: "secured".to_owned(),
        url: None,
        oauth: Some(GatewayUpstreamOauthConfig {
            mode: GatewayUpstreamOauthMode::AuthorizationCodePkce,
            registration: GatewayUpstreamOauthRegistration::Dynamic,
            scopes: None,
            prefer_client_metadata_document: None,
        }),
        ..UpstreamConfig::default()
    };

    let error = to_auth_upstream(&upstream).expect_err("missing url is rejected");
    assert_eq!(error.kind(), "internal_error");
}

#[test]
fn converts_client_metadata_document_registration() {
    let upstream = UpstreamConfig {
        name: "secured".to_owned(),
        url: Some("https://example.com/mcp".to_owned()),
        oauth: Some(GatewayUpstreamOauthConfig {
            mode: GatewayUpstreamOauthMode::AuthorizationCodePkce,
            registration: GatewayUpstreamOauthRegistration::ClientMetadataDocument {
                url: "https://example.com/client-metadata.json".to_owned(),
            },
            scopes: None,
            prefer_client_metadata_document: None,
        }),
        ..UpstreamConfig::default()
    };

    let converted = to_auth_upstream(&upstream).expect("convert OAuth config");
    match converted.oauth.expect("oauth").registration {
        soma_auth::upstream::config::UpstreamOauthRegistration::ClientMetadataDocument { url } => {
            assert_eq!(url, "https://example.com/client-metadata.json");
        }
        other => panic!("expected ClientMetadataDocument, got {other:?}"),
    }
}

#[test]
fn converts_preregistered_registration() {
    let upstream = UpstreamConfig {
        name: "secured".to_owned(),
        url: Some("https://example.com/mcp".to_owned()),
        oauth: Some(GatewayUpstreamOauthConfig {
            mode: GatewayUpstreamOauthMode::AuthorizationCodePkce,
            registration: GatewayUpstreamOauthRegistration::Preregistered {
                client_id: "client-123".to_owned(),
                client_secret_env: Some("UPSTREAM_CLIENT_SECRET".to_owned()),
            },
            scopes: None,
            prefer_client_metadata_document: None,
        }),
        ..UpstreamConfig::default()
    };

    let converted = to_auth_upstream(&upstream).expect("convert OAuth config");
    match converted.oauth.expect("oauth").registration {
        soma_auth::upstream::config::UpstreamOauthRegistration::Preregistered {
            client_id,
            client_secret_env,
        } => {
            assert_eq!(client_id, "client-123");
            assert_eq!(client_secret_env, Some("UPSTREAM_CLIENT_SECRET".to_owned()));
        }
        other => panic!("expected Preregistered, got {other:?}"),
    }
}
