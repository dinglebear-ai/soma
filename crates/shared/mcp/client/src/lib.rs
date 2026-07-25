// Render per-item feature-requirement badges when rustdoc runs on nightly with
// `--cfg docsrs` (docs.rs posture; locally via `cargo xtask doc --docsrs-cfg`).
// Inert under the stable CI doc gate: stable rustdoc never sets `docsrs`.
#![cfg_attr(docsrs, feature(doc_auto_cfg))]
//! Reusable outbound MCP client runtime.
//!
//! This crate owns upstream configuration, stdio and HTTP/WebSocket transport
//! setup, upstream discovery, response caps, and per-upstream call helpers.

pub mod config;
pub mod net;
#[cfg(feature = "oauth")]
pub mod oauth;
pub mod process;
pub mod security;
pub mod upstream;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("{field}: {message}")]
    InvalidField {
        field: &'static str,
        message: String,
    },
}

impl ConfigError {
    pub(crate) fn invalid(field: &'static str, message: impl Into<String>) -> Self {
        Self::InvalidField {
            field,
            message: message.into(),
        }
    }
}

/// Crate version from Cargo metadata.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
