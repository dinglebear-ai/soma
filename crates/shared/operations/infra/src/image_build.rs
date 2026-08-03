use std::path::{Component, Path, PathBuf};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use soma_fleet::{HostId, HostRecord, TopologyRevision};
use soma_ops::{MutationSendState, OperationId, OperationName, Timestamp, VerificationStatus};
use tokio_util::sync::CancellationToken;

use crate::{
    BuildContextFingerprint, ImageIdentity, InfraError, MutationProgressReporter, MutationResult,
    MutationVerification,
};

const MAX_TAG_CHARS: usize = 256;
const MAX_DOCKERFILE_CHARS: usize = 4096;

/// Deadline-bound request for one Docker image build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageBuildRequest {
    operation_id: OperationId,
    operation: OperationName,
    context: PathBuf,
    dockerfile: Option<PathBuf>,
    tag: String,
    no_cache: bool,
    expected_context: BuildContextFingerprint,
    deadline: Timestamp,
}

impl ImageBuildRequest {
    /// Creates a validated image build request.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        operation_id: OperationId,
        operation: OperationName,
        context: PathBuf,
        dockerfile: Option<PathBuf>,
        tag: impl Into<String>,
        no_cache: bool,
        expected_context: BuildContextFingerprint,
        deadline: Timestamp,
    ) -> Result<Self, InfraError> {
        validate_context(&context)?;
        if let Some(path) = &dockerfile {
            validate_dockerfile(path)?;
        }
        let tag = tag.into();
        let count = tag.chars().count();
        if count == 0
            || count > MAX_TAG_CHARS
            || tag.starts_with('-')
            || tag.chars().any(char::is_control)
        {
            return Err(InfraError::InvalidRequest {
                domain: "image-build",
                message: "invalid image tag".into(),
            });
        }
        if expected_context.path != context {
            return Err(InfraError::InvalidRequest {
                domain: "image-build",
                message: "expected context path differs from request".into(),
            });
        }
        expected_context.validate()?;
        Ok(Self {
            operation_id,
            operation,
            context,
            dockerfile,
            tag,
            no_cache,
            expected_context,
            deadline,
        })
    }
    /// Returns operation identity.
    #[must_use]
    pub fn operation_id(&self) -> &OperationId {
        &self.operation_id
    }
    /// Returns canonical operation.
    #[must_use]
    pub fn operation(&self) -> &OperationName {
        &self.operation
    }
    /// Returns absolute build context.
    #[must_use]
    pub fn context(&self) -> &Path {
        &self.context
    }
    /// Returns relative Dockerfile path.
    #[must_use]
    pub fn dockerfile(&self) -> Option<&Path> {
        self.dockerfile.as_deref()
    }
    /// Returns output tag.
    #[must_use]
    pub fn tag(&self) -> &str {
        &self.tag
    }
    /// Returns no-cache flag.
    #[must_use]
    pub const fn no_cache(&self) -> bool {
        self.no_cache
    }
    /// Returns planned context fingerprint.
    #[must_use]
    pub const fn expected_context(&self) -> &BuildContextFingerprint {
        &self.expected_context
    }
    /// Returns deadline.
    #[must_use]
    pub const fn deadline(&self) -> Timestamp {
        self.deadline
    }
}

/// Receipt returned after a build command reaches a terminal process state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageBuildReceipt {
    /// Target host.
    pub host: HostId,
    /// Exact topology revision.
    pub topology_revision: TopologyRevision,
    /// Output image tag.
    pub tag: String,
    /// Backend send state.
    pub send_state: MutationSendState,
    /// Bounded stdout log.
    pub stdout: String,
    /// Bounded stderr log.
    pub stderr: String,
    /// Whether either output stream was truncated.
    pub output_truncated: bool,
    /// Progress delivery failures that did not alter execution truth.
    pub progress_delivery_errors: Vec<String>,
}

/// Verified image build outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageBuildOutcome {
    /// Target host.
    pub host: HostId,
    /// Exact topology revision.
    pub topology_revision: TopologyRevision,
    /// Output image tag.
    pub tag: String,
    /// Verified context fingerprint.
    pub context: BuildContextFingerprint,
    /// Image identity before build.
    pub before: Option<ImageIdentity>,
    /// Image identity after build.
    pub after: Option<ImageIdentity>,
    /// Whether content identity changed.
    pub changed: bool,
    /// Backend send state.
    pub send_state: MutationSendState,
    /// Bounded stdout log.
    pub stdout: String,
    /// Bounded stderr log.
    pub stderr: String,
    /// Whether output was truncated.
    pub output_truncated: bool,
    /// Progress delivery failures.
    pub progress_delivery_errors: Vec<String>,
    /// Independent verification status.
    pub verification_status: VerificationStatus,
    /// Verification explanation.
    pub verification: MutationVerification,
}

/// Driver for one Docker image build command.
#[async_trait]
pub trait ImageBuildMutator: Send + Sync {
    /// Executes one image build while preserving send uncertainty.
    async fn build_image(
        &self,
        host: &HostRecord,
        request: &ImageBuildRequest,
        progress: &dyn MutationProgressReporter,
        cancellation: &CancellationToken,
    ) -> MutationResult<ImageBuildReceipt>;
}

fn validate_context(path: &Path) -> Result<(), InfraError> {
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(InfraError::InvalidRequest {
            domain: "image-build",
            message: "build context must be an absolute normalized path".into(),
        });
    }
    Ok(())
}
fn validate_dockerfile(path: &Path) -> Result<(), InfraError> {
    let text = path.to_string_lossy();
    if path.is_absolute()
        || text.is_empty()
        || text.chars().count() > MAX_DOCKERFILE_CHARS
        || text.contains('~')
        || text.contains('$')
        || text.chars().any(char::is_control)
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_) | Component::CurDir))
    {
        return Err(InfraError::InvalidRequest {
            domain: "image-build",
            message: "Dockerfile must be a bounded relative path without traversal or expansion"
                .into(),
        });
    }
    Ok(())
}

#[cfg(test)]
#[path = "image_build_tests.rs"]
mod tests;
