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

pub async fn execute_operation(
    client: &reqwest::Client,
    op: &OperationHandle,
    params: serde_json::Value,
) -> Result<serde_json::Value, OpenApiError> {
    execute_operation_inner(client, op, params, true).await
}

#[cfg(test)]
pub(crate) async fn execute_operation_no_ssrf(
    client: &reqwest::Client,
    op: &OperationHandle,
    params: serde_json::Value,
) -> Result<serde_json::Value, OpenApiError> {
    execute_operation_inner(client, op, params, false).await
}

async fn execute_operation_inner(
    client: &reqwest::Client,
    op: &OperationHandle,
    params: serde_json::Value,
    enforce_ssrf: bool,
) -> Result<serde_json::Value, OpenApiError> {
    let (used_path_params, url) = params::build_url_with_params(op, &params)?;
    let (send_client, pinned_ip) = if enforce_ssrf {
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
