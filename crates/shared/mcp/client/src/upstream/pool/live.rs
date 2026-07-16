use std::collections::BTreeMap;
use std::process::Stdio;
#[cfg(feature = "oauth")]
use std::sync::Arc;
use std::sync::Once;

use rmcp::model::{CallToolRequestParams, GetPromptRequestParams, ReadResourceRequestParams, Tool};
use rmcp::service::RunningService;
use rmcp::transport::{
    streamable_http_client::StreamableHttpClientTransportConfig, StreamableHttpClientTransport,
    TokioChildProcess,
};
use rmcp::{RoleClient, ServiceExt};
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
    CapScope, PromptDescriptor, ResourceDescriptor, ResponseCaps, ToolDescriptor, TransportKind,
    UpstreamError, UpstreamSnapshot,
};

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

pub(super) struct LiveUpstream {
    _service: RunningService<RoleClient, ()>,
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
    let (service, peer, kind) = if let Some(url) = config.url.as_deref() {
        match decide_http_transport(url) {
            HttpTransportDecision::WebSocket => connect_websocket(config, url).await?,
            HttpTransportDecision::Json | HttpTransportDecision::Sse => {
                connect_http(config, url, context).await?
            }
        }
    } else if let Some(command) = config.command.as_deref() {
        connect_stdio(config, command, guard).await?
    } else {
        return Err(UpstreamError::Unsupported {
            upstream: config.name.clone(),
            capability: "transport",
        });
    };

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

async fn connect_http(
    config: &UpstreamConfig,
    url: &str,
    context: LiveConnectContext<'_>,
) -> Result<
    (
        RunningService<RoleClient, ()>,
        rmcp::service::Peer<RoleClient>,
        LiveKind,
    ),
    UpstreamError,
> {
    ensure_rustls_crypto_provider();
    let transport_kind = match decide_http_transport(url) {
        HttpTransportDecision::Json => TransportKind::HttpJson,
        HttpTransportDecision::Sse => TransportKind::HttpSse,
        HttpTransportDecision::WebSocket => TransportKind::WebSocket,
    };
    let mut transport_config = StreamableHttpClientTransportConfig::with_uri(url.to_owned());
    #[cfg(feature = "oauth")]
    if config.oauth.is_some() {
        let oauth = context.oauth.ok_or_else(|| UpstreamError::LiveConnect {
            upstream: config.name.clone(),
            message: "oauth upstream requires subject-scoped connection context".to_owned(),
        })?;
        let client = BodyCappedHttpClient::default_with_caps(
            context.response_caps.limit_for(CapScope::HttpJson),
            context.response_caps.limit_for(CapScope::HttpSseEvent),
        );
        let auth_client = oauth
            .provider
            .authenticated_http_client(config, oauth.subject, client)
            .await
            .map_err(|error| UpstreamError::LiveConnect {
                upstream: config.name.clone(),
                message: error.to_string(),
            })?;
        let transport = StreamableHttpClientTransport::with_client(auth_client, transport_config);
        let service = ().serve(transport).await.map_err(|error| {
            UpstreamError::connect(config, format!("oauth http connect failed: {error}"))
        })?;
        let peer = service.peer().clone();
        return Ok((service, peer, LiveKind::Http(transport_kind)));
    }
    #[cfg(not(feature = "oauth"))]
    if config.oauth.is_some() {
        return Err(UpstreamError::LiveConnect {
            upstream: config.name.clone(),
            message: "oauth upstream support is not compiled into soma-mcp-client".to_owned(),
        });
    }
    if let Some(token) = bearer_token_from_env(config) {
        transport_config = transport_config.auth_header(token);
    }
    let client = BodyCappedHttpClient::default_with_caps(
        context.response_caps.limit_for(CapScope::HttpJson),
        context.response_caps.limit_for(CapScope::HttpSseEvent),
    );
    let transport = StreamableHttpClientTransport::with_client(client, transport_config);
    let service = ()
        .serve(transport)
        .await
        .map_err(|error| UpstreamError::connect(config, format!("http connect failed: {error}")))?;
    let peer = service.peer().clone();
    Ok((service, peer, LiveKind::Http(transport_kind)))
}

async fn connect_websocket(
    config: &UpstreamConfig,
    url: &str,
) -> Result<
    (
        RunningService<RoleClient, ()>,
        rmcp::service::Peer<RoleClient>,
        LiveKind,
    ),
    UpstreamError,
> {
    ensure_rustls_crypto_provider();
    let transport_config = WebSocketTransportConfig::new(url.to_owned())
        .with_authorization(websocket_authorization(config));
    let service =
        ().serve(connect_websocket_transport(transport_config))
            .await
            .map_err(|error| {
                UpstreamError::connect(config, format!("websocket connect failed: {error}"))
            })?;
    let peer = service.peer().clone();
    Ok((service, peer, LiveKind::WebSocket))
}

async fn connect_stdio(
    config: &UpstreamConfig,
    command: &str,
    guard: &SpawnGuard,
) -> Result<
    (
        RunningService<RoleClient, ()>,
        rmcp::service::Peer<RoleClient>,
        LiveKind,
    ),
    UpstreamError,
> {
    let spec = crate::upstream::pool::connect_stdio::plan_stdio_connection(config, guard).map_err(
        |error| UpstreamError::LiveConnect {
            upstream: config.name.clone(),
            message: error.to_string(),
        },
    )?;
    let mut cmd = Command::new(command);
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
        .map_err(|error| UpstreamError::LiveConnect {
            upstream: config.name.clone(),
            message: format!("stdio spawn failed: {error}"),
        })?;
    drain_stderr(config.name.clone(), stderr);
    let service = ().serve(transport).await.map_err(|error| {
        UpstreamError::connect(config, format!("stdio initialize failed: {error}"))
    })?;
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
