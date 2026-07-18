//! Starts the product MCP stdio transport.
//!
//! `serve()` builds the stdio `AppState` (via `bootstrap::stdio_state`),
//! constructs the Soma MCP adapter (`soma_mcp::rmcp_server`), and runs the
//! `soma-mcp-server` stdio lifecycle until the transport closes. Stdio is
//! always `AuthPolicy::LoopbackDev`: it is a local trusted pipe between
//! parent and child process, so HTTP auth middleware does not apply.

use anyhow::Result;
use rmcp::{transport::stdio, ServiceExt};
use soma_mcp as mcp;

/// Serve the MCP protocol over stdio until the client disconnects.
pub(crate) async fn serve() -> Result<()> {
    let state = crate::bootstrap::stdio_state().await?;
    let svc = mcp::rmcp_server(crate::bootstrap::mcp_state_for_state(&state))
        .serve(stdio())
        .await?;
    svc.waiting().await?;
    Ok(())
}

#[cfg(test)]
#[path = "stdio_tests.rs"]
mod tests;
