use crate::config::UpstreamConfig;
use crate::upstream::pool::{InProcessUpstream, PoolOptions, UpstreamPool};
use crate::upstream::{ResponseCaps, ToolDescriptor};

#[tokio::test]
async fn discovery_lists_registered_in_process_upstreams() {
    let pool = UpstreamPool::default();
    let config = UpstreamConfig {
        name: "mock".to_owned(),
        ..UpstreamConfig::default()
    };
    let upstream = InProcessUpstream::new("mock")
        .with_tool(ToolDescriptor::new("echo"), serde_json::json!({"ok": true}));

    pool.register_in_process(config, upstream).unwrap();

    let snapshots = pool.discover().await.unwrap();
    assert_eq!(snapshots[0].name, "mock");
    assert_eq!(snapshots[0].tools[0].name, "echo");
}

#[tokio::test]
async fn caps_bound_discovery_payloads() {
    let pool = UpstreamPool::new(PoolOptions {
        response_caps: ResponseCaps {
            tools_list_bytes: 1,
            ..ResponseCaps::default()
        },
        discovery_concurrency: 4,
    });
    pool.register_config(UpstreamConfig {
        name: "too-big".to_owned(),
        ..UpstreamConfig::default()
    })
    .unwrap();

    assert!(pool
        .discover()
        .await
        .unwrap_err()
        .to_string()
        .contains("exceeding"));
}

#[tokio::test]
async fn subject_scoped_paths_use_discovery_concurrency_cap() {
    let pool = UpstreamPool::new(PoolOptions {
        response_caps: ResponseCaps::default(),
        discovery_concurrency: 0,
    });

    assert_eq!(pool.discovery_concurrency(), 1);
    assert_eq!(pool.subject_scoped_discovery_limit(), 1);
}
