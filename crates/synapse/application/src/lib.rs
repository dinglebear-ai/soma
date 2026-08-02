//! Product-owned compatibility and application contracts for Synapse.
//!
//! This crate translates legacy Flux and Scout requests into the neutral
//! operation contracts from `soma-ops`. It does not execute Docker, SSH,
//! filesystem, or host operations and does not depend on the imported donor
//! workspace.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod binding;
mod catalog;
mod diagnostic;
mod error;
mod normalize;
mod projection;
mod schema;

pub use binding::{
    LegacyAccess, LegacyOperationBinding, LegacyPresentation, LegacyTool, LegacyTransport,
};
pub use catalog::SynapseCatalog;
pub use diagnostic::DiagnosticProjection;
pub use error::CompatibilityError;
pub use normalize::NormalizedOperationRequest;
pub use projection::LegacyProjectedResult;
pub use schema::OperationSchemaContract;
