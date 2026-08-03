//! Canonical product application runtime for Synapse.
//!
//! The crate owns the checked-in operation catalog, optional historical request
//! bindings, all canonical reads, and plan-bound verified mutation execution through
//! `soma-fleet` and `soma-infra`. It does not depend on the imported donor workspace.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod binding;
mod catalog;
mod catalog_validation;
mod diagnostic;
mod error;
mod execution_error;
mod mutation_admission;
mod mutation_compose;
mod mutation_dispatch;
mod mutation_pull;
mod mutation_pull_execute;
mod mutation_pull_result;
mod mutation_result;
mod mutation_runtime;
mod normalize;
mod runtime;
mod runtime_docker;
mod runtime_files;
mod runtime_host;
mod runtime_observability;
mod runtime_params;
mod runtime_result;
mod schema;

pub use binding::{
    LegacyAccess, LegacyOperationBinding, LegacyPresentation, LegacyTool, LegacyTransport,
};
pub use catalog::SynapseCatalog;
pub use diagnostic::DiagnosticProjection;
pub use error::CompatibilityError;
pub use execution_error::ExecutionError;
pub use mutation_runtime::{SynapseMutationPorts, SynapseMutationRuntime};
pub use normalize::NormalizedOperationRequest;
pub use runtime::{SynapseReadPorts, SynapseReadRuntime};
pub use schema::OperationSchemaContract;

#[cfg(test)]
#[path = "../tests/support/mutation_pull_compose.rs"]
mod mutation_pull_test_compose;
#[cfg(test)]
#[path = "../tests/support/mutation_pull_support.rs"]
mod mutation_pull_test_support;
#[cfg(test)]
mod runtime_test_docker;
#[cfg(test)]
mod runtime_test_observability;
#[cfg(test)]
mod runtime_test_support;
#[cfg(test)]
#[path = "runtime_tests.rs"]
mod runtime_tests;
