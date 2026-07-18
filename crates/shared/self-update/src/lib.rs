//! A transport-neutral, standalone binary self-update transaction.
//!
//! The caller must authenticate every [`UpdateDirective`] independently (or
//! verify a detached signature). A SHA-256 digest received from the same
//! server as the artifact proves integrity in transit, not publisher identity.

#![forbid(unsafe_code)]

mod directive;
mod error;
mod staging;
mod transaction;
mod unix;
mod validation;

use std::path::{Path, PathBuf};
use std::time::Duration;

pub use error::{Result, UpdateError};
pub use staging::StagedArtifact;
pub use transaction::{ConfirmationOutcome, InstallOutcome};
#[cfg(unix)]
pub use unix::reexec;
pub use unix::restart_command;
pub use validation::ValidatedArtifact;

/// Network transports permitted for an artifact URL.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtifactTransportPolicy {
    /// Require HTTPS for every artifact.
    HttpsOnly,
    /// Permit HTTP only when the host is the local machine.
    HttpsOrLoopbackHttp,
}

/// An authenticated update instruction supplied by the adopting service.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateDirective {
    version: String,
    artifact_url: String,
    sha256: String,
}

impl UpdateDirective {
    /// Constructs and validates an update directive.
    pub fn new(
        version: impl Into<String>,
        artifact_url: impl Into<String>,
        sha256: impl Into<String>,
    ) -> Result<Self> {
        let version = version.into();
        let artifact_url = artifact_url.into();
        let sha256 = sha256.into();
        if version.trim().is_empty() {
            return Err(UpdateError::InvalidDirective("version must not be empty"));
        }
        if artifact_url.trim().is_empty() {
            return Err(UpdateError::InvalidDirective(
                "artifact URL must not be empty",
            ));
        }
        if sha256.len() != 64 || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(UpdateError::InvalidDigest(sha256));
        }
        Ok(Self {
            version,
            artifact_url,
            sha256: sha256.to_ascii_lowercase(),
        })
    }

    /// Target version reported by the authenticated directive.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Artifact reference exactly as supplied by the directive.
    pub fn artifact_url(&self) -> &str {
        &self.artifact_url
    }

    /// Normalized lowercase SHA-256 digest.
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
}

/// Caller-controlled paths used by an update transaction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateLayout {
    executable: PathBuf,
    state_file: PathBuf,
}

impl UpdateLayout {
    /// Creates a layout. Paths are never derived from directive content.
    pub fn new(executable: impl Into<PathBuf>, state_file: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            state_file: state_file.into(),
        }
    }

    /// Executable replaced by a successful install.
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    /// Durable transaction marker path.
    pub fn state_file(&self) -> &Path {
        &self.state_file
    }
}

/// Resource and lifecycle policy for an updater.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdatePolicy {
    transport: ArtifactTransportPolicy,
    max_artifact_bytes: u64,
    validation_timeout: Duration,
    max_unconfirmed_restarts: u32,
}

impl Default for UpdatePolicy {
    fn default() -> Self {
        Self {
            transport: ArtifactTransportPolicy::HttpsOnly,
            max_artifact_bytes: 128 * 1024 * 1024,
            validation_timeout: Duration::from_secs(10),
            max_unconfirmed_restarts: 3,
        }
    }
}

impl UpdatePolicy {
    /// Replaces the artifact transport policy.
    pub fn with_transport(mut self, transport: ArtifactTransportPolicy) -> Self {
        self.transport = transport;
        self
    }

    /// Replaces the maximum artifact size.
    pub fn with_max_artifact_bytes(mut self, bytes: u64) -> Result<Self> {
        if bytes == 0 {
            return Err(UpdateError::InvalidPolicy(
                "maximum artifact size must be greater than zero",
            ));
        }
        self.max_artifact_bytes = bytes;
        Ok(self)
    }

    /// Replaces the validation timeout.
    pub fn with_validation_timeout(mut self, timeout: Duration) -> Result<Self> {
        if timeout.is_zero() {
            return Err(UpdateError::InvalidPolicy(
                "validation timeout must be greater than zero",
            ));
        }
        self.validation_timeout = timeout;
        Ok(self)
    }

    /// Replaces the number of unconfirmed restarts allowed before rollback.
    pub fn with_max_unconfirmed_restarts(mut self, attempts: u32) -> Result<Self> {
        if attempts == 0 {
            return Err(UpdateError::InvalidPolicy(
                "restart limit must be greater than zero",
            ));
        }
        self.max_unconfirmed_restarts = attempts;
        Ok(self)
    }

    pub fn transport(&self) -> ArtifactTransportPolicy {
        self.transport
    }

    pub fn max_artifact_bytes(&self) -> u64 {
        self.max_artifact_bytes
    }

    pub fn validation_timeout(&self) -> Duration {
        self.validation_timeout
    }

    pub fn max_unconfirmed_restarts(&self) -> u32 {
        self.max_unconfirmed_restarts
    }
}

/// Reusable update coordinator.
#[derive(Clone, Debug)]
pub struct Updater {
    layout: UpdateLayout,
    policy: UpdatePolicy,
}

impl Updater {
    pub fn new(layout: UpdateLayout, policy: UpdatePolicy) -> Self {
        Self { layout, policy }
    }

    pub fn layout(&self) -> &UpdateLayout {
        &self.layout
    }

    pub fn policy(&self) -> &UpdatePolicy {
        &self.policy
    }
}

/// Work required while starting a service with possible pending update state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecoveryAction {
    NoPendingUpdate,
    PendingUpdate {
        target: String,
        attempts: u32,
        max_attempts: u32,
    },
    StaleMarkerRemoved {
        target: String,
        running: String,
    },
    RollbackInstalled {
        executable: PathBuf,
        restored_version: String,
    },
}
