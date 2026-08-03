use soma_fleet::HostRecord;
use soma_ops::{MutationSendState, Timestamp, VerificationStatus};
use tokio_util::sync::CancellationToken;

use crate::{
    ComposePullClient, ComposePullOutcome, ComposePullRequest, ComposePulledImage,
    ImageListOptions, ImageReader, InfraError, MutationFailure, MutationProgressReporter,
    MutationResult, MutationVerification,
};

/// Coordinates Compose image pulls and verifies every configured local image identity.
#[derive(Debug, Clone, Copy, Default)]
pub struct ComposePullEngine;

impl ComposePullEngine {
    /// Pulls and verifies one Compose project or service.
    pub async fn execute(
        &self,
        compose: &dyn ComposePullClient,
        images: &dyn ImageReader,
        host: &HostRecord,
        request: &ComposePullRequest,
        progress: &dyn MutationProgressReporter,
        cancellation: &CancellationToken,
    ) -> MutationResult<ComposePullOutcome> {
        ensure_admitted(request, cancellation)?;
        let config = compose
            .config(host, request.project(), request.deadline(), cancellation)
            .await
            .map_err(|error| MutationFailure::new(MutationSendState::NotSent, error))?;
        let expected = expected_images(&config.services, request.service())?;
        let before_rows = images
            .list_images(host, &ImageListOptions::default(), cancellation)
            .await
            .map_err(|error| MutationFailure::new(MutationSendState::NotSent, error))?;
        let before = expected
            .iter()
            .map(|(service, reference)| {
                (
                    service.clone(),
                    reference.clone(),
                    crate::image_pull_engine::find_image(&before_rows, reference),
                )
            })
            .collect::<Vec<_>>();
        let receipt = compose
            .pull_compose_images(host, request, progress, cancellation)
            .await?;
        let after_read = images
            .list_images(host, &ImageListOptions::default(), cancellation)
            .await;
        let (rows, verification_status, summary) = match after_read {
            Ok(after_rows) => verified_rows(before, &after_rows),
            Err(error) => {
                let rows = before
                    .into_iter()
                    .map(|(service, reference, before)| ComposePulledImage {
                        service,
                        reference,
                        before,
                        after: None,
                        changed: false,
                        verified: false,
                    })
                    .collect();
                (
                    rows,
                    VerificationStatus::Inconclusive,
                    format!("Compose pull completed but image verification failed: {error}"),
                )
            }
        };
        let changed = rows.iter().any(|row| row.changed);
        Ok(ComposePullOutcome {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            project: request.project().name().to_owned(),
            service: request.service().map(str::to_owned),
            send_state: receipt.send_state,
            images: rows,
            changed,
            progress_delivery_errors: receipt.progress_delivery_errors,
            output_truncated: receipt.output_truncated,
            verification_status,
            verification: MutationVerification {
                status: format!("{verification_status:?}").to_ascii_lowercase(),
                summary,
            },
        })
    }
}

type BeforeImage = (String, String, Option<crate::ImageIdentity>);

fn verified_rows(
    before: Vec<BeforeImage>,
    after_rows: &[crate::ImageSummary],
) -> (Vec<ComposePulledImage>, VerificationStatus, String) {
    let rows = before
        .into_iter()
        .map(|(service, reference, before)| {
            let after = crate::image_pull_engine::find_image(after_rows, &reference);
            let changed = match (&before, &after) {
                (Some(before), Some(after)) => before.id != after.id,
                (None, Some(_)) => true,
                _ => false,
            };
            ComposePulledImage {
                service,
                reference,
                before,
                verified: after.is_some(),
                after,
                changed,
            }
        })
        .collect::<Vec<_>>();
    if rows.iter().all(|row| row.verified) {
        (
            rows,
            VerificationStatus::Verified,
            "all configured Compose image references resolve locally".into(),
        )
    } else {
        (
            rows,
            VerificationStatus::Failed,
            "one or more configured Compose image references were not found locally".into(),
        )
    }
}

fn expected_images(
    services: &std::collections::BTreeMap<String, crate::ComposeServiceConfig>,
    selected: Option<&str>,
) -> MutationResult<Vec<(String, String)>> {
    if let Some(selected) = selected {
        let service = services
            .get(selected)
            .ok_or_else(|| invalid_request(format!("Compose service {selected} was not found")))?;
        let image = service
            .image
            .clone()
            .filter(|image| !image.is_empty())
            .ok_or_else(|| {
                invalid_request(format!("Compose service {selected} has no image reference"))
            })?;
        return Ok(vec![(selected.to_owned(), image)]);
    }
    let images = services
        .iter()
        .filter_map(|(name, service)| service.image.clone().map(|image| (name.clone(), image)))
        .filter(|(_, image)| !image.is_empty())
        .collect::<Vec<_>>();
    if images.is_empty() {
        Err(invalid_request(
            "Compose project has no pullable image references".into(),
        ))
    } else {
        Ok(images)
    }
}

fn invalid_request(message: String) -> MutationFailure {
    MutationFailure::new(
        MutationSendState::NotSent,
        InfraError::InvalidRequest {
            domain: "compose-pull",
            message,
        },
    )
}

fn ensure_admitted(
    request: &ComposePullRequest,
    cancellation: &CancellationToken,
) -> MutationResult<()> {
    if cancellation.is_cancelled() {
        return Err(MutationFailure::new(
            MutationSendState::NotSent,
            soma_fleet::FleetError::Cancelled.into(),
        ));
    }
    if Timestamp::now() >= request.deadline() {
        return Err(MutationFailure::new(
            MutationSendState::NotSent,
            soma_fleet::FleetError::DeadlineExceeded.into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "compose_pull_engine_tests.rs"]
mod tests;
