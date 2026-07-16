//! Minimal shape for starting a thread and sending one text turn.
//!
//! This can consume model credits, so run it deliberately:
//! `cargo run -p codex-app-server-client --example session_turn -- "say hi"`

use codex_app_server_client::{CodexSession, DenyAllApprovalHandler, SessionOptions};

#[tokio::main]
async fn main() -> codex_app_server_client::Result<()> {
    let prompt = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "Say hello in one sentence.".to_owned());
    let mut session = CodexSession::spawn(SessionOptions::new(
        "codex_app_server_client_turn_example",
        env!("CARGO_PKG_VERSION"),
    ))
    .await?;

    let result = session
        .run_text_turn_with_model_and_handler("gpt-5", prompt, &DenyAllApprovalHandler::default())
        .await?;

    println!("{}", result.agent_message());
    Ok(())
}
