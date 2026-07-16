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
//! use codex_app_server_client::{CodexSession, DenyAllApprovalHandler, SessionOptions};
//!
//! # async fn run() -> codex_app_server_client::Result<()> {
//! let mut session = CodexSession::spawn(SessionOptions::new("my_integration", "0.1.0")).await?;
//! let result = session
//!     .run_text_turn_with_model_and_handler(
//!         "gpt-5",
//!         "Say hello in one sentence.",
//!         &DenyAllApprovalHandler::default(),
//!     )
//!     .await?;
//! println!("{}", result.agent_message());
//! # Ok(())
//! # }
//! ```

mod approvals;
#[cfg(test)]
#[path = "build_support.rs"]
mod build_support;
mod builders;
mod client;
mod compat;
mod daemon;
mod error;
mod events;
pub mod protocol;
#[cfg(feature = "rest")]
pub mod rest;
mod session;
mod transport;

pub use approvals::{
    AllowAllApprovalHandler, ApprovalFuture, ApprovalHandler, AsyncFnApprovalHandler,
    DenyAllApprovalHandler, FnApprovalHandler, ReadOnlyApprovalHandler, ServerRequestReply,
};
pub use client::{
    CodexAppServerClient, Event, EventStream, PendingServerRequest, DEFAULT_CALL_TIMEOUT,
    SERVER_NOTIFICATION_METHODS,
};
pub use compat::{
    CompatibilityReport, SurfaceSummary, CLIENT_NOTIFICATION_METHOD_COUNT,
    CLIENT_REQUEST_METHOD_COUNT, CODEX_SCHEMA_VERSION, SERVER_NOTIFICATION_METHOD_COUNT,
    SERVER_REQUEST_METHOD_COUNT,
};
pub use daemon::CodexDaemon;
pub use error::{Error, Result};
pub use events::EventCollector;
pub use session::{CodexSession, SessionOptions, TextTurnResult};
pub use transport::MAX_LINE_BYTES;
