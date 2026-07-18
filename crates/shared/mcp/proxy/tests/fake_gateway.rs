//! Acceptance test for PR 14: a completely unrelated "gateway" can be built
//! from `soma-mcp-client` + `soma-mcp-proxy` (which brings in
//! `soma-mcp-server` for its route-to-`rmcp` projection) without depending
//! on, or even knowing about, any Soma product crate.
//!
//! This file only pulls in `soma_mcp_client`, `soma_mcp_proxy`, and
//! `serde_json` — no `soma-domain`, `soma-application`, `soma-mcp`, or
//! `soma-gateway` symbol appears anywhere below.

use soma_mcp_client::{
    config::UpstreamConfig,
    upstream::{
        pool::{InProcessUpstream, ToolCall, UpstreamPool},
        ToolDescriptor,
    },
};
use soma_mcp_proxy::{rmcp_tool_from_route, tool_routes_from_candidates};

#[tokio::test]
async fn fake_gateway_discovers_projects_and_calls_an_upstream_tool() {
    let pool = UpstreamPool::default();
    let upstream = InProcessUpstream::new("weather").with_tool(
        ToolDescriptor {
            name: "forecast".to_owned(),
            description: Some("today's forecast".to_owned()),
            input_schema: Some(serde_json::json!({"type": "object"})),
            output_schema: None,
            destructive: false,
        },
        serde_json::json!({"forecast": "sunny"}),
    );
    pool.register_in_process(
        UpstreamConfig {
            name: "weather".to_owned(),
            ..UpstreamConfig::default()
        },
        upstream,
    )
    .expect("register in-process upstream");

    let snapshots = pool.discover().await.expect("discover should succeed");
    assert_eq!(snapshots.len(), 1);

    let tools = pool
        .exposed_tools("weather")
        .expect("tools should be exposed");
    let candidates = tools
        .into_iter()
        .map(|tool| ("weather".to_owned(), tool))
        .collect();
    let routes = tool_routes_from_candidates(candidates, std::iter::empty::<&str>());
    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0].name, "forecast");
    assert_eq!(routes[0].upstream, "weather");

    // The route projects into a real rmcp::model::Tool, via soma-mcp-server.
    let rmcp_tool = rmcp_tool_from_route(&routes[0]);
    assert_eq!(rmcp_tool.name, "forecast");
    assert_eq!(rmcp_tool.description.as_deref(), Some("today's forecast"));

    let result = pool
        .call_tool(ToolCall {
            upstream: "weather".to_owned(),
            tool: "forecast".to_owned(),
            params: serde_json::json!({}),
        })
        .await
        .expect("call should succeed");
    assert_eq!(result, serde_json::json!({"forecast": "sunny"}));
}
