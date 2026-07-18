//! Typed errors for the UniFi client.
//!
//! Every fallible public function in this crate returns [`Result`], never a
//! boxed/opaque error. Callers that need to react differently to "the API
//! key was rejected" versus "the controller is unreachable" can match on
//! [`UnifiError`] instead of parsing a message string.

use std::time::Duration;

use thiserror::Error;

/// Result alias for this crate's fallible operations.
pub type Result<T> = std::result::Result<T, UnifiError>;

/// Everything that can go wrong talking to a UniFi controller.
///
/// Marked `#[non_exhaustive]`: this crate is meant to be published and
/// consumed externally, so a caller that matches on this must include a
/// wildcard arm — adding a new variant (e.g. a future status-class split)
/// must never be a semver-breaking change for downstream code.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum UnifiError {
    /// `UnifiConfig::url` was empty.
    #[error(
        "UNIFI_URL is not set - set it to your controller's base URL, e.g. UNIFI_URL=https://unifi.local"
    )]
    MissingUrl,

    /// `UnifiConfig::api_key` was empty.
    #[error("UNIFI_API_KEY is not set - generate an API key in UniFi Settings > API")]
    MissingApiKey,

    /// The underlying `reqwest::Client` failed to build.
    #[error("failed to build HTTP client: {0}")]
    ClientBuild(#[source] reqwest::Error),

    /// The request exceeded the client timeout.
    #[error(
        "{method} {url} timed out - check UNIFI_SKIP_TLS_VERIFY=true for self-signed certs, or verify the controller is reachable"
    )]
    Timeout {
        /// HTTP method of the request that timed out.
        method: String,
        /// Full request URL.
        url: String,
        /// Underlying transport error (kept for `Error::source()` chain-walking
        /// even though the message above doesn't repeat its text).
        #[source]
        source: reqwest::Error,
    },

    /// The controller could not be reached (DNS, TCP, or TLS handshake failure).
    #[error(
        "UniFi controller at {url} unreachable ({method}) - check UNIFI_URL is correct and the controller is running. For self-signed certs set UNIFI_SKIP_TLS_VERIFY=true"
    )]
    Connect {
        /// HTTP method of the request that failed to connect.
        method: String,
        /// Full request URL.
        url: String,
        /// Underlying transport error (kept for `Error::source()` chain-walking
        /// even though the message above doesn't repeat its text).
        #[source]
        source: reqwest::Error,
    },

    /// A transport-level failure other than timeout or connect.
    #[error("{method} {url} failed: {source}")]
    Request {
        /// HTTP method of the failed request.
        method: String,
        /// Full request URL.
        url: String,
        /// Underlying transport error.
        #[source]
        source: reqwest::Error,
    },

    /// The controller rejected the API key (HTTP 401). Unlike the other
    /// status-class variants this has no `method` field: a rejected key is
    /// rejected the same way for every verb, so there's nothing extra to
    /// carry — not an oversight.
    #[error(
        "UNIFI_API_KEY rejected by {0} (HTTP 401) - generate a new API key in UniFi Settings > API"
    )]
    Unauthorized(
        /// Full request URL.
        String,
    ),

    /// The API key is valid but lacks permission for the request (HTTP 403).
    #[error("UniFi API key lacks permission for {method} {url} (HTTP 403)")]
    Forbidden {
        /// HTTP method of the forbidden request.
        method: String,
        /// Full request URL.
        url: String,
    },

    /// The controller has no such endpoint (HTTP 404).
    #[error("UniFi endpoint not found for {method} {url} (HTTP 404)")]
    NotFound {
        /// HTTP method of the request.
        method: String,
        /// Full request URL.
        url: String,
    },

    /// The controller rejected the request for exceeding its rate limit (HTTP 429).
    #[error(
        "UniFi rate limit exceeded for {method} {url} (HTTP 429){}",
        retry_after.map(|d| format!(" - retry after {}s", d.as_secs())).unwrap_or_default()
    )]
    RateLimited {
        /// HTTP method of the rate-limited request.
        method: String,
        /// Full request URL.
        url: String,
        /// Parsed `Retry-After` response header, when present and expressed
        /// in seconds (the HTTP-date form is not parsed).
        retry_after: Option<Duration>,
    },

    /// A `GET` returned a successful status with no body, where a JSON body was expected.
    #[error("UniFi returned an empty body for {method} {url}")]
    EmptyBody {
        /// HTTP method of the request.
        method: String,
        /// Full request URL.
        url: String,
    },

    /// The response body was not valid JSON.
    #[error("failed to parse JSON response from {url}: {source}")]
    Decode {
        /// Full request URL.
        url: String,
        /// Underlying JSON parse error.
        #[source]
        source: serde_json::Error,
    },

    /// The controller returned a non-success status not covered by a more specific variant.
    #[error("UniFi HTTP {status} from {url}: {body}")]
    UnexpectedStatus {
        /// HTTP status code.
        status: u16,
        /// Full request URL.
        url: String,
        /// Response body, if any was returned.
        body: Box<serde_json::Value>,
    },

    /// [`crate::ActionDispatcher::execute`] was asked to run an action that has no
    /// registered [`crate::capabilities::Capability`].
    #[error("unknown UniFi action: {0}")]
    UnknownAction(String),

    /// A request's parameters were malformed (wrong type, wrong API family, unknown
    /// hybrid target, etc). `context` is an action name or an HTTP method + path,
    /// whichever the caller had on hand.
    #[error("invalid request for {context}: {message}")]
    InvalidRequest {
        /// What was being executed when validation failed.
        context: String,
        /// What was wrong with the request.
        message: String,
    },

    /// Path-template substitution failed (unmatched brace, missing parameter, wrong type).
    #[error("path template error: {0}")]
    PathTemplate(String),

    /// The `*path` connector wildcard segment failed validation (traversal, encoded
    /// separators, or outside the allowed API prefixes).
    #[error("unsafe or unsupported connector path: {0}")]
    ConnectorPath(String),

    /// Hybrid action routing (official vs. internal) failed.
    #[error("hybrid action routing error: {0}")]
    HybridRouting(String),
}
