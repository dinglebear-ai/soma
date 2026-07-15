//! Spawns `codex app-server`, completes the handshake, and lists experimental
//! feature flags - a good end-to-end smoke check that doesn't require an
//! active Codex login or start a real (billable) turn.
//!
//! Run with: `cargo run -p codex-app-server-client --example basic`

use codex_app_server_client::protocol::{ClientInfo, InitializeParams};
use codex_app_server_client::{CodexAppServerClient, Event};

#[tokio::main]
async fn main() -> codex_app_server_client::Result<()> {
    tracing_subscriber::fmt::init();

    let (client, mut events) = CodexAppServerClient::spawn("codex", &[])?;

    // Drain events on a background task. We don't expect any server->client
    // requests before we make our first turn, but notifications can still
    // arrive at any point after `initialized`.
    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            match event {
                Event::Notification(n) => tracing::debug!(method = n.method_name(), "notification"),
                Event::Request(req) => {
                    tracing::warn!(method = req.method_name(), "unexpected server request");
                    req.respond_error(-32000, "no handler registered in this example", None);
                }
                Event::Closed => break,
            }
        }
    });

    let init = client
        .initialize(InitializeParams {
            client_info: ClientInfo {
                name: "codex_app_server_client_example".into(),
                title: Some("codex-app-server-client basic example".into()),
                version: env!("CARGO_PKG_VERSION").into(),
            },
            capabilities: None,
        })
        .await?;
    println!(
        "connected to codex {} on {}",
        init.platform_family, init.platform_os
    );
    client.send_initialized()?;

    let features = client
        .experimental_feature_list(serde_json::from_value(serde_json::json!({}))?)
        .await?;
    println!("{} experimental feature(s):", features.data.len());
    for feature in features.data {
        println!(
            "  {} [{:?}] enabled={}",
            feature.name, feature.stage, feature.enabled
        );
    }

    Ok(())
}
