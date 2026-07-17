//! Re-exports of the generic provider error type plus adapter-local error
//! types shared across sidecar-executing adapters (ai-sdk, python).

pub use soma_provider_core::{redact_public, ProviderError};

/// A bounded sidecar child process failed for a reason short of an
/// application-level `ProviderError` — surfaced by [`crate::sidecar`] and
/// converted to a `ProviderError` at each adapter's call boundary.
#[cfg(feature = "sidecar")]
#[derive(Debug)]
pub enum SidecarError {
    Io(std::io::Error),
    Join(tokio::task::JoinError),
    Timeout,
}

#[cfg(feature = "sidecar")]
impl std::fmt::Display for SidecarError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Join(error) => write!(f, "{error}"),
            Self::Timeout => write!(f, "sidecar process timed out"),
        }
    }
}

#[cfg(feature = "sidecar")]
impl std::error::Error for SidecarError {}
