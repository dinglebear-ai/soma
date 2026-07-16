use crate::config::{GatewayConfig, UpstreamConfig};
use crate::gateway::manager::GatewayManager;

use super::*;

#[tokio::test]
async fn projection_counts_health_and_discovery() {
    let manager = GatewayManager::new(GatewayConfig {
        upstream: vec![
            UpstreamConfig {
                name: "on".to_owned(),
                url: Some("https://example.com/mcp".to_owned()),
                ..UpstreamConfig::default()
            },
            UpstreamConfig {
                name: "off".to_owned(),
                enabled: false,
                url: Some("https://example.com/off".to_owned()),
                ..UpstreamConfig::default()
            },
        ],
        ..GatewayConfig::default()
    })
    .unwrap();

    let projection = GatewayProjection::from_manager(&manager).await.unwrap();

    assert_eq!(projection.upstream_count, 2);
    assert_eq!(projection.connected_count, 0);
    assert_eq!(projection.discovered_tool_count, 0);
    assert_eq!(projection.exposed_tool_count, 0);
}
