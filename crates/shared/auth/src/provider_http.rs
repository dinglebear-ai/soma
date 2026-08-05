use std::borrow::Cow;
use std::time::Instant;

use reqwest::Url;
use reqwest::header;
use serde::de::DeserializeOwned;
use tracing::{info, warn};

use crate::error::AuthError;
use crate::oauth_provider::AuthorizeUrlRequest;

/// Installs the process-wide rustls crypto provider, if one isn't already
/// installed.
///
/// rmcp's HTTP transport (and, transitively, reqwest) requires a rustls
/// crypto provider to be installed before the first TLS-capable client is
/// built. The real binary installs one at startup; test binaries never go
/// through that path, so every `OAuthProvider::new` (Google, Authelia,
/// GitHub) also calls this before building its `reqwest::Client`. The
/// `Result` is intentionally discarded — `Err` only means a provider was
/// already installed elsewhere (e.g. by an earlier-constructed provider in
/// the same process), which is safe to ignore.
pub(crate) fn install_rustls_default_once() {
    drop(rustls::crypto::ring::default_provider().install_default());
}

/// Build the common OAuth authorization URL shared by all upstream providers.
///
/// `extra_pairs` keeps provider-specific parameters in the same position as
/// the previous hand-written implementations (Google uses it for offline
/// access; Authelia and GitHub pass an empty slice).
pub(crate) fn build_authorize_url(
    authorize_endpoint: &Url,
    client_id: &str,
    redirect_uri: &Url,
    scopes: &[String],
    request: &AuthorizeUrlRequest,
    extra_pairs: &[(&str, &str)],
) -> Url {
    let mut url = authorize_endpoint.clone();
    let scope = scopes.join(" ");
    let mut pairs = url.query_pairs_mut();
    pairs
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri.as_str())
        .append_pair("response_type", "code")
        .append_pair("scope", &scope);
    for (key, value) in extra_pairs {
        pairs.append_pair(key, value);
    }
    pairs
        .append_pair("state", &request.state)
        .append_pair("code_challenge", &request.code_challenge)
        .append_pair("code_challenge_method", &request.code_challenge_method);
    if request.force_consent {
        pairs.append_pair("prompt", "consent");
    }
    drop(pairs);
    url
}

pub(crate) struct RequestTrace<'a> {
    provider_id: &'static str,
    operation: &'static str,
    method: &'static str,
    endpoint: &'a Url,
    started: Instant,
}

impl<'a> RequestTrace<'a> {
    pub(crate) fn start(
        provider_id: &'static str,
        operation: &'static str,
        method: &'static str,
        endpoint: &'a Url,
    ) -> Self {
        info!(
            provider = provider_id,
            operation,
            method,
            host = endpoint.host_str().unwrap_or_default(),
            path = endpoint.path(),
            "request.start"
        );
        Self {
            provider_id,
            operation,
            method,
            endpoint,
            started: Instant::now(),
        }
    }

    pub(crate) fn finish(&self, status: reqwest::StatusCode) {
        info!(
            provider = self.provider_id,
            operation = self.operation,
            method = self.method,
            host = self.endpoint.host_str().unwrap_or_default(),
            path = self.endpoint.path(),
            status = status.as_u16(),
            elapsed_ms = self.started.elapsed().as_millis(),
            "request.finish"
        );
    }

    pub(crate) fn error(&self, status: Option<reqwest::StatusCode>, error: &reqwest::Error) {
        if let Some(status) = status {
            warn!(
                provider = self.provider_id,
                operation = self.operation,
                method = self.method,
                host = self.endpoint.host_str().unwrap_or_default(),
                path = self.endpoint.path(),
                status = status.as_u16(),
                elapsed_ms = self.started.elapsed().as_millis(),
                error = %error,
                "request.error"
            );
        } else {
            warn!(
                provider = self.provider_id,
                operation = self.operation,
                method = self.method,
                host = self.endpoint.host_str().unwrap_or_default(),
                path = self.endpoint.path(),
                elapsed_ms = self.started.elapsed().as_millis(),
                error = %error,
                "request.error"
            );
        }
    }
}

pub(crate) struct RequestErrors {
    provider_id: &'static str,
    transport_context: Cow<'static, str>,
    status_context: Cow<'static, str>,
    decode_context: Cow<'static, str>,
}

impl RequestErrors {
    pub(crate) fn new<T, S, D>(
        provider_id: &'static str,
        transport_context: T,
        status_context: S,
        decode_context: D,
    ) -> Self
    where
        T: Into<Cow<'static, str>>,
        S: Into<Cow<'static, str>>,
        D: Into<Cow<'static, str>>,
    {
        Self {
            provider_id,
            transport_context: transport_context.into(),
            status_context: status_context.into(),
            decode_context: decode_context.into(),
        }
    }
}

/// Cap on how much of an upstream error response body gets echoed into an
/// `AuthError` message / log line — bounds the blast radius of a malicious
/// or buggy upstream sending back an oversized body.
const ERROR_BODY_SNIPPET_MAX_CHARS: usize = 500;
const ERROR_BODY_MAX_BYTES: usize = 64 * 1024;
const SUCCESS_BODY_MAX_BYTES: usize = 1024 * 1024;

async fn read_bounded_body(
    response: &mut reqwest::Response,
    max_bytes: usize,
    context: &str,
) -> Result<Vec<u8>, AuthError> {
    if response
        .content_length()
        .is_some_and(|content_length| content_length > max_bytes as u64)
    {
        return Err(AuthError::Decode(format!(
            "{context}: response body exceeds {max_bytes} bytes"
        )));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| AuthError::Network(format!("{context}: {error}")))?
    {
        if body.len().saturating_add(chunk.len()) > max_bytes {
            return Err(AuthError::Decode(format!(
                "{context}: response body exceeds {max_bytes} bytes"
            )));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

pub(crate) async fn read_json_response<T: DeserializeOwned>(
    trace: RequestTrace<'_>,
    request: reqwest::RequestBuilder,
    errors: RequestErrors,
) -> Result<T, AuthError> {
    let mut response = request.send().await.map_err(|error| {
        let auth_error = AuthError::Network(format!("{}: {error}", errors.transport_context));
        trace.error(None, &error);
        warn!(
            provider = errors.provider_id,
            error = %error,
            kind = auth_error.kind(),
            "{}",
            errors.transport_context
        );
        auth_error
    })?;
    let status = response.status();
    let retry_after_ms = response
        .headers()
        .get(header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map(|seconds| seconds.saturating_mul(1_000));

    // Check the status via `error_for_status_ref` (borrows, doesn't consume)
    // so a non-2xx response's body can still be read afterward by the bounded
    // chunk reader. `error_for_status()` on the owned `Response` would consume
    // it, and the resulting `reqwest::Error` carries only the status line,
    // leaving operators debugging a 400 from a token endpoint with no
    // indication of *why* (e.g. `{"error":"invalid_grant",...}`).
    if let Err(status_error) = response.error_for_status_ref() {
        let body = read_bounded_body(&mut response, ERROR_BODY_MAX_BYTES, &errors.status_context)
            .await
            .unwrap_or_default();
        let body = String::from_utf8_lossy(&body);
        let body_snippet = error_body_snippet(&body);
        let auth_error = if let Some(retry_after_ms) = retry_after_ms {
            // GitHub's secondary rate limit (abuse detection) responds with
            // 403, not 429, but does carry `Retry-After` — trust the header's
            // presence over the exact status code so we don't miss it.
            AuthError::RateLimited {
                message: format!("{}: {status}{body_snippet}", errors.status_context),
                retry_after_ms,
            }
        } else if status.is_server_error() {
            AuthError::Server(format!(
                "{}: {status_error}{body_snippet}",
                errors.status_context
            ))
        } else {
            AuthError::AuthFailed(format!(
                "{}: {status_error}{body_snippet}",
                errors.status_context
            ))
        };
        trace.error(Some(status), &status_error);
        warn!(
            provider = errors.provider_id,
            error = %status_error,
            kind = auth_error.kind(),
            "{}",
            errors.status_context
        );
        return Err(auth_error);
    }

    trace.finish(status);
    let body = read_bounded_body(
        &mut response,
        SUCCESS_BODY_MAX_BYTES,
        &errors.decode_context,
    )
    .await?;
    serde_json::from_slice::<T>(&body).map_err(|error| {
        let auth_error = AuthError::Decode(format!("{}: {error}", errors.decode_context));
        warn!(
            provider = errors.provider_id,
            error = %error,
            kind = auth_error.kind(),
            "{}",
            errors.decode_context
        );
        auth_error
    })
}

/// Bounded-length ` - <body>` suffix for an error message/log line. Returns
/// an empty string (so callers can append it directly with no dangling
/// separator) when the body is empty or unreadable. Invalid UTF-8 is decoded
/// with `String::from_utf8_lossy`; "unreadable" means the bounded chunk read
/// itself failed, handled by the caller's `.unwrap_or_default()`.
fn error_body_snippet(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let snippet: String = trimmed.chars().take(ERROR_BODY_SNIPPET_MAX_CHARS).collect();
    format!(" - {snippet}")
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{RequestErrors, RequestTrace, build_authorize_url, read_json_response};
    use crate::error::AuthError;
    use crate::oauth_provider::AuthorizeUrlRequest;

    #[derive(Debug, Deserialize)]
    struct TestPayload {
        #[allow(dead_code)]
        value: String,
    }

    fn test_errors() -> RequestErrors {
        RequestErrors::new(
            "test-provider",
            "transport failed",
            "status error",
            "decode failed",
        )
    }

    #[test]
    fn authorize_url_builder_preserves_common_extra_and_consent_parameters() {
        let endpoint = reqwest::Url::parse("https://example.test/authorize").unwrap();
        let redirect = reqwest::Url::parse("https://client.test/auth/callback").unwrap();
        let request = AuthorizeUrlRequest {
            state: "state".to_string(),
            code_challenge: "challenge".to_string(),
            code_challenge_method: "S256".to_string(),
            force_consent: true,
        };

        let url = build_authorize_url(
            &endpoint,
            "client-id",
            &redirect,
            &["openid".to_string(), "email".to_string()],
            &request,
            &[("access_type", "offline")],
        );
        let pairs: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();

        assert_eq!(
            pairs.get("client_id").map(String::as_str),
            Some("client-id")
        );
        assert_eq!(
            pairs.get("redirect_uri").map(String::as_str),
            Some("https://client.test/auth/callback")
        );
        assert_eq!(pairs.get("scope").map(String::as_str), Some("openid email"));
        assert_eq!(
            pairs.get("access_type").map(String::as_str),
            Some("offline")
        );
        assert_eq!(pairs.get("state").map(String::as_str), Some("state"));
        assert_eq!(pairs.get("prompt").map(String::as_str), Some("consent"));
    }

    /// GitHub's secondary rate limit responds with 403 (not 429) but carries
    /// `Retry-After` — this must classify as `AuthError::RateLimited` with
    /// the header value converted to milliseconds, not `AuthFailed`.
    #[tokio::test]
    async fn a_403_with_retry_after_classifies_as_rate_limited() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rate-limited"))
            .respond_with(
                ResponseTemplate::new(403)
                    .insert_header("Retry-After", "5")
                    .set_body_json(serde_json::json!({"message": "rate limited"})),
            )
            .mount(&server)
            .await;
        let url = server
            .uri()
            .parse::<reqwest::Url>()
            .unwrap()
            .join("/rate-limited")
            .unwrap();
        let client = reqwest::Client::new();
        let trace = RequestTrace::start("test-provider", "op", "GET", &url);

        let error =
            read_json_response::<TestPayload>(trace, client.get(url.clone()), test_errors())
                .await
                .unwrap_err();
        match error {
            AuthError::RateLimited { retry_after_ms, .. } => {
                assert_eq!(retry_after_ms, 5_000);
            }
            other => panic!("expected RateLimited, got {other:?}"),
        }
    }

    /// A generic 4xx with no `Retry-After` header must classify as
    /// `AuthError::AuthFailed`, not `RateLimited` or `Server`.
    #[tokio::test]
    async fn a_generic_4xx_without_retry_after_classifies_as_auth_failed() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/bad-request"))
            .respond_with(
                ResponseTemplate::new(400)
                    .set_body_json(serde_json::json!({"message": "bad request"})),
            )
            .mount(&server)
            .await;
        let url = server
            .uri()
            .parse::<reqwest::Url>()
            .unwrap()
            .join("/bad-request")
            .unwrap();
        let client = reqwest::Client::new();
        let trace = RequestTrace::start("test-provider", "op", "GET", &url);

        let error =
            read_json_response::<TestPayload>(trace, client.get(url.clone()), test_errors())
                .await
                .unwrap_err();
        assert!(
            matches!(error, AuthError::AuthFailed(_)),
            "expected AuthFailed, got {error:?}"
        );
    }

    /// A transport-level failure (connection refused) must classify as
    /// `AuthError::Network`, not surface as a raw `reqwest::Error`.
    #[tokio::test]
    async fn a_transport_failure_classifies_as_network_error() {
        // Bind a listener to grab a free port, then drop it immediately so
        // the port is guaranteed closed — connecting to it fails fast with
        // "connection refused" instead of relying on an arbitrary unused
        // port number that might coincidentally be listened on.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let url = reqwest::Url::parse(&format!("http://{addr}/unreachable")).unwrap();
        let client = reqwest::Client::new();
        let trace = RequestTrace::start("test-provider", "op", "GET", &url);

        let error =
            read_json_response::<TestPayload>(trace, client.get(url.clone()), test_errors())
                .await
                .unwrap_err();
        assert!(
            matches!(error, AuthError::Network(_)),
            "expected Network, got {error:?}"
        );
    }

    /// A 200 response whose body doesn't match the target type must
    /// classify as `AuthError::Decode`, not panic or surface a raw
    /// deserialization error.
    #[tokio::test]
    async fn a_200_response_with_an_unexpected_body_shape_classifies_as_decode_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/ok"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"unexpected": "shape"})),
            )
            .mount(&server)
            .await;
        let url = server
            .uri()
            .parse::<reqwest::Url>()
            .unwrap()
            .join("/ok")
            .unwrap();
        let client = reqwest::Client::new();
        let trace = RequestTrace::start("test-provider", "op", "GET", &url);

        let error =
            read_json_response::<TestPayload>(trace, client.get(url.clone()), test_errors())
                .await
                .unwrap_err();
        assert!(
            matches!(error, AuthError::Decode(_)),
            "expected Decode, got {error:?}"
        );
    }

    #[tokio::test]
    async fn an_oversized_success_body_is_rejected_before_json_decode() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/oversized-success"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![
                b'x';
                super::SUCCESS_BODY_MAX_BYTES
                    + 1
            ]))
            .mount(&server)
            .await;
        let url = server
            .uri()
            .parse::<reqwest::Url>()
            .unwrap()
            .join("/oversized-success")
            .unwrap();
        let client = reqwest::Client::new();
        let trace = RequestTrace::start("test-provider", "op", "GET", &url);

        let error =
            read_json_response::<TestPayload>(trace, client.get(url.clone()), test_errors())
                .await
                .unwrap_err();
        assert!(matches!(error, AuthError::Decode(_)), "{error:?}");
        assert!(error.to_string().contains("exceeds"), "{error}");
    }

    #[tokio::test]
    async fn an_oversized_error_body_does_not_get_buffered_or_reflected() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/oversized-error"))
            .respond_with(ResponseTemplate::new(400).set_body_bytes(vec![
                b's';
                super::ERROR_BODY_MAX_BYTES
                    + 1
            ]))
            .mount(&server)
            .await;
        let url = server
            .uri()
            .parse::<reqwest::Url>()
            .unwrap()
            .join("/oversized-error")
            .unwrap();
        let client = reqwest::Client::new();
        let trace = RequestTrace::start("test-provider", "op", "GET", &url);

        let error =
            read_json_response::<TestPayload>(trace, client.get(url.clone()), test_errors())
                .await
                .unwrap_err();
        assert!(matches!(error, AuthError::AuthFailed(_)), "{error:?}");
        assert!(error.to_string().len() < 1_000, "{error}");
    }
}
