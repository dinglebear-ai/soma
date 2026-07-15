use super::*;

#[test]
fn proxy_resources_and_prompts_default_true() {
    let cfg: UpstreamConfig = toml::from_str(r#"name = "axon""#).unwrap();
    assert!(cfg.proxy_resources);
    assert!(cfg.proxy_prompts);
}

#[test]
fn bearer_token_env_rejects_token_values() {
    for value in [
        "Bearer abc123",
        "sk-proj-secret",
        "ghp_secret",
        "github_pat_secret",
        "aaaaaaaa.bbbbbbbb.cccccccc",
        "lowercase",
    ] {
        assert!(validate_bearer_token_env(value).is_err(), "{value}");
    }
    validate_bearer_token_env("AXON_TOKEN").unwrap();
}

#[test]
fn redacted_view_masks_targets_args_and_env_values() {
    let cfg = UpstreamConfig {
        name: "axon".to_owned(),
        url: Some("https://user:pass@example.com/mcp?api_key=secret&page=1".to_owned()),
        bearer_token_env: Some("AXON_TOKEN".to_owned()),
        args: vec!["--api-key".to_owned(), "secret".to_owned()],
        env: [("AXON_TOKEN".to_owned(), "secret".to_owned())].into(),
        ..UpstreamConfig::default()
    };
    let view = serde_json::to_string(&cfg.redacted_view()).unwrap();
    assert!(!view.contains("secret"));
    assert!(!view.contains("user:pass"));
    assert!(!view.contains("AXON_TOKEN"));
    assert!(view.contains("page=1"));
}

#[test]
fn oauth_requires_url_and_redacted_view_only_shows_enabled() {
    let mut cfg = UpstreamConfig {
        name: "oauth".to_owned(),
        oauth: Some(GatewayUpstreamOauthConfig {
            mode: GatewayUpstreamOauthMode::AuthorizationCodePkce,
            registration: GatewayUpstreamOauthRegistration::Preregistered {
                client_id: "client-id".to_owned(),
                client_secret_env: Some("UPSTREAM_CLIENT_SECRET".to_owned()),
            },
            scopes: Some(vec!["read".to_owned()]),
            prefer_client_metadata_document: Some(true),
        }),
        ..UpstreamConfig::default()
    };

    assert!(cfg.validate().is_err());
    cfg.url = Some("https://upstream.example/mcp".to_owned());
    cfg.validate().unwrap();

    let view = serde_json::to_string(&cfg.redacted_view()).unwrap();
    assert!(view.contains("oauth_enabled"));
    assert!(!view.contains("client-id"));
    assert!(!view.contains("UPSTREAM_CLIENT_SECRET"));
}
