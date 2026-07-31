// Render per-item feature-requirement badges when rustdoc runs on nightly with
// `--cfg docsrs` (docs.rs posture; locally via `cargo xtask doc --docsrs-cfg`).
// Inert under the stable CI doc gate: stable rustdoc never sets `docsrs`.
#![cfg_attr(docsrs, feature(doc_auto_cfg))]
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

#[cfg(any(feature = "python", feature = "wasm"))]
mod broker_state;
#[cfg(any(feature = "python", feature = "wasm"))]
mod secret_name;

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
#[cfg(feature = "python")]
pub mod python_protocol;

#[cfg(feature = "wasm")]
pub mod wasm;

/// Configure the process-shared durable provider state file before any
/// Python or component provider is constructed.
#[cfg(any(feature = "python", feature = "wasm"))]
pub fn configure_provider_state_path(path: std::path::PathBuf) -> Result<(), String> {
    broker_state::configure(path)
}
#[cfg(feature = "wasm")]
mod wasm_limits;
#[cfg(feature = "wasm")]
mod wasm_memory;

#[cfg(feature = "openapi")]
pub mod openapi;

#[cfg(feature = "codemode")]
pub mod codemode;

#[cfg(feature = "gateway")]
pub mod gateway;

/// Crate version from Cargo metadata.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
