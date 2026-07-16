use crate::config::{GatewayConfig, UpstreamConfig};
use crate::gateway::manager::GatewayManager;

use super::*;

#[tokio::test]
async fn list_view_contains_counts_not_backend_state() {
    let manager = GatewayManager::new(GatewayConfig {
        upstream: vec![UpstreamConfig {
            name: "mock".to_owned(),
            url: Some("https://example.com/mcp".to_owned()),
            ..UpstreamConfig::default()
        }],
        ..GatewayConfig::default()
    })
    .unwrap();

    let view = gateway_list_view(&manager).await.unwrap();

    assert_eq!(view["upstream_count"], 1);
    assert!(view.get("backend_url").is_none());
}
