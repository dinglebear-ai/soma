//! Thin, product-neutral projections onto `soma-gateway`, the canonical
//! upstream-MCP connection/routing engine (plan section "Upstream MCP
//! decision").
//!
//! Two independent pieces live here:
//!
//! - [`UpstreamMcpProvider`]: a per-manifest ad-hoc "connect to one upstream
//!   MCP server and proxy a tool call" provider, migrated from the
//!   soma-service `mcp` provider kind that predated `soma-gateway`.
//! - [`project_gateway_action_catalog`]: a pure function that projects
//!   `soma-gateway`'s own admin action catalog
//!   (`soma_gateway::gateway::catalog::GatewayActionCatalog`) into a
//!   `soma_provider_core::ProviderCatalog`, so a host can expose gateway
//!   administration as a drop-in-shaped provider surface.
//!
//! ## Deviation: transport is not yet pooled through `soma-mcp-client`
//!
//! `UpstreamMcpProvider` still opens its own per-call `rmcp` session with
//! the raw `TokioChildProcess` / `StreamableHttpClientTransport` transports
//! rather than routing through `soma-mcp-client`'s pooled `UpstreamPool`.
//! That would be the fuller reconciliation the plan calls for (acceptance:
//! "no second upstream MCP transport stack"), but it is a genuinely
//! cross-cutting change, not a mechanical swap:
//!
//! - `UpstreamPool`/`UpstreamConfig` (`crates/shared/mcp/client/src/config.rs`)
//!   currently support only a single `bearer_token_env` for HTTP auth, while
//!   this provider's manifest contract (`provider.meta.mcp.http.headers`)
//!   supports arbitrary, `${VAR}`-interpolated custom headers with no test
//!   coverage proving that capability is unused. Narrowing it silently to
//!   single-bearer-token auth risks a real, undetectable behavior regression.
//! - `UpstreamConfig::validate()` runs `SpawnGuard` command validation and a
//!   restricted `name` character set that this provider's manifest-driven
//!   stdio commands have never been checked against; migrating without
//!   reconciling those rules risks rejecting previously-working manifests.
//! - `UpstreamConfig` has no per-upstream timeout field equivalent to
//!   `provider.meta.mcp.timeout_ms`.
//! - `UpstreamPool` is a registered, stateful pool (`register_config` then
//!   `ensure_connected`/`call_tool`); this provider is stateless per
//!   `ProviderCall` today and has no "register once" phase to hook into.
//!
//! Tracked as its own scoped follow-up: bead `rmcp-template-fnz0`. This
//! adapter is still a real improvement over the pre-PR10 state: it is
//! physically consolidated into the shared, product-neutral adapters crate
//! (no soma-service dependency), and it is the only upstream-MCP transport
//! implementation outside `soma-mcp-client`/`soma-gateway` in the workspace.

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
use soma_provider_core::{
    Provider, ProviderCall, ProviderCatalog, ProviderError, ProviderOutput, ProviderTool,
};
use tokio::{io::AsyncReadExt, process::Command};

#[derive(Clone)]
pub struct UpstreamMcpProvider {
    catalog: ProviderCatalog,
}

impl UpstreamMcpProvider {
    pub fn new(catalog: ProviderCatalog) -> Self {
        Self { catalog }
    }

    pub fn arc(catalog: ProviderCatalog) -> Arc<Self> {
        Arc::new(Self::new(catalog))
    }
}

#[async_trait]
impl Provider for UpstreamMcpProvider {
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

impl UpstreamMcpProvider {
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
    // Keep stderr piped (rather than the previous `Stdio::null()`) so a
    // failed spawn/handshake/call can be diagnosed — see `attach_stderr`.
    let (transport, stderr) =
        TokioChildProcess::builder(Command::new(&runtime.command).configure(|cmd| {
            cmd.args(&runtime.args)
                .env_clear()
                .envs(runtime.env.iter().map(|(key, value)| (key, value)))
                .stderr(Stdio::piped());
            if let Some(cwd) = &runtime.cwd {
                cmd.current_dir(cwd);
            }
        }))
        .spawn()
        .map_err(|error| {
            ProviderError::execution(&catalog.provider.name, call.action.clone(), error)
        })?;
    let service = match ().serve(transport).await {
        Ok(service) => service,
        Err(error) => {
            let provider_error =
                ProviderError::execution(&catalog.provider.name, call.action.clone(), error);
            return Err(attach_stderr(provider_error, stderr).await);
        }
    };
    let result = service
        .call_tool(CallToolRequestParams::new(upstream.name.clone()).with_arguments(params))
        .await;
    let result = match result {
        Ok(result) => Ok(result),
        Err(error) => {
            let provider_error =
                ProviderError::execution(&catalog.provider.name, call.action.clone(), error);
            Err(attach_stderr(provider_error, stderr).await)
        }
    };
    if let Err(error) = service.cancel().await {
        tracing::debug!(
            provider = %catalog.provider.name,
            action = %call.action,
            error = %error,
            "failed to cancel upstream MCP stdio session cleanly"
        );
    }
    result
}

/// Best-effort attaches whatever the child has written to stderr as private
/// (server-log-only, never returned to the MCP client — see
/// `ProviderError::private_diagnostics`) diagnostics on an already-built
/// error. Bounded in both time (the child may still be alive with an idle,
/// non-EOF stderr pipe) and size, so a chatty or hung upstream can't stall or
/// balloon a single failed call.
async fn attach_stderr(
    error: ProviderError,
    stderr: Option<impl tokio::io::AsyncRead + Unpin>,
) -> ProviderError {
    const MAX_STDERR_BYTES: usize = 8 * 1024;
    const READ_BUDGET: Duration = Duration::from_millis(200);

    let Some(mut stderr) = stderr else {
        return error;
    };
    let mut buffer = Vec::new();
    let _ = tokio::time::timeout(
        READ_BUDGET,
        tokio::io::AsyncReadExt::take(&mut stderr, MAX_STDERR_BYTES as u64)
            .read_to_end(&mut buffer),
    )
    .await;
    let text = String::from_utf8_lossy(&buffer).trim().to_owned();
    if text.is_empty() {
        error
    } else {
        error.with_private_diagnostics(format!("upstream stderr: {text}"))
    }
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
    if let Err(error) = service.cancel().await {
        tracing::debug!(
            provider = %catalog.provider.name,
            action = %call.action,
            error = %error,
            "failed to cancel upstream MCP http session cleanly"
        );
    }
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

    /// Merges the caller's params with this tool's manifest-declared
    /// `static_args`. `static_args` are a pin, not a default: they are
    /// applied *after* the caller's params so a manifest can restrict which
    /// upstream action/argument a drop-in tool reaches (e.g. pinning
    /// `action: "echo"` on a generic upstream tool) without a caller being
    /// able to override it by supplying the same key. Any previous version
    /// of this method that applied `static_args` first and let caller params
    /// win on key collision inverted this contract.
    fn params(&self, call_params: Value) -> Map<String, Value> {
        let mut params = match call_params {
            Value::Object(map) => map,
            _ => Map::new(),
        };
        params.extend(self.static_args.clone());
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

/// Projects `soma-gateway`'s own admin action catalog into a
/// `soma_provider_core::ProviderCatalog`, so a host can advertise gateway
/// administration (list/add/remove/reload upstreams, OAuth lifecycle, ...)
/// through the same drop-in-provider-shaped surface as every other adapter
/// in this crate.
///
/// This is a pure catalog *projection*, not a wired dispatcher: no
/// `soma-service` deployment currently constructs a live `soma_gateway`
/// manager instance, so there is nothing for a `call()` implementation to
/// dispatch through yet. Wiring a live dispatcher is deferred to whichever
/// product integration crate first constructs a running gateway (tracked as
/// a PR10 follow-up) — see the module-level deviation notes.
///
/// Returns `Err` if `provider_id` is not a valid `ProviderId` (lowercase,
/// `[a-z0-9-_]`, no leading/trailing/doubled separators) rather than
/// panicking — this is a `pub fn` in a shared library crate and `provider_id`
/// may come from caller-supplied configuration, not only compile-time
/// literals.
pub fn project_gateway_action_catalog(
    provider_id: impl Into<String>,
    title: impl Into<String>,
    actions: &soma_gateway::gateway::catalog::GatewayActionCatalog,
) -> Result<ProviderCatalog, soma_provider_core::ProviderIdError> {
    let tools = actions
        .list()
        .into_iter()
        .map(|action| {
            let mut tool = ProviderTool::new(
                action.name,
                format!(
                    "Gateway administration action `{}`{}.",
                    action.name,
                    if action.admin_required {
                        " (admin only)"
                    } else {
                        ""
                    }
                ),
                json!({"type": "object", "additionalProperties": true}),
            );
            tool.destructive = action.destructive;
            tool.requires_admin = action.admin_required;
            tool.meta = json!({
                "gateway": {
                    "discovery": action.discovery,
                    "spawn_validation_required": action.spawn_validation_required,
                }
            });
            tool
        })
        .collect();

    let mut manifest = soma_provider_core::ProviderManifest::new(
        soma_provider_core::ProviderId::new(provider_id.into())?,
        title,
        soma_gateway::VERSION,
    );
    manifest.tools = tools;
    Ok(manifest)
}

#[cfg(test)]
#[path = "gateway_tests.rs"]
mod tests;
