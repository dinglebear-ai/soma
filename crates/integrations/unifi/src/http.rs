//! Shared HTTP plumbing: one pooled [`reqwest::Client`] per controller, one
//! place that maps transport and status-code failures to [`UnifiError`].
//!
//! [`UnifiClient`](crate::UnifiClient) and the dynamic action dispatcher both
//! call [`request_json`](crate::http::request_json) with the same borrowed
//! `Client` rather than each building their own — a fresh `reqwest::Client`
//! per call defeats connection pooling and keep-alive under load, so build
//! one with [`build_client`](crate::http::build_client) and reuse it for the
//! client's lifetime.

use std::time::Duration;

use reqwest::{Client, Method, StatusCode};
use serde_json::{json, Value};

use crate::error::{Result, UnifiError};
use crate::UnifiConfig;

/// Builds the pooled HTTP client used for every request against one controller.
///
/// # Errors
/// Returns [`UnifiError::ClientBuild`] if `reqwest` fails to construct the
/// client (in practice, only from an invalid TLS configuration).
pub fn build_client(cfg: &UnifiConfig) -> Result<Client> {
    reqwest::ClientBuilder::new()
        .danger_accept_invalid_certs(cfg.skip_tls_verify)
        .cookie_store(true)
        .timeout(cfg.request_timeout)
        .build()
        .map_err(UnifiError::ClientBuild)
}

/// Issues one request against `base_url` using the caller-supplied `client`,
/// and maps the transport/status outcome into a [`Result`].
///
/// # Errors
/// Returns [`UnifiError::Timeout`] / [`UnifiError::Connect`] / [`UnifiError::Request`]
/// for transport failures, [`UnifiError::Unauthorized`] / [`UnifiError::Forbidden`] /
/// [`UnifiError::NotFound`] / [`UnifiError::RateLimited`] for the status codes UniFi
/// controllers use for those conditions, and [`UnifiError::UnexpectedStatus`] for any
/// other non-success status (its `body` is JSON when the response was JSON, otherwise
/// the raw text). For a success status, returns [`UnifiError::EmptyBody`] for a `GET`
/// with no response body, or [`UnifiError::Decode`] if the body isn't valid
/// JSON.
pub async fn request_json(
    client: &Client,
    base_url: &str,
    api_key: &str,
    method: Method,
    path: &str,
    query: Option<&Value>,
    body: Option<&Value>,
) -> Result<Value> {
    let url = format!("{}{path}", base_url.trim_end_matches('/'));
    let mut request = client
        .request(method.clone(), &url)
        .header("X-API-KEY", api_key)
        .header("Accept", "application/json");

    if let Some(query) = query {
        let query = query
            .as_object()
            .ok_or_else(|| UnifiError::InvalidRequest {
                context: format!("{method} {path}"),
                message: "query must be a JSON object".to_string(),
            })?;
        request = request.query(query);
    }
    if let Some(body) = body {
        request = request.json(body);
    }

    let response = request
        .send()
        .await
        .map_err(|source| map_transport_error(&method, &url, source))?;
    let status = response.status();
    match status {
        StatusCode::UNAUTHORIZED => return Err(UnifiError::Unauthorized(url)),
        StatusCode::FORBIDDEN => {
            return Err(UnifiError::Forbidden {
                method: method.to_string(),
                url,
            })
        }
        StatusCode::NOT_FOUND => {
            return Err(UnifiError::NotFound {
                method: method.to_string(),
                url,
            })
        }
        StatusCode::TOO_MANY_REQUESTS => {
            let retry_after = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok())
                .map(Duration::from_secs);
            return Err(UnifiError::RateLimited {
                method: method.to_string(),
                url,
                retry_after,
            });
        }
        _ => {}
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|source| map_transport_error(&method, &url, source))?;

    // Check the status before doing anything with the body: a non-success
    // response (500, a proxy's HTML error page, ...) is a status failure
    // first and foremost. Parsing it strictly as JSON here would turn a
    // perfectly diagnosable "HTTP 500" into an opaque `UnifiError::Decode`
    // whenever the error body isn't JSON — which upstream error pages
    // commonly aren't. Capture the body best-effort instead.
    if !status.is_success() {
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice::<Value>(&bytes)
                .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()))
        };
        return Err(UnifiError::UnexpectedStatus {
            status: status.as_u16(),
            url,
            body: Box::new(body),
        });
    }

    if bytes.is_empty() {
        if method == Method::GET {
            return Err(UnifiError::EmptyBody {
                method: method.to_string(),
                url,
            });
        }
        return Ok(json!({
            "success": true,
            "status": status.as_u16(),
            "method": method.as_str(),
            "path": path,
        }));
    }
    serde_json::from_slice::<Value>(&bytes).map_err(|source| UnifiError::Decode { url, source })
}

fn map_transport_error(method: &Method, url: &str, source: reqwest::Error) -> UnifiError {
    if source.is_timeout() {
        UnifiError::Timeout {
            method: method.to_string(),
            url: url.to_string(),
            source,
        }
    } else if source.is_connect() {
        UnifiError::Connect {
            method: method.to_string(),
            url: url.to_string(),
            source,
        }
    } else {
        UnifiError::Request {
            method: method.to_string(),
            url: url.to_string(),
            source,
        }
    }
}
