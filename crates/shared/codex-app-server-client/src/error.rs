/// Errors returned by [`crate::CodexAppServerClient`].
///
/// `#[non_exhaustive]` because this crate expects to grow new failure modes
/// over time (see the README's schema-regeneration workflow) - matching
/// exhaustively on this enum today would make every future variant addition
/// a breaking change for downstream crates.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("failed to spawn `{command}`: {source}")]
    Spawn {
        command: String,
        #[source]
        source: std::io::Error,
    },

    #[error("transport I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("failed to (de)serialize a protocol message: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("the app-server connection closed before a response arrived")]
    TransportClosed,

    #[error("no response after {after:?}")]
    Timeout { after: std::time::Duration },

    #[error("app-server returned a JSON-RPC error (code {code}): {message}")]
    Rpc {
        code: i64,
        message: String,
        data: Option<serde_json::Value>,
    },
}

pub type Result<T> = std::result::Result<T, Error>;
