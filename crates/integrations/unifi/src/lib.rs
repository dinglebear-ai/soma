// The README is this crate's entire rustdoc landing page — not just a
// GitHub-facing summary. Every code block in it is a real doctest run by
// `cargo test --doc`, and there is deliberately no separate, shorter `//!`
// summary here to drift out of sync with it: one doc, one source of truth.
#![doc = include_str!("../README.md")]
#![deny(missing_docs)]
#![forbid(unsafe_code)]
// A library crate should never panic on data it doesn't control. Scoped to
// non-test builds only — `.unwrap()`/`.expect()` in test code is normal,
// idiomatic Rust, not a smell, and this crate's test suite uses both
// extensively and correctly. The handful of non-test sites this denies by
// default all have a documented, build-time-only justification (a bundled
// data file that ships with the crate, not caller input) and are
// explicitly `#[allow]`'d at that exact site — this exists so a *new*
// unwrap/expect/panic in production code has to be a deliberate, reviewed
// choice, not an accident.
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

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
pub use config::{DEFAULT_REQUEST_TIMEOUT, UnifiConfig};
pub use error::{Result, UnifiError};
pub use service::UnifiService;
