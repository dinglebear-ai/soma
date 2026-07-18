use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use soma_application::{ExecutionContext, GatewayRouteScope, SomaApplication};
use soma_config::McpConfig;
use soma_domain::{AuthorizationMode, Principal, RequestId, Surface, TraceContext};
use soma_mcp_server::ResponsePageStore;

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;

#[derive(Clone)]
pub struct McpState {
    application: Arc<SomaApplication>,
    config: McpConfig,
    authorization_mode: AuthorizationMode,
    response_pages: ResponsePageStore,
}

impl McpState {
    pub fn new(
        application: Arc<SomaApplication>,
        config: McpConfig,
        authorization_mode: AuthorizationMode,
        response_pages: ResponsePageStore,
    ) -> Self {
        Self {
            application,
            config,
            authorization_mode,
            response_pages,
        }
    }

    pub fn application(&self) -> &SomaApplication {
        self.application.as_ref()
    }

    pub fn config(&self) -> &McpConfig {
        &self.config
    }

    pub fn with_server_name(mut self, server_name: impl Into<String>) -> Self {
        self.config.server_name = server_name.into();
        self
    }

    pub fn authorization_mode(&self) -> AuthorizationMode {
        self.authorization_mode
    }

    pub fn response_pages(&self) -> &ResponsePageStore {
        &self.response_pages
    }

    pub fn execution_context(
        &self,
        principal: Option<Principal>,
        trace: Option<TraceContext>,
    ) -> ExecutionContext {
        ExecutionContext {
            principal,
            authorization_mode: self.authorization_mode,
            surface: Surface::Mcp,
            trace,
            destructive_confirmation: Default::default(),
            response_limit: None,
            request_id: next_request_id(),
        }
    }
}

fn next_request_id() -> RequestId {
    static REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);
    let sequence = REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    RequestId::new(format!("mcp-{}-{sequence}", std::process::id()))
        .expect("generated MCP request ids are valid")
}

/// MCP route filtering supplied by the product composition layer.
pub type McpRouteScope = GatewayRouteScope;
