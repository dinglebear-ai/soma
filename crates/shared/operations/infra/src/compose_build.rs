use std::path::{Component, Path, PathBuf};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use soma_fleet::{HostId, HostRecord, TopologyRevision};
use soma_ops::{MutationSendState, OperationId, OperationName, Timestamp, VerificationStatus};
use tokio_util::sync::CancellationToken;

use crate::{
    BuildContextFingerprint, ComposeProjectRef, ImageIdentity, InfraError,
    MutationProgressReporter, MutationResult, MutationVerification,
};

/// One planned Compose service build artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposeBuildArtifact {
    /// Compose service name.
    pub service: String,
    /// Expected output image tag.
    pub image: String,
    /// Resolved absolute context path.
    pub context: PathBuf,
    /// Planned context fingerprint.
    pub fingerprint: BuildContextFingerprint,
}

/// Deadline-bound Compose build request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposeBuildRequest {
    operation_id: OperationId,
    operation: OperationName,
    project: ComposeProjectRef,
    service: Option<String>,
    artifacts: Vec<ComposeBuildArtifact>,
    deadline: Timestamp,
}

impl ComposeBuildRequest {
    /// Creates a validated Compose build request.
    pub fn new(
        operation_id: OperationId,
        operation: OperationName,
        project: ComposeProjectRef,
        service: Option<String>,
        artifacts: Vec<ComposeBuildArtifact>,
        deadline: Timestamp,
    ) -> Result<Self, InfraError> {
        if artifacts.is_empty() {
            return Err(invalid(
                "no build-enabled services with explicit image tags were selected",
            ));
        }
        let mut names = std::collections::BTreeSet::new();
        for artifact in &artifacts {
            crate::compose_pull::validate_service(&artifact.service)?;
            if !names.insert(artifact.service.clone()) {
                return Err(invalid("duplicate Compose build service"));
            }
            if artifact.image.is_empty()
                || artifact.image.starts_with('-')
                || artifact.image.chars().any(char::is_control)
            {
                return Err(invalid("invalid Compose build image tag"));
            }
            if artifact.fingerprint.path != artifact.context {
                return Err(invalid("Compose context fingerprint path mismatch"));
            }
            artifact.fingerprint.validate()?;
        }
        if let Some(service) = &service {
            crate::compose_pull::validate_service(service)?;
            if artifacts.len() != 1 || artifacts[0].service != *service {
                return Err(invalid(
                    "service filter does not match planned build artifact",
                ));
            }
        }
        Ok(Self {
            operation_id,
            operation,
            project,
            service,
            artifacts,
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
    /// Returns project.
    #[must_use]
    pub const fn project(&self) -> &ComposeProjectRef {
        &self.project
    }
    /// Returns optional service.
    #[must_use]
    pub fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }
    /// Returns planned artifacts.
    #[must_use]
    pub fn artifacts(&self) -> &[ComposeBuildArtifact] {
        &self.artifacts
    }
    /// Returns deadline.
    #[must_use]
    pub const fn deadline(&self) -> Timestamp {
        self.deadline
    }
}

/// Compose build process receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposeBuildReceipt {
    /// Host.
    pub host: HostId,
    /// Topology revision.
    pub topology_revision: TopologyRevision,
    /// Project name.
    pub project: String,
    /// Optional service.
    pub service: Option<String>,
    /// Send state.
    pub send_state: MutationSendState,
    /// Bounded stdout.
    pub stdout: String,
    /// Bounded stderr.
    pub stderr: String,
    /// Output truncation flag.
    pub output_truncated: bool,
    /// Progress delivery failures.
    pub progress_delivery_errors: Vec<String>,
}

/// One verified Compose service build result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposeBuiltImage {
    /// Service.
    pub service: String,
    /// Image tag.
    pub image: String,
    /// Context fingerprint.
    pub context: BuildContextFingerprint,
    /// Before identity.
    pub before: Option<ImageIdentity>,
    /// After identity.
    pub after: Option<ImageIdentity>,
    /// Whether identity changed.
    pub changed: bool,
    /// Whether output image was verified.
    pub verified: bool,
}

/// Verified Compose build outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposeBuildOutcome {
    /// Host.
    pub host: HostId,
    /// Topology revision.
    pub topology_revision: TopologyRevision,
    /// Project.
    pub project: String,
    /// Optional service.
    pub service: Option<String>,
    /// Per-service images.
    pub images: Vec<ComposeBuiltImage>,
    /// Whether any identity changed.
    pub changed: bool,
    /// Send state.
    pub send_state: MutationSendState,
    /// Bounded stdout.
    pub stdout: String,
    /// Bounded stderr.
    pub stderr: String,
    /// Output truncation.
    pub output_truncated: bool,
    /// Progress delivery failures.
    pub progress_delivery_errors: Vec<String>,
    /// Verification status.
    pub verification_status: VerificationStatus,
    /// Verification explanation.
    pub verification: MutationVerification,
}

/// Driver for one Compose build command.
#[async_trait]
pub trait ComposeBuildMutator: Send + Sync {
    /// Executes a Compose build.
    async fn build_compose(
        &self,
        host: &HostRecord,
        request: &ComposeBuildRequest,
        progress: &dyn MutationProgressReporter,
        cancellation: &CancellationToken,
    ) -> MutationResult<ComposeBuildReceipt>;
}

/// Resolves an absolute or Compose-file-relative build context without permitting root escape.
pub fn resolve_compose_build_context(
    config_file: &Path,
    context: &str,
) -> Result<PathBuf, InfraError> {
    let raw = Path::new(context);
    let joined = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        config_file
            .parent()
            .ok_or_else(|| invalid("Compose config has no parent directory"))?
            .join(raw)
    };
    let mut normalized = PathBuf::from("/");
    for part in joined.components() {
        match part {
            Component::RootDir => {}
            Component::Normal(value) => normalized.push(value),
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    return Err(invalid("Compose build context escapes filesystem root"));
                }
            }
            Component::Prefix(_) => {
                return Err(invalid("unsupported Compose build context prefix"));
            }
        }
    }
    Ok(normalized)
}
fn invalid(message: &str) -> InfraError {
    InfraError::InvalidRequest {
        domain: "compose-build",
        message: message.into(),
    }
}

#[cfg(test)]
#[path = "compose_build_tests.rs"]
mod tests;
