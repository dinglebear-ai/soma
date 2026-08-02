use soma_ops::OperationName;

use crate::CompatibilityError;

/// Failure while executing one canonical Synapse operation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ExecutionError {
    /// Canonical contract validation failed.
    #[error(transparent)]
    Compatibility(#[from] CompatibilityError),
    /// Fleet topology or transport failed.
    #[error(transparent)]
    Fleet(#[from] soma_fleet::FleetError),
    /// Infrastructure driver failed.
    #[error(transparent)]
    Infra(#[from] soma_infra::InfraError),
    /// The operation exists but is not a read operation handled by this runtime.
    #[error("canonical read runtime cannot execute {0}")]
    UnsupportedOperation(OperationName),
    /// No host was supplied and no unambiguous default could be selected.
    #[error("operation requires an explicit host because the topology has multiple candidates")]
    HostRequired,
    /// Requested host was absent from the current topology snapshot.
    #[error("host is not present in the current topology: {0}")]
    HostNotFound(String),
    /// A canonical parameter had an unexpected runtime representation.
    #[error("invalid canonical parameter {field}: {message}")]
    InvalidParameter {
        /// Invalid field.
        field: String,
        /// Corrective detail.
        message: String,
    },
    /// Compose project discovery could not resolve a usable config file.
    #[error("Compose project {project} was not found on host {host}")]
    ProjectNotFound {
        /// Host identity.
        host: String,
        /// Project name.
        project: String,
    },
    /// Typed infrastructure output could not be serialized.
    #[error("canonical result serialization failed: {0}")]
    Serialization(String),
}
