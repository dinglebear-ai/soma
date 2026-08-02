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
    /// Operation plan construction or fingerprint validation failed.
    #[error(transparent)]
    Plan(#[from] soma_ops::PlanError),
    /// Product authorization evidence did not match the mutation request.
    #[error(transparent)]
    Authorization(#[from] soma_ops::AuthorizationError),
    /// Canonical operation result construction failed.
    #[error(transparent)]
    Result(#[from] soma_ops::ResultError),
    /// Canonical target construction failed.
    #[error(transparent)]
    Target(#[from] soma_ops::TargetRefError),
    /// Infrastructure mutation failed before a terminal result could be built.
    #[error(transparent)]
    Mutation(#[from] soma_infra::MutationFailure),
    /// The operation exists but is not handled by this runtime.
    #[error("canonical runtime cannot execute {0}")]
    UnsupportedOperation(OperationName),
    /// Supplied plan does not match the exact operation, context, target, or topology.
    #[error("mutation plan mismatch: {0}")]
    PlanMismatch(String),
    /// An idempotent mutation omitted its required idempotency key.
    #[error("idempotent mutation requires an idempotency key")]
    MissingIdempotencyKey,
    /// Synapse policy requires explicit confirmation for disruptive mutations.
    #[error("disruptive mutation requires a confirmation reference")]
    ConfirmationRequired,
    /// A required product mutation port was not configured.
    #[error("{domain} mutation port is unavailable for host {host}")]
    MutationPortUnavailable {
        /// Mutation domain.
        domain: &'static str,
        /// Host identity.
        host: String,
    },
    /// The mutation deadline passed before admission.
    #[error("mutation deadline has passed")]
    DeadlineExceeded,
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

#[cfg(test)]
#[path = "execution_error_tests.rs"]
mod tests;
