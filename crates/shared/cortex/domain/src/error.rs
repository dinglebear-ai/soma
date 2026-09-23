use thiserror::Error;

/// Storage- and transport-neutral failures produced while evaluating Cortex
/// domain rules.
///
/// Operational failures such as SQLite busy/timeout, constraint violations,
/// pool starvation, and opaque runtime errors intentionally remain outside the
/// domain crate. Adapters translate those failures at the application boundary.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DomainError {
    /// Caller-supplied or derived input violates a domain invariant.
    #[error("{0}")]
    InvalidInput(String),
    /// A requested semantic entity cannot be resolved.
    #[error("{0}")]
    NotFound(String),
}

/// Result type for storage-neutral Cortex domain operations.
pub type DomainResult<T> = Result<T, DomainError>;

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;
