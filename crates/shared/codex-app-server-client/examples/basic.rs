//! Spawns `codex app-server`, completes the handshake, and lists experimental
//! feature flags - a good end-to-end smoke check that doesn't require an
//! active Codex login or start a real (billable) turn.
//!
//! Run with: `cargo run -p codex-app-server-client --example basic`

use codex_app_server_client::{CodexSession, SessionOptions};

#[tokio::main]
async fn main() -> codex_app_server_client::Result<()> {
    tracing_subscriber::fmt::init();

    let session = CodexSession::spawn(
        SessionOptions::new("codex_app_server_client_example", env!("CARGO_PKG_VERSION"))
            .with_title("codex-app-server-client basic example"),
    )
    .await?;
    let init = session.initialize_response();
    println!(
        "connected to codex {} on {}",
        init.platform_family, init.platform_os
    );

    let features = session
        .client()
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
