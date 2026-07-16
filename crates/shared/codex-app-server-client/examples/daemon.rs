//! Connect to an app-server listening on a Unix socket.
//!
//! Start one separately with the printed command, then run this example.

#[cfg(unix)]
#[tokio::main]
async fn main() -> codex_app_server_client::Result<()> {
    use codex_app_server_client::{CodexDaemon, SessionOptions};

    let socket = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/codex-app-server.sock".to_owned());
    let daemon = CodexDaemon::new(socket);
    eprintln!(
        "start command: codex {}",
        daemon.app_server_args().join(" ")
    );

    let session = daemon
        .connect(SessionOptions::new(
            "codex_app_server_client_daemon_example",
            env!("CARGO_PKG_VERSION"),
        ))
        .await?;
    println!(
        "connected to {} on {}",
        session.initialize_response().platform_family,
        session.initialize_response().platform_os
    );
    Ok(())
}

#[cfg(not(unix))]
fn main() {
    eprintln!("daemon socket example requires Unix");
}
