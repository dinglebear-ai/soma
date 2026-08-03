use soma_fleet::HostRecord;
use soma_ops::{MutationSendState, VerificationStatus};
use tokio_util::sync::CancellationToken;

use crate::{
    BuildContextInspector, ImageBuildMutator, ImageBuildOutcome, ImageBuildRequest,
    ImageListOptions, ImageReader, MutationFailure, MutationProgressReporter, MutationResult,
    MutationVerification,
};

/// Borrowed services required for one image build execution.
#[derive(Clone, Copy)]
pub struct ImageBuildServices<'a> {
    /// Descriptor-confined context inspector.
    pub contexts: &'a dyn BuildContextInspector,
    /// Image build mutation driver.
    pub mutator: &'a dyn ImageBuildMutator,
    /// Docker image-store reader used for verification.
    pub images: &'a dyn ImageReader,
}

/// Coordinates context verification, image build, and image-store verification.
#[derive(Debug, Clone, Copy, Default)]
pub struct ImageBuildEngine;

impl ImageBuildEngine {
    /// Builds one image from an unchanged context and verifies the resulting identity.
    pub async fn execute(
        &self,
        services: ImageBuildServices<'_>,
        host: &HostRecord,
        request: &ImageBuildRequest,
        progress: &dyn MutationProgressReporter,
        cancellation: &CancellationToken,
    ) -> MutationResult<ImageBuildOutcome> {
        ensure_admitted(request, cancellation)?;
        let actual = services
            .contexts
            .fingerprint(host, request.context(), request.deadline(), cancellation)
            .await
            .map_err(|error| MutationFailure::new(MutationSendState::NotSent, error))?;
        if &actual != request.expected_context() {
            return Err(MutationFailure::new(
                MutationSendState::NotSent,
                crate::InfraError::InvalidRequest {
                    domain: "image-build",
                    message: "build context changed after planning".into(),
                },
            ));
        }
        let before = services
            .images
            .list_images(host, &ImageListOptions::default(), cancellation)
            .await
            .map_err(|error| MutationFailure::new(MutationSendState::NotSent, error))?;
        let before = crate::image_pull_engine::find_image(&before, request.tag());
        let receipt = services
            .mutator
            .build_image(host, request, progress, cancellation)
            .await?;
        let after_read = services
            .images
            .list_images(host, &ImageListOptions::default(), cancellation)
            .await;
        let (after, verification_status, summary) = match after_read {
            Ok(images) => match crate::image_pull_engine::find_image(&images, request.tag()) {
                Some(image) => (
                    Some(image),
                    VerificationStatus::Verified,
                    "the requested build tag resolves to a local image identity".into(),
                ),
                None => (
                    None,
                    VerificationStatus::Failed,
                    "the build command completed but the output tag was not found locally".into(),
                ),
            },
            Err(error) => (
                None,
                VerificationStatus::Inconclusive,
                format!("the build command completed but image verification failed: {error}"),
            ),
        };
        let changed = match (&before, &after) {
            (Some(before), Some(after)) => before.id != after.id,
            (None, Some(_)) => true,
            _ => false,
        };
        Ok(ImageBuildOutcome {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            tag: request.tag().to_owned(),
            context: actual,
            before,
            after,
            changed,
            send_state: receipt.send_state,
            stdout: receipt.stdout,
            stderr: receipt.stderr,
            output_truncated: receipt.output_truncated,
            progress_delivery_errors: receipt.progress_delivery_errors,
            verification_status,
            verification: MutationVerification {
                status: format!("{verification_status:?}").to_ascii_lowercase(),
                summary,
            },
        })
    }
}

fn ensure_admitted(
    request: &ImageBuildRequest,
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

#[cfg(test)]
#[path = "image_build_engine_tests.rs"]
mod tests;
