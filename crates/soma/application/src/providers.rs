//! Provider source implementations feeding the `ProviderRegistry`: static
//! Rust actions, file-backed manifests and resources, and remote catalogs.

/// File-backed provider discovery and manifest loading from `providers/`.
pub mod filesystem;
/// Provider catalogs built from a remote inspection report.
pub mod remote;
/// Providers serving `providers/resources/` files (static and dynamic).
pub mod resource_files;
pub(crate) mod resource_uri;
/// Provider exposing Soma's built-in Rust actions.
pub mod static_rust;
