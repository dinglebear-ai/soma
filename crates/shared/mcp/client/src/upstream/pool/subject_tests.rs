use serde_json::json;

use crate::config::{
    GatewayUpstreamOauthConfig, GatewayUpstreamOauthMode, GatewayUpstreamOauthRegistration,
    UpstreamConfig,
};
use crate::upstream::pool::{InProcessUpstream, PoolOptions, ToolCall, UpstreamPool};
use crate::upstream::{ToolDescriptor, TransportKind, UpstreamError, UpstreamSnapshot};

fn oauth_upstream(name: &str) -> UpstreamConfig {
    UpstreamConfig {
        name: name.to_owned(),
        url: Some("http://127.0.0.1:9/mcp".to_owned()),
        oauth: Some(GatewayUpstreamOauthConfig {
            mode: GatewayUpstreamOauthMode::AuthorizationCodePkce,
            registration: GatewayUpstreamOauthRegistration::Preregistered {
                client_id: "client".to_owned(),
                client_secret_env: None,
            },
            scopes: None,
            prefer_client_metadata_document: None,
        }),
        ..UpstreamConfig::default()
    }
}

#[tokio::test]
async fn oauth_subject_call_requires_runtime_cache() {
    let pool = UpstreamPool::new(PoolOptions::default());
    pool.register_config(oauth_upstream("oauth")).unwrap();

    let error = pool
        .call_tool_for_subject(
            ToolCall {
                upstream: "oauth".to_owned(),
                tool: "echo".to_owned(),
                params: json!({}),
            },
            Some("alice"),
        )
        .await
        .expect_err("oauth upstream without runtime should fail clearly");

    assert!(matches!(error, UpstreamError::LiveConnect { .. }));
    assert!(error.to_string().contains("OAuth runtime"));
}

#[tokio::test]
async fn non_oauth_subject_call_uses_shared_pool_path() {
    let pool = UpstreamPool::new(PoolOptions::default());
    let config = UpstreamConfig {
        name: "plain".to_owned(),
        command: Some("python3".to_owned()),
        ..UpstreamConfig::default()
    };
    let mut snapshot = UpstreamSnapshot::empty("plain", TransportKind::InProcess);
    snapshot.tools.push(ToolDescriptor::new("echo"));
    let upstream = InProcessUpstream::new("plain")
        .with_snapshot(snapshot)
        .with_tool(ToolDescriptor::new("echo"), json!({"ok": true}));
    pool.register_in_process(config, upstream).unwrap();

    let result = pool
        .call_tool_for_subject(
            ToolCall {
                upstream: "plain".to_owned(),
                tool: "echo".to_owned(),
                params: json!({}),
            },
            Some("alice"),
        )
        .await
        .expect("non-oauth subject should fall back to shared pool");

    assert_eq!(result, json!({"ok": true}));
}
