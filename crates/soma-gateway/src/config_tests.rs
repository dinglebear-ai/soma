use super::*;

#[test]
fn validates_nested_config_sections() {
    let cfg = GatewayConfig {
        upstream: vec![UpstreamConfig {
            name: "demo".to_owned(),
            url: Some("https://example.com/mcp".to_owned()),
            bearer_token_env: Some("DEMO_TOKEN".to_owned()),
            ..UpstreamConfig::default()
        }],
        protected_mcp_routes: vec![ProtectedMcpRouteConfig {
            name: "public".to_owned(),
            public_host: "mcp.example.com".to_owned(),
            public_path: "/demo".to_owned(),
            upstream: Some("demo".to_owned()),
            ..ProtectedMcpRouteConfig::default()
        }],
        virtual_servers: vec![VirtualServerConfig {
            id: "demo".to_owned(),
            service: "demo".to_owned(),
            enabled: true,
            ..VirtualServerConfig::default()
        }],
    };
    cfg.validate().unwrap();
}

#[test]
fn redacted_view_excludes_raw_env_and_secret_values() {
    let cfg = GatewayConfig {
        upstream: vec![UpstreamConfig {
            name: "demo".to_owned(),
            url: Some("https://user:pass@example.com/mcp?token=raw".to_owned()),
            bearer_token_env: Some("DEMO_TOKEN".to_owned()),
            args: vec!["--api-key".to_owned(), "secret".to_owned()],
            env: [("DEMO_TOKEN".to_owned(), "secret".to_owned())].into(),
            ..UpstreamConfig::default()
        }],
        ..GatewayConfig::default()
    };

    let rendered = serde_json::to_string(&cfg.redacted_view()).unwrap();
    assert!(!rendered.contains("secret"));
    assert!(!rendered.contains("DEMO_TOKEN"));
    assert!(!rendered.contains("user:pass"));
    assert!(rendered.contains("[redacted]"));
}
