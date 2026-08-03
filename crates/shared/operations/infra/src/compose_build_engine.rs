use soma_fleet::HostRecord;
use soma_ops::{MutationSendState, VerificationStatus};
use tokio_util::sync::CancellationToken;

use crate::{
    BuildContextInspector, ComposeBuildMutator, ComposeBuildOutcome, ComposeBuildRequest,
    ComposeBuiltImage, ImageListOptions, ImageReader, MutationFailure, MutationProgressReporter,
    MutationResult, MutationVerification,
};

/// Borrowed services required for one Compose build execution.
#[derive(Clone, Copy)]
pub struct ComposeBuildServices<'a> {
    /// Descriptor-confined context inspector.
    pub contexts: &'a dyn BuildContextInspector,
    /// Compose build mutation driver.
    pub mutator: &'a dyn ComposeBuildMutator,
    /// Docker image-store reader used for verification.
    pub images: &'a dyn ImageReader,
}

/// Coordinates context drift checks, Compose build, and image verification.
#[derive(Debug, Clone, Copy, Default)]
pub struct ComposeBuildEngine;

impl ComposeBuildEngine {
    /// Builds selected Compose services and verifies every output image.
    pub async fn execute(
        &self,
        services: ComposeBuildServices<'_>,
        host: &HostRecord,
        request: &ComposeBuildRequest,
        progress: &dyn MutationProgressReporter,
        cancellation: &CancellationToken,
    ) -> MutationResult<ComposeBuildOutcome> {
        ensure_admitted(request, cancellation)?;
        for artifact in request.artifacts() {
            let actual = services
                .contexts
                .fingerprint(host, &artifact.context, request.deadline(), cancellation)
                .await
                .map_err(|error| MutationFailure::new(MutationSendState::NotSent, error))?;
            if actual != artifact.fingerprint {
                return Err(MutationFailure::new(
                    MutationSendState::NotSent,
                    crate::InfraError::InvalidRequest {
                        domain: "compose-build",
                        message: format!(
                            "build context changed after planning for service {}",
                            artifact.service
                        ),
                    },
                ));
            }
        }
        let before = services
            .images
            .list_images(host, &ImageListOptions::default(), cancellation)
            .await
            .map_err(|error| MutationFailure::new(MutationSendState::NotSent, error))?;
        let receipt = services
            .mutator
            .build_compose(host, request, progress, cancellation)
            .await?;
        let after_read = services
            .images
            .list_images(host, &ImageListOptions::default(), cancellation)
            .await;
        let (rows, status, summary) = match after_read {
            Ok(after) => {
                let rows = request
                    .artifacts()
                    .iter()
                    .map(|artifact| {
                        let old = crate::image_pull_engine::find_image(&before, &artifact.image);
                        let new = crate::image_pull_engine::find_image(&after, &artifact.image);
                        let changed = match (&old, &new) {
                            (Some(a), Some(b)) => a.id != b.id,
                            (None, Some(_)) => true,
                            _ => false,
                        };
                        ComposeBuiltImage {
                            service: artifact.service.clone(),
                            image: artifact.image.clone(),
                            context: artifact.fingerprint.clone(),
                            before: old,
                            verified: new.is_some(),
                            after: new,
                            changed,
                        }
                    })
                    .collect::<Vec<_>>();
                let verified = rows.iter().all(|row| row.verified);
                (
                    rows,
                    if verified {
                        VerificationStatus::Verified
                    } else {
                        VerificationStatus::Failed
                    },
                    if verified {
                        "all Compose build output images resolve locally".into()
                    } else {
                        "one or more Compose build output images were not found locally".into()
                    },
                )
            }
            Err(error) => (
                Vec::new(),
                VerificationStatus::Inconclusive,
                format!("Compose build completed but image verification failed: {error}"),
            ),
        };
        let changed = rows.iter().any(|row| row.changed);
        Ok(ComposeBuildOutcome {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            project: request.project().name().into(),
            service: request.service().map(str::to_owned),
            images: rows,
            changed,
            send_state: receipt.send_state,
            stdout: receipt.stdout,
            stderr: receipt.stderr,
            output_truncated: receipt.output_truncated,
            progress_delivery_errors: receipt.progress_delivery_errors,
            verification_status: status,
            verification: MutationVerification {
                status: format!("{status:?}").to_ascii_lowercase(),
                summary,
            },
        })
    }
}
fn ensure_admitted(
    request: &ComposeBuildRequest,
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
#[path = "compose_build_engine_tests.rs"]
mod tests;
