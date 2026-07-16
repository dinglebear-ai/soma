use crate::config::{GatewayConfig, UpstreamConfig};
use crate::gateway::manager::GatewayLifecycle;

use super::*;

#[tokio::test]
async fn reload_builds_fresh_pool_and_swaps_config() {
    let manager = GatewayManager::new(GatewayConfig::default()).unwrap();
    manager
        .reload(GatewayConfig {
            upstream: vec![UpstreamConfig {
                name: "fresh".to_owned(),
                url: Some("https://example.com/mcp".to_owned()),
                ..UpstreamConfig::default()
            }],
            ..GatewayConfig::default()
        })
        .unwrap();

    assert_eq!(manager.lifecycle(), GatewayLifecycle::Ready);
    assert_eq!(manager.discover().await.unwrap()[0].name, "fresh");
}

#[tokio::test]
async fn invalid_reload_restores_ready_lifecycle_without_replacing_config() {
    let manager = GatewayManager::new(GatewayConfig::default()).unwrap();
    let result = manager.reload(GatewayConfig {
        upstream: vec![UpstreamConfig {
            name: "bad name".to_owned(),
            url: Some("https://example.com/mcp".to_owned()),
            ..UpstreamConfig::default()
        }],
        ..GatewayConfig::default()
    });

    assert!(result.is_err());
    assert_eq!(manager.lifecycle(), GatewayLifecycle::Ready);
    assert!(manager.discover().await.unwrap().is_empty());
}
