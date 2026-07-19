use std::path::PathBuf;

/// Errors returned by update policy and transaction operations.
#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("invalid update directive: {0}")]
    InvalidDirective(&'static str),
    #[error("invalid SHA-256 digest: {0}")]
    InvalidDigest(String),
    #[error("invalid update policy: {0}")]
    InvalidPolicy(&'static str),
    #[error("invalid base URL {url}: {message}")]
    InvalidBaseUrl { url: String, message: String },
    #[error("invalid artifact URL {url}: {message}")]
    InvalidArtifactUrl { url: String, message: String },
    #[error("artifact URL crosses origins from {base} to {artifact}")]
    CrossOriginArtifact { base: String, artifact: String },
    #[error("artifact transport is not permitted: {0}")]
    InsecureTransport(String),
    #[error("artifact exceeds {limit} byte limit (received at least {actual})")]
    ArtifactTooLarge { limit: u64, actual: u64 },
    #[error("artifact digest mismatch: expected {expected}, got {actual}")]
    DigestMismatch { expected: String, actual: String },
    #[error("validator timed out after {timeout:?}")]
    ValidationTimedOut { timeout: std::time::Duration },
    #[error("validator exited unsuccessfully (code {code:?}): {stderr}")]
    ValidationFailed { code: Option<i32>, stderr: String },
    #[error("validator output is not valid UTF-8")]
    InvalidVersionOutput,
    #[error("validator {stream} exceeded the {limit} byte output limit")]
    ValidationOutputTooLarge { stream: &'static str, limit: usize },
    #[error("validator output did not contain exact version {expected}: {output}")]
    VersionMismatch { expected: String, output: String },
    #[error("another update transaction holds lock {path}")]
    UpdateInProgress { path: PathBuf },
    #[error("invalid update marker {path}: {message}")]
    InvalidMarker { path: PathBuf, message: String },
    #[error("rollback backup is missing: {path}")]
    MissingRollback { path: PathBuf },
    #[error("running version {running} does not match pending target {target}")]
    RunningVersionMismatch { running: String, target: String },
    #[error("the provided installer is unsupported on this platform")]
    UnsupportedPlatform,
    #[error("update layout paths collide: {first} and {second}")]
    InvalidLayout { first: PathBuf, second: PathBuf },
    #[error("an update to {target} is already pending at {path}")]
    PendingUpdateExists { path: PathBuf, target: String },
    #[error("state authority migration is blocked by transaction state at {path}: {message}")]
    StateMigrationBlocked { path: PathBuf, message: String },
    #[error("staged artifact is not a regular file: {path}")]
    InvalidStagedArtifact { path: PathBuf },
    #[error("staged artifact identity changed after validation: {path}")]
    ArtifactIdentityChanged { path: PathBuf },
    #[error("executable identity or mode changed after staging: {path}")]
    ExecutableIdentityChanged { path: PathBuf },
    #[error("staging failed for {path}: {operation}; cleanup also failed: {cleanup}")]
    ArtifactCleanupFailed {
        path: PathBuf,
        operation: Box<UpdateError>,
        cleanup: std::io::Error,
    },
    #[error("update operation failed: {operation}; transaction cleanup also failed: {cleanup}")]
    TransactionCleanupFailed {
        operation: Box<UpdateError>,
        cleanup: Box<UpdateError>,
    },
    #[error("I/O operation failed for {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl UpdateError {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

pub type Result<T> = std::result::Result<T, UpdateError>;
