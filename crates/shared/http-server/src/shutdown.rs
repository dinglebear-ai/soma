//! Graceful shutdown signal.
//!
//! Resolves on `Ctrl+C` or, on Unix, `SIGTERM` — the two signals a process
//! manager (systemd, Docker, Kubernetes) or an interactive terminal sends to
//! ask a server to stop accepting new work and drain in-flight requests.
//!
//! Known limitation: if registering a signal handler itself fails (e.g. a
//! sandboxed/seccomp-restricted environment that disallows the underlying
//! syscalls), that branch logs an `error` and then never resolves — it does
//! not abort the process, it just stops competing in the `tokio::select!`
//! below so the *other* signal can still trigger shutdown. If both
//! registrations fail, this future never resolves at all and the only way
//! to stop the process becomes an uncatchable `SIGKILL` (no request
//! draining). That failure mode is easy to miss: the only evidence is a
//! startup-time log line, well before the eventual shutdown attempt that
//! silently doesn't work. Since SIGTERM is the primary signal process
//! managers actually send, a failed SIGTERM registration in particular
//! means production `docker stop`/`kubectl delete pod` shutdowns hard-kill
//! instead of draining, not just Ctrl+C in a local terminal.
pub async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(%error, "CTRL+C handler failed");
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => {
                tracing::error!(%error, "SIGTERM handler failed");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
    tracing::info!("shutdown signal received");
}
