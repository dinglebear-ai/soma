//! Runs the optional REST adapter around Codex app-server.
//!
//! This does not change Codex app-server's native transport. The HTTP routes in
//! this example call `codex app-server` through this crate's JSON-RPC client.
//!
//! `cargo run -p codex-app-server-client --features rest --example rest_server`

use std::net::SocketAddr;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = std::env::var("CODEX_APP_SERVER_REST_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:43210".to_owned())
        .parse::<SocketAddr>()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;

    println!("codex app-server REST adapter listening on http://{addr}");
    println!("POST /v1/text-turn will start a fresh codex app-server session per request");

    axum::serve(listener, codex_app_server_client::rest::router()).await?;
    Ok(())
}
