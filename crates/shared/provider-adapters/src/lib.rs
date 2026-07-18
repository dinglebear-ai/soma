//! Reusable, product-neutral implementations of `soma-provider-core`
//! contracts, plus feature-gated bridges onto other shared engines
//! (`soma-openapi`, `soma-codemode`, `soma-gateway`). See plan section 3.9
//! and PR10's deviation notes (in this crate's individual module docs) for
//! the reasoning behind what did and did not move here from soma-service.
//!
//! No module here may depend on a `crates/soma/*` or `apps/*` crate under
//! any feature — `cargo tree -p soma-provider-adapters --all-features` must
//! stay shared-only.

#![forbid(unsafe_code)]

pub mod error;
pub mod manifest_file;

#[cfg(feature = "sidecar")]
pub mod sidecar;

#[cfg(feature = "static-echo")]
pub mod static_rust;

#[cfg(feature = "ai-sdk")]
pub mod ai_sdk;

#[cfg(feature = "python")]
pub mod python;
#[cfg(feature = "python")]
mod python_bridge;

#[cfg(feature = "wasm")]
pub mod wasm;

#[cfg(feature = "openapi")]
pub mod openapi;

#[cfg(feature = "codemode")]
pub mod codemode;

#[cfg(feature = "gateway")]
pub mod gateway;

/// Crate version from Cargo metadata.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
