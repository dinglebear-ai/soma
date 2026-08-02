use std::path::PathBuf;

use soma_fleet::{FleetError, HostId};

const PUBLIC_DIAGNOSTIC_LIMIT: usize = 2048;

/// Result type for neutral infrastructure operations.
pub type InfraResult<T> = Result<T, InfraError>;

/// Product-neutral infrastructure operation failure.
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
    #[error("{domain} operation is unsupported for host {host}")]
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
    #[error("Docker access failed: {0}")]
    Docker(String),
}

pub(crate) fn public_diagnostic(bytes: &[u8]) -> String {
    #[derive(Clone, Copy)]
    enum EscapeState {
        None,
        Escape,
        ControlSequence,
    }

    let text = String::from_utf8_lossy(bytes);
    let mut sanitized = String::with_capacity(text.len().min(PUBLIC_DIAGNOSTIC_LIMIT));
    let mut escape = EscapeState::None;
    for character in text.chars() {
        match escape {
            EscapeState::Escape => {
                escape = if character == '[' {
                    EscapeState::ControlSequence
                } else {
                    EscapeState::None
                };
                continue;
            }
            EscapeState::ControlSequence => {
                if ('@'..='~').contains(&character) {
                    escape = EscapeState::None;
                }
                continue;
            }
            EscapeState::None => {}
        }
        if character == '\u{1b}' {
            escape = EscapeState::Escape;
            continue;
        }
        if character.is_control() {
            if !sanitized.ends_with(' ') && sanitized.len() < PUBLIC_DIAGNOSTIC_LIMIT {
                sanitized.push(' ');
            }
        } else if sanitized.len() + character.len_utf8() <= PUBLIC_DIAGNOSTIC_LIMIT {
            sanitized.push(character);
        } else {
            break;
        }
    }
    sanitized.trim().to_owned()
}

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;
