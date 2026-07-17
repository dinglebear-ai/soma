//! The generic OpenAPI provider kind: proxies a drop-in provider tool to a
//! declared `base_url` + `path`/`method`, gated by the manifest's
//! `capabilities.network.allowed_hosts` grant.
//!
//! ## Delegates to `soma-openapi`
//!
//! Dispatch goes through `soma_openapi::http::execute_operation_for_allowlisted_host`
//! — the same request-building, capped response reading, and JSON
//! parsing/status handling as `soma-openapi`'s registry-driven dispatch path
//! (`crates/shared/openapi/src/http.rs`). This module keeps only what is
//! specific to the drop-in-manifest shape: resolving `base_url`/`path`/
//! `method` out of `provider.meta`/`tool.meta`, and the operator-controlled
//! `capabilities.network.allowed_hosts` gate.
//!
//! That entry point (unlike `soma_openapi::http::execute_operation`, used by
//! `soma-openapi`'s own spec-driven dispatch) skips the crate's DNS-pinned
//! SSRF guard. This adapter's trust model is different and is itself part of
//! the tested, documented contract: an operator explicitly allowlists hosts
//! via `provider.capabilities.network.allowed_hosts`, and that allowlist may
//! legitimately include loopback/private hosts (a local sidecar service, for
//! example) — see `openapi_provider_executes_pinned_local_operation` in
//! `apps/soma/tests/openapi_provider.rs`, which calls a plain-HTTP
//! `127.0.0.1` server and would be rejected by the DNS-pinned guard. The
//! allowlist check below (`validate_base_url`) is this adapter's own SSRF
//! defense and runs before any request is dispatched. For that defense to
//! hold, two things it does NOT rely on `soma-openapi` for are handled here:
//! `validate_base_url` fails closed (denies dispatch) when a provider's
//! `capabilities.network` grant is absent or disabled, rather than treating
//! that as "no restriction needed"; and [`dispatch_client`] disables HTTP
//! redirects, so an allowlisted host can't hand a request off to a
//! non-allowlisted address via a 3xx response.
//!
//! A small amount of behavior differs from the pre-delegation implementation,
//! documented here rather than hidden:
//! - `{name}` placeholders in a declared operation `path` are now honored as
//!   path parameters (previously inert literal text); this is additive.
//! - An operation path that resolves outside the declared `base_url`'s own
//!   path prefix (e.g. via `..` segments) is now rejected; previously
//!   unchecked.
//! - `call.params` must now be a JSON object for every method, not only
//!   GET/DELETE; previously POST/PUT/PATCH accepted any JSON value as the
//!   request body.
//! - A non-2xx upstream response no longer includes the HTTP status code in
//!   the error message (the `openapi_upstream_error` error `code` is
//!   unchanged).

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use serde_json::Value;
use soma_provider_core::{
    Provider, ProviderCall, ProviderCatalog, ProviderError, ProviderOutput, ProviderTool,
};
use url::Url;

/// reqwest 0.13's rustls backend panics without an installed crypto
/// provider; install ring once, tolerating a provider some embedder
/// installed earlier. (Not rmcp-specific despite the historical wording of
/// this helper elsewhere in the workspace — this module dispatches over
/// plain reqwest via `soma-openapi`, never through an rmcp transport.)
fn ensure_rustls_crypto_provider() {
    static INSTALL: std::sync::Once = std::sync::Once::new();
    INSTALL.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// A per-call HTTP client hardened for dispatch against an
/// operator-allowlisted host (see `validate_base_url`): redirects are
/// disabled so an allowlisted host cannot silently hand off the request to
/// an arbitrary, non-allowlisted address via a 3xx response — the allowlist
/// check in `validate_base_url` only inspects the *configured* `base_url`,
/// so an unchecked redirect would otherwise bypass it entirely. A bounded
/// connect/request timeout also prevents a hung upstream from blocking a
/// call indefinitely.
fn dispatch_client() -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
}

#[derive(Clone)]
pub struct OpenApiProvider {
    catalog: ProviderCatalog,
}

impl OpenApiProvider {
    pub fn curated(catalog: ProviderCatalog) -> Self {
        Self { catalog }
    }

    pub fn arc(catalog: ProviderCatalog) -> Arc<Self> {
        Arc::new(Self::curated(catalog))
    }
}

#[async_trait]
impl Provider for OpenApiProvider {
    fn catalog(&self) -> ProviderCatalog {
        self.catalog.clone()
    }

    async fn call(&self, call: ProviderCall) -> Result<ProviderOutput, ProviderError> {
        ensure_rustls_crypto_provider();
        let tool = self.tool(&call)?;
        let operation = OpenApiOperation::from_catalog(&self.catalog, tool, &call)?;
        if !call.params.is_object() {
            return Err(ProviderError::validation(
                &self.catalog.provider.name,
                &call.action,
                "openapi_params_must_be_object",
                "OpenAPI provider params must be a JSON object",
            ));
        }

        let client = dispatch_client().map_err(|error| {
            ProviderError::opaque_execution(&self.catalog.provider.name, &call.action, error)
        })?;
        let handle = soma_openapi::registry::OperationHandle {
            operation_id: call.action.clone(),
            method: operation.method,
            path_template: operation.path,
            base_url: operation.base,
            credential: None,
        };
        let value = soma_openapi::http::execute_operation_for_allowlisted_host(
            &client,
            &handle,
            call.params,
        )
        .await
        .map_err(|error| map_openapi_error(&self.catalog.provider.name, &call.action, error))?;
        Ok(ProviderOutput::json(value))
    }
}

impl OpenApiProvider {
    fn tool(&self, call: &ProviderCall) -> Result<&ProviderTool, ProviderError> {
        self.catalog
            .tools
            .iter()
            .find(|tool| tool.name == call.action)
            .ok_or_else(|| {
                ProviderError::validation(
                    &self.catalog.provider.name,
                    &call.action,
                    "unknown_openapi_action",
                    format!("OpenAPI provider has no action `{}`", call.action),
                )
            })
    }
}

struct OpenApiOperation {
    method: reqwest::Method,
    base: Url,
    path: String,
}

impl OpenApiOperation {
    fn from_catalog(
        catalog: &ProviderCatalog,
        tool: &ProviderTool,
        call: &ProviderCall,
    ) -> Result<Self, ProviderError> {
        let base_url = catalog
            .meta
            .get("openapi")
            .and_then(|value| value.get("base_url"))
            .and_then(Value::as_str)
            .or_else(|| catalog.meta.get("base_url").and_then(Value::as_str))
            .ok_or_else(|| {
                ProviderError::validation(
                    &catalog.provider.name,
                    &call.action,
                    "missing_openapi_base_url",
                    "OpenAPI provider requires provider.meta.openapi.base_url",
                )
            })?;
        let base = Url::parse(base_url).map_err(|error| {
            ProviderError::validation(
                &catalog.provider.name,
                &call.action,
                "invalid_openapi_base_url",
                error.to_string(),
            )
        })?;
        validate_base_url(catalog, &call.action, &base)?;

        let operation_meta = tool.meta.get("openapi");
        let path = operation_meta
            .and_then(|value| value.get("path"))
            .and_then(Value::as_str)
            .or_else(|| tool.rest.as_ref().and_then(|rest| rest.path.as_deref()))
            .unwrap_or("");
        validate_relative_path(catalog, &call.action, path)?;

        let method_str = operation_meta
            .and_then(|value| value.get("method"))
            .and_then(Value::as_str)
            .or_else(|| tool.rest.as_ref().and_then(|rest| rest.method.as_deref()))
            .unwrap_or("POST")
            .to_ascii_uppercase();
        let method = parse_supported_method(catalog, &call.action, &method_str)?;

        Ok(Self {
            method,
            base,
            path: path.to_owned(),
        })
    }
}

fn parse_supported_method(
    catalog: &ProviderCatalog,
    action: &str,
    method: &str,
) -> Result<reqwest::Method, ProviderError> {
    match method {
        "GET" => Ok(reqwest::Method::GET),
        "POST" => Ok(reqwest::Method::POST),
        "PUT" => Ok(reqwest::Method::PUT),
        "PATCH" => Ok(reqwest::Method::PATCH),
        "DELETE" => Ok(reqwest::Method::DELETE),
        other => Err(ProviderError::validation(
            &catalog.provider.name,
            action,
            "unsupported_openapi_method",
            format!("unsupported OpenAPI provider method `{other}`"),
        )),
    }
}

fn validate_base_url(
    catalog: &ProviderCatalog,
    action: &str,
    url: &Url,
) -> Result<(), ProviderError> {
    if !matches!(url.scheme(), "http" | "https") {
        return Err(ProviderError::validation(
            &catalog.provider.name,
            action,
            "openapi_scheme_denied",
            "OpenAPI provider base_url must use http or https",
        ));
    }
    if url.host_str().is_none() {
        return Err(ProviderError::validation(
            &catalog.provider.name,
            action,
            "openapi_host_required",
            "OpenAPI provider base_url must include a host",
        ));
    }
    // Fail closed: an OpenAPI provider always makes a network call, so an
    // absent or disabled `capabilities.network` block must deny dispatch
    // rather than silently skip the allowlist check. This adapter bypasses
    // `soma-openapi`'s own DNS-pinned SSRF guard specifically because it
    // trusts this allowlist as its replacement defense (see the module doc)
    // — that trust model only holds if the allowlist is mandatory, not
    // opt-in per manifest.
    let host = url.host_str().unwrap_or_default();
    let network = catalog.capabilities.network.as_ref().ok_or_else(|| {
        ProviderError::validation(
            &catalog.provider.name,
            action,
            "openapi_network_capability_required",
            "OpenAPI provider requires an enabled capabilities.network.allowed_hosts grant",
        )
    })?;
    if !network.enabled {
        return Err(ProviderError::validation(
            &catalog.provider.name,
            action,
            "openapi_network_capability_required",
            "OpenAPI provider requires an enabled capabilities.network.allowed_hosts grant",
        ));
    }
    if !network.allowed_hosts.iter().any(|allowed| allowed == host) {
        return Err(ProviderError::validation(
            &catalog.provider.name,
            action,
            "openapi_host_not_allowed",
            format!("OpenAPI provider host `{host}` is not declared in allowed_hosts"),
        ));
    }
    Ok(())
}

/// Rejects an operation path that is itself an absolute URL, so a manifest
/// cannot redirect a call away from the pinned, allowlisted `base_url`. The
/// remaining relative-path resolution (joining onto `base_url`, `{name}`
/// path-parameter substitution, and containment under `base_url`'s own path
/// prefix) is delegated to `soma_openapi::http`.
fn validate_relative_path(
    catalog: &ProviderCatalog,
    action: &str,
    path: &str,
) -> Result<(), ProviderError> {
    if path.starts_with("http://") || path.starts_with("https://") || path.starts_with("//") {
        return Err(ProviderError::validation(
            &catalog.provider.name,
            action,
            "openapi_absolute_operation_url_denied",
            "OpenAPI provider operation paths must be relative to the pinned base_url",
        ));
    }
    Ok(())
}

fn map_openapi_error(
    provider: &str,
    action: &str,
    error: soma_openapi::OpenApiError,
) -> ProviderError {
    use soma_openapi::OpenApiError as E;
    match &error {
        E::InvalidPathParam { param, .. } => ProviderError::validation(
            provider,
            action,
            "openapi_invalid_path_param",
            format!("OpenAPI provider path parameter `{param}` is missing or invalid"),
        ),
        E::UpstreamTimeout { .. } => ProviderError::new(
            "openapi_upstream_timeout",
            provider,
            Some(action.to_owned()),
            "OpenAPI upstream request timed out",
            "Check the provider endpoint, input, and credentials, then retry.",
        ),
        E::RequestBlockedPrivateAddr { .. } => ProviderError::validation(
            provider,
            action,
            "openapi_operation_path_escapes_base",
            "OpenAPI provider operation path resolved outside the pinned base_url",
        ),
        other => ProviderError::new(
            "openapi_upstream_error",
            provider,
            Some(action.to_owned()),
            format!("OpenAPI upstream request failed: {other}"),
            "Check the provider endpoint, input, and credentials, then retry.",
        ),
    }
}

#[cfg(test)]
#[path = "openapi_tests.rs"]
mod tests;
