use std::{collections::HashMap, process::Stdio, sync::Arc, time::Duration};

use async_trait::async_trait;
use reqwest::header::{HeaderName, HeaderValue};
use rmcp::{
    model::CallToolRequestParams,
    transport::{
        streamable_http_client::StreamableHttpClientTransportConfig, ConfigureCommandExt,
        StreamableHttpClientTransport, TokioChildProcess,
    },
    ServiceExt,
};
use serde_json::{json, Map, Value};
use soma_contracts::providers::{ProviderCatalog, ProviderTool};
use tokio::process::Command;

use crate::{
    provider_errors::ProviderError,
    provider_registry::{Provider, ProviderCall, ProviderOutput},
};

#[derive(Clone)]
pub struct McpProvider {
    catalog: ProviderCatalog,
}

impl McpProvider {
    pub fn new(catalog: ProviderCatalog) -> Self {
        Self { catalog }
    }

    pub fn arc(catalog: ProviderCatalog) -> Arc<Self> {
        Arc::new(Self::new(catalog))
    }
}

#[async_trait]
impl Provider for McpProvider {
    fn catalog(&self) -> ProviderCatalog {
        self.catalog.clone()
    }

    async fn call(&self, call: ProviderCall) -> Result<ProviderOutput, ProviderError> {
        let runtime = McpRuntime::from_catalog(&self.catalog, &call)?;
        let tool = self.tool(&call)?;
        let upstream = UpstreamTool::from_tool(tool, &call);
        let params = upstream.params(call.params.clone());
        let timeout = runtime.timeout();

        let fut = async {
            match &runtime.transport {
                McpTransport::Stdio(stdio) => {
                    call_stdio(&self.catalog, &call, stdio, &upstream, params).await
                }
                McpTransport::Http(http) => {
                    call_http(&self.catalog, &call, http, &upstream, params).await
                }
            }
        };

        let result = tokio::time::timeout(timeout, fut).await.map_err(|_| {
            ProviderError::new(
                "mcp_provider_timeout",
                &self.catalog.provider.name,
                Some(call.action.clone()),
                format!("upstream MCP tool call exceeded {}ms", timeout.as_millis()),
                "Increase provider.meta.mcp.timeout_ms or fix the upstream MCP server.",
            )
        })??;

        Ok(ProviderOutput::json(
            result
                .structured_content
                .unwrap_or_else(|| json!({ "content": result.content })),
        ))
    }
}

impl McpProvider {
    fn tool(&self, call: &ProviderCall) -> Result<&ProviderTool, ProviderError> {
        self.catalog
            .tools
            .iter()
            .find(|tool| tool.name == call.action)
            .ok_or_else(|| {
                ProviderError::validation(
                    &self.catalog.provider.name,
                    &call.action,
                    "unknown_mcp_action",
                    format!("MCP provider has no action `{}`", call.action),
                )
            })
    }
}

async fn call_stdio(
    catalog: &ProviderCatalog,
    call: &ProviderCall,
    runtime: &McpStdioRuntime,
    upstream: &UpstreamTool,
    params: Map<String, Value>,
) -> Result<rmcp::model::CallToolResult, ProviderError> {
    let (transport, _stderr) =
        TokioChildProcess::builder(Command::new(&runtime.command).configure(|cmd| {
            cmd.args(&runtime.args)
                .env_clear()
                .envs(runtime.env.iter().map(|(key, value)| (key, value)))
                .stderr(Stdio::null());
            if let Some(cwd) = &runtime.cwd {
                cmd.current_dir(cwd);
            }
        }))
        .spawn()
        .map_err(|error| {
            ProviderError::execution(&catalog.provider.name, call.action.clone(), error)
        })?;
    let service = ().serve(transport).await.map_err(|error| {
        ProviderError::execution(&catalog.provider.name, call.action.clone(), error)
    })?;
    let result = service
        .call_tool(CallToolRequestParams::new(upstream.name.clone()).with_arguments(params))
        .await
        .map_err(|error| {
            ProviderError::execution(&catalog.provider.name, call.action.clone(), error)
        });
    let _ = service.cancel().await;
    result
}

/// rmcp's streamable HTTP transport (reqwest 0.13) panics when the process has
/// no rustls crypto provider installed; install ring once, tolerating a
/// provider some embedder installed earlier.
fn ensure_rustls_crypto_provider() {
    static INSTALL: std::sync::Once = std::sync::Once::new();
    INSTALL.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

async fn call_http(
    catalog: &ProviderCatalog,
    call: &ProviderCall,
    runtime: &McpHttpRuntime,
    upstream: &UpstreamTool,
    params: Map<String, Value>,
) -> Result<rmcp::model::CallToolResult, ProviderError> {
    ensure_rustls_crypto_provider();
    let mut config = StreamableHttpClientTransportConfig::with_uri(runtime.url.clone());
    if !runtime.headers.is_empty() {
        config = config.custom_headers(runtime.headers.clone());
    }
    let transport = StreamableHttpClientTransport::from_config(config);
    let service = ().serve(transport).await.map_err(|error| {
        ProviderError::execution(&catalog.provider.name, call.action.clone(), error)
    })?;
    let result = service
        .call_tool(CallToolRequestParams::new(upstream.name.clone()).with_arguments(params))
        .await
        .map_err(|error| {
            ProviderError::execution(&catalog.provider.name, call.action.clone(), error)
        });
    let _ = service.cancel().await;
    result
}

struct McpRuntime {
    transport: McpTransport,
    timeout_ms: u64,
}

enum McpTransportKind {
    Stdio,
    Http,
}

enum McpTransport {
    Stdio(McpStdioRuntime),
    Http(McpHttpRuntime),
}

struct McpStdioRuntime {
    command: String,
    args: Vec<String>,
    cwd: Option<String>,
    env: Vec<(String, String)>,
}

struct McpHttpRuntime {
    url: String,
    headers: HashMap<HeaderName, HeaderValue>,
}

impl McpRuntime {
    fn from_catalog(catalog: &ProviderCatalog, call: &ProviderCall) -> Result<Self, ProviderError> {
        let meta = catalog
            .meta
            .get("mcp")
            .or_else(|| catalog.meta.get("runtime"))
            .ok_or_else(|| {
                ProviderError::validation(
                    &catalog.provider.name,
                    &call.action,
                    "missing_mcp_runtime",
                    "MCP provider requires provider.meta.mcp runtime config",
                )
            })?;
        let timeout_ms = meta
            .get("timeout_ms")
            .and_then(Value::as_u64)
            .unwrap_or(10_000);
        let transport = transport_kind(meta).map_err(|message| {
            ProviderError::validation(
                &catalog.provider.name,
                &call.action,
                "invalid_mcp_transport",
                message,
            )
        })?;
        match transport {
            McpTransportKind::Http => Ok(Self {
                transport: McpTransport::Http(McpHttpRuntime::from_meta(meta, catalog, call)?),
                timeout_ms,
            }),
            McpTransportKind::Stdio => {
                let stdio = meta.get("stdio").unwrap_or(meta);
                let timeout_ms = meta
                    .get("timeout_ms")
                    .or_else(|| stdio.get("timeout_ms"))
                    .and_then(Value::as_u64)
                    .unwrap_or(timeout_ms);
                Ok(Self {
                    transport: McpTransport::Stdio(McpStdioRuntime::from_meta(
                        stdio, catalog, call,
                    )?),
                    timeout_ms,
                })
            }
        }
    }

    fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout_ms)
    }
}

impl McpStdioRuntime {
    fn from_meta(
        stdio: &Value,
        catalog: &ProviderCatalog,
        call: &ProviderCall,
    ) -> Result<Self, ProviderError> {
        let command = stdio
            .get("command")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                ProviderError::validation(
                    &catalog.provider.name,
                    &call.action,
                    "missing_mcp_command",
                    "MCP provider stdio runtime requires command",
                )
            })?
            .to_owned();
        let args = string_array(stdio.get("args")).map_err(|message| {
            ProviderError::validation(
                &catalog.provider.name,
                &call.action,
                "invalid_mcp_args",
                message,
            )
        })?;
        let cwd = stdio
            .get("cwd")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let env = env_pairs(stdio.get("env")).map_err(|message| {
            ProviderError::validation(
                &catalog.provider.name,
                &call.action,
                "invalid_mcp_env",
                message,
            )
        })?;
        Ok(Self {
            command,
            args,
            cwd,
            env,
        })
    }
}

impl McpHttpRuntime {
    fn from_meta(
        meta: &Value,
        catalog: &ProviderCatalog,
        call: &ProviderCall,
    ) -> Result<Self, ProviderError> {
        let http = meta.get("http").unwrap_or(meta);
        let url = http
            .get("url")
            .or_else(|| meta.get("url"))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                ProviderError::validation(
                    &catalog.provider.name,
                    &call.action,
                    "missing_mcp_url",
                    "MCP provider HTTP runtime requires url",
                )
            })?
            .to_owned();
        validate_http_url(&url).map_err(|message| {
            ProviderError::validation(
                &catalog.provider.name,
                &call.action,
                "invalid_mcp_url",
                message,
            )
        })?;
        let headers = header_pairs(http.get("headers").or_else(|| meta.get("headers"))).map_err(
            |message| {
                ProviderError::validation(
                    &catalog.provider.name,
                    &call.action,
                    "invalid_mcp_headers",
                    message,
                )
            },
        )?;
        Ok(Self { url, headers })
    }
}

fn transport_kind(meta: &Value) -> Result<McpTransportKind, String> {
    let explicit = meta
        .get("transport")
        .and_then(Value::as_str)
        .map(|value| value.to_ascii_lowercase());
    let has_url = meta.get("url").and_then(Value::as_str).is_some()
        || meta
            .get("http")
            .and_then(|http| http.get("url"))
            .and_then(Value::as_str)
            .is_some();
    let has_stdio =
        meta.get("stdio").is_some() || meta.get("command").and_then(Value::as_str).is_some();

    match explicit.as_deref() {
        Some("http" | "streamable-http" | "streamable_http") => {
            if !has_url {
                return Err("transport=http requires url".to_owned());
            }
            Ok(McpTransportKind::Http)
        }
        Some("stdio") => {
            if !has_stdio {
                return Err("transport=stdio requires stdio.command or command".to_owned());
            }
            Ok(McpTransportKind::Stdio)
        }
        Some(other) => Err(format!("unsupported MCP transport `{other}`")),
        None if has_url => Ok(McpTransportKind::Http),
        None => Ok(McpTransportKind::Stdio),
    }
}

fn validate_http_url(value: &str) -> Result<(), String> {
    let parsed = url::Url::parse(value).map_err(|error| format!("url is invalid: {error}"))?;
    match parsed.scheme() {
        "http" | "https" => {}
        scheme => return Err(format!("url scheme `{scheme}` is not supported")),
    }
    if parsed.host_str().is_none() {
        return Err("url must include a host".to_owned());
    }
    Ok(())
}

struct UpstreamTool {
    name: String,
    static_args: Map<String, Value>,
}

impl UpstreamTool {
    fn from_tool(tool: &ProviderTool, call: &ProviderCall) -> Self {
        let meta = tool.meta.get("mcp");
        let name = meta
            .and_then(|value| value.get("upstream_tool"))
            .and_then(Value::as_str)
            .unwrap_or(&tool.name)
            .to_owned();
        let static_args = meta
            .and_then(|value| value.get("static_args"))
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        tracing::debug!(
            provider_action = %call.action,
            upstream_tool = %name,
            "proxying MCP provider tool call"
        );
        Self { name, static_args }
    }

    fn params(&self, call_params: Value) -> Map<String, Value> {
        let mut params = self.static_args.clone();
        if let Value::Object(map) = call_params {
            params.extend(map);
        }
        params
    }
}

fn string_array(value: Option<&Value>) -> Result<Vec<String>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let Some(values) = value.as_array() else {
        return Err("args must be an array of strings".to_owned());
    };
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(ToOwned::to_owned)
                .ok_or_else(|| "args must be an array of strings".to_owned())
        })
        .collect()
}

fn env_pairs(value: Option<&Value>) -> Result<Vec<(String, String)>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let Some(values) = value.as_object() else {
        return Err("env must be an object of string values".to_owned());
    };
    values
        .iter()
        .map(|(key, value)| {
            value
                .as_str()
                .map(|value| (key.clone(), value.to_owned()))
                .ok_or_else(|| "env must be an object of string values".to_owned())
        })
        .collect()
}

fn header_pairs(value: Option<&Value>) -> Result<HashMap<HeaderName, HeaderValue>, String> {
    let Some(value) = value else {
        return Ok(HashMap::new());
    };
    let Some(values) = value.as_object() else {
        return Err("headers must be an object of string values".to_owned());
    };
    values
        .iter()
        .map(|(key, value)| {
            let name = HeaderName::from_bytes(key.as_bytes())
                .map_err(|error| format!("header `{key}` is invalid: {error}"))?;
            let raw_value = value
                .as_str()
                .ok_or_else(|| "headers must be an object of string values".to_owned())?;
            let expanded = expand_env_templates(raw_value)?;
            let header_value = HeaderValue::from_str(&expanded)
                .map_err(|error| format!("header `{key}` value is invalid: {error}"))?;
            Ok((name, header_value))
        })
        .collect()
}

fn expand_env_templates(value: &str) -> Result<String, String> {
    let mut output = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find("${") {
        let (prefix, after_start) = rest.split_at(start);
        output.push_str(prefix);
        let after_start = &after_start[2..];
        let Some(end) = after_start.find('}') else {
            return Err("header env interpolation has an unterminated ${VAR}".to_owned());
        };
        let (name, after_end) = after_start.split_at(end);
        if name.is_empty() {
            return Err("header env interpolation requires a variable name".to_owned());
        }
        let env_value = std::env::var(name)
            .map_err(|_| format!("header references missing environment variable `{name}`"))?;
        output.push_str(&env_value);
        rest = &after_end[1..];
    }
    output.push_str(rest);
    Ok(output)
}
