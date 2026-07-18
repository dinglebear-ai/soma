//! [`GotifyError`] and the crate's [`Result`] alias.

use std::time::Duration;

use thiserror::Error;

/// Result alias for this crate's fallible operations.
pub type Result<T> = std::result::Result<T, GotifyError>;

/// Everything that can go wrong talking to a Gotify server.
///
/// Marked `#[non_exhaustive]`: this crate is meant to be published and
/// consumed externally, so a caller that matches on this must include a
/// wildcard arm — adding a new variant must never be a semver break for
/// downstream code.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum GotifyError {
    /// [`crate::GotifyConfig::url`] was empty at [`crate::GotifyClient::new`].
    #[error("GOTIFY_URL is not configured")]
    MissingUrl,

    /// The requested operation needs a client token
    /// ([`crate::GotifyConfig::client_token`]), but none is configured.
    /// Client tokens authenticate management operations (messages,
    /// applications, clients, current user) — create one under **Clients**
    /// in the Gotify web UI.
    #[error(
        "GOTIFY_CLIENT_TOKEN is required for this operation \
         (create one under Clients in the Gotify web UI)"
    )]
    MissingClientToken,

    /// [`crate::GotifyClient::send_message`] needs an app token
    /// ([`crate::GotifyConfig::app_token`]), but none is configured. App
    /// tokens are distinct from client tokens — create one under
    /// **Applications** in the Gotify web UI.
    #[error(
        "GOTIFY_APP_TOKEN is required to send a message \
         (create one under Applications in the Gotify web UI)"
    )]
    MissingAppToken,

    /// The underlying `reqwest::Client` failed to construct.
    #[error("failed to build the HTTP client: {0}")]
    ClientBuild(#[source] reqwest::Error),

    /// HTTP 401 — the configured token was rejected.
    #[error("Gotify rejected the request as unauthorized (HTTP 401): {0}")]
    Unauthorized(String),

    /// HTTP 404.
    #[error("Gotify endpoint not found for {method} {url} (HTTP 404)")]
    NotFound {
        /// HTTP method of the request.
        method: String,
        /// Full request URL.
        url: String,
    },

    /// HTTP 429.
    #[error(
        "Gotify rate limit exceeded for {method} {url} (HTTP 429){}",
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

    /// Any other non-success status. `body` is JSON when the response was
    /// JSON, otherwise the raw text; boxed to keep this enum small.
    #[error("Gotify HTTP {status} for {url}: {body}")]
    UnexpectedStatus {
        /// HTTP status code.
        status: u16,
        /// Full request URL.
        url: String,
        /// Best-effort response body.
        body: Box<serde_json::Value>,
    },

    /// The response body wasn't valid JSON on an otherwise-successful status.
    #[error("failed to decode Gotify response from {url}: {source}")]
    Decode {
        /// Full request URL.
        url: String,
        /// The underlying deserialization failure.
        #[source]
        source: serde_json::Error,
    },

    /// Request timed out.
    #[error("Gotify request timed out: {method} {url}")]
    Timeout {
        /// HTTP method of the request.
        method: String,
        /// Full request URL.
        url: String,
        /// The underlying transport error.
        #[source]
        source: reqwest::Error,
    },

    /// Failed to connect to the configured URL.
    #[error("failed to connect to Gotify at {url} ({method}) — check GOTIFY_URL is reachable")]
    Connect {
        /// HTTP method of the request.
        method: String,
        /// Full request URL.
        url: String,
        /// The underlying transport error.
        #[source]
        source: reqwest::Error,
    },

    /// Any other transport-level failure.
    #[error("Gotify request failed: {method} {url}: {source}")]
    Request {
        /// HTTP method of the request.
        method: String,
        /// Full request URL.
        url: String,
        /// The underlying transport error.
        #[source]
        source: reqwest::Error,
    },
}
