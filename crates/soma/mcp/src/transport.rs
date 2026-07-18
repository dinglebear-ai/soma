//! MCP transport configuration and allowed-host/origin helpers.
//!
//! Separated from `rmcp_server.rs` to keep the `ServerHandler` impl focused on
//! protocol concerns. Everything in this file is about wiring the HTTP transport
//! layer: how connections are accepted and which origins are allowed. The
//! deterministic host/origin computation and the generic Streamable HTTP
//! transport wiring live in `soma-mcp-server`; this file only adapts Soma's
//! `McpConfig` into the primitives that the role crate expects.

#[cfg(test)]
#[path = "transport_tests.rs"]
mod tests;

use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use soma_mcp_server::http::{self, AllowedHostsInput, AllowedOriginsInput};

use soma_config::McpConfig;

use super::rmcp_server::{rmcp_server as make_server, SomaRmcpServer};

// ── Transport builders ────────────────────────────────────────────────────────

pub fn streamable_http_config(config: &McpConfig) -> StreamableHttpServerConfig {
    http::streamable_http_config(allowed_hosts(config), allowed_origins(config))
}

pub fn streamable_http_service(
    state: crate::McpState,
    config: StreamableHttpServerConfig,
) -> StreamableHttpService<SomaRmcpServer, LocalSessionManager> {
    http::streamable_http_service(move || Ok(make_server(state.clone())), config)
}

// ── Allowed hosts / origins ───────────────────────────────────────────────────

pub fn allowed_hosts(config: &McpConfig) -> Vec<String> {
    http::allowed_hosts(AllowedHostsInput {
        bind_host: &config.host,
        port: config.port,
        extra_hosts: &config.allowed_hosts,
        public_url: config.auth.public_url.as_deref(),
        public_url_label: "SOMA_MCP_PUBLIC_URL",
    })
}

pub fn allowed_origins(config: &McpConfig) -> Vec<String> {
    http::allowed_origins(AllowedOriginsInput {
        port: config.port,
        extra_origins: &config.allowed_origins,
        public_url: config.auth.public_url.as_deref(),
        extra_origins_label: "SOMA_MCP_ALLOWED_ORIGINS",
        public_url_label: "SOMA_MCP_PUBLIC_URL",
    })
}
