use soma_fleet::HostRecord;
use soma_ops::MutationSendState;
use tokio_util::sync::CancellationToken;

use crate::{
    ContainerListOptions, ContainerState, DockerCleanupClient, DockerPruneFingerprint,
    DockerPruneOutcome, DockerPruneRequest, DockerPruneTarget, ImageIdentity, ImageListOptions,
    ImageRemovalFingerprint, ImageRemovalOutcome, ImageRemovalRequest, ImageSummary, InfraError,
    InfraResult, MutationFailure, MutationResult,
};

/// Verified Docker image-removal and prune coordinator.
#[derive(Debug, Clone, Copy, Default)]
pub struct DockerCleanupEngine;

impl DockerCleanupEngine {
    /// Resolves one image reference into a stable local identity.
    pub async fn inspect_image(
        &self,
        client: &dyn DockerCleanupClient,
        host: &HostRecord,
        reference: &str,
        cancellation: &CancellationToken,
    ) -> InfraResult<ImageRemovalFingerprint> {
        let images = client
            .list_images(
                host,
                &ImageListOptions {
                    all: true,
                    dangling_only: false,
                },
                cancellation,
            )
            .await?;
        let identity =
            find_image(&images, reference).ok_or_else(|| InfraError::InvalidRequest {
                domain: "docker-cleanup",
                message: format!("image not found: {reference}"),
            })?;
        ImageRemovalFingerprint::new(reference, identity)
    }

    /// Captures the deterministic inventory relevant to one prune target.
    pub async fn inspect_prune(
        &self,
        client: &dyn DockerCleanupClient,
        host: &HostRecord,
        target: DockerPruneTarget,
        cancellation: &CancellationToken,
    ) -> InfraResult<DockerPruneFingerprint> {
        let containers = if target_includes(target, DockerPruneTarget::Containers) {
            client
                .list_containers(host, &ContainerListOptions::default(), cancellation)
                .await?
                .into_iter()
                .filter(|container| {
                    !matches!(
                        container.state,
                        ContainerState::Running
                            | ContainerState::Paused
                            | ContainerState::Restarting
                            | ContainerState::Removing
                    )
                })
                .filter_map(|container| container.id)
                .collect()
        } else {
            Vec::new()
        };
        let images = if target_includes(target, DockerPruneTarget::Images) {
            client
                .list_images(
                    host,
                    &ImageListOptions {
                        all: true,
                        dangling_only: true,
                    },
                    cancellation,
                )
                .await?
                .into_iter()
                .map(|image| image.id)
                .collect()
        } else {
            Vec::new()
        };
        let volumes = if target_includes(target, DockerPruneTarget::Volumes) {
            client
                .list_volumes(host, cancellation)
                .await?
                .into_iter()
                .map(|volume| volume.name)
                .collect()
        } else {
            Vec::new()
        };
        let networks = if target_includes(target, DockerPruneTarget::Networks) {
            client
                .list_networks(host, cancellation)
                .await?
                .into_iter()
                .filter_map(|network| network.id.or(network.name))
                .collect()
        } else {
            Vec::new()
        };
        let build_cache_bytes = if target_includes(target, DockerPruneTarget::BuildCache) {
            client
                .disk_usage(host, cancellation)
                .await?
                .build_cache
                .size_bytes
        } else {
            0
        };
        DockerPruneFingerprint {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            target,
            containers,
            images,
            volumes,
            networks,
            build_cache_bytes,
            sha256: String::new(),
        }
        .finalize()
    }

    /// Removes one exact planned image and verifies it is absent afterward.
    pub async fn remove_image(
        &self,
        client: &dyn DockerCleanupClient,
        host: &HostRecord,
        request: &ImageRemovalRequest,
        cancellation: &CancellationToken,
    ) -> MutationResult<ImageRemovalOutcome> {
        admit(request.force, request.deadline, cancellation)?;
        let current = self
            .inspect_image(client, host, &request.fingerprint.reference, cancellation)
            .await
            .map_err(not_sent)?;
        if current != request.fingerprint {
            return Err(not_sent(InfraError::InvalidRequest {
                domain: "docker-cleanup",
                message: "image identity changed after planning".into(),
            }));
        }
        let receipt = client.remove_image(host, request, cancellation).await?;
        let images = client
            .list_images(
                host,
                &ImageListOptions {
                    all: true,
                    dangling_only: false,
                },
                cancellation,
            )
            .await
            .map_err(|error| MutationFailure::new(receipt.send_state, error))?;
        let removed = find_image(&images, &request.fingerprint.reference).is_none()
            && images
                .iter()
                .all(|image| image.id != request.fingerprint.identity.id);
        if !removed {
            return Err(MutationFailure::new(
                receipt.send_state,
                InfraError::Docker("removed image remains visible after mutation".into()),
            ));
        }
        Ok(ImageRemovalOutcome {
            before: request.fingerprint.clone(),
            removed,
            receipt,
        })
    }

    /// Prunes one exact planned inventory and verifies reported identities are gone.
    pub async fn prune(
        &self,
        client: &dyn DockerCleanupClient,
        host: &HostRecord,
        request: &DockerPruneRequest,
        cancellation: &CancellationToken,
    ) -> MutationResult<DockerPruneOutcome> {
        admit(request.force, request.deadline, cancellation)?;
        let current = self
            .inspect_prune(client, host, request.fingerprint.target, cancellation)
            .await
            .map_err(not_sent)?;
        if current != request.fingerprint {
            return Err(not_sent(InfraError::InvalidRequest {
                domain: "docker-cleanup",
                message: "prune inventory changed after planning".into(),
            }));
        }
        let receipt = client.prune(host, request, cancellation).await?;
        let after = self
            .inspect_prune(client, host, request.fingerprint.target, cancellation)
            .await
            .map_err(|error| MutationFailure::new(receipt.send_state, error))?;
        verify_prune(&receipt, &request.fingerprint, &after)
            .map_err(|error| MutationFailure::new(receipt.send_state, error))?;
        let changed = receipt
            .scopes
            .iter()
            .any(|scope| !scope.deleted.is_empty() || scope.space_reclaimed > 0);
        Ok(DockerPruneOutcome {
            before: request.fingerprint.clone(),
            after,
            receipt,
            changed,
        })
    }
}

fn find_image(images: &[ImageSummary], reference: &str) -> Option<ImageIdentity> {
    images.iter().find_map(|image| {
        let matches = image.id == reference
            || image.repo_tags.iter().any(|tag| tag == reference)
            || image.repo_digests.iter().any(|digest| digest == reference);
        matches.then(|| ImageIdentity {
            id: image.id.clone(),
            repo_tags: image.repo_tags.clone(),
            repo_digests: image.repo_digests.clone(),
        })
    })
}

fn target_includes(target: DockerPruneTarget, candidate: DockerPruneTarget) -> bool {
    target == DockerPruneTarget::All || target == candidate
}

fn verify_prune(
    receipt: &crate::DockerPruneReceipt,
    before: &DockerPruneFingerprint,
    after: &DockerPruneFingerprint,
) -> InfraResult<()> {
    for scope in &receipt.scopes {
        if scope.target == DockerPruneTarget::BuildCache {
            if scope.space_reclaimed > 0
                && after.build_cache_bytes
                    > before
                        .build_cache_bytes
                        .saturating_sub(scope.space_reclaimed)
            {
                return Err(InfraError::Docker(
                    "build-cache usage did not reflect reported reclaimed bytes".into(),
                ));
            }
            continue;
        }
        let remaining = match scope.target {
            DockerPruneTarget::Containers => &after.containers,
            DockerPruneTarget::Images => &after.images,
            DockerPruneTarget::Volumes => &after.volumes,
            DockerPruneTarget::Networks => &after.networks,
            DockerPruneTarget::BuildCache | DockerPruneTarget::All => continue,
        };
        if scope
            .deleted
            .iter()
            .any(|deleted| remaining.contains(deleted))
        {
            return Err(InfraError::Docker(format!(
                "{} prune verification still sees a deleted identity",
                scope.target.as_str()
            )));
        }
    }
    Ok(())
}

fn admit(
    force: bool,
    deadline: soma_ops::Timestamp,
    cancellation: &CancellationToken,
) -> MutationResult<()> {
    if !force {
        return Err(not_sent(InfraError::InvalidRequest {
            domain: "docker-cleanup",
            message: "force=true is required".into(),
        }));
    }
    if cancellation.is_cancelled() {
        return Err(not_sent(soma_fleet::FleetError::Cancelled.into()));
    }
    if deadline <= soma_ops::Timestamp::now() {
        return Err(not_sent(soma_fleet::FleetError::DeadlineExceeded.into()));
    }
    Ok(())
}

fn not_sent(error: InfraError) -> MutationFailure {
    MutationFailure::new(MutationSendState::NotSent, error)
}

#[cfg(test)]
#[path = "docker_cleanup_engine_tests.rs"]
mod tests;
