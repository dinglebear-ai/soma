use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use rmcp::model::{
    ErrorCode, ErrorData, ListPromptsResult, ListResourcesResult, ListToolsResult,
    PaginatedRequestParams, Prompt, ProtocolVersion, Resource, ServerCapabilities, ServerInfo,
    SubscriptionFilter, Tool,
};
use rmcp::service::{RequestContext, ServiceError, SubscriptionContext};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use rmcp::{RoleServer, ServerHandler};

use crate::config::UpstreamConfig;
use crate::upstream::pool::UpstreamPool;

use super::*;

const RESOURCE_URI: &str = "test://subscription-resource";

#[derive(Clone)]
struct SubscriptionServer {
    attempts: Arc<AtomicUsize>,
    tools: Arc<tokio::sync::RwLock<Vec<Tool>>>,
    resources: Arc<tokio::sync::RwLock<Vec<Resource>>>,
    prompts: Arc<tokio::sync::RwLock<Vec<Prompt>>>,
    tool_change: Arc<tokio::sync::Notify>,
    resource_change: Arc<tokio::sync::Notify>,
    prompt_change: Arc<tokio::sync::Notify>,
    resource_update_uri: Arc<tokio::sync::RwLock<String>>,
    resource_update: Arc<tokio::sync::Notify>,
}

impl SubscriptionServer {
    fn new() -> Self {
        Self {
            attempts: Arc::new(AtomicUsize::new(0)),
            tools: Arc::new(tokio::sync::RwLock::new(vec![test_tool("before")])),
            resources: Arc::new(tokio::sync::RwLock::new(vec![Resource::new(
                RESOURCE_URI,
                "subscription-resource",
            )])),
            prompts: Arc::new(tokio::sync::RwLock::new(vec![Prompt::new(
                "before_prompt",
                Some("before prompt"),
                None,
            )])),
            tool_change: Arc::new(tokio::sync::Notify::new()),
            resource_change: Arc::new(tokio::sync::Notify::new()),
            prompt_change: Arc::new(tokio::sync::Notify::new()),
            resource_update_uri: Arc::new(tokio::sync::RwLock::new(RESOURCE_URI.to_owned())),
            resource_update: Arc::new(tokio::sync::Notify::new()),
        }
    }

    async fn replace_tools_and_notify(&self, names: &[&str]) {
        *self.tools.write().await = names.iter().map(|name| test_tool(name)).collect();
        self.tool_change.notify_one();
    }

    async fn replace_resources_and_notify(&self, uris: &[&str]) {
        *self.resources.write().await = uris
            .iter()
            .map(|uri| Resource::new((*uri).to_owned(), "changed-resource"))
            .collect();
        self.resource_change.notify_one();
    }

    async fn replace_prompts_and_notify(&self, names: &[&str]) {
        *self.prompts.write().await = names
            .iter()
            .map(|name| Prompt::new((*name).to_owned(), Some("changed prompt"), None))
            .collect();
        self.prompt_change.notify_one();
    }

    async fn notify_resource_update(&self, uri: &str) {
        *self.resource_update_uri.write().await = uri.to_owned();
        self.resource_update.notify_waiters();
    }
}

impl ServerHandler for SubscriptionServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_tool_list_changed()
                .enable_prompts()
                .enable_prompts_list_changed()
                .enable_resources()
                .enable_resources_list_changed()
                .enable_resources_subscribe()
                .build(),
        )
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(
            self.tools.read().await.clone(),
        ))
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        Ok(ListResourcesResult::with_all_items(
            self.resources.read().await.clone(),
        ))
    }

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, ErrorData> {
        Ok(ListPromptsResult::with_all_items(
            self.prompts.read().await.clone(),
        ))
    }

    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        self.attempts.fetch_add(1, Ordering::SeqCst);
        Some(requested.supported_by(&self.get_info().capabilities))
    }

    async fn listen(&self, context: SubscriptionContext) -> Result<(), ErrorData> {
        loop {
            tokio::select! {
                () = context.cancelled() => return Ok(()),
                () = self.tool_change.notified() => {
                    context
                        .sink()
                        .notify_tool_list_changed()
                        .await
                        .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;
                }
                () = self.resource_change.notified() => {
                    context
                        .sink()
                        .notify_resource_list_changed()
                        .await
                        .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;
                }
                () = self.prompt_change.notified() => {
                    context
                        .sink()
                        .notify_prompt_list_changed()
                        .await
                        .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;
                }
                () = self.resource_update.notified() => {
                    let uri = self.resource_update_uri.read().await.clone();
                    context
                        .sink()
                        .notify_resource_updated(uri)
                        .await
                        .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;
                }
            }
        }
    }
}

fn test_tool(name: &str) -> Tool {
    Tool::new(
        name.to_owned(),
        "notification test tool",
        Arc::new(serde_json::Map::new()),
    )
}

#[test]
fn oversized_resource_update_uri_is_dropped_before_broadcast() {
    assert!(resource_updated_event("leaf", "x".repeat(MAX_RESOURCE_UPDATE_URI_BYTES),).is_some());
    assert!(
        resource_updated_event("leaf", "x".repeat(MAX_RESOURCE_UPDATE_URI_BYTES + 1),).is_none()
    );
}

#[test]
fn subscription_listen_is_only_attempted_for_modern_protocol() {
    assert!(subscription_listen_supported_protocol(
        &ProtocolVersion::V_2026_07_28
    ));
    assert!(!subscription_listen_supported_protocol(
        &ProtocolVersion::V_2025_11_25
    ));
}

#[test]
fn exact_resource_subscriptions_respect_exposure_filters() {
    let resources = vec![
        crate::upstream::ResourceDescriptor {
            uri: "test://visible".to_owned(),
            name: None,
        },
        crate::upstream::ResourceDescriptor {
            uri: "test://hidden".to_owned(),
            name: None,
        },
    ];
    let expose = vec!["test://visible".to_owned()];
    assert_eq!(
        super::listen::subscription_resource_uris(&resources, Some(&expose)),
        ["test://visible"]
    );
}

#[test]
fn subscription_filter_respects_proxy_flags() {
    let capabilities = SubscriptionServer::new().get_info().capabilities;
    let hidden = super::listen::requested_subscription_filter(
        &capabilities,
        false,
        false,
        vec![RESOURCE_URI.to_owned()],
    );
    assert_eq!(hidden.tools_list_changed, Some(true));
    assert_eq!(hidden.prompts_list_changed, None);
    assert_eq!(hidden.resources_list_changed, None);
    assert_eq!(hidden.resource_subscriptions, None);

    let exposed = super::listen::requested_subscription_filter(
        &capabilities,
        true,
        true,
        vec![RESOURCE_URI.to_owned()],
    );
    assert_eq!(exposed.tools_list_changed, Some(true));
    assert_eq!(exposed.prompts_list_changed, Some(true));
    assert_eq!(exposed.resources_list_changed, Some(true));
    assert_eq!(
        exposed.resource_subscriptions,
        Some(vec![RESOURCE_URI.to_owned()])
    );
}

#[test]
fn terminal_subscription_errors_stop_unchanged_retries() {
    let method_not_found = ServiceError::McpError(ErrorData::new(
        ErrorCode::METHOD_NOT_FOUND,
        "Method not found",
        None,
    ));
    assert!(terminal_subscription_listen_error(&method_not_found, 0));

    let invalid_params = ServiceError::McpError(ErrorData::new(
        ErrorCode::INVALID_PARAMS,
        "Invalid request parameters",
        None,
    ));
    assert!(!terminal_subscription_listen_error(&invalid_params, 0));
    assert!(terminal_subscription_listen_error(&invalid_params, 1));

    let limit = ServiceError::McpError(ErrorData::new(
        ErrorCode::INTERNAL_ERROR,
        "Subscription limit reached",
        None,
    ));
    assert!(terminal_subscription_listen_error(&limit, 0));
    assert!(!terminal_subscription_listen_error(
        &ServiceError::TransportClosed,
        9
    ));
}

#[test]
fn subscription_retry_delay_is_bounded_and_dephased() {
    let alpha = subscription_retry_delay("alpha", 0);
    let bravo = subscription_retry_delay("bravo", 0);
    assert_ne!(alpha, bravo);
    let capped = subscription_retry_delay("alpha", 8);
    assert!((Duration::from_secs(48)..=Duration::from_secs(72)).contains(&capped));
    assert!(subscription_retry_delay("alpha", 2) > alpha);
}

#[test]
fn stable_subscription_resets_retry_attempt() {
    assert_eq!(
        next_subscription_retry_attempt(7, Duration::from_secs(30)),
        0
    );
    assert_eq!(
        next_subscription_retry_attempt(7, Duration::from_secs(29)),
        8
    );
}

#[test]
fn replacing_subscription_generation_cancels_the_old_owner() {
    let pool = UpstreamPool::default();
    let first = pool.begin_subscription_generation("leaf");
    let second = pool.begin_subscription_generation("leaf");

    assert!(first.is_cancelled());
    assert!(pool.subscription_generation_is_current("leaf", &second));
    pool.cancel_upstream_subscription("leaf");
    assert!(second.is_cancelled());
}

#[test]
fn dropping_setup_guard_retires_phantom_generation() {
    let pool = UpstreamPool::default();
    let generation = pool.begin_subscription_generation("leaf");
    {
        let _guard = SubscriptionGenerationGuard::new(
            &pool.subscription_tasks,
            "leaf",
            Arc::clone(&generation),
        );
    }

    assert!(generation.is_cancelled());
    assert!(!pool.subscription_generation_is_current("leaf", &generation));
}

#[test]
fn dropping_pool_cancels_owned_subscription_generations() {
    let generation = {
        let pool = UpstreamPool::default();
        let generation = pool.begin_subscription_generation("leaf");
        assert!(!generation.is_cancelled());
        generation
    };

    assert!(generation.is_cancelled());
}

#[tokio::test]
async fn live_subscription_emits_tool_change_and_refreshes_exact_snapshot() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind subscription test server");
    let addr = listener.local_addr().expect("subscription server addr");
    let fixture = SubscriptionServer::new();
    let server_fixture = fixture.clone();
    let service: StreamableHttpService<SubscriptionServer, LocalSessionManager> =
        StreamableHttpService::new(
            move || Ok(server_fixture.clone()),
            Default::default(),
            StreamableHttpServerConfig::default().with_legacy_session_mode(false),
        );
    let router = axum::Router::new().nest_service("/mcp", service);
    let server = tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("subscription test server");
    });

    let pool = UpstreamPool::default();
    let mut notifications = pool.subscribe_notifications();
    pool.register_config(UpstreamConfig {
        name: "leaf".to_owned(),
        url: Some(format!("http://{addr}/mcp")),
        ..UpstreamConfig::default()
    })
    .expect("register subscription upstream");
    pool.ensure_connected("leaf")
        .await
        .expect("connect subscription upstream");
    assert_eq!(fixture.attempts.load(Ordering::SeqCst), 1);

    fixture.replace_tools_and_notify(&["after"]).await;
    let event = tokio::time::timeout(Duration::from_secs(3), notifications.recv())
        .await
        .expect("tool list_changed arrives")
        .expect("notification bus remains open");
    let UpstreamNotificationEvent::ToolListChanged { upstream } = event else {
        panic!("expected tool list_changed event");
    };
    assert_eq!(upstream, "leaf");
    assert!(pool.refresh_tools_after_list_changed(&upstream).await);

    let snapshot = pool
        .discover_upstream("leaf")
        .await
        .expect("read refreshed snapshot");
    assert_eq!(
        snapshot
            .tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>(),
        ["after"]
    );

    fixture
        .replace_resources_and_notify(&["test://after-resource"])
        .await;
    let event = tokio::time::timeout(Duration::from_secs(3), notifications.recv())
        .await
        .expect("resource list_changed arrives")
        .expect("notification bus remains open");
    let UpstreamNotificationEvent::ResourceListChanged { upstream } = event else {
        panic!("expected resource list_changed event");
    };
    assert!(pool.refresh_resources_after_list_changed(&upstream).await);
    assert_eq!(
        fixture.attempts.load(Ordering::SeqCst),
        2,
        "visible resource URI changes must replace the listen generation"
    );
    let snapshot = pool
        .discover_upstream("leaf")
        .await
        .expect("read resource-refreshed snapshot");
    assert_eq!(
        snapshot
            .resources
            .iter()
            .map(|resource| resource.uri.as_str())
            .collect::<Vec<_>>(),
        ["test://after-resource"]
    );

    tokio::time::sleep(Duration::from_millis(20)).await;
    fixture
        .notify_resource_update("test://after-resource")
        .await;
    let event = tokio::time::timeout(Duration::from_secs(3), notifications.recv())
        .await
        .expect("newly subscribed resource update arrives")
        .expect("notification bus remains open");
    let UpstreamNotificationEvent::ResourceUpdated { upstream, uri } = event else {
        panic!("expected resource_updated event");
    };
    assert_eq!(upstream, "leaf");
    assert_eq!(uri, "test://after-resource");

    fixture.replace_prompts_and_notify(&["after_prompt"]).await;
    let event = tokio::time::timeout(Duration::from_secs(3), notifications.recv())
        .await
        .expect("prompt list_changed arrives")
        .expect("notification bus remains open");
    let UpstreamNotificationEvent::PromptListChanged { upstream } = event else {
        panic!("expected prompt list_changed event");
    };
    assert!(pool.refresh_prompts_after_list_changed(&upstream).await);
    let snapshot = pool
        .discover_upstream("leaf")
        .await
        .expect("read prompt-refreshed snapshot");
    assert_eq!(
        snapshot
            .prompts
            .iter()
            .map(|prompt| prompt.name.as_str())
            .collect::<Vec<_>>(),
        ["after_prompt"]
    );

    pool.cancel_upstream_subscription("leaf");
    server.abort();
}
