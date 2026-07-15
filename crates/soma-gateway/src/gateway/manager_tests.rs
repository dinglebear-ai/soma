use crate::config::{GatewayConfig, UpstreamConfig};
use crate::upstream::pool::{InProcessUpstream, UpstreamPool};
use crate::upstream::ToolDescriptor;
use crate::usage::MemoryUsageSink;

use super::*;

#[test]
fn manager_builds_from_gateway_config() {
    let manager = GatewayManager::new(GatewayConfig {
        upstream: vec![UpstreamConfig {
            name: "mock".to_owned(),
            ..UpstreamConfig::default()
        }],
        ..GatewayConfig::default()
    })
    .unwrap();

    assert_eq!(manager.lifecycle(), GatewayLifecycle::Ready);
    assert_eq!(manager.discover().unwrap()[0].name, "mock");
}

#[test]
fn manager_fails_fast_when_reloading() {
    let manager = GatewayManager::new(GatewayConfig::default()).unwrap();
    manager.set_lifecycle_for_tests(GatewayLifecycle::Reloading);

    assert!(matches!(
        manager.discover().unwrap_err(),
        GatewayManagerError::GatewayReloading
    ));
}

#[test]
fn manager_records_usage_for_routed_calls() {
    let sink = MemoryUsageSink::shared();
    let manager = GatewayManager::with_usage(GatewayConfig::default(), sink.clone()).unwrap();
    let pool = UpstreamPool::default();
    pool.register_in_process(
        UpstreamConfig {
            name: "mock".to_owned(),
            ..UpstreamConfig::default()
        },
        InProcessUpstream::new("mock")
            .with_tool(ToolDescriptor::new("echo"), serde_json::json!({"ok": true})),
    )
    .unwrap();
    manager.install_pool_for_tests(pool);

    manager
        .call_tool("mock", "echo", serde_json::json!({}))
        .unwrap();

    let events = sink.events();
    assert_eq!(events[0].action, "call_tool");
    assert_eq!(events[0].upstream.as_deref(), Some("mock"));
    assert!(events[0].success);
}
