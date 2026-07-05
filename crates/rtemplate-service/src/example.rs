//! Transport client for the Example service.
//!
//! **Template note**: this client has two modes:
//!   - empty `RTEMPLATE_API_URL` keeps the offline template stub working;
//!   - non-empty `RTEMPLATE_API_URL` forwards operations to a deployed `example-server`
//!     REST API, which is the local CLI/stdio adapter shape for platform servers.
//!
//! The pattern:
//!   - `ExampleClient::new()` builds the transport (HTTP client, connection pool, etc.)
//!   - Each method corresponds to one remote operation and returns `Result<Value>`
//!   - `ExampleService` in `app.rs` wraps this and adds any business logic
//!   - MCP tools in `mcp/tools.rs` call `ExampleService`, never `ExampleClient` directly

#[cfg(feature = "client")]
use anyhow::Context;
use anyhow::Result;
use serde_json::{json, Value};

use rtemplate_contracts::config::ExampleConfig;

#[cfg(feature = "client")]
use reqwest::{header, Url};
#[cfg(feature = "client")]
use std::time::Duration;

// Unit tests live in a sidecar file — see src/example_tests.rs for the pattern.
// TEMPLATE: Copy this block into every module that needs unit tests.
#[cfg(test)]
#[path = "example_tests.rs"]
mod tests;

/// HTTP (or other transport) client for the example service.
///
/// For application/platform servers, the lightweight local binary uses this as
/// an adapter to the deployed `example-server` REST API. For upstream-client
/// servers, replace the REST envelope with the upstream service's native API.
#[derive(Clone)]
pub struct ExampleClient {
    #[cfg_attr(not(feature = "client"), allow(dead_code))]
    target: ExampleTarget,
    #[cfg(feature = "client")]
    client: reqwest::Client,
}

#[derive(Clone)]
enum ExampleTarget {
    /// Offline stub mode used by the template when no deployed API is configured.
    Stub,
    /// Deployed platform API reached by the local CLI/stdio adapter.
    #[cfg(feature = "client")]
    DeployedApi {
        base_url: Url,
        bearer_token: Option<String>,
    },
}

impl ExampleClient {
    /// Construct a new client from configuration.
    ///
    /// If `RTEMPLATE_API_URL` is empty, the template uses local stub responses so
    /// tests and first-run scaffolds work without a deployed service. If it is
    /// set, operations are forwarded to direct `{RTEMPLATE_API_URL}/v1/*` routes.
    pub fn new(cfg: &ExampleConfig) -> Result<Self> {
        let api_url = cfg.api_url.trim();
        let target = build_target(api_url, &cfg.api_key)?;

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
    /// TEMPLATE: Add non-sensitive runtime metrics (uptime, version, etc.).
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

    /// Readiness probe of the upstream dependency.
    ///
    /// Stub mode is always ready (there is no upstream). Deployed mode issues a
    /// short, timeout-bounded GET against the upstream `/health` so a wedged or
    /// unreachable upstream surfaces as not-ready instead of hanging the probe.
    /// Used by the `/readyz` route; keep it cheap and side-effect free.
    pub async fn ready(&self) -> Result<()> {
        #[cfg(not(feature = "client"))]
        {
            return Ok(());
        }
        #[cfg(feature = "client")]
        {
            let ExampleTarget::DeployedApi {
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
        #[cfg(not(feature = "client"))]
        {
            let _ = (action, relative_path, body);
            return Ok(None);
        }
        #[cfg(feature = "client")]
        {
            let ExampleTarget::DeployedApi {
                base_url,
                bearer_token,
            } = &self.target
            else {
                return Ok(None);
            };

            let url = api_url(base_url, relative_path)?;
            let mut request = if let Some(body) = body {
                self.client.post(url).json(&body)
            } else {
                self.client.get(url)
            };
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

#[cfg(feature = "client")]
fn build_target(api_url: &str, api_key: &str) -> Result<ExampleTarget> {
    if api_url.is_empty() {
        return Ok(ExampleTarget::Stub);
    }
    let base_url =
        Url::parse(api_url).with_context(|| format!("invalid RTEMPLATE_API_URL: {api_url}"))?;
    let bearer_token = non_empty(api_key);
    Ok(ExampleTarget::DeployedApi {
        base_url,
        bearer_token,
    })
}

#[cfg(not(feature = "client"))]
fn build_target(api_url: &str, _api_key: &str) -> Result<ExampleTarget> {
    if !api_url.is_empty() {
        anyhow::bail!("rtemplate-service was built without the `client` feature");
    }
    Ok(ExampleTarget::Stub)
}

#[cfg(feature = "client")]
fn api_url(base_url: &Url, relative_path: &str) -> Result<Url> {
    let mut url = base_url.clone();
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| anyhow::anyhow!("RTEMPLATE_API_URL cannot be a base for REST paths"))?;
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

#[cfg(feature = "observability")]
fn add_status_warnings(status: &mut Value) {
    if let Some(warning) = rtemplate_observability::binary_status::stale_binary_warning() {
        status["warnings"] = json!([warning]);
    }
}

#[cfg(not(feature = "observability"))]
fn add_status_warnings(_status: &mut Value) {}
