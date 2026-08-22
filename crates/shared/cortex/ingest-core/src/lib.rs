//! Transport-neutral ingest primitives extracted from Cortex.
//!
//! This crate deliberately owns only hot-path transformations that are useful
//! without Cortex storage, runtime, MCP, HTTP, CLI, or deployment code.
//! Consumers can normalize log messages into stable error signatures and bound
//! untrusted metadata before handing records to their own persistence layer.

#![warn(missing_docs)]

/// Bounded, redacted JSON metadata helpers for ingest pipelines.
pub mod metadata;
/// Log-message normalization and stable signature hashing.
pub mod normalize;
