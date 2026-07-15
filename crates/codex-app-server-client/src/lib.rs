//! Standalone async Rust client for the [Codex CLI's `app-server` v2 JSON-RPC
//! protocol](https://developers.openai.com/codex/app-server).
//!
//! `codex app-server` is the interface Codex uses to power rich clients (the
//! VS Code extension, the Codex app). This crate spawns (or connects to) an
//! app-server process, speaks its newline-delimited JSON-RPC 2.0 wire format,
//! and exposes every v2 method as a typed async function.
//!
//! This crate has **zero dependencies on anything else in the workspace it
//! lives in** - only published crates.io dependencies - so it can be lifted
//! into another project wholesale. See `README.md` for how its vendored
//! protocol schema was derived and how to regenerate it.
//!
//! # Quick start
//!
//! ```no_run
//! use codex_app_server_client::protocol::{ClientInfo, InitializeParams};
//! use codex_app_server_client::CodexAppServerClient;
//!
//! # async fn run() -> codex_app_server_client::Result<()> {
//! let (client, mut events) = CodexAppServerClient::spawn("codex", &[])?;
//!
//! client
//!     .initialize(InitializeParams {
//!         client_info: ClientInfo {
//!             name: "my_integration".into(),
//!             title: None,
//!             version: "0.1.0".into(),
//!         },
//!         capabilities: None,
//!     })
//!     .await?;
//! client.send_initialized()?;
//!
//! // Drain events (notifications + approval/elicitation requests) on a
//! // separate task - this is how you observe turn/item streaming.
//! tokio::spawn(async move {
//!     while let Some(event) = events.recv().await {
//!         match event {
//!             codex_app_server_client::Event::Notification(n) => {
//!                 tracing::debug!(?n, "app-server notification");
//!             }
//!             codex_app_server_client::Event::Request(req) => {
//!                 // e.g. execCommandApproval, item/tool/requestUserInput ...
//!                 req.respond_error(-1, "auto-denied by example", None);
//!             }
//!             codex_app_server_client::Event::Closed => break,
//!         }
//!     }
//! });
//!
//! // Every params type derives Serialize/Deserialize with `#[serde(default)]`
//! // on optional fields, so building one from a partial JSON object is a
//! // convenient alternative to naming every field (there's no generated
//! // builder API).
//! let params = serde_json::from_value(serde_json::json!({ "model": "gpt-5.4" }))?;
//! let thread = client.thread_start(params).await?;
//! println!("started thread {}", thread.thread.id);
//! # Ok(())
//! # }
//! ```

#[cfg(test)]
#[path = "build_support.rs"]
mod build_support;
mod client;
mod error;
pub mod protocol;
mod transport;

pub use client::{
    CodexAppServerClient, Event, EventStream, PendingServerRequest, DEFAULT_CALL_TIMEOUT,
    SERVER_NOTIFICATION_METHODS,
};
pub use error::{Error, Result};
pub use transport::MAX_LINE_BYTES;
