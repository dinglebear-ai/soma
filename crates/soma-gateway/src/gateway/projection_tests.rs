use crate::config::{GatewayConfig, UpstreamConfig};
use crate::gateway::manager::GatewayManager;

use super::*;

#[test]
fn projection_counts_health_and_discovery() {
    let manager = GatewayManager::new(GatewayConfig {
        upstream: vec![
            UpstreamConfig {
                name: "on".to_owned(),
                ..UpstreamConfig::default()
            },
            UpstreamConfig {
                name: "off".to_owned(),
                enabled: false,
                ..UpstreamConfig::default()
            },
        ],
        ..GatewayConfig::default()
    })
    .unwrap();

    let projection = GatewayProjection::from_manager(&manager).unwrap();

    assert_eq!(projection.upstream_count, 2);
    assert_eq!(projection.connected_count, 1);
}
