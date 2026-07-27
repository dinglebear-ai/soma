use std::collections::BTreeMap;
use std::process::Stdio;
#[cfg(feature = "oauth")]
use std::sync::Arc;
use std::sync::Once;

use rmcp::model::{
    CallToolRequestParams, CallToolResponse, ClientCapabilities, ClientInfo,
    GetPromptRequestParams, GetPromptResponse, Implementation, ProtocolVersion,
    ReadResourceRequestParams, ReadResourceResponse, RequestMetaObject, Tool,
};
use rmcp::service::{ClientInitializeError, ClientServiceExt, RunningService};
use rmcp::transport::{
    streamable_http_client::StreamableHttpClientTransportConfig, StreamableHttpClientTransport,
    TokioChildProcess,
};
use rmcp::{ClientHandler, RoleClient};
use serde_json::{Map, Value};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use crate::config::UpstreamConfig;
#[cfg(feature = "oauth")]
use crate::oauth::UpstreamOAuthProvider;
use crate::process::guard::SpawnGuard;
use crate::upstream::http_body_cap::BodyCappedHttpClient;
use crate::upstream::http_client::{decide_http_transport, HttpTransportDecision};
use crate::upstream::transport::websocket::{
    connect as connect_websocket_transport, WebSocketTransportConfig,
};
use crate::upstream::{
    CapScope, McpRequestOutcome, McpRoundTrip, PromptDescriptor, ResourceDescriptor, ResponseCaps,
    ToolDescriptor, TransportKind, UpstreamError, UpstreamSnapshot,
};

use super::lifecycle_compat::{compatibility_retry, log_fallback, LifecycleAttempt};

#[derive(Clone, Copy, Debug, Default)]
struct UpstreamClientHandler;

#[derive(Clone, Debug)]
struct RequestScopedClientHandler {
    info: ClientInfo,
}

impl ClientHandler for RequestScopedClientHandler {
    fn get_info(&self) -> ClientInfo {
        self.info.clone()
    }
}

impl ClientHandler for UpstreamClientHandler {
    fn get_info(&self) -> ClientInfo {
        ClientInfo::new(
            ClientCapabilities::builder().enable_tasks().build(),
            Implementation::new("soma-upstream-client", env!("CARGO_PKG_VERSION")),
        )
        .with_protocol_version(ProtocolVersion::LATEST)
    }
}

enum ConnectAttemptError {
    Fatal(UpstreamError),
    Lifecycle(Box<ClientInitializeError>),
}

impl ConnectAttemptError {
    fn into_upstream(self, config: &UpstreamConfig, prefix: &str) -> UpstreamError {
        match self {
            Self::Fatal(error) => error,
            Self::Lifecycle(error) => UpstreamError::connect(config, format!("{prefix}: {error}")),
        }
    }
}

#[derive(Clone)]
pub(super) struct LiveConnectContext<'a> {
    response_caps: &'a ResponseCaps,
    #[cfg(feature = "oauth")]
    oauth: Option<LiveOauthContext<'a>>,
}

#[cfg(feature = "oauth")]
#[derive(Clone)]
pub(super) struct LiveOauthContext<'a> {
    pub subject: &'a str,
    pub provider: Arc<dyn UpstreamOAuthProvider>,
}

impl<'a> LiveConnectContext<'a> {
    pub(super) fn shared(response_caps: &'a ResponseCaps) -> Self {
        Self {
            response_caps,
            #[cfg(feature = "oauth")]
            oauth: None,
        }
    }

    #[cfg(feature = "oauth")]
    pub(super) fn oauth(
        response_caps: &'a ResponseCaps,
        subject: &'a str,
        provider: Arc<dyn UpstreamOAuthProvider>,
    ) -> Self {
        Self {
            response_caps,
            oauth: Some(LiveOauthContext { subject, provider }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LiveKind {
    Http(TransportKind),
    Stdio,
    WebSocket,
}

type LiveConnectionFor<H> = (
    RunningService<RoleClient, H>,
    rmcp::service::Peer<RoleClient>,
    LiveKind,
);

pub(super) struct LiveUpstream {
    _service: RunningService<RoleClient, UpstreamClientHandler>,
    peer: rmcp::service::Peer<RoleClient>,
}

impl LiveUpstream {
    pub(super) fn peer(&self) -> rmcp::service::Peer<RoleClient> {
        self.peer.clone()
    }
}

pub(super) async fn connect_live(
    config: &UpstreamConfig,
    guard: &SpawnGuard,
    context: LiveConnectContext<'_>,
) -> Result<(LiveUpstream, UpstreamSnapshot), UpstreamError> {
    let (service, peer, kind) =
        connect_with_handler(config, guard, context, UpstreamClientHandler).await?;

    let tools = peer
        .list_all_tools()
        .await
        .map_err(|error| UpstreamError::connect(config, error))?;
    let resources = if config.proxy_resources {
        list_resources_or_empty(config, &peer).await?
    } else {
        Vec::new()
    };
    let prompts = if config.proxy_prompts {
        list_prompts_or_empty(config, &peer).await?
    } else {
        Vec::new()
    };

    let mut snapshot = UpstreamSnapshot::empty(
        config.name.clone(),
        match kind {
            LiveKind::Http(transport) => transport,
            LiveKind::Stdio => TransportKind::Stdio,
            LiveKind::WebSocket => TransportKind::WebSocket,
        },
    );
    snapshot.tools = tools.into_iter().map(tool_descriptor).collect();
    snapshot.resources = resources.into_iter().map(resource_descriptor).collect();
    snapshot.prompts = prompts.into_iter().map(prompt_descriptor).collect();
    Ok((
        LiveUpstream {
            _service: service,
            peer,
        },
        snapshot,
    ))
}

async fn connect_with_handler<H>(
    config: &UpstreamConfig,
    guard: &SpawnGuard,
    context: LiveConnectContext<'_>,
    handler: H,
) -> Result<LiveConnectionFor<H>, UpstreamError>
where
    H: ClientHandler + Clone,
{
    if let Some(url) = config.url.as_deref() {
        return match decide_http_transport(url) {
            HttpTransportDecision::WebSocket => {
                connect_websocket_with_handler(config, url, handler).await
            }
            HttpTransportDecision::Json | HttpTransportDecision::Sse => {
                connect_http_with_handler(config, url, context, handler).await
            }
        };
    }
    if let Some(command) = config.command.as_deref() {
        return connect_stdio_with_handler(config, command, guard, handler).await;
    }
    Err(UpstreamError::Unsupported {
        upstream: config.name.clone(),
        capability: "transport",
    })
}

pub(super) async fn call_live_tool(
    upstream: &str,
    peer: rmcp::service::Peer<RoleClient>,
    tool: String,
    params: Value,
) -> Result<Value, UpstreamError> {
    let Value::Object(args) = params else {
        return Err(UpstreamError::ParamsMustBeObject);
    };
    let result = peer
        .call_tool(CallToolRequestParams::new(tool).with_arguments(args))
        .await
        .map_err(|error| UpstreamError::LiveCall {
            upstream: upstream.to_owned(),
            operation: "tools/call",
            message: error.to_string(),
        })?;
    if let Some(value) = result.structured_content.clone() {
        return Ok(value);
    }
    Ok(serde_json::to_value(result).unwrap_or(Value::Null))
}

pub(super) async fn call_live_tool_once(
    upstream: &str,
    peer: rmcp::service::Peer<RoleClient>,
    tool: String,
    params: Value,
    round_trip: McpRoundTrip,
) -> Result<McpRequestOutcome, UpstreamError> {
    let Value::Object(args) = params else {
        return Err(UpstreamError::ParamsMustBeObject);
    };
    let mut request = CallToolRequestParams::new(tool).with_arguments(args);
    request.input_responses = round_trip.input_responses;
    request.request_state = round_trip.request_state;
    let response = peer
        .call_tool_once(request)
        .await
        .map_err(|error| UpstreamError::LiveCall {
            upstream: upstream.to_owned(),
            operation: "tools/call",
            message: error.to_string(),
        })?;
    match response {
        CallToolResponse::Complete(result) => {
            serialize_live_outcome(upstream, "tools/call", result, McpRequestOutcome::Complete)
        }
        CallToolResponse::InputRequired(result) => serialize_live_outcome(
            upstream,
            "tools/call",
            result,
            McpRequestOutcome::InputRequired,
        ),
        CallToolResponse::Task(result) => {
            serialize_live_outcome(upstream, "tools/call", result, McpRequestOutcome::Task)
        }
        _ => Err(unsupported_live_outcome(upstream, "tools/call")),
    }
}

pub(super) async fn call_live_tool_once_scoped(
    config: &UpstreamConfig,
    context: LiveConnectContext<'_>,
    tool: String,
    params: Value,
    mut round_trip: McpRoundTrip,
) -> Result<McpRequestOutcome, UpstreamError> {
    let meta = round_trip
        .request_meta
        .take()
        .ok_or_else(|| UpstreamError::LiveCall {
            upstream: config.name.clone(),
            operation: "tools/call",
            message: "request-scoped relay requires downstream MCP metadata".to_owned(),
        })?;
    let handler = request_scoped_handler(&config.name, "tools/call", meta)?;
    let (mut service, peer, _) =
        connect_with_handler(config, &SpawnGuard::default(), context, handler).await?;
    let outcome = call_live_tool_once(
        &config.name,
        peer,
        tool,
        params,
        McpRoundTrip {
            input_responses: round_trip.input_responses,
            request_state: round_trip.request_state,
            request_meta: None,
        },
    )
    .await;
    if let Err(error) = service.close().await {
        tracing::debug!(
            upstream = %config.name,
            error = %error,
            "request-scoped upstream relay did not close cleanly"
        );
    }
    outcome
}

pub(super) async fn read_live_resource(
    upstream: &str,
    peer: rmcp::service::Peer<RoleClient>,
    uri: String,
) -> Result<Value, UpstreamError> {
    let result = peer
        .read_resource(ReadResourceRequestParams::new(uri))
        .await
        .map_err(|error| UpstreamError::LiveCall {
            upstream: upstream.to_owned(),
            operation: "resources/read",
            message: error.to_string(),
        })?;
    serde_json::to_value(result).map_err(|error| UpstreamError::LiveCall {
        upstream: upstream.to_owned(),
        operation: "resources/read",
        message: error.to_string(),
    })
}

pub(super) async fn read_live_resource_once(
    upstream: &str,
    peer: rmcp::service::Peer<RoleClient>,
    uri: String,
    round_trip: McpRoundTrip,
) -> Result<McpRequestOutcome, UpstreamError> {
    let mut request = ReadResourceRequestParams::new(uri);
    request.input_responses = round_trip.input_responses;
    request.request_state = round_trip.request_state;
    let response =
        peer.read_resource_once(request)
            .await
            .map_err(|error| UpstreamError::LiveCall {
                upstream: upstream.to_owned(),
                operation: "resources/read",
                message: error.to_string(),
            })?;
    match response {
        ReadResourceResponse::Complete(result) => serialize_live_outcome(
            upstream,
            "resources/read",
            result,
            McpRequestOutcome::Complete,
        ),
        ReadResourceResponse::InputRequired(result) => serialize_live_outcome(
            upstream,
            "resources/read",
            result,
            McpRequestOutcome::InputRequired,
        ),
        _ => Err(unsupported_live_outcome(upstream, "resources/read")),
    }
}

pub(super) async fn get_live_prompt(
    upstream: &str,
    peer: rmcp::service::Peer<RoleClient>,
    name: String,
    arguments: Option<Map<String, Value>>,
) -> Result<Value, UpstreamError> {
    let mut params = GetPromptRequestParams::new(name);
    params.arguments = arguments;
    let result = peer
        .get_prompt(params)
        .await
        .map_err(|error| UpstreamError::LiveCall {
            upstream: upstream.to_owned(),
            operation: "prompts/get",
            message: error.to_string(),
        })?;
    serde_json::to_value(result).map_err(|error| UpstreamError::LiveCall {
        upstream: upstream.to_owned(),
        operation: "prompts/get",
        message: error.to_string(),
    })
}

pub(super) async fn get_live_prompt_once(
    upstream: &str,
    peer: rmcp::service::Peer<RoleClient>,
    name: String,
    arguments: Option<Map<String, Value>>,
    round_trip: McpRoundTrip,
) -> Result<McpRequestOutcome, UpstreamError> {
    let mut request = GetPromptRequestParams::new(name);
    request.arguments = arguments;
    request.input_responses = round_trip.input_responses;
    request.request_state = round_trip.request_state;
    let response =
        peer.get_prompt_once(request)
            .await
            .map_err(|error| UpstreamError::LiveCall {
                upstream: upstream.to_owned(),
                operation: "prompts/get",
                message: error.to_string(),
            })?;
    match response {
        GetPromptResponse::Complete(result) => {
            serialize_live_outcome(upstream, "prompts/get", result, McpRequestOutcome::Complete)
        }
        GetPromptResponse::InputRequired(result) => serialize_live_outcome(
            upstream,
            "prompts/get",
            result,
            McpRequestOutcome::InputRequired,
        ),
        _ => Err(unsupported_live_outcome(upstream, "prompts/get")),
    }
}

fn request_scoped_handler(
    upstream: &str,
    operation: &'static str,
    value: Value,
) -> Result<RequestScopedClientHandler, UpstreamError> {
    let meta: RequestMetaObject =
        serde_json::from_value(value).map_err(|error| UpstreamError::LiveCall {
            upstream: upstream.to_owned(),
            operation,
            message: format!("invalid downstream MCP request metadata: {error}"),
        })?;
    let protocol_version = meta.protocol_version().unwrap_or(ProtocolVersion::LATEST);
    let implementation = meta.client_info().unwrap_or_else(|| {
        Implementation::new("soma-request-scoped-relay", env!("CARGO_PKG_VERSION"))
    });
    let capabilities = meta.client_capabilities().unwrap_or_default();
    Ok(RequestScopedClientHandler {
        info: ClientInfo::new(capabilities, implementation).with_protocol_version(protocol_version),
    })
}

fn unsupported_live_outcome(upstream: &str, operation: &'static str) -> UpstreamError {
    UpstreamError::LiveCall {
        upstream: upstream.to_owned(),
        operation,
        message: "upstream returned an unsupported MCP response variant".to_owned(),
    }
}

fn serialize_live_outcome<T: serde::Serialize>(
    upstream: &str,
    operation: &'static str,
    result: T,
    wrap: impl FnOnce(Value) -> McpRequestOutcome,
) -> Result<McpRequestOutcome, UpstreamError> {
    serde_json::to_value(result)
        .map(wrap)
        .map_err(|error| UpstreamError::LiveCall {
            upstream: upstream.to_owned(),
            operation,
            message: error.to_string(),
        })
}

async fn connect_http_with_handler<H>(
    config: &UpstreamConfig,
    url: &str,
    context: LiveConnectContext<'_>,
    handler: H,
) -> Result<LiveConnectionFor<H>, UpstreamError>
where
    H: ClientHandler + Clone,
{
    match connect_http_once(
        config,
        url,
        context.clone(),
        LifecycleAttempt::Modern,
        handler.clone(),
    )
    .await
    {
        Ok(connection) => Ok(connection),
        Err(ConnectAttemptError::Fatal(error)) => Err(error),
        Err(ConnectAttemptError::Lifecycle(error)) => {
            let Some(attempt) = compatibility_retry(&error) else {
                return Err(UpstreamError::connect(
                    config,
                    format!("http connect failed: {error}"),
                ));
            };
            log_fallback(&config.name, "http", attempt, &error);
            connect_http_once(config, url, context, attempt, handler)
                .await
                .map_err(|error| error.into_upstream(config, "http connect failed"))
        }
    }
}

async fn connect_http_once<H>(
    config: &UpstreamConfig,
    url: &str,
    context: LiveConnectContext<'_>,
    lifecycle: LifecycleAttempt,
    handler: H,
) -> Result<LiveConnectionFor<H>, ConnectAttemptError>
where
    H: ClientHandler + Clone,
{
    ensure_rustls_crypto_provider();
    let transport_kind = match decide_http_transport(url) {
        HttpTransportDecision::Json => TransportKind::HttpJson,
        HttpTransportDecision::Sse => TransportKind::HttpSse,
        HttpTransportDecision::WebSocket => TransportKind::WebSocket,
    };
    let mut transport_config = StreamableHttpClientTransportConfig::with_uri(url.to_owned());
    #[cfg(feature = "oauth")]
    if config.oauth.is_some() {
        let oauth = context.oauth.ok_or_else(|| {
            ConnectAttemptError::Fatal(UpstreamError::LiveConnect {
                upstream: config.name.clone(),
                message: "oauth upstream requires subject-scoped connection context".to_owned(),
            })
        })?;
        let client = BodyCappedHttpClient::default_with_caps(
            context.response_caps.limit_for(CapScope::HttpJson),
            context.response_caps.limit_for(CapScope::HttpSseEvent),
        );
        let auth_client = oauth
            .provider
            .authenticated_http_client(config, oauth.subject, client)
            .await
            .map_err(|error| {
                ConnectAttemptError::Fatal(UpstreamError::LiveConnect {
                    upstream: config.name.clone(),
                    message: error.to_string(),
                })
            })?;
        let transport = StreamableHttpClientTransport::with_client(auth_client, transport_config);
        let service = handler
            .clone()
            .serve_with_lifecycle(transport, lifecycle.mode())
            .await
            .map_err(|error| ConnectAttemptError::Lifecycle(Box::new(error)))?;
        let peer = service.peer().clone();
        return Ok((service, peer, LiveKind::Http(transport_kind)));
    }
    #[cfg(not(feature = "oauth"))]
    if config.oauth.is_some() {
        return Err(ConnectAttemptError::Fatal(UpstreamError::LiveConnect {
            upstream: config.name.clone(),
            message: "oauth upstream support is not compiled into soma-mcp-client".to_owned(),
        }));
    }
    if let Some(token) = bearer_token_from_env(config) {
        transport_config = transport_config.auth_header(token);
    }
    let client = BodyCappedHttpClient::default_with_caps(
        context.response_caps.limit_for(CapScope::HttpJson),
        context.response_caps.limit_for(CapScope::HttpSseEvent),
    );
    let transport = StreamableHttpClientTransport::with_client(client, transport_config);
    let service = handler
        .serve_with_lifecycle(transport, lifecycle.mode())
        .await
        .map_err(|error| ConnectAttemptError::Lifecycle(Box::new(error)))?;
    let peer = service.peer().clone();
    Ok((service, peer, LiveKind::Http(transport_kind)))
}

async fn connect_websocket_with_handler<H>(
    config: &UpstreamConfig,
    url: &str,
    handler: H,
) -> Result<LiveConnectionFor<H>, UpstreamError>
where
    H: ClientHandler + Clone,
{
    match connect_websocket_once(config, url, LifecycleAttempt::Modern, handler.clone()).await {
        Ok(connection) => Ok(connection),
        Err(error) => {
            let Some(attempt) = compatibility_retry(&error) else {
                return Err(UpstreamError::connect(
                    config,
                    format!("websocket connect failed: {error}"),
                ));
            };
            log_fallback(&config.name, "websocket", attempt, &error);
            connect_websocket_once(config, url, attempt, handler)
                .await
                .map_err(|error| {
                    UpstreamError::connect(config, format!("websocket connect failed: {error}"))
                })
        }
    }
}

async fn connect_websocket_once<H>(
    config: &UpstreamConfig,
    url: &str,
    lifecycle: LifecycleAttempt,
    handler: H,
) -> Result<LiveConnectionFor<H>, ClientInitializeError>
where
    H: ClientHandler,
{
    ensure_rustls_crypto_provider();
    let transport_config = WebSocketTransportConfig::new(url.to_owned())
        .with_authorization(websocket_authorization(config));
    let service = handler
        .serve_with_lifecycle(
            connect_websocket_transport(transport_config),
            lifecycle.mode(),
        )
        .await?;
    let peer = service.peer().clone();
    Ok((service, peer, LiveKind::WebSocket))
}

async fn connect_stdio_with_handler<H>(
    config: &UpstreamConfig,
    command: &str,
    guard: &SpawnGuard,
    handler: H,
) -> Result<LiveConnectionFor<H>, UpstreamError>
where
    H: ClientHandler + Clone,
{
    match connect_stdio_once(
        config,
        command,
        guard,
        LifecycleAttempt::Modern,
        handler.clone(),
    )
    .await
    {
        Ok(connection) => Ok(connection),
        Err(ConnectAttemptError::Fatal(error)) => Err(error),
        Err(ConnectAttemptError::Lifecycle(error)) => {
            let Some(attempt) = compatibility_retry(&error) else {
                return Err(UpstreamError::connect(
                    config,
                    format!("stdio MCP handshake failed: {error}"),
                ));
            };
            log_fallback(&config.name, "stdio", attempt, &error);
            connect_stdio_once(config, command, guard, attempt, handler)
                .await
                .map_err(|error| error.into_upstream(config, "stdio MCP handshake failed"))
        }
    }
}

async fn connect_stdio_once<H>(
    config: &UpstreamConfig,
    command_name: &str,
    guard: &SpawnGuard,
    lifecycle: LifecycleAttempt,
    handler: H,
) -> Result<LiveConnectionFor<H>, ConnectAttemptError>
where
    H: ClientHandler,
{
    let spec = crate::upstream::pool::connect_stdio::plan_stdio_connection(config, guard).map_err(
        |error| {
            ConnectAttemptError::Fatal(UpstreamError::LiveConnect {
                upstream: config.name.clone(),
                message: error.to_string(),
            })
        },
    )?;
    let mut cmd = Command::new(command_name);
    cmd.args(&spec.args)
        .env_clear()
        .envs(stdio_env())
        .envs(spec.env.iter())
        .stderr(Stdio::piped());
    if let Some(env_name) = config.bearer_token_env.as_deref() {
        if let Ok(token) = std::env::var(env_name) {
            cmd.env(env_name, token);
        }
    }

    #[cfg(unix)]
    let command = {
        use process_wrap::tokio::{CommandWrap, ProcessGroup};
        let mut wrapped = CommandWrap::from(cmd);
        wrapped.wrap(ProcessGroup::leader());
        wrapped
    };
    #[cfg(not(unix))]
    let command = cmd;

    let (transport, stderr) = TokioChildProcess::builder(command)
        .spawn()
        .map_err(|error| {
            ConnectAttemptError::Fatal(UpstreamError::LiveConnect {
                upstream: config.name.clone(),
                message: format!("stdio spawn failed: {error}"),
            })
        })?;
    drain_stderr(config.name.clone(), stderr);
    let service = handler
        .serve_with_lifecycle(transport, lifecycle.mode())
        .await
        .map_err(|error| ConnectAttemptError::Lifecycle(Box::new(error)))?;
    let peer = service.peer().clone();
    Ok((service, peer, LiveKind::Stdio))
}

async fn list_resources_or_empty(
    config: &UpstreamConfig,
    peer: &rmcp::service::Peer<RoleClient>,
) -> Result<Vec<rmcp::model::Resource>, UpstreamError> {
    match peer.list_all_resources().await {
        Ok(resources) => Ok(resources),
        Err(error) if capability_is_absent(&error.to_string()) => Ok(Vec::new()),
        Err(error) => Err(UpstreamError::LiveConnect {
            upstream: config.name.clone(),
            message: format!("resources/list failed: {error}"),
        }),
    }
}

async fn list_prompts_or_empty(
    config: &UpstreamConfig,
    peer: &rmcp::service::Peer<RoleClient>,
) -> Result<Vec<rmcp::model::Prompt>, UpstreamError> {
    match peer.list_all_prompts().await {
        Ok(prompts) => Ok(prompts),
        Err(error) if capability_is_absent(&error.to_string()) => Ok(Vec::new()),
        Err(error) => Err(UpstreamError::LiveConnect {
            upstream: config.name.clone(),
            message: format!("prompts/list failed: {error}"),
        }),
    }
}

fn tool_descriptor(tool: Tool) -> ToolDescriptor {
    ToolDescriptor {
        name: tool.name.to_string(),
        description: tool.description.map(|value| value.to_string()),
        input_schema: Some(Value::Object((*tool.input_schema).clone())),
        output_schema: tool
            .output_schema
            .map(|schema| Value::Object((*schema).clone())),
        destructive: tool
            .annotations
            .as_ref()
            .and_then(|annotations| annotations.destructive_hint)
            .unwrap_or(true),
    }
}

fn resource_descriptor(resource: rmcp::model::Resource) -> ResourceDescriptor {
    ResourceDescriptor {
        uri: resource.uri,
        name: Some(resource.name),
    }
}

fn prompt_descriptor(prompt: rmcp::model::Prompt) -> PromptDescriptor {
    PromptDescriptor {
        name: prompt.name,
        description: prompt.description,
    }
}

fn normalize_bearer_value(token: &str) -> String {
    token
        .trim()
        .strip_prefix("Bearer ")
        .unwrap_or_else(|| token.trim())
        .to_owned()
}

fn websocket_authorization(config: &UpstreamConfig) -> Option<String> {
    bearer_token_from_env(config).map(|token| format!("Bearer {token}"))
}

fn bearer_token_from_env(config: &UpstreamConfig) -> Option<String> {
    let env_name = config.bearer_token_env.as_deref()?;
    let token = std::env::var(env_name).ok()?;
    let token = normalize_bearer_value(&token);
    (!token.is_empty()).then_some(token)
}

fn capability_is_absent(error: &str) -> bool {
    error.contains("-32601")
        || error.contains("Method not found")
        || error.contains("method not found")
}

fn ensure_rustls_crypto_provider() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

fn stdio_env() -> BTreeMap<String, String> {
    const ALLOWLIST: &[&str] = &[
        "PATH",
        "HOME",
        "USER",
        "LOGNAME",
        "TERM",
        "TZ",
        "TMPDIR",
        "TMP",
        "TEMP",
        "LANG",
        "LC_ALL",
        "XDG_CACHE_HOME",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "SSL_CERT_FILE",
        "SSL_CERT_DIR",
        "NODE_EXTRA_CA_CERTS",
        "REQUESTS_CA_BUNDLE",
        "CURL_CA_BUNDLE",
    ];
    ALLOWLIST
        .iter()
        .filter_map(|key| {
            std::env::var(key)
                .ok()
                .map(|value| ((*key).to_owned(), value))
        })
        .collect()
}

fn drain_stderr(upstream: String, stderr: Option<tokio::process::ChildStderr>) {
    let Some(mut stderr) = stderr else {
        return;
    };
    tokio::spawn(async move {
        let mut bytes = Vec::new();
        if stderr.read_to_end(&mut bytes).await.is_ok() && !bytes.is_empty() {
            tracing::debug!(
                upstream,
                stderr = %String::from_utf8_lossy(&bytes),
                "upstream stdio stderr"
            );
        }
    });
}

#[cfg(test)]
#[path = "live_tests.rs"]
mod tests;
