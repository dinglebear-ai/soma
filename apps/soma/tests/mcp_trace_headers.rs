//! Real Streamable HTTP round trips proving trusted trace-header extraction
//! through the actual `/mcp` route.
#![cfg(feature = "mcp-http")]

use std::net::TcpListener as StdTcpListener;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use rmcp::{
    model::CallToolRequestParams, service::ServiceExt, transport::StreamableHttpClientTransport,
};
use serde_json::json;
use soma::server::AppState;
use soma_config::{McpConfig, TraceHeaderMode};
use soma_test_support::{tracing_test_lock, SharedBuf};

const VALID_TRACEPARENT: &str = "00-0af7651916cd43dd8448eb211c80319c-00f067aa0ba902b7-01";

struct ServerHandle {
    port: u16,
    join: Option<tokio::task::JoinHandle<()>>,
}

impl ServerHandle {
    async fn spawn(state: AppState) -> anyhow::Result<Self> {
        let std_listener = StdTcpListener::bind("127.0.0.1:0")?;
        let port = std_listener.local_addr()?.port();
        std_listener.set_nonblocking(true)?;
        let listener = tokio::net::TcpListener::from_std(std_listener)?;

        let app = soma::server::router(state);
        let join = tokio::spawn(async move {
            if let Err(err) = axum::serve(listener, app.into_make_service()).await {
                eprintln!("trace-header test server exited with error: {err}");
            }
        });
        Ok(Self {
            port,
            join: Some(join),
        })
    }

    fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        if let Some(join) = self.join.take() {
            join.abort();
        }
    }
}

struct TracingCapture {
    buf: SharedBuf,
    guard: tracing::subscriber::DefaultGuard,
}

impl TracingCapture {
    fn start() -> Self {
        let buf = SharedBuf::new();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(buf.writer())
            .with_ansi(false)
            .without_time()
            .finish();
        let guard = tracing::subscriber::set_default(subscriber);
        Self { buf, guard }
    }

    fn finish(self) -> String {
        drop(self.guard);
        self.buf.contents()
    }
}

fn client_with_headers(headers: &[(&str, &str)]) -> reqwest::Client {
    let mut header_map = HeaderMap::new();
    for (name, value) in headers {
        header_map.insert(
            HeaderName::from_bytes(name.as_bytes()).expect("valid header name"),
            HeaderValue::from_str(value).expect("valid header value"),
        );
    }
    reqwest::Client::builder()
        .default_headers(header_map)
        .build()
        .expect("reqwest client should build")
}

async fn call_status(port: u16, client: reqwest::Client) -> anyhow::Result<()> {
    let url = format!("http://127.0.0.1:{port}/mcp");
    let transport = StreamableHttpClientTransport::with_client(
        client,
        rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig::with_uri(url),
    );
    let service = ().serve(transport).await?;
    service
        .call_tool(
            CallToolRequestParams::new("soma")
                .with_arguments(json!({"action": "status"}).as_object().unwrap().clone()),
        )
        .await?;
    service.cancel().await?;
    Ok(())
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "current_thread")]
async fn off_mode_never_reports_http_trace_headers_present() -> anyhow::Result<()> {
    let _lock = tracing_test_lock();
    let capture = TracingCapture::start();

    let server = ServerHandle::spawn(soma::testing::loopback_state_with_mcp_config(
        McpConfig::default(),
    ))
    .await?;
    call_status(
        server.port(),
        client_with_headers(&[("traceparent", VALID_TRACEPARENT)]),
    )
    .await?;

    let logs = capture.finish();
    assert!(
        logs.contains("http_trace_headers_present=false"),
        "logs were: {logs}"
    );
    Ok(())
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "current_thread")]
async fn trusted_mode_extracts_traceparent_from_a_real_http_request() -> anyhow::Result<()> {
    let _lock = tracing_test_lock();
    let capture = TracingCapture::start();

    let config = McpConfig {
        trace_headers: TraceHeaderMode::Trusted,
        ..McpConfig::default()
    };
    let server = ServerHandle::spawn(soma::testing::loopback_state_with_mcp_config(config)).await?;
    call_status(
        server.port(),
        client_with_headers(&[
            ("traceparent", VALID_TRACEPARENT),
            ("baggage", "region=us-east-1"),
        ]),
    )
    .await?;

    let logs = capture.finish();
    assert!(
        logs.contains("http_trace_headers_present=true"),
        "logs were: {logs}"
    );
    assert!(
        logs.contains("trace_id_prefix=Some(\"0af76519\")"),
        "logs were: {logs}"
    );
    assert!(logs.contains("baggage_member_count=0"), "logs were: {logs}");
    assert!(!logs.contains("us-east-1"), "raw baggage leaked: {logs}");
    Ok(())
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "current_thread")]
async fn trusted_with_baggage_mode_summarizes_baggage_without_leaking_raw_values(
) -> anyhow::Result<()> {
    let _lock = tracing_test_lock();
    let capture = TracingCapture::start();

    let config = McpConfig {
        trace_headers: TraceHeaderMode::TrustedWithBaggage,
        ..McpConfig::default()
    };
    let server = ServerHandle::spawn(soma::testing::loopback_state_with_mcp_config(config)).await?;
    call_status(
        server.port(),
        client_with_headers(&[
            ("traceparent", VALID_TRACEPARENT),
            ("baggage", "accessToken=super-secret-value"),
        ]),
    )
    .await?;

    let logs = capture.finish();
    assert!(logs.contains("baggage_member_count=1"), "logs were: {logs}");
    assert!(
        logs.contains("sensitive_baggage_member_count=1"),
        "logs were: {logs}"
    );
    assert!(
        !logs.contains("super-secret-value"),
        "raw baggage leaked: {logs}"
    );
    assert!(!logs.contains("accessToken"), "baggage key leaked: {logs}");
    Ok(())
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "current_thread")]
async fn trusted_gateway_unscoped_extracts_trace_headers_with_no_principal() -> anyhow::Result<()> {
    let _lock = tracing_test_lock();
    let capture = TracingCapture::start();

    let config = McpConfig {
        trace_headers: TraceHeaderMode::Trusted,
        ..McpConfig::default()
    };
    let server =
        ServerHandle::spawn(soma::testing::trusted_gateway_state_with_mcp_config(config)).await?;
    call_status(
        server.port(),
        client_with_headers(&[("traceparent", VALID_TRACEPARENT)]),
    )
    .await?;

    let logs = capture.finish();
    assert!(
        logs.contains("trace_id_prefix=Some(\"0af76519\")"),
        "logs were: {logs}"
    );
    Ok(())
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "current_thread")]
async fn mounted_auth_failure_never_emits_trace_fields_even_with_headers_present(
) -> anyhow::Result<()> {
    let _lock = tracing_test_lock();
    let capture = TracingCapture::start();

    let config = McpConfig {
        trace_headers: TraceHeaderMode::TrustedWithBaggage,
        ..McpConfig::default()
    };
    let server = ServerHandle::spawn(soma::testing::bearer_state_with_mcp_config(
        "expected-token",
        config,
    ))
    .await?;

    let response = client_with_headers(&[
        ("traceparent", VALID_TRACEPARENT),
        ("tracestate", "vendor=value"),
        ("baggage", "accessToken=super-secret-value"),
    ])
    .post(format!("http://127.0.0.1:{}/mcp", server.port()))
    .header("Content-Type", "application/json")
    .header("Accept", "application/json, text/event-stream")
    .body(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}"#)
    .send()
    .await?;
    assert_eq!(response.status(), 401);

    let logs = capture.finish();
    assert!(!logs.contains("trace_id_prefix"), "logs were: {logs}");
    assert!(!logs.contains("super-secret-value"), "logs were: {logs}");
    Ok(())
}
