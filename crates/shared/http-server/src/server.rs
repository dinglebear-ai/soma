//! Listener binding and the Axum server run loop.
//!
//! Product binaries decide the bind address, build their own composed
//! `Router`, and choose a shutdown signal (typically
//! [`crate::shutdown_signal`]). This module owns the mechanical part every
//! Axum HTTP surface repeats: bind a [`TcpListener`], hand it and the router
//! to `axum::serve`, and optionally wire graceful shutdown.

use std::fmt;
use std::future::Future;

use axum::Router;
use tokio::net::{TcpListener, ToSocketAddrs};

/// Error binding a listener or running the server loop.
///
/// Non-exhaustive: this crate is shared plumbing consumed by multiple
/// product surfaces, so new failure variants may be added without that
/// being a breaking change for callers that only use `?`/`Display`/
/// `Error::source` rather than exhaustively matching.
#[derive(Debug)]
#[non_exhaustive]
pub enum ServerError {
    /// Binding `addr` failed (e.g. already in use, permission denied).
    Bind {
        addr: String,
        source: std::io::Error,
    },
    /// The `axum::serve` run loop returned an error.
    Serve(std::io::Error),
}

impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServerError::Bind { addr, source } => write!(f, "failed to bind {addr}: {source}"),
            ServerError::Serve(source) => write!(f, "server loop failed: {source}"),
        }
    }
}

impl std::error::Error for ServerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ServerError::Bind { source, .. } | ServerError::Serve(source) => Some(source),
        }
    }
}

/// Bind a TCP listener at `addr`.
///
/// Accepts anything Tokio can resolve to socket addresses — a `SocketAddr`,
/// a `"host:port"` string (including a hostname, resolved via DNS), or any
/// other [`ToSocketAddrs`] implementor — matching `TcpListener::bind`'s own
/// flexibility so callers don't have to pre-parse a config-supplied address
/// string.
///
/// Pass a literal address with port `0` to let the OS assign an ephemeral
/// port (useful in tests); read it back with `listener.local_addr()`.
pub async fn bind<A>(addr: A) -> Result<TcpListener, ServerError>
where
    A: ToSocketAddrs + fmt::Display,
{
    let description = addr.to_string();
    TcpListener::bind(addr)
        .await
        .map_err(|source| ServerError::Bind {
            addr: description,
            source,
        })
}

/// Serve `router` on `listener` until the process is killed.
///
/// No graceful shutdown — prefer [`serve_with_shutdown`] for anything that
/// should drain in-flight requests before exiting.
pub async fn serve(listener: TcpListener, router: Router) -> Result<(), ServerError> {
    axum::serve(listener, router.into_make_service())
        .await
        .map_err(ServerError::Serve)
}

/// Serve `router` on `listener`, draining in-flight requests once `shutdown`
/// resolves.
///
/// `shutdown` is any future — [`crate::shutdown_signal`] is a ready-made one
/// that resolves on `Ctrl+C` or `SIGTERM`.
pub async fn serve_with_shutdown<F>(
    listener: TcpListener,
    router: Router,
    shutdown: F,
) -> Result<(), ServerError>
where
    F: Future<Output = ()> + Send + 'static,
{
    axum::serve(listener, router.into_make_service())
        .with_graceful_shutdown(shutdown)
        .await
        .map_err(ServerError::Serve)
}

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
