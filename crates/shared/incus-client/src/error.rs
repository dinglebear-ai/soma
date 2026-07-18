/// Errors returned by [`crate::Client`] and every resource method built on
/// top of it.
///
/// `#[non_exhaustive]` because this crate expects to grow new failure modes
/// as more of the Incus API surface is covered - matching exhaustively on
/// this enum today would make every future variant addition a breaking
/// change for downstream crates.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("transport I/O error: {0}")]
    Transport(#[from] std::io::Error),

    #[error("Incus API error (status {status_code}): {message}")]
    Api { status_code: u16, message: String },

    #[error("failed to (de)serialize a request or response body: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("response did not match any known Incus envelope shape: {0}")]
    InvalidResponse(String),

    #[error("response body exceeded the {limit}-byte cap")]
    ResponseTooLarge { limit: usize },

    #[error("operation is not cancellable (may_cancel is false)")]
    NotCancellable,

    #[error("operation {id} failed (status {status_code}): {}", err.as_deref().unwrap_or("no error message"))]
    OperationFailed {
        id: uuid::Uuid,
        status_code: u16,
        err: Option<String>,
    },

    #[error("precondition failed updating {resource} (stale ETag - re-fetch and retry)")]
    PreconditionFailed { resource: String },

    #[error("invalid request: {0}")]
    InvalidRequest(String),

    #[error("request timed out after {after:?}")]
    Timeout { after: std::time::Duration },
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;
