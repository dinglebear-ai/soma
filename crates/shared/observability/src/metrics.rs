//! Prometheus metrics recorder and renderer.
//!
//! The server installs a single global Prometheus recorder at startup via
//! [`init`], then exposes the gathered metrics over a `/metrics` route that
//! calls [`render`]. Emitting code anywhere in the workspace uses the
//! lightweight `metrics` facade (`metrics::counter!`, `metrics::histogram!`);
//! when no recorder is installed (stdio mode, CLI, tests) those macros are
//! cheap no-ops, so emitting metrics is always safe.
//!
//! Everything here is fail-soft: a metrics problem must never take the server
//! down, so install failures are logged and swallowed and `/metrics` simply
//! reports "not initialized" until a recorder exists.

use std::sync::OnceLock;

use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

static HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

/// Install the global Prometheus recorder. Idempotent: a second call is a no-op,
/// and a failed install is logged rather than propagated so startup continues.
pub fn init() {
    if HANDLE.get().is_some() {
        return;
    }
    match PrometheusBuilder::new().install_recorder() {
        Ok(handle) => {
            // Ignore a race where another thread set it first — either handle works.
            let _ = HANDLE.set(handle);
        }
        Err(error) => {
            tracing::warn!(%error, "failed to install Prometheus recorder; /metrics will be empty");
        }
    }
}

/// Render the current metrics in Prometheus text exposition format, or `None`
/// if no recorder has been installed (so the route can answer 503).
pub fn render() -> Option<String> {
    HANDLE.get().map(PrometheusHandle::render)
}

/// Whether a recorder has been installed. Primarily for tests and diagnostics.
pub fn is_installed() -> bool {
    HANDLE.get().is_some()
}

#[cfg(test)]
#[path = "metrics_tests.rs"]
mod tests;
