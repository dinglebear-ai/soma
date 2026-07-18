//! Standalone client for UniFi Network Controllers: authentication, both the
//! official and internal REST APIs, capability discovery, and dynamic action
//! dispatch.
//!
//! This crate has no dependency on `rmcp`, `axum`, or anything soma-specific
//! — it only knows how to talk to a UniFi controller. It's meant to be
//! embedded by an MCP server, a CLI, an HTTP handler, or anything else; see
//! [`crate::service::UnifiService`] for the layer most embedders should
//! build on rather than [`UnifiClient`] directly.
//!
//! # Quick start
//!
//! ```no_run
//! use unifi::{UnifiClient, UnifiConfig};
//!
//! # async fn run() -> Result<(), unifi::UnifiError> {
//! let client = UnifiClient::new(&UnifiConfig {
//!     url: "https://unifi.local".to_string(),
//!     api_key: std::env::var("UNIFI_API_KEY").unwrap_or_default(),
//!     ..UnifiConfig::default()
//! })?;
//!
//! let clients = client.clients().await?;
//! println!("{clients}");
//! # Ok(())
//! # }
//! ```
//!
//! # Error handling
//!
//! Every fallible function returns [`UnifiError`] (aliased as [`Result`]),
//! never `anyhow::Error` or a boxed `dyn Error` — match on it when a caller
//! needs to react differently to, say, an expired API key versus an
//! unreachable controller.
//!
//! # Layout
//!
//! - `client` / [`UnifiClient`] — the pooled HTTP client and its named,
//!   fixed endpoints (`clients`, `devices`, `wlans`, ...).
//! - `service` / [`UnifiService`] — the facade embedders should depend on.
//! - [`actions`] — dynamic action dispatch ([`ActionDispatcher`]) driven by
//!   the [`capabilities`] catalog.
//! - [`api`] — path/URL construction for the official and internal APIs.
//! - [`capabilities`] — the action catalog, built from the JSON inventories
//!   in `data/`.
//! - `config` / [`UnifiConfig`] — connection configuration.
//! - [`http`] — the one place HTTP requests are made and errors mapped.
//! - [`error`] — [`UnifiError`].

#![deny(missing_docs)]

/// Dynamic action dispatch driven by the [`capabilities`] catalog.
pub mod actions;
/// Path/URL construction for the official and internal controller APIs.
pub mod api;
/// The action catalog, built from the JSON inventories in `data/`.
pub mod capabilities;
/// [`UnifiError`] and the crate's [`Result`] alias.
pub mod error;
/// The one place HTTP requests are made and errors mapped.
pub mod http;

mod client;
mod config;
mod service;
mod util;

pub use actions::{ActionDispatcher, ActionRequest};
pub use client::UnifiClient;
pub use config::{UnifiConfig, DEFAULT_REQUEST_TIMEOUT};
pub use error::{Result, UnifiError};
pub use service::UnifiService;
