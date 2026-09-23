//! Pure Cortex inventory snapshot contracts.
//!
//! This crate owns the serializable inventory vocabulary shared by collectors,
//! graph projection, storage, and transports. It deliberately contains no SSH,
//! Docker, HTTP, persistence, scheduling, or runtime orchestration code.

/// Bounded inventory constants and collection-safe utility helpers.
pub mod limits;
/// Serializable homelab inventory snapshot schema.
pub mod schema;

pub use schema::*;
