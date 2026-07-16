use crate::config::{GatewayConfig, UpstreamConfig};
use crate::upstream::TransportKind;

use super::*;

#[tokio::test]
async fn build_pool_from_config_keeps_transport_state_in_pool() {
    let pool = build_pool_from_config(&GatewayConfig {
        upstream: vec![UpstreamConfig {
            name: "http".to_owned(),
            url: Some("https://example.test/mcp".to_owned()),
            ..UpstreamConfig::default()
        }],
        ..GatewayConfig::default()
    })
    .unwrap();

    assert_eq!(
        pool.discover_upstream("http").await.unwrap().transport,
        TransportKind::HttpJson
    );
}
