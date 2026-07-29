use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use rmcp::{
    ServerHandler, ServiceExt,
    model::{ProtocolVersion, ServerNotification, SubscriptionFilter},
    service::{ClientLifecycleMode, ClientServiceExt},
};
use serde_json::{Map, Value};
use soma_application::{
    ExecutionContext, GatewayExecuteRequest, GatewayPort, GatewayPromptRoute, GatewayReloadRequest,
    GatewayResourceRoute, GatewayRouteScope, GatewayToolRoute, PortError,
};
use tokio::sync::Notify;

use super::{CatalogChanges, CatalogSnapshot};

fn snapshot(tools: &[&str], resources: &[&str], prompts: &[&str]) -> CatalogSnapshot {
    CatalogSnapshot {
        tools: tools.iter().map(|value| (*value).to_owned()).collect(),
        resources: resources.iter().map(|value| (*value).to_owned()).collect(),
        prompts: prompts.iter().map(|value| (*value).to_owned()).collect(),
    }
}

#[test]
fn change_detection_reports_only_moved_catalog_categories() {
    let previous = snapshot(&["alpha"], &["resource:a"], &["prompt-a"]);
    let next = snapshot(&["alpha", "beta"], &["resource:a"], &["prompt-b"]);

    assert_eq!(
        next.changes_from(&previous),
        CatalogChanges {
            tools: true,
            resources: false,
            prompts: true,
        }
    );
    assert_eq!(previous.changes_from(&previous), CatalogChanges::default());
}

#[test]
fn soma_advertises_each_catalog_list_changed_capability() {
    let server = crate::rmcp_server(crate::testing::loopback_state());
    let capabilities = server.get_info().capabilities;

    assert_eq!(
        capabilities
            .tools
            .and_then(|capability| capability.list_changed),
        Some(true)
    );
    assert_eq!(
        capabilities
            .resources
            .and_then(|capability| capability.list_changed),
        Some(true)
    );
    assert_eq!(
        capabilities
            .prompts
            .and_then(|capability| capability.list_changed),
        Some(true)
    );
}

#[derive(Default)]
struct MutableGateway {
    extra_tool: AtomicBool,
    tool_reads: AtomicUsize,
    tool_read_notify: Notify,
}

impl MutableGateway {
    fn expose_extra_tool(&self) {
        self.extra_tool.store(true, Ordering::SeqCst);
    }

    async fn wait_for_initial_snapshot(&self) {
        loop {
            if self.tool_reads.load(Ordering::SeqCst) >= 1 {
                return;
            }
            self.tool_read_notify.notified().await;
        }
    }
}

#[async_trait]
impl GatewayPort for MutableGateway {
    async fn status(&self, _context: &ExecutionContext) -> Result<Value, PortError> {
        Ok(serde_json::json!({}))
    }

    async fn reload(
        &self,
        _request: GatewayReloadRequest,
        _context: &ExecutionContext,
    ) -> Result<Value, PortError> {
        Ok(serde_json::json!({}))
    }

    async fn execute(
        &self,
        _request: GatewayExecuteRequest,
        _context: &ExecutionContext,
    ) -> Result<Value, PortError> {
        Ok(serde_json::json!({}))
    }

    async fn list_mcp_tools(
        &self,
        _scope: Option<&GatewayRouteScope>,
        _context: &ExecutionContext,
    ) -> Result<Vec<GatewayToolRoute>, PortError> {
        self.tool_reads.fetch_add(1, Ordering::SeqCst);
        self.tool_read_notify.notify_waiters();
        Ok(self
            .extra_tool
            .load(Ordering::SeqCst)
            .then(|| GatewayToolRoute {
                name: "newly_visible".to_owned(),
                description: Some("subscription mutation proof".to_owned()),
                input_schema: Some(serde_json::json!({"type": "object"})),
                output_schema: None,
                destructive: false,
            })
            .into_iter()
            .collect())
    }

    async fn call_mcp_tool(
        &self,
        _name: &str,
        _params: Value,
        _scope: Option<&GatewayRouteScope>,
        _context: &ExecutionContext,
    ) -> Result<Option<Value>, PortError> {
        Ok(None)
    }

    async fn list_mcp_resources(
        &self,
        _scope: Option<&GatewayRouteScope>,
        _context: &ExecutionContext,
    ) -> Result<Vec<GatewayResourceRoute>, PortError> {
        Ok(Vec::new())
    }

    async fn read_mcp_resource(
        &self,
        _uri: &str,
        _scope: Option<&GatewayRouteScope>,
        _context: &ExecutionContext,
    ) -> Result<Option<Value>, PortError> {
        Ok(None)
    }

    async fn list_mcp_prompts(
        &self,
        _scope: Option<&GatewayRouteScope>,
        _context: &ExecutionContext,
    ) -> Result<Vec<GatewayPromptRoute>, PortError> {
        Ok(Vec::new())
    }

    async fn get_mcp_prompt(
        &self,
        _name: &str,
        _arguments: Option<Map<String, Value>>,
        _scope: Option<&GatewayRouteScope>,
        _context: &ExecutionContext,
    ) -> Result<Option<Value>, PortError> {
        Ok(None)
    }
}

#[tokio::test]
async fn subscription_delivers_tool_list_change_after_visible_contract_moves() {
    let gateway = Arc::new(MutableGateway::default());
    let state = crate::testing::loopback_state_with_gateway(gateway.clone());
    let (server_transport, client_transport) = tokio::io::duplex(64 * 1024);
    let server_handle = tokio::spawn(async move {
        crate::rmcp_server(state)
            .serve(server_transport)
            .await
            .expect("server should accept client")
            .waiting()
            .await
            .expect("server should stop cleanly");
    });
    let mut client = ()
        .serve_with_lifecycle(
            client_transport,
            ClientLifecycleMode::Discover {
                preferred_versions: vec![ProtocolVersion::V_2026_07_28],
            },
        )
        .await
        .expect("client should negotiate modern lifecycle");
    let mut subscription = client
        .peer()
        .listen(SubscriptionFilter::builder().tools_list_changed().build())
        .await
        .expect("server should acknowledge tool subscription");
    assert_eq!(subscription.acknowledged().tools_list_changed, Some(true));

    gateway.wait_for_initial_snapshot().await;
    gateway.expose_extra_tool();
    let notification = tokio::time::timeout(Duration::from_secs(4), subscription.next())
        .await
        .expect("catalog watcher should deliver promptly")
        .expect("subscription should remain valid")
        .expect("subscription should emit one notification");
    assert!(matches!(
        notification,
        ServerNotification::ToolListChangedNotification(_)
    ));

    subscription.cancel().await.expect("cancel subscription");
    client.close().await.expect("close client");
    server_handle.await.expect("join server task");
}
