use soma_fleet::{HostRecord, TopologyRevision};
use soma_ops::{MutationSendState, VerificationStatus};
use tokio_util::sync::CancellationToken;

use crate::{
    DockerArtifactClient, ImageIdentity, ImageListOptions, ImagePullOutcome, ImagePullRequest,
    ImageSummary, MutationFailure, MutationProgressReporter, MutationResult, MutationVerification,
};

/// Coordinates one image pull and independent image-store verification.
#[derive(Debug, Clone, Copy, Default)]
pub struct ImagePullEngine;

impl ImagePullEngine {
    /// Pulls an image and verifies its local content identity.
    pub async fn execute(
        &self,
        client: &dyn DockerArtifactClient,
        host: &HostRecord,
        request: &ImagePullRequest,
        progress: &dyn MutationProgressReporter,
        cancellation: &CancellationToken,
    ) -> MutationResult<ImagePullOutcome> {
        ensure_admitted(request, cancellation)?;
        let before = client
            .list_images(host, &ImageListOptions::default(), cancellation)
            .await
            .map_err(|error| MutationFailure::new(MutationSendState::NotSent, error))?;
        let before = find_image(&before, request.image());
        let receipt = client
            .pull_image(host, request, progress, cancellation)
            .await?;
        let after_read = client
            .list_images(host, &ImageListOptions::default(), cancellation)
            .await;
        let (after, verification_status, summary) = match after_read {
            Ok(images) => match find_image(&images, request.image()) {
                Some(image) => (
                    Some(image),
                    VerificationStatus::Verified,
                    "the requested image reference resolves to a local content identity".into(),
                ),
                None => (
                    None,
                    VerificationStatus::Failed,
                    "the pull stream completed but the requested image reference was not found locally"
                        .into(),
                ),
            },
            Err(error) => (
                None,
                VerificationStatus::Inconclusive,
                format!("the pull stream completed but image verification failed: {error}"),
            ),
        };
        let changed = match (&before, &after) {
            (Some(before), Some(after)) => before.id != after.id,
            (None, Some(_)) => true,
            _ => false,
        };
        Ok(ImagePullOutcome {
            host: host.id().clone(),
            topology_revision: TopologyRevision::clone(host.revision()),
            image: request.image().to_owned(),
            changed,
            send_state: receipt.send_state,
            before,
            after,
            total_events: receipt.total_events,
            progress: receipt.progress,
            progress_truncated: receipt.progress_truncated,
            progress_delivery_errors: receipt.progress_delivery_errors,
            verification_status,
            verification: MutationVerification {
                status: verification_status_text(verification_status),
                summary,
            },
        })
    }
}

fn ensure_admitted(
    request: &ImagePullRequest,
    cancellation: &CancellationToken,
) -> MutationResult<()> {
    if cancellation.is_cancelled() {
        return Err(MutationFailure::new(
            MutationSendState::NotSent,
            soma_fleet::FleetError::Cancelled.into(),
        ));
    }
    if soma_ops::Timestamp::now() >= request.deadline() {
        return Err(MutationFailure::new(
            MutationSendState::NotSent,
            soma_fleet::FleetError::DeadlineExceeded.into(),
        ));
    }
    Ok(())
}

pub(crate) fn find_image(images: &[ImageSummary], reference: &str) -> Option<ImageIdentity> {
    let canonical = crate::canonical_image_reference(reference);
    images
        .iter()
        .find(|image| {
            image.id == reference
                || image.repo_tags.iter().any(|tag| tag == &canonical)
                || image.repo_digests.iter().any(|digest| digest == reference)
        })
        .map(|image| ImageIdentity {
            id: image.id.clone(),
            repo_tags: image.repo_tags.clone(),
            repo_digests: image.repo_digests.clone(),
        })
}

fn verification_status_text(status: VerificationStatus) -> String {
    format!("{status:?}").to_ascii_lowercase()
}

#[cfg(test)]
#[path = "image_pull_engine_tests.rs"]
mod tests;
