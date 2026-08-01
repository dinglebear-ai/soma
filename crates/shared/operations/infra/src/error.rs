use std::path::PathBuf;

use soma_fleet::{FleetError, HostId};

/// Result type for neutral infrastructure reads.
pub type InfraResult<T> = Result<T, InfraError>;

/// Product-neutral infrastructure inspection failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum InfraError {
    /// Fleet transport or topology failed.
    #[error(transparent)]
    Fleet(#[from] FleetError),
    /// A typed request violated a closed contract.
    #[error("invalid {domain} request: {message}")]
    InvalidRequest {
        /// Infrastructure domain.
        domain: &'static str,
        /// Corrective detail.
        message: String,
    },
    /// The target transport cannot execute the requested driver.
    #[error("{domain} inspection is unsupported for host {host}")]
    UnsupportedTarget {
        /// Infrastructure domain.
        domain: &'static str,
        /// Target host.
        host: HostId,
    },
    /// A bounded command returned a non-zero status.
    #[error("{domain} command failed on {host} with exit {exit_code:?}: {stderr}")]
    CommandFailed {
        /// Infrastructure domain.
        domain: &'static str,
        /// Target host.
        host: HostId,
        /// Process exit status when available.
        exit_code: Option<i32>,
        /// Bounded stderr text.
        stderr: String,
    },
    /// Driver output could not be parsed into the neutral contract.
    #[error("failed to parse {domain} output: {message}")]
    Parse {
        /// Infrastructure domain.
        domain: &'static str,
        /// Parse detail.
        message: String,
    },
    /// Descriptor-confined filesystem access failed.
    #[error("filesystem {operation} failed for {path}: {message}")]
    Filesystem {
        /// Read operation.
        operation: &'static str,
        /// Requested path.
        path: PathBuf,
        /// Failure detail.
        message: String,
    },
    /// Requested path was not within an admitted read root.
    #[error("path is outside admitted read roots: {0}")]
    PathOutsideRoots(PathBuf),
    /// Docker API access failed.
    #[error("Docker read failed: {0}")]
    Docker(String),
}

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;
