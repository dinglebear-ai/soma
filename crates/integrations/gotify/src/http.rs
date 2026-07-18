//! Shared HTTP plumbing: one pooled [`reqwest::Client`] per server, one place
//! that maps transport and status-code failures to [`GotifyError`].
//!
//! [`GotifyClient`](crate::GotifyClient)'s every method calls
//! [`request_json`](crate::http::request_json) with the same borrowed
//! `Client` rather than each building their own — a fresh `reqwest::Client`
//! per call defeats connection pooling and keep-alive under load, so build
//! one with [`build_client`](crate::http::build_client) and reuse it for the
//! client's lifetime.

use std::time::Duration;

use reqwest::{Client, Method, StatusCode};
use serde_json::Value;

use crate::error::{GotifyError, Result};
use crate::GotifyConfig;

/// Builds the pooled HTTP client used for every request against one server.
///
/// # Errors
/// Returns [`GotifyError::ClientBuild`] if `reqwest` fails to construct the
/// client.
pub fn build_client(cfg: &GotifyConfig) -> Result<Client> {
    reqwest::ClientBuilder::new()
        .timeout(cfg.request_timeout)
        .build()
        .map_err(GotifyError::ClientBuild)
}

/// Issues one request against `base_url` using the caller-supplied `client`,
/// and maps the transport/status outcome into a [`Result`].
///
/// `token`, when `Some`, is sent as the `X-Gotify-Key` header — `None` for
/// the two unauthenticated endpoints (`health`, `version`). `action` is a
/// friendly name (e.g. `"send_message"`) used only for the `tracing`
/// span/logs this wraps the request in — every
/// [`GotifyClient`](crate::GotifyClient) method funnels through this one
/// function, so this is the one place instrumentation needs to live for it
/// to cover all of them consistently.
///
/// # Errors
/// Returns [`GotifyError::Timeout`] / [`GotifyError::Connect`] /
/// [`GotifyError::Request`] for transport failures,
/// [`GotifyError::Unauthorized`] / [`GotifyError::NotFound`] /
/// [`GotifyError::RateLimited`] for the status codes Gotify uses for those
/// conditions, and [`GotifyError::UnexpectedStatus`] for any other
/// non-success status (its `body` is JSON when the response was JSON,
/// otherwise the raw text). A `204 No Content` (used by Gotify's delete
/// endpoints) returns a synthetic `{"status": "ok"}`. Otherwise returns
/// [`GotifyError::Decode`] if the body isn't valid JSON.
#[tracing::instrument(skip(client, token, query, body), fields(url))]
#[allow(clippy::too_many_arguments)]
pub async fn request_json(
    client: &Client,
    base_url: &str,
    token: Option<&str>,
    action: &str,
    method: Method,
    path: &str,
    query: Option<&[(&str, String)]>,
    body: Option<&Value>,
) -> Result<Value> {
    let url = format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        path.trim_start_matches('/')
    );
    tracing::Span::current().record("url", tracing::field::display(&url));
    let result = send_request(client, &url, token, method, query, body).await;
    match &result {
        Ok(value) => {
            let count = value
                .get("messages")
                .or(Some(value))
                .and_then(Value::as_array)
                .map(Vec::len);
            tracing::debug!(action, count, "upstream call ok");
        }
        Err(error) => tracing::warn!(action, %error, "upstream call failed"),
    }
    result
}

async fn send_request(
    client: &Client,
    url: &str,
    token: Option<&str>,
    method: Method,
    query: Option<&[(&str, String)]>,
    body: Option<&Value>,
) -> Result<Value> {
    let url = url.to_string();
    let mut request = client.request(method.clone(), &url);
    if let Some(token) = token {
        request = request.header("X-Gotify-Key", token);
    }
    if let Some(query) = query {
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

    // Gotify's delete endpoints return 204 with no body.
    if status.as_u16() == 204 {
        return Ok(serde_json::json!({ "status": "ok" }));
    }

    match status {
        StatusCode::UNAUTHORIZED => return Err(GotifyError::Unauthorized(url)),
        StatusCode::NOT_FOUND => {
            return Err(GotifyError::NotFound {
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
            return Err(GotifyError::RateLimited {
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
    // perfectly diagnosable "HTTP 500" into an opaque `GotifyError::Decode`
    // whenever the error body isn't JSON — which upstream error pages
    // commonly aren't. Capture the body best-effort instead.
    if !status.is_success() {
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice::<Value>(&bytes)
                .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()))
        };
        return Err(GotifyError::UnexpectedStatus {
            status: status.as_u16(),
            url,
            body: Box::new(body),
        });
    }

    if bytes.is_empty() {
        return Ok(serde_json::json!({ "status": "ok" }));
    }
    serde_json::from_slice::<Value>(&bytes).map_err(|source| GotifyError::Decode { url, source })
}

fn map_transport_error(method: &Method, url: &str, source: reqwest::Error) -> GotifyError {
    if source.is_timeout() {
        GotifyError::Timeout {
            method: method.to_string(),
            url: url.to_string(),
            source,
        }
    } else if source.is_connect() {
        GotifyError::Connect {
            method: method.to_string(),
            url: url.to_string(),
            source,
        }
    } else {
        GotifyError::Request {
            method: method.to_string(),
            url: url.to_string(),
            source,
        }
    }
}
