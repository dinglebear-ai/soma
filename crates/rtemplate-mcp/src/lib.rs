//! MCP protocol layer — tool dispatch, schemas, prompts, and server handler.
//!
//! This module is strictly MCP concerns: the `ServerHandler` impl, tool schemas,
//! prompt templates, and dispatch shims. Application state lives in `rtemplate_runtime::server`.

mod conformance;
mod prompts;
mod response_paging;
pub mod rmcp_server;
mod schemas;
mod tools;
#[cfg(feature = "http")]
mod transport;

pub use rmcp_server::{rmcp_server, ExampleRmcpServer};
#[cfg(feature = "http")]
pub use transport::{allowed_origins, streamable_http_config, streamable_http_service};

#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub use tools::execute_tool_without_peer_for_test;

#[cfg(test)]
mod testing {
    use rtemplate_contracts::config::{ExampleConfig, McpConfig};
    use rtemplate_runtime::server::{AppState, AuthPolicy};
    use rtemplate_service::{ExampleClient, ExampleService};

    pub fn loopback_state() -> AppState {
        let client = ExampleClient::new(&ExampleConfig {
            api_url: String::new(),
            api_key: "test".into(),
        })
        .expect("stub client should always build");
        let service = ExampleService::new(client);
        let provider_registry =
            rtemplate_service::static_provider_registry(service.clone()).expect("static registry");
        AppState {
            config: McpConfig::default(),
            auth_policy: AuthPolicy::LoopbackDev,
            service,
            provider_registry,
            response_pages: Default::default(),
        }
    }
}

#[cfg(test)]
#[path = "mcp_tests.rs"]
mod tests;
