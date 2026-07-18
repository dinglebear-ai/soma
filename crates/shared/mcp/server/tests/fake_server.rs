//! Acceptance test for PR 14: a completely unrelated MCP server can be built
//! on top of `soma-mcp-server` without depending on, or even knowing about,
//! any Soma product crate.
//!
//! This file only pulls in `rmcp`, `serde_json`, and `soma_mcp_server` — no
//! `soma-domain`, `soma-application`, or `soma-mcp` symbol appears anywhere
//! below. `FakeServer` is a stand-in for a hypothetical, entirely unrelated
//! product's MCP server.

use rmcp::{
    model::{CallToolRequestParams, CallToolResult, ListToolsResult, PaginatedRequestParams},
    service::RequestContext,
    ErrorData, RoleServer, ServerHandler,
};
use serde_json::{Map, Value};

use soma_mcp_server::{
    conformance,
    error_result::unknown_tool_error,
    protocol::tool_from_descriptor,
    response_paging::{response_page_request, tool_result_from_json, ResponsePagingOptions},
    ResponsePageStore,
};

/// A stand-in for an unrelated product's inbound MCP server. It exposes one
/// native tool (`ping`) plus the generic MCP conformance fixtures, and is
/// built entirely from `soma-mcp-server` primitives.
#[derive(Clone, Default)]
struct FakeServer {
    response_pages: ResponsePageStore,
}

impl FakeServer {
    fn list_tools_sync(&self) -> ListToolsResult {
        let mut tools = vec![tool_from_descriptor(
            "ping",
            Some("Replies pong".to_owned()),
            None,
            None,
            false,
        )];
        tools.extend(conformance::tool_definitions());
        ListToolsResult {
            tools,
            ..Default::default()
        }
    }

    fn call_tool_sync(
        &self,
        name: &str,
        arguments: Option<Map<String, Value>>,
    ) -> Result<CallToolResult, ErrorData> {
        if let Some(result) = conformance::call_tool(name) {
            return Ok(result);
        }
        if name != "ping" {
            return Err(unknown_tool_error(name, &["ping"]));
        }
        let page_request = response_page_request(arguments.as_ref())?;
        tool_result_from_json(
            serde_json::json!({"message": "pong"}),
            &self.response_pages,
            page_request,
            ResponsePagingOptions::default(),
            "ping",
            None,
            None,
        )
    }
}

impl ServerHandler for FakeServer {
    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(self.list_tools_sync())
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        self.call_tool_sync(&request.name, request.arguments)
    }
}

// Compile-time proof that `FakeServer` satisfies `rmcp::ServerHandler` using
// nothing but `soma-mcp-server`, `rmcp`, and `serde_json`.
fn assert_server_handler<T: ServerHandler>() {}
const _: fn() = || assert_server_handler::<FakeServer>();

#[test]
fn fake_server_lists_its_own_tool_and_conformance_fixtures() {
    let server = FakeServer::default();
    let names: Vec<_> = server
        .list_tools_sync()
        .tools
        .into_iter()
        .map(|tool| tool.name.to_string())
        .collect();

    assert!(names.contains(&"ping".to_string()));
    assert!(names.contains(&"test_simple_text".to_string()));
}

#[test]
fn fake_server_calls_its_own_tool_successfully() {
    let server = FakeServer::default();
    let result = server
        .call_tool_sync("ping", None)
        .expect("ping should succeed");

    assert_ne!(result.is_error, Some(true));
}

#[test]
fn fake_server_serves_conformance_fixtures_by_name() {
    let server = FakeServer::default();
    let result = server
        .call_tool_sync("test_simple_text", None)
        .expect("conformance fixture should succeed");

    assert_ne!(result.is_error, Some(true));
}

#[test]
fn fake_server_rejects_unknown_tool_with_a_protocol_error() {
    let server = FakeServer::default();
    let error = server
        .call_tool_sync("does-not-exist", None)
        .expect_err("unknown tool should be rejected");

    assert!(error.message.contains("unknown tool"));
    let data = error.data.expect("unknown tool error should carry data");
    assert_eq!(data["code"], "unknown_tool");
}
