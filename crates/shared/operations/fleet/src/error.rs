use crate::{HostId, RequestError, TopologyError, TopologyRevision};

/// Fleet operation result using the shared error vocabulary.
pub type FleetResult<T> = Result<T, FleetError>;

/// Product-neutral fleet discovery, connection, command, and transfer failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum FleetError {
    /// Topology contract validation failed.
    #[error(transparent)]
    Topology(#[from] TopologyError),
    /// Request contract validation failed.
    #[error(transparent)]
    Request(#[from] RequestError),
    /// A requested host did not exist in the current snapshot.
    #[error("fleet target not found: {0}")]
    TargetNotFound(HostId),
    /// The caller bound work to an obsolete host revision.
    #[error("stale topology for {host}: expected {expected}, current revision is {actual}")]
    StaleTopology {
        /// Host whose topology changed.
        host: HostId,
        /// Revision bound by the request or cached connection.
        expected: TopologyRevision,
        /// Current topology revision.
        actual: TopologyRevision,
    },
    /// Cancellation was observed before completion.
    #[error("fleet operation cancelled")]
    Cancelled,
    /// The absolute or per-target deadline elapsed.
    #[error("fleet operation deadline exceeded")]
    DeadlineExceeded,
    /// A connection driver failed.
    #[error("connection to {host} failed: {message}")]
    Connection {
        /// Failed host.
        host: HostId,
        /// Driver-safe diagnostic text.
        message: String,
    },
    /// A command driver failed before returning a process result.
    #[error("command on {host} failed: {message}")]
    Command {
        /// Failed host.
        host: HostId,
        /// Driver-safe diagnostic text.
        message: String,
    },
    /// A post-spawn remote command was detached and may still be running.
    #[error("remote command on {host} detached after {reason}; it may still be running")]
    RemoteCommandDetached {
        /// Remote target.
        host: HostId,
        /// Cancellation or deadline reason.
        reason: &'static str,
    },
    /// A file transfer driver failed.
    #[error("transfer from {source_host} to {destination_host} failed: {message}")]
    Transfer {
        /// Source host.
        source_host: HostId,
        /// Destination host.
        destination_host: HostId,
        /// Driver-safe diagnostic text.
        message: String,
    },
    /// Transfer lifecycle accounting was invalid.
    #[error("invalid transfer lifecycle: {0}")]
    TransferLifecycle(String),
    /// Event emission failed after the underlying action.
    #[error("fleet event sink failed: {0}")]
    EventSink(String),
}

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;
