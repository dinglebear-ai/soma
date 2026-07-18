use std::path::PathBuf;
use std::time::Duration;

/// Default per-request timeout applied to every plain HTTP call made over
/// the transport (see `transport::unix::execute_capped`). Chosen as a sane
/// default for a local Unix-socket daemon call - `wait_for_operation`'s own
/// explicit `timeout` parameter is unrelated and unaffected by this (it
/// already has its own bounded semantics via the server-side `timeout`
/// query param on Incus's long-poll `.../wait` endpoint).
pub(crate) const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Connection configuration for [`crate::Client`].
///
/// This epic only supports a local Unix-socket target. A `remote(url)`
/// constructor for the mutual-TLS transport is intentionally absent - see
/// the crate root doc comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientConfig {
    pub(crate) socket_path: PathBuf,
    pub(crate) request_timeout: Option<Duration>,
}

impl ClientConfig {
    /// Configure a client that connects to the Incus daemon over the given
    /// Unix domain socket path (e.g. `/var/lib/incus/unix.socket`).
    ///
    /// Defaults to a 30-second per-request timeout - override it with
    /// [`ClientConfig::with_request_timeout`].
    #[must_use]
    pub fn unix_socket(path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: path.into(),
            request_timeout: Some(DEFAULT_REQUEST_TIMEOUT),
        }
    }

    /// Overrides the default 30-second per-request timeout. Pass `None` to
    /// disable it and wait indefinitely.
    #[must_use]
    pub fn with_request_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.request_timeout = timeout;
        self
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
