use serde::{Deserialize, Serialize};
use soma_ops::MutationSendState;

use crate::InfraError;

/// Infrastructure mutation failure with explicit backend send state.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("infrastructure mutation failed after send state {send_state:?}: {error}")]
pub struct MutationFailure {
    send_state: MutationSendState,
    error: InfraError,
}

impl MutationFailure {
    /// Creates a mutation failure without discarding send uncertainty.
    #[must_use]
    pub const fn new(send_state: MutationSendState, error: InfraError) -> Self {
        Self { send_state, error }
    }

    /// Returns whether the mutation may have reached the backend.
    #[must_use]
    pub const fn send_state(&self) -> MutationSendState {
        self.send_state
    }

    /// Returns the underlying infrastructure failure.
    #[must_use]
    pub const fn error(&self) -> &InfraError {
        &self.error
    }

    /// Consumes the wrapper and returns the infrastructure failure.
    #[must_use]
    pub fn into_error(self) -> InfraError {
        self.error
    }
}

/// Result type for infrastructure mutations.
pub type MutationResult<T> = Result<T, MutationFailure>;

/// Stable verification detail for a mutation outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationVerification {
    /// Verification status text suitable for canonical result details.
    pub status: String,
    /// Bounded verification explanation.
    pub summary: String,
}

#[cfg(test)]
#[path = "mutation_tests.rs"]
mod tests;
