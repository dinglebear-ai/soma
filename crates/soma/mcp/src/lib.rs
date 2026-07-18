//! MCP protocol layer — tool dispatch, schemas, prompts, and server handler.
//!
//! This module is strictly MCP concerns: the `ServerHandler` impl, tool schemas,
//! prompt templates, and dispatch shims. Business operations flow through `SomaApplication`.

mod gateway_proxy;
mod prompts;
mod protocol_errors;
mod rmcp_auth;
pub mod rmcp_server;
mod schemas;
mod state;
mod tools;
#[cfg(feature = "http")]
mod transport;

pub use rmcp_server::{rmcp_server, SomaRmcpServer};
pub use state::{McpRouteScope, McpState};
#[cfg(feature = "http")]
pub use transport::{allowed_origins, streamable_http_config, streamable_http_service};

pub(crate) const ACTION_DISCRIMINATOR_FIELD: &str = "_soma_action";

#[cfg(test)]
pub(crate) fn assert_result_has_no_meta(result: &rmcp::model::CallToolResult) {
    assert!(result.meta.is_none(), "result meta should stay empty");
    let serialized = serde_json::to_value(result).expect("result should serialize");
    assert!(
        serialized.get("_meta").is_none(),
        "serialized result included _meta: {serialized}"
    );
}

#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub use tools::execute_tool_without_peer_for_test;

#[cfg(test)]
mod testing {
    use std::sync::Arc;

    use soma_application::{ApplicationPorts, GatewayPort};
    use soma_config::McpConfig;
    use soma_domain::AuthorizationMode;

    pub fn loopback_state() -> super::McpState {
        state(McpConfig::default(), AuthorizationMode::LoopbackDev)
    }

    pub fn loopback_state_with_gateway(gateway: Arc<dyn GatewayPort>) -> super::McpState {
        state_with_ports(
            McpConfig::default(),
            AuthorizationMode::LoopbackDev,
            ApplicationPorts::unavailable().with_gateway(gateway),
        )
    }

    pub fn bearer_state(token: &str) -> super::McpState {
        state(
            McpConfig {
                api_token: Some(token.to_owned()),
                ..McpConfig::default()
            },
            AuthorizationMode::Mounted,
        )
    }

    fn state(config: McpConfig, authorization_mode: AuthorizationMode) -> super::McpState {
        state_with_ports(config, authorization_mode, ApplicationPorts::unavailable())
    }

    fn state_with_ports(
        config: McpConfig,
        authorization_mode: AuthorizationMode,
        ports: ApplicationPorts,
    ) -> super::McpState {
        let application = soma_test_support::default_application_with_ports(ports);
        super::McpState::new(application, config, authorization_mode, Default::default())
    }
}

#[cfg(test)]
#[path = "mcp_tests.rs"]
mod tests;
