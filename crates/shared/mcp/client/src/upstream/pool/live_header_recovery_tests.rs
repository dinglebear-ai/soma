use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ErrorData,
    ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerInfo, Tool,
};
use rmcp::{RoleClient, RoleServer, ServerHandler, ServiceExt};

use super::{
    call_tool_once_with_header_recovery, call_tool_with_header_recovery, is_tool_header_mismatch,
};

#[derive(Clone)]
struct HeaderMismatchServer {
    list_calls: Arc<AtomicUsize>,
    tool_calls: Arc<AtomicUsize>,
    always_mismatch: bool,
}

impl ServerHandler for HeaderMismatchServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        self.list_calls.fetch_add(1, Ordering::SeqCst);
        Ok(ListToolsResult::with_all_items(vec![Tool::new(
            "header.tool",
            "header recovery fixture",
            Arc::new(serde_json::Map::new()),
        )]))
    }

    async fn call_tool(
        &self,
        _request: CallToolRequestParams,
        _context: rmcp::service::RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let attempt = self.tool_calls.fetch_add(1, Ordering::SeqCst);
        if self.always_mismatch || attempt == 0 {
            return Err(ErrorData::header_mismatch(
                "missing Mcp-Param-owner header for `owner`",
                None,
            ));
        }
        Ok(CallToolResult::success(vec![ContentBlock::text("recovered")]).into())
    }
}

async fn peer_for(
    server: HeaderMismatchServer,
) -> (
    rmcp::service::RunningService<RoleClient, ()>,
    tokio::task::JoinHandle<()>,
) {
    let (server_transport, client_transport) = tokio::io::duplex(64 * 1024);
    let server_task = tokio::spawn(async move {
        let running = server
            .serve(server_transport)
            .await
            .expect("header mismatch server starts");
        let _ = running.waiting().await;
    });
    let client = ().serve(client_transport).await.expect("header mismatch client starts");
    (client, server_task)
}

#[test]
fn header_mismatch_detection_uses_the_rmcp_error_code() {
    let ordinary = rmcp::ServiceError::McpError(ErrorData::invalid_params(
        "header mismatch: application text only",
        None,
    ));
    assert!(!is_tool_header_mismatch(&ordinary));

    let mismatch = rmcp::ServiceError::McpError(ErrorData::header_mismatch(
        "missing Mcp-Param-owner header",
        None,
    ));
    assert!(is_tool_header_mismatch(&mismatch));
}

#[tokio::test]
async fn header_mismatch_refreshes_schema_and_retries_once() {
    let list_calls = Arc::new(AtomicUsize::new(0));
    let tool_calls = Arc::new(AtomicUsize::new(0));
    let (client, server_task) = peer_for(HeaderMismatchServer {
        list_calls: Arc::clone(&list_calls),
        tool_calls: Arc::clone(&tool_calls),
        always_mismatch: false,
    })
    .await;
    let peer = client.peer().clone();

    let response = call_tool_once_with_header_recovery(
        &peer,
        "header-mismatch",
        CallToolRequestParams::new("header.tool"),
        512 * 1024,
    )
    .await
    .expect("HeaderMismatch should self-heal");

    assert!(matches!(response, CallToolResponse::Complete(_)));
    assert_eq!(list_calls.load(Ordering::SeqCst), 1);
    assert_eq!(tool_calls.load(Ordering::SeqCst), 2);
    drop(client);
    server_task.abort();
}

#[tokio::test]
async fn plain_call_path_also_refreshes_schema_and_retries_once() {
    let list_calls = Arc::new(AtomicUsize::new(0));
    let tool_calls = Arc::new(AtomicUsize::new(0));
    let (client, server_task) = peer_for(HeaderMismatchServer {
        list_calls: Arc::clone(&list_calls),
        tool_calls: Arc::clone(&tool_calls),
        always_mismatch: false,
    })
    .await;
    let peer = client.peer().clone();

    call_tool_with_header_recovery(
        &peer,
        "header-mismatch",
        CallToolRequestParams::new("header.tool"),
        512 * 1024,
    )
    .await
    .expect("plain tools/call should self-heal");

    assert_eq!(list_calls.load(Ordering::SeqCst), 1);
    assert_eq!(tool_calls.load(Ordering::SeqCst), 2);
    drop(client);
    server_task.abort();
}

#[tokio::test]
async fn header_recovery_respects_the_tools_list_byte_cap() {
    let list_calls = Arc::new(AtomicUsize::new(0));
    let tool_calls = Arc::new(AtomicUsize::new(0));
    let (client, server_task) = peer_for(HeaderMismatchServer {
        list_calls: Arc::clone(&list_calls),
        tool_calls: Arc::clone(&tool_calls),
        always_mismatch: false,
    })
    .await;
    let peer = client.peer().clone();

    let error = call_tool_once_with_header_recovery(
        &peer,
        "header-mismatch",
        CallToolRequestParams::new("header.tool"),
        1,
    )
    .await
    .expect_err("catalog cap must stop recovery before replay");

    assert!(matches!(
        error,
        crate::upstream::UpstreamError::ResponseTooLarge {
            scope: crate::upstream::CapScope::ToolsList,
            ..
        }
    ));
    assert_eq!(list_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        tool_calls.load(Ordering::SeqCst),
        1,
        "a failed bounded refresh must not replay tools/call"
    );
    drop(client);
    server_task.abort();
}

#[tokio::test]
async fn repeated_header_mismatch_is_not_retried_forever() {
    let list_calls = Arc::new(AtomicUsize::new(0));
    let tool_calls = Arc::new(AtomicUsize::new(0));
    let (client, server_task) = peer_for(HeaderMismatchServer {
        list_calls: Arc::clone(&list_calls),
        tool_calls: Arc::clone(&tool_calls),
        always_mismatch: true,
    })
    .await;
    let peer = client.peer().clone();

    let error = call_tool_once_with_header_recovery(
        &peer,
        "header-mismatch",
        CallToolRequestParams::new("header.tool"),
        512 * 1024,
    )
    .await
    .expect_err("second HeaderMismatch must be returned");

    assert!(error.to_string().contains("Mcp error"));
    assert_eq!(list_calls.load(Ordering::SeqCst), 1);
    assert_eq!(tool_calls.load(Ordering::SeqCst), 2);
    drop(client);
    server_task.abort();
}
