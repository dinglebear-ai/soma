use crate::config::{GatewayConfig, UpstreamConfig};
use crate::gateway::manager::GatewayLifecycle;
use crate::upstream::pool::{InProcessUpstream, UpstreamPool};
use crate::upstream::{
    PromptDescriptor, ResourceDescriptor, ToolDescriptor, TransportKind, UpstreamSnapshot,
};

use super::*;

#[tokio::test]
async fn rmcp_tool_routes_carries_schema_and_destructive_flag() {
    let manager = manager_with_pool(pool_with_snapshots(vec![snapshot(
        "one",
        vec![destructive_tool("delete_thing")],
        vec![],
        vec![],
    )]));

    let tools = manager.rmcp_tool_routes().await.expect("rmcp tool routes");

    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "delete_thing");
    let annotations = tools[0].annotations.as_ref().expect("annotations");
    assert_eq!(
        annotations.title.as_deref(),
        Some("Dangerous upstream tool")
    );
    assert_eq!(annotations.read_only_hint, Some(false));
    assert_eq!(annotations.destructive_hint, Some(true));
    assert_eq!(annotations.idempotent_hint, Some(false));
    assert_eq!(annotations.open_world_hint, Some(true));
}

#[tokio::test]
async fn rmcp_resource_routes_falls_back_to_native_uri_when_unnamed() {
    let manager = manager_with_pool(pool_with_snapshots(vec![snapshot(
        "one",
        vec![],
        vec![ResourceDescriptor {
            uri: "native://thing".to_owned(),
            name: None,
        }],
        vec![],
    )]));

    let resources = manager
        .rmcp_resource_routes()
        .await
        .expect("rmcp resource routes");

    assert_eq!(resources.len(), 1);
    assert_eq!(resources[0].name, "native://thing");
}

#[tokio::test]
async fn rmcp_prompt_routes_carries_description() {
    let manager = manager_with_pool(pool_with_snapshots(vec![snapshot(
        "one",
        vec![],
        vec![],
        vec![prompt("help")],
    )]));

    let prompts = manager
        .rmcp_prompt_routes()
        .await
        .expect("rmcp prompt routes");

    assert_eq!(prompts.len(), 1);
    assert_eq!(prompts[0].name, "help");
    assert_eq!(prompts[0].description.as_deref(), Some("prompt"));
}

#[tokio::test]
async fn rmcp_tool_routes_propagates_not_ready_error_instead_of_panicking() {
    let manager = GatewayManager::new(GatewayConfig::default()).expect("manager");
    manager.set_lifecycle_for_tests(GatewayLifecycle::Reloading);

    let result = manager.rmcp_tool_routes().await;

    assert!(
        matches!(result, Err(GatewayManagerError::GatewayReloading)),
        "expected GatewayReloading, got {result:?}"
    );
}

#[tokio::test]
async fn rmcp_resource_routes_propagates_not_ready_error_instead_of_panicking() {
    let manager = GatewayManager::new(GatewayConfig::default()).expect("manager");
    manager.set_lifecycle_for_tests(GatewayLifecycle::Reloading);

    let result = manager.rmcp_resource_routes().await;

    assert!(
        matches!(result, Err(GatewayManagerError::GatewayReloading)),
        "expected GatewayReloading, got {result:?}"
    );
}

#[tokio::test]
async fn rmcp_prompt_routes_propagates_not_ready_error_instead_of_panicking() {
    let manager = GatewayManager::new(GatewayConfig::default()).expect("manager");
    manager.set_lifecycle_for_tests(GatewayLifecycle::Reloading);

    let result = manager.rmcp_prompt_routes().await;

    assert!(
        matches!(result, Err(GatewayManagerError::GatewayReloading)),
        "expected GatewayReloading, got {result:?}"
    );
}

fn manager_with_pool(pool: UpstreamPool) -> GatewayManager {
    let manager = GatewayManager::new(GatewayConfig::default()).expect("manager");
    manager.install_pool_for_tests(pool);
    manager
}

fn pool_with_snapshots(snapshots: Vec<UpstreamSnapshot>) -> UpstreamPool {
    let pool = UpstreamPool::default();
    for snapshot in snapshots {
        let name = snapshot.name.clone();
        pool.register_in_process(
            UpstreamConfig {
                name: name.clone(),
                ..UpstreamConfig::default()
            },
            InProcessUpstream::new(name).with_snapshot(snapshot),
        )
        .expect("register in-process upstream");
    }
    pool
}

fn snapshot(
    name: &str,
    tools: Vec<ToolDescriptor>,
    resources: Vec<ResourceDescriptor>,
    prompts: Vec<PromptDescriptor>,
) -> UpstreamSnapshot {
    let mut snapshot = UpstreamSnapshot::empty(name, TransportKind::InProcess);
    snapshot.tools = tools;
    snapshot.resources = resources;
    snapshot.prompts = prompts;
    snapshot
}

fn destructive_tool(name: &str) -> ToolDescriptor {
    ToolDescriptor {
        name: name.to_owned(),
        description: None,
        input_schema: Some(serde_json::json!({"type": "object"})),
        output_schema: None,
        annotations: Some(serde_json::json!({
            "title": "Dangerous upstream tool",
            "readOnlyHint": false,
            "destructiveHint": true,
            "idempotentHint": false,
            "openWorldHint": true
        })),
        destructive: true,
    }
}

fn prompt(name: &str) -> PromptDescriptor {
    PromptDescriptor {
        name: name.to_owned(),
        description: Some("prompt".to_owned()),
    }
}
