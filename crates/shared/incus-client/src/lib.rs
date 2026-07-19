//! Async Rust client for the [Incus REST API](https://linuxcontainers.org/incus/docs/main/rest-api/)
//! (system container and VM management).
//!
//! This crate speaks to the Incus daemon over a **local Unix domain socket
//! only**. Incus also supports a remote mutual-TLS HTTPS transport with a
//! trust-on-first-use certificate model, but that surface is *not*
//! implemented here — it's tracked as a separate follow-up epic pending a
//! real remote consumer, since a from-scratch TLS trust implementation
//! carries real security risk that isn't worth taking on speculatively.
//!
//! Every operation-returning mutation surfaces a [`operations::Operation`]
//! rather than assuming synchronous completion — see
//! [`operations::Client::wait_for_operation`] for the recommended way to
//! wait for one to finish.
//!
//! Unix-only: Incus itself only runs on Linux, and this crate's transport is
//! a Unix domain socket ([`tokio::net::UnixStream`]) with no cross-platform
//! equivalent, so the entire crate compiles to nothing on non-Unix targets
//! (Windows CI, for instance) rather than hard-failing the workspace build -
//! there is no meaningful non-Unix version of a client for a Linux-only
//! daemon.

#![cfg(unix)]

pub mod config;
pub mod error;
pub mod operations;
pub mod resources;
pub mod transport;

#[cfg(feature = "events")]
pub mod events;

pub use config::ClientConfig;
pub use error::{Error, Result};
pub use transport::{Client, WithEtag};
