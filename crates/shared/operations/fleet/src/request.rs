use std::path::{Component, PathBuf};

/// Invalid fleet command or transfer request.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum RequestError {
    /// Program was empty, oversized, or contained control characters.
    #[error("invalid command program")]
    InvalidProgram,
    /// One positional argument was invalid.
    #[error("invalid command argument at index {index}")]
    InvalidArgument {
        /// Invalid argument position.
        index: usize,
    },
    /// Too many positional arguments were supplied.
    #[error("command has {count} arguments; maximum is {max}")]
    TooManyArguments {
        /// Supplied argument count.
        count: usize,
        /// Maximum accepted count.
        max: usize,
    },
    /// Working or transfer path was not absolute and normalized.
    #[error("fleet path must be absolute and contain no parent traversal: {0}")]
    InvalidAbsolutePath(PathBuf),
    /// Output budget was zero or exceeded the hard ceiling.
    #[error("invalid {stream} output limit: {bytes} bytes")]
    InvalidOutputLimit {
        /// Output stream.
        stream: &'static str,
        /// Requested byte limit.
        bytes: usize,
    },
    /// Transfer byte bound was zero or exceeded the hard ceiling.
    #[error("invalid transfer limit {bytes}; maximum is {max}")]
    InvalidTransferLimit {
        /// Requested byte limit.
        bytes: u64,
        /// Hard maximum.
        max: u64,
    },
    /// Request deadline was not in the future.
    #[error("fleet request deadline has elapsed")]
    DeadlineElapsed,
    /// Content digest was not lowercase SHA-256.
    #[error("invalid SHA-256 digest")]
    InvalidSha256,
}

pub(crate) fn validate_absolute_path(path: PathBuf) -> Result<PathBuf, RequestError> {
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
    {
        Err(RequestError::InvalidAbsolutePath(path))
    } else {
        Ok(path)
    }
}

#[cfg(test)]
#[path = "request_tests.rs"]
mod tests;
