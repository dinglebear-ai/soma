use crate::config::UpstreamConfig;
use crate::upstream::pool::UpstreamPool;
use crate::upstream::UpstreamHealth;

#[tokio::test]
async fn websocket_configs_start_unconnected_not_routable() {
    let pool = UpstreamPool::default();
    pool.register_config(UpstreamConfig {
        name: "ws".to_owned(),
        url: Some("wss://example.test/mcp".to_owned()),
        ..UpstreamConfig::default()
    })
    .unwrap();

    assert_eq!(pool.connected_count(), 0);
    assert!(matches!(
        pool.upstream_health("ws").unwrap(),
        UpstreamHealth::Unsupported { .. }
    ));
}

#[tokio::test]
async fn disabled_upstreams_are_not_connected() {
    let pool = UpstreamPool::default();
    pool.register_config(UpstreamConfig {
        name: "off".to_owned(),
        enabled: false,
        ..UpstreamConfig::default()
    })
    .unwrap();

    assert_eq!(pool.connected_count(), 0);
    assert_eq!(
        pool.upstream_health("off").unwrap(),
        UpstreamHealth::Disabled
    );
}
