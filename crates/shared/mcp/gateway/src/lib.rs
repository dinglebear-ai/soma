//! Reusable MCP aggregation gateway runtime.
//!
//! This crate intentionally starts with a narrow public surface. Runtime modules
//! are added phase by phase behind tests that enforce the dependency direction.

#[cfg(feature = "codemode")]
pub mod codemode_journal;
pub mod config;
pub mod dispatch_helpers;
pub mod gateway;
pub mod registry;
pub mod usage;

pub use soma_mcp_client::{net, process, security, upstream};

/// Crate version from Cargo metadata.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Optional feature names that product crates may enable.
pub const FEATURE_NAMES: &[&str] = &[
    "oauth",
    "codemode",
    "openapi",
    "palette",
    "protected-routes",
];

/// Whether the optional upstream OAuth adapter is compiled in.
pub const OAUTH_ENABLED: bool = cfg!(feature = "oauth");

/// Whether the optional Code Mode gateway adapter is compiled in.
pub const CODEMODE_ENABLED: bool = cfg!(feature = "codemode");

/// Whether the optional OpenAPI gateway adapter is compiled in.
pub const OPENAPI_ENABLED: bool = cfg!(feature = "openapi");

/// Whether the optional palette projection adapter is compiled in.
pub const PALETTE_ENABLED: bool = cfg!(feature = "palette");

/// Whether protected-route support is compiled in.
pub const PROTECTED_ROUTES_ENABLED: bool = cfg!(feature = "protected-routes");

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
