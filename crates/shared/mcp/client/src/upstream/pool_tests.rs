#[tokio::test]
async fn pool_defaults_normalize_discovery_concurrency() {
    let pool = super::UpstreamPool::new(super::PoolOptions {
        response_caps: crate::upstream::ResponseCaps::default(),
        discovery_concurrency: 0,
    });

    assert_eq!(pool.discovery_concurrency(), 1);
}

#[tokio::test]
async fn pool_registers_http_sse_transport_from_url() {
    let pool = super::UpstreamPool::default();
    pool.register_config(crate::config::UpstreamConfig {
        name: "sse".to_owned(),
        url: Some("https://example.test/mcp?transport=sse".to_owned()),
        ..crate::config::UpstreamConfig::default()
    })
    .unwrap();

    assert_eq!(
        pool.discover_upstream("sse").await.unwrap().transport,
        crate::upstream::TransportKind::HttpSse
    );
}
