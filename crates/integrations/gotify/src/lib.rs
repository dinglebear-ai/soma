// The README is this crate's entire rustdoc landing page — not just a
// GitHub-facing summary. Every code block in it is a real doctest run by
// `cargo test --doc`, and there is deliberately no separate, shorter `//!`
// summary here to drift out of sync with it: one doc, one source of truth.
#![doc = include_str!("../README.md")]
#![deny(missing_docs)]
#![forbid(unsafe_code)]
// A library crate should never panic on data it doesn't control. Scoped to
// non-test builds only — `.unwrap()`/`.expect()` in test code is normal,
// idiomatic Rust, not a smell. This crate has no bundled data file to parse
// at build time (unlike `unifi`), so unlike that crate this currently has
// zero `#[allow]` exceptions.
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

/// [`GotifyError`] and the crate's [`Result`] alias.
pub mod error;
/// The one place HTTP requests are made and errors mapped.
pub mod http;

mod client;
mod config;
mod service;

pub use client::GotifyClient;
pub use config::{GotifyConfig, DEFAULT_REQUEST_TIMEOUT};
pub use error::{GotifyError, Result};
pub use service::GotifyService;
