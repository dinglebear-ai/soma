//! Process shutdown signal.
//!
//! `apps/soma` owns operating-system signals and graceful process shutdown
//! (plan section 3.1). This module is the single place that answers "should
//! the process stop now?" — it wraps `soma_http_server::shutdown_signal` so
//! `http.rs` never reaches into transport plumbing to answer a
//! composition-root question.

use std::future::Future;

/// Resolves when the process receives a shutdown signal (`Ctrl+C`, or
/// `SIGTERM` on Unix). Pass the returned future to
/// `soma_http_server::serve_with_shutdown` to drain in-flight requests
/// before the HTTP server exits.
pub(crate) fn signal() -> impl Future<Output = ()> {
    soma_http_server::shutdown_signal()
}

#[cfg(test)]
#[path = "shutdown_tests.rs"]
mod tests;
