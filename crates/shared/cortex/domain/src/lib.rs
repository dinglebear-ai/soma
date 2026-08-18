//! Storage- and transport-neutral Cortex domain contracts.
//!
//! This crate owns product meaning that remains useful when SQLite, HTTP, MCP,
//! CLI, process supervision, and host-specific collectors are replaced. It
//! intentionally does not expose database row types, filesystem paths, scanner
//! implementations, receiver counters, runtime configuration, or transport
//! request/response envelopes.
//!
//! The source was extracted from Cortex donor commit
//! `7edf23fadb94650c2d2a2f9c80111fb44319eea8`. Database-to-domain mapping
//! remains adapter work and belongs to `cortex-storage-sqlite`.

pub mod actor;
pub mod ai;
pub mod error;
pub mod evidence;
pub mod graph;
pub mod heartbeat;
pub mod hook_incident_findings;
pub mod incident_findings;
pub mod investigation;
pub mod logs;
pub mod mcp_incident_findings;
pub mod skill_incident_findings;
pub mod topology;

pub use actor::RequestActor;
pub use ai::*;
pub use error::{DomainError, DomainResult};
pub use evidence::*;
pub use graph::*;
pub use heartbeat::*;
pub use investigation::*;
pub use logs::*;
pub use topology::*;
