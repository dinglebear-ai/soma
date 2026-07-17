//! Transport client for the Soma runtime.
//!
//! **Soma note**: this client has two modes:
//!   - empty `SOMA_API_URL` keeps the offline stub working;
//!   - non-empty `SOMA_API_URL` forwards operations to a deployed `soma serve`
//!     REST API, which is the local CLI/stdio adapter shape for platform servers.
//!
//! The pattern:
//!   - `SomaClient::new()` builds the transport (HTTP client, connection pool, etc.)
//!   - Each method corresponds to one remote operation and returns `Result<Value>`
//!   - `SomaService` (in `soma-service`) wraps this and adds any business logic
//!   - MCP tools in `mcp/tools.rs` call `SomaService`, never `SomaClient` directly

#[cfg(feature = "client")]
use anyhow::Context;
use anyhow::Result;
use serde_json::{json, Value};

use soma_config::{EffectiveRuntimeMode, SomaConfig};

#[cfg(feature = "client")]
use reqwest::{header, Url};
#[cfg(feature = "client")]
use std::time::Duration;

// Unit tests live in a sidecar file — see src/client_tests.rs for the pattern.
// CUSTOMIZE: Copy this block into every module that needs unit tests.
#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;

/// HTTP (or other transport) client for the Soma runtime.
///
/// For application/platform servers, the lightweight local binary uses this as
/// an adapter to the deployed `soma serve` REST API. For upstream-client
/// servers, replace the REST envelope with the upstream service's native API.
#[derive(Clone)]
pub struct SomaClient {
    #[cfg_attr(not(feature = "client"), allow(dead_code))]
    target: SomaTarget,
    #[cfg(feature = "client")]
    client: reqwest::Client,
}

#[derive(Clone)]
enum SomaTarget {
    /// Offline stub mode used by Soma when no deployed API is configured.
    Stub,
    /// Deployed platform API reached by the local CLI/stdio adapter.
    #[cfg(feature = "client")]
    DeployedApi {
        base_url: Url,
        bearer_token: Option<String>,
    },
}

impl SomaClient {
    /// Construct a new client from configuration.
    ///
    /// If `SOMA_API_URL` is empty, Soma uses local stub responses so
    /// tests and first-run scaffolds work without a deployed service. If it is
    /// set, operations are forwarded to direct `{SOMA_API_URL}/v1/*` routes.
    pub fn new(cfg: &SomaConfig) -> Result<Self> {
        let target = build_target(cfg)?;

        #[cfg(feature = "client")]
        {
            let client = reqwest::ClientBuilder::new()
                .timeout(Duration::from_secs(30))
                .build()
                .context("failed to build HTTP client")?;
            Ok(Self { target, client })
        }
        #[cfg(not(feature = "client"))]
        {
            Ok(Self { target })
        }
    }

    /// Say hello to `name`, or "World" if not provided.
    pub async fn greet(&self, name: Option<&str>) -> Result<Value> {
        let body = name.map_or_else(|| json!({}), |name| json!({ "name": name }));
        if let Some(value) = self.post_deployed_api("greet", "v1/greet", body).await? {
            return Ok(value);
        }

        let target = name.unwrap_or("World");
        Ok(json!({
            "greeting": format!("Hello, {target}!"),
            "target": target,
            "server": "",
        }))
    }

    /// Echo a message back unchanged.
    pub async fn echo(&self, message: &str) -> Result<Value> {
        if let Some(value) = self
            .post_deployed_api("echo", "v1/echo", json!({ "message": message }))
            .await?
        {
            return Ok(value);
        }

        Ok(json!({ "echo": message }))
    }

    /// Return a status snapshot of the remote service.
    ///
    /// Note: this value is returned by the unauthenticated `/status` endpoint,
    /// so it must not include secrets or sensitive topology (e.g. `api_url`).
    /// CUSTOMIZE: Add non-sensitive runtime metrics (uptime, version, etc.).
    pub async fn status(&self) -> Result<Value> {
        if let Some(value) = self.get_deployed_api("status", "v1/status").await? {
            return Ok(value);
        }

        let mut status = json!({
            "status": "ok",
            // api_url intentionally omitted — topology leak on unauthenticated endpoint.
            "note": "stub — replace with real health endpoint",
        });
        add_status_warnings(&mut status);
        Ok(status)
    }

    /// Call one direct REST action on a running Soma HTTP server.
    ///
    /// This is used by remote adapter surfaces (`soma <command>` and eventually
    /// stdio MCP adapter mode) so the local binary does not need local provider
    /// manifests to execute provider-backed actions.
    pub async fn call_rest_action(&self, action: &str, params: Value) -> Result<Value> {
        validate_action_path_segment(action)?;
        let call = self.resolve_remote_rest_call(action, &params).await?;
        self.call_deployed_api_method(action, &call.method, &call.relative_path, call.body)
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!("SOMA_API_URL is required for remote runtime mode action={action}")
            })
    }

    /// Fetch the live provider catalog from a deployed Soma HTTP server.
    pub async fn provider_catalog(&self) -> Result<Value> {
        self.get_deployed_api("providers", "v1/providers")
            .await?
            .ok_or_else(|| anyhow::anyhow!("SOMA_API_URL is required to read remote providers"))
    }

    /// Readiness probe of the upstream dependency.
    ///
    /// Stub mode is always ready (there is no upstream). Deployed mode issues a
    /// short, timeout-bounded GET against the upstream `/health` so a wedged or
    /// unreachable upstream surfaces as not-ready instead of hanging the probe.
    /// Used by the `/readyz` route; keep it cheap and side-effect free.
    pub async fn ready(&self) -> Result<()> {
        #[cfg(not(feature = "client"))]
        {
            Ok(())
        }
        #[cfg(feature = "client")]
        {
            let SomaTarget::DeployedApi {
                base_url,
                bearer_token,
            } = &self.target
            else {
                return Ok(());
            };

            let url = api_url(base_url, "health")?;
            let mut request = self.client.get(url).timeout(Duration::from_secs(2));
            if let Some(token) = bearer_token {
                request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
            }

            let response = request
                .send()
                .await
                .context("upstream readiness probe failed")?;
            if !response.status().is_success() {
                anyhow::bail!("upstream not ready: HTTP {}", response.status());
            }
            Ok(())
        }
    }

    async fn post_deployed_api(
        &self,
        action: &str,
        relative_path: &str,
        body: Value,
    ) -> Result<Option<Value>> {
        self.call_deployed_api(action, relative_path, Some(body))
            .await
    }

    async fn get_deployed_api(&self, action: &str, relative_path: &str) -> Result<Option<Value>> {
        self.call_deployed_api(action, relative_path, None).await
    }

    async fn call_deployed_api(
        &self,
        action: &str,
        relative_path: &str,
        body: Option<Value>,
    ) -> Result<Option<Value>> {
        let method = if body.is_some() { "POST" } else { "GET" };
        self.call_deployed_api_method(action, method, relative_path, body)
            .await
    }

    async fn call_deployed_api_method(
        &self,
        action: &str,
        method: &str,
        relative_path: &str,
        body: Option<Value>,
    ) -> Result<Option<Value>> {
        #[cfg(not(feature = "client"))]
        {
            let _ = (action, method, relative_path, body);
            Ok(None)
        }
        #[cfg(feature = "client")]
        {
            let SomaTarget::DeployedApi {
                base_url,
                bearer_token,
            } = &self.target
            else {
                return Ok(None);
            };

            let url = api_url(base_url, relative_path)?;
            let method = reqwest::Method::from_bytes(method.as_bytes())
                .with_context(|| format!("invalid remote REST method action={action}"))?;
            let mut request = self.client.request(method, url);
            if let Some(body) = body {
                request = request.json(&body);
            }
            if let Some(token) = bearer_token {
                request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
            }

            let response = request
                .send()
                .await
                .with_context(|| format!("failed to call deployed API action={action}"))?;
            let status = response.status();
            let body = response
                .text()
                .await
                .with_context(|| format!("failed to read deployed API response action={action}"))?;

            if !status.is_success() {
                anyhow::bail!("deployed API action={action} returned HTTP {status}: {body}");
            }

            let value = serde_json::from_str(&body)
                .with_context(|| format!("deployed API returned invalid JSON action={action}"))?;
            Ok(Some(value))
        }
    }
}

#[derive(Debug)]
struct RemoteRestCall {
    method: String,
    relative_path: String,
    body: Option<Value>,
}

impl SomaClient {
    async fn resolve_remote_rest_call(
        &self,
        action: &str,
        params: &Value,
    ) -> Result<RemoteRestCall> {
        if remote_action_uses_get(action, params) {
            return Ok(RemoteRestCall {
                method: "GET".to_owned(),
                relative_path: format!("v1/{action}"),
                body: None,
            });
        }

        if matches!(action, "greet" | "echo") {
            return Ok(RemoteRestCall {
                method: "POST".to_owned(),
                relative_path: format!("v1/{action}"),
                body: Some(params.clone()),
            });
        }

        let catalog = self.provider_catalog().await?;
        if let Some(route) = remote_provider_route(&catalog, action)? {
            return Ok(RemoteRestCall {
                method: route.method,
                relative_path: route.relative_path,
                body: Some(params.clone()),
            });
        }

        Ok(RemoteRestCall {
            method: "POST".to_owned(),
            relative_path: format!("v1/tools/{action}"),
            body: Some(params.clone()),
        })
    }
}

#[cfg(feature = "client")]
fn build_target(cfg: &SomaConfig) -> Result<SomaTarget> {
    if cfg.effective_runtime_mode() == EffectiveRuntimeMode::Local {
        return Ok(SomaTarget::Stub);
    }
    let api_url = cfg.api_url.trim();
    if api_url.is_empty() {
        return Ok(SomaTarget::Stub);
    }
    let base_url =
        Url::parse(api_url).with_context(|| format!("invalid SOMA_API_URL: {api_url}"))?;
    let bearer_token = non_empty(&cfg.api_key);
    Ok(SomaTarget::DeployedApi {
        base_url,
        bearer_token,
    })
}

#[cfg(not(feature = "client"))]
fn build_target(cfg: &SomaConfig) -> Result<SomaTarget> {
    if cfg.effective_runtime_mode() == EffectiveRuntimeMode::Remote && !cfg.api_url.is_empty() {
        anyhow::bail!("soma-client was built without the `client` feature");
    }
    Ok(SomaTarget::Stub)
}

#[cfg(feature = "client")]
fn api_url(base_url: &Url, relative_path: &str) -> Result<Url> {
    let mut url = base_url.clone();
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| anyhow::anyhow!("SOMA_API_URL cannot be a base for REST paths"))?;
        segments.pop_if_empty();
        for segment in relative_path.split('/') {
            if !segment.is_empty() {
                segments.push(segment);
            }
        }
    }
    Ok(url)
}

#[cfg(feature = "client")]
fn non_empty(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn remote_action_uses_get(action: &str, params: &Value) -> bool {
    params.as_object().is_some_and(|object| object.is_empty())
        && matches!(action, "status" | "help")
}

#[derive(Debug)]
struct RemoteProviderRoute {
    method: String,
    relative_path: String,
}

fn remote_provider_route(catalog: &Value, action: &str) -> Result<Option<RemoteProviderRoute>> {
    let Some(providers) = catalog.get("providers").and_then(Value::as_array) else {
        return Ok(None);
    };
    for provider in providers {
        let Some(tools) = provider.get("tools").and_then(Value::as_array) else {
            continue;
        };
        for tool in tools {
            if !remote_tool_matches_action(tool, action) {
                continue;
            }
            if tool
                .get("surfaces")
                .and_then(|surfaces| surfaces.get("rest"))
                .and_then(Value::as_bool)
                == Some(false)
            {
                anyhow::bail!("remote provider action `{action}` is not REST-exposed");
            }
            let canonical = tool
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow::anyhow!("remote provider catalog tool missing name"))?;
            if let Some(rest) = tool.get("rest") {
                if let Some(path) = rest.get("path").and_then(Value::as_str) {
                    return Ok(Some(RemoteProviderRoute {
                        method: rest_method(rest),
                        relative_path: trim_relative_rest_path(path),
                    }));
                }
            }
            if let Some(rest) = tool.get("generic_rest") {
                if let Some(path) = rest.get("path").and_then(Value::as_str) {
                    return Ok(Some(RemoteProviderRoute {
                        method: rest_method(rest),
                        relative_path: trim_relative_rest_path(path),
                    }));
                }
            }
            return Ok(Some(RemoteProviderRoute {
                method: "POST".to_owned(),
                relative_path: format!("v1/tools/{canonical}"),
            }));
        }
    }
    Ok(None)
}

fn rest_method(rest: &Value) -> String {
    rest.get("method")
        .and_then(Value::as_str)
        .unwrap_or("POST")
        .to_ascii_uppercase()
}

fn remote_tool_matches_action(tool: &Value, action: &str) -> bool {
    if tool.get("name").and_then(Value::as_str) == Some(action) {
        return true;
    }
    let Some(cli) = tool.get("cli") else {
        return false;
    };
    if cli.get("command").and_then(Value::as_str) == Some(action) {
        return true;
    }
    cli.get("aliases")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|alias| alias.as_str() == Some(action))
}

fn trim_relative_rest_path(path: &str) -> String {
    path.trim_start_matches('/').to_owned()
}

fn validate_action_path_segment(action: &str) -> Result<()> {
    if action.is_empty() || action.contains('/') {
        anyhow::bail!("remote REST action must be a non-empty path segment");
    }
    Ok(())
}

#[cfg(feature = "observability")]
fn add_status_warnings(status: &mut Value) {
    if let Some(warning) = soma_observability::binary_status::stale_binary_warning() {
        status["warnings"] = json!([warning]);
    }
}

#[cfg(not(feature = "observability"))]
fn add_status_warnings(_status: &mut Value) {}
