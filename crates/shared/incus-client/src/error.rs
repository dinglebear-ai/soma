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

    #[error("{resource} not found")]
    NotFound { resource: String },

    #[error("invalid request: {0}")]
    InvalidRequest(String),

    /// A WebSocket-level failure from the `events` feature's
    /// `/1.0/events` subscription that is *not* a plain socket I/O error
    /// (those still map to `Transport`) - a protocol violation, oversized
    /// frame, or similar. Kept separate from `Transport` so a caller can
    /// tell "the connection itself broke" apart from "the daemon sent
    /// something the WebSocket layer rejected" without string-matching.
    #[cfg(feature = "events")]
    #[error("WebSocket protocol error on /1.0/events: {0}")]
    WebSocketProtocol(String),

    /// `request_fully_sent` tells a caller building retry logic whether the
    /// request had already been fully written to the daemon when the
    /// timeout fired: if `true`, a mutating call (create/update/delete) may
    /// have already been received and acted on server-side even though the
    /// caller only sees this timeout - retrying could duplicate the
    /// operation. If `false`, nothing was sent and a retry is safe. Incus
    /// operations are not inherently idempotent, so this distinction
    /// matters for anything more than a manual "try again and see."
    #[error("request timed out after {after:?} (request fully sent: {request_fully_sent})")]
    Timeout {
        after: std::time::Duration,
        request_fully_sent: bool,
    },
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;
