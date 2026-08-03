use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use soma_fleet::{HostId, HostRecord, TopologyRevision};
use soma_ops::{MutationSendState, OperationId, OperationName, Timestamp, VerificationStatus};
use tokio_util::sync::CancellationToken;

use crate::{
    ContainerReader, ImageReader, InfraError, MutationProgressReporter, MutationResult,
    MutationVerification,
};

const MAX_IMAGE_REFERENCE_CHARS: usize = 512;
#[cfg(feature = "bollard-driver")]
const MAX_PROGRESS_FRAMES: usize = 256;
#[cfg(feature = "bollard-driver")]
const MAX_PROGRESS_DELIVERY_ERRORS: usize = 16;

/// Deadline-bound request to pull one Docker/OCI image reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImagePullRequest {
    operation_id: OperationId,
    operation: OperationName,
    image: String,
    deadline: Timestamp,
}

impl ImagePullRequest {
    /// Creates a validated image pull request.
    pub fn new(
        operation_id: OperationId,
        operation: OperationName,
        image: impl Into<String>,
        deadline: Timestamp,
    ) -> Result<Self, InfraError> {
        let image = image.into();
        validate_image_reference(&image)?;
        Ok(Self {
            operation_id,
            operation,
            image,
            deadline,
        })
    }

    /// Returns the operation execution identity.
    #[must_use]
    pub fn operation_id(&self) -> &OperationId {
        &self.operation_id
    }

    /// Returns the canonical operation name.
    #[must_use]
    pub fn operation(&self) -> &OperationName {
        &self.operation
    }

    /// Returns the requested image reference.
    #[must_use]
    pub fn image(&self) -> &str {
        &self.image
    }

    /// Returns the absolute deadline.
    #[must_use]
    pub const fn deadline(&self) -> Timestamp {
        self.deadline
    }
}

/// Stable image identity observed through the Docker read API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageIdentity {
    /// Docker image content ID.
    pub id: String,
    /// Repository tags bound to the image.
    pub repo_tags: Vec<String>,
    /// Repository digests bound to the image.
    pub repo_digests: Vec<String>,
}

/// One retained neutral image-pull progress frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImagePullProgressFrame {
    /// One-based stream sequence.
    pub sequence: u64,
    /// Docker status text.
    pub status: Option<String>,
    /// Layer or object identity.
    pub id: Option<String>,
    /// Current byte count when reported.
    pub current: Option<u64>,
    /// Total byte count when reported.
    pub total: Option<u64>,
    /// Human-readable progress text.
    pub message: Option<String>,
    /// Engine-reported error text.
    pub error: Option<String>,
}

/// Receipt returned after the image pull stream completes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImagePullReceipt {
    /// Target host.
    pub host: HostId,
    /// Exact topology revision.
    pub topology_revision: TopologyRevision,
    /// Requested image reference.
    pub image: String,
    /// Backend send state.
    pub send_state: MutationSendState,
    /// Total stream frames observed.
    pub total_events: u64,
    /// Bounded retained progress frames.
    pub progress: Vec<ImagePullProgressFrame>,
    /// Whether additional frames were omitted.
    pub progress_truncated: bool,
    /// Bounded progress sink failures that did not rewrite execution truth.
    pub progress_delivery_errors: Vec<String>,
}

#[cfg(feature = "bollard-driver")]
impl ImagePullReceipt {
    pub(crate) fn retain_frame(&mut self, frame: ImagePullProgressFrame) {
        self.total_events = self.total_events.saturating_add(1);
        if self.progress.len() < MAX_PROGRESS_FRAMES {
            self.progress.push(frame);
        } else {
            self.progress_truncated = true;
        }
    }

    pub(crate) fn retain_delivery_error(&mut self, error: String) {
        if self.progress_delivery_errors.len() < MAX_PROGRESS_DELIVERY_ERRORS {
            self.progress_delivery_errors.push(error);
        }
    }
}

/// Verified image pull outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImagePullOutcome {
    /// Target host.
    pub host: HostId,
    /// Exact topology revision.
    pub topology_revision: TopologyRevision,
    /// Requested image reference.
    pub image: String,
    /// Whether the verified image content identity changed.
    pub changed: bool,
    /// Backend send state.
    pub send_state: MutationSendState,
    /// Image identity observed before the pull.
    pub before: Option<ImageIdentity>,
    /// Image identity observed after the pull.
    pub after: Option<ImageIdentity>,
    /// Total stream events observed.
    pub total_events: u64,
    /// Bounded retained progress.
    pub progress: Vec<ImagePullProgressFrame>,
    /// Whether retained progress was truncated.
    pub progress_truncated: bool,
    /// Progress delivery failures that did not alter mutation truth.
    pub progress_delivery_errors: Vec<String>,
    /// Independent verification status.
    pub verification_status: VerificationStatus,
    /// Verification explanation.
    pub verification: MutationVerification,
}

/// Driver for one image-pull stream.
#[async_trait]
pub trait ImagePullMutator: Send + Sync {
    /// Pulls one image while emitting canonical progress and preserving send uncertainty.
    async fn pull_image(
        &self,
        host: &HostRecord,
        request: &ImagePullRequest,
        progress: &dyn MutationProgressReporter,
        cancellation: &CancellationToken,
    ) -> MutationResult<ImagePullReceipt>;
}

/// Complete Docker client required by artifact mutations.
pub trait DockerArtifactClient: ContainerReader + ImageReader + ImagePullMutator {}

impl<T> DockerArtifactClient for T where T: ContainerReader + ImageReader + ImagePullMutator {}

/// Factory for host- and revision-bound artifact mutation clients.
#[async_trait]
pub trait DockerArtifactClientProvider: Send + Sync {
    /// Returns a client bound to the exact host revision.
    async fn artifact_client(
        &self,
        host: &HostRecord,
        cancellation: &CancellationToken,
    ) -> Result<Arc<dyn DockerArtifactClient>, InfraError>;
}

pub(crate) fn validate_image_reference(image: &str) -> Result<(), InfraError> {
    let chars = image.chars().count();
    if chars == 0
        || chars > MAX_IMAGE_REFERENCE_CHARS
        || image.chars().any(char::is_control)
        || image.chars().any(char::is_whitespace)
        || image.starts_with('-')
    {
        return Err(InfraError::InvalidRequest {
            domain: "image-pull",
            message: "invalid image reference".into(),
        });
    }
    Ok(())
}

/// Returns the canonical tag used by Docker when no tag or digest is supplied.
#[must_use]
pub fn canonical_image_reference(image: &str) -> String {
    if image.contains('@') {
        return image.to_owned();
    }
    let last_slash = image.rfind('/');
    let last_colon = image.rfind(':');
    if last_colon.is_some_and(|colon| last_slash.is_none_or(|slash| colon > slash)) {
        image.to_owned()
    } else {
        format!("{image}:latest")
    }
}

#[cfg(test)]
#[path = "image_pull_tests.rs"]
mod tests;
