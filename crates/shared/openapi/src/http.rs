pub(crate) mod body;
pub(crate) mod client;
pub(crate) mod params;
pub(crate) mod resolve;

#[cfg(test)]
mod body_tests;
#[cfg(test)]
mod client_tests;
#[cfg(test)]
mod params_tests;
#[cfg(test)]
mod resolve_tests;

use crate::error::OpenApiError;
use crate::registry::OperationHandle;

pub const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;

pub use client::build_dispatch_client;

pub async fn fetch_url_capped(
    url: &url::Url,
    cap: usize,
    label: &str,
) -> Result<String, OpenApiError> {
    let (pinned_client, pinned_ip) = resolve::pinned_client_for(url, label).await?;
    let response = pinned_client
        .get(url.clone())
        .send()
        .await
        .map_err(|error| map_send_error(error, label))?;
    resolve::recheck_peer(&response, pinned_ip, label)?;
    if !response.status().is_success() {
        return Err(OpenApiError::SpecParse {
            label: label.to_string(),
        });
    }
    body::collect_spec_capped(response, cap, label).await
}

/// Which trust boundary `execute_operation_inner` should enforce for a given
/// call. Kept as an enum rather than two independent booleans (`enforce_ssrf`,
/// `lenient_body`) so illegal combinations — like enforcing DNS-pinned SSRF
/// protection while also being lenient about non-JSON bodies, a combination
/// nothing in this crate has ever needed or tested — are unrepresentable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DispatchTrust {
    /// Registry/spec-driven dispatch against arbitrary, potentially
    /// public-internet hosts: enforce the DNS-pinned SSRF guard and require a
    /// valid JSON success body.
    Pinned,
    /// The caller has already restricted the target host through its own
    /// allowlist (e.g. a drop-in provider manifest's declared
    /// `capabilities.network.allowed_hosts`): skip the SSRF guard and
    /// tolerate a non-JSON success body.
    AllowlistedHost,
    /// Test-only: skip the SSRF guard but keep the strict JSON-body
    /// requirement, isolating SSRF-guard behavior from body-parsing
    /// behavior in unit tests.
    #[cfg(test)]
    UnpinnedStrictBody,
}

impl DispatchTrust {
    fn enforce_ssrf(self) -> bool {
        matches!(self, Self::Pinned)
    }

    fn lenient_body(self) -> bool {
        matches!(self, Self::AllowlistedHost)
    }
}

pub async fn execute_operation(
    client: &reqwest::Client,
    op: &OperationHandle,
    params: serde_json::Value,
) -> Result<serde_json::Value, OpenApiError> {
    execute_operation_inner(client, op, params, DispatchTrust::Pinned).await
}

/// Dispatches an operation while skipping this crate's DNS-pinned SSRF guard
/// and tolerating a non-JSON success body (wrapped as `{ "text": <body> }`
/// instead of erroring).
///
/// This exists solely for `soma-provider-adapters::openapi`'s manifest-driven
/// OpenAPI provider. That adapter's trust model is an operator-declared
/// `capabilities.network.allowed_hosts` allowlist rather than public-internet
/// DNS pinning, and the allowlist may legitimately include loopback/private
/// hosts (e.g. a local sidecar) — see that crate's `openapi` module docs.
/// Callers MUST have already restricted `op.base_url`'s host through an
/// equivalent explicit allowlist before calling this. Registry/spec-driven
/// dispatch (`dispatch_openapi_call`) must keep going through
/// `execute_operation`, never this function.
pub async fn execute_operation_for_allowlisted_host(
    client: &reqwest::Client,
    op: &OperationHandle,
    params: serde_json::Value,
) -> Result<serde_json::Value, OpenApiError> {
    execute_operation_inner(client, op, params, DispatchTrust::AllowlistedHost).await
}

#[cfg(test)]
pub(crate) async fn execute_operation_no_ssrf(
    client: &reqwest::Client,
    op: &OperationHandle,
    params: serde_json::Value,
) -> Result<serde_json::Value, OpenApiError> {
    execute_operation_inner(client, op, params, DispatchTrust::UnpinnedStrictBody).await
}

async fn execute_operation_inner(
    client: &reqwest::Client,
    op: &OperationHandle,
    params: serde_json::Value,
    trust: DispatchTrust,
) -> Result<serde_json::Value, OpenApiError> {
    let (used_path_params, url) = params::build_url_with_params(op, &params)?;
    let (send_client, pinned_ip) = if trust.enforce_ssrf() {
        let (client, ip) = resolve::pinned_client_for(&url, &op.operation_id).await?;
        (client, Some(ip))
    } else {
        (client.clone(), None)
    };

    let url = params::apply_query(url, op, &params, &used_path_params);
    let mut request = send_client.request(op.method.clone(), url);
    request = params::inject_credential(request, op);
    request = params::apply_body(request, op, &params, &used_path_params);

    let response = request
        .send()
        .await
        .map_err(|error| map_send_error(error, &op.operation_id))?;
    if let Some(pinned_ip) = pinned_ip {
        resolve::recheck_peer(&response, pinned_ip, &op.operation_id)?;
    }

    if !response.status().is_success() {
        return Err(OpenApiError::UpstreamRequest {
            label: op.operation_id.clone(),
        });
    }
    let body =
        body::collect_response_capped(response, MAX_RESPONSE_BYTES, &op.operation_id).await?;
    if trust.lenient_body() {
        let parsed = serde_json::from_str::<serde_json::Value>(&body)
            .unwrap_or_else(|_| serde_json::json!({ "text": body }));
        return Ok(parsed);
    }
    if body.trim().is_empty() {
        return Ok(serde_json::Value::Null);
    }
    serde_json::from_str(&body).map_err(|_| OpenApiError::UpstreamRequest {
        label: op.operation_id.clone(),
    })
}

pub(crate) fn map_send_error(error: reqwest::Error, label: &str) -> OpenApiError {
    if error.is_timeout() {
        OpenApiError::UpstreamTimeout {
            label: label.to_string(),
        }
    } else {
        OpenApiError::UpstreamRequest {
            label: label.to_string(),
        }
    }
}
