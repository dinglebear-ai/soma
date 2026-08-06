use std::future::Future;
use std::time::Duration;

use async_trait::async_trait;
use bollard::query_parameters::{
    PruneBuildOptions, PruneContainersOptions, PruneImagesOptions, PruneNetworksOptions,
    PruneVolumesOptions, RemoveImageOptionsBuilder,
};
use soma_fleet::HostRecord;
use soma_ops::{MutationSendState, Timestamp};
use tokio_util::sync::CancellationToken;

use crate::{
    BollardReadClient, DockerCleanupMutator, DockerPruneReceipt, DockerPruneRequest,
    DockerPruneScopeReceipt, DockerPruneTarget, ImageRemovalReceipt, ImageRemovalRequest,
    InfraError, MutationFailure, MutationResult,
};

#[async_trait]
impl DockerCleanupMutator for BollardReadClient {
    async fn remove_image(
        &self,
        host: &HostRecord,
        request: &ImageRemovalRequest,
        cancellation: &CancellationToken,
    ) -> MutationResult<ImageRemovalReceipt> {
        self.validate_host(host).map_err(not_sent)?;
        ensure_admitted(request.force, request.deadline, cancellation)?;
        let options = RemoveImageOptionsBuilder::default()
            .force(request.force)
            .build();
        let rows = await_send(
            request.deadline,
            cancellation,
            self.docker()
                .remove_image(&request.fingerprint.reference, Some(options), None),
        )
        .await?;
        let mut deleted = rows
            .iter()
            .filter_map(|row| row.deleted.clone())
            .collect::<Vec<_>>();
        let mut untagged = rows
            .iter()
            .filter_map(|row| row.untagged.clone())
            .collect::<Vec<_>>();
        deleted.sort();
        deleted.dedup();
        untagged.sort();
        untagged.dedup();
        Ok(ImageRemovalReceipt {
            send_state: MutationSendState::Sent,
            deleted,
            untagged,
        })
    }

    async fn prune(
        &self,
        host: &HostRecord,
        request: &DockerPruneRequest,
        cancellation: &CancellationToken,
    ) -> MutationResult<DockerPruneReceipt> {
        self.validate_host(host).map_err(not_sent)?;
        ensure_admitted(request.force, request.deadline, cancellation)?;
        let mut scopes = Vec::new();
        for target in request.fingerprint.target.expanded() {
            let scope = self
                .prune_scope(*target, request.deadline, cancellation)
                .await
                .map_err(|failure| {
                    let completed = scopes
                        .iter()
                        .map(|scope: &DockerPruneScopeReceipt| scope.target.as_str())
                        .collect::<Vec<_>>()
                        .join(",");
                    MutationFailure::new(
                        failure.send_state(),
                        InfraError::Docker(format!(
                            "prune scope {} failed after completed scopes [{completed}]: {}",
                            target.as_str(),
                            failure.error()
                        )),
                    )
                })?;
            scopes.push(scope);
        }
        Ok(DockerPruneReceipt {
            send_state: MutationSendState::Sent,
            scopes,
        })
    }
}

impl BollardReadClient {
    async fn prune_scope(
        &self,
        target: DockerPruneTarget,
        deadline: Timestamp,
        cancellation: &CancellationToken,
    ) -> MutationResult<DockerPruneScopeReceipt> {
        match target {
            DockerPruneTarget::Containers => {
                let response = await_send(
                    deadline,
                    cancellation,
                    self.docker()
                        .prune_containers(None::<PruneContainersOptions>),
                )
                .await?;
                Ok(scope(
                    target,
                    response.containers_deleted.unwrap_or_default(),
                    response.space_reclaimed,
                ))
            }
            DockerPruneTarget::Images => {
                let response = await_send(
                    deadline,
                    cancellation,
                    self.docker().prune_images(None::<PruneImagesOptions>),
                )
                .await?;
                let deleted = response
                    .images_deleted
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|row| row.deleted)
                    .collect();
                Ok(scope(target, deleted, response.space_reclaimed))
            }
            DockerPruneTarget::Volumes => {
                let response = await_send(
                    deadline,
                    cancellation,
                    self.docker().prune_volumes(None::<PruneVolumesOptions>),
                )
                .await?;
                Ok(scope(
                    target,
                    response.volumes_deleted.unwrap_or_default(),
                    response.space_reclaimed,
                ))
            }
            DockerPruneTarget::Networks => {
                let response = await_send(
                    deadline,
                    cancellation,
                    self.docker().prune_networks(None::<PruneNetworksOptions>),
                )
                .await?;
                Ok(scope(
                    target,
                    response.networks_deleted.unwrap_or_default(),
                    None,
                ))
            }
            DockerPruneTarget::BuildCache => {
                let response = await_send(
                    deadline,
                    cancellation,
                    self.docker().prune_build(None::<PruneBuildOptions>),
                )
                .await?;
                Ok(scope(
                    target,
                    response.caches_deleted.unwrap_or_default(),
                    response.space_reclaimed,
                ))
            }
            DockerPruneTarget::All => unreachable!("expanded before execution"),
        }
    }
}

fn scope(
    target: DockerPruneTarget,
    mut deleted: Vec<String>,
    reclaimed: Option<i64>,
) -> DockerPruneScopeReceipt {
    deleted.sort();
    deleted.dedup();
    DockerPruneScopeReceipt {
        target,
        deleted,
        space_reclaimed: reclaimed.unwrap_or_default().max(0) as u64,
    }
}

fn ensure_admitted(
    force: bool,
    deadline: Timestamp,
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
    if deadline <= Timestamp::now() {
        return Err(not_sent(soma_fleet::FleetError::DeadlineExceeded.into()));
    }
    Ok(())
}

async fn await_send<T, F>(
    deadline: Timestamp,
    cancellation: &CancellationToken,
    future: F,
) -> MutationResult<T>
where
    F: Future<Output = Result<T, bollard::errors::Error>>,
{
    let remaining = deadline
        .unix_millis()
        .saturating_sub(Timestamp::now().unix_millis());
    if remaining <= 0 {
        return Err(not_sent(soma_fleet::FleetError::DeadlineExceeded.into()));
    }
    tokio::select! {
        () = cancellation.cancelled() => Err(MutationFailure::new(
            MutationSendState::Unknown,
            soma_fleet::FleetError::Cancelled.into(),
        )),
        () = tokio::time::sleep(Duration::from_millis(remaining as u64)) => Err(
            MutationFailure::new(
                MutationSendState::Unknown,
                soma_fleet::FleetError::DeadlineExceeded.into(),
            )
        ),
        result = future => result.map_err(|error| MutationFailure::new(
            MutationSendState::Unknown,
            InfraError::Docker(error.to_string()),
        )),
    }
}

fn not_sent(error: InfraError) -> MutationFailure {
    MutationFailure::new(MutationSendState::NotSent, error)
}

#[cfg(test)]
#[path = "bollard_cleanup_tests.rs"]
mod tests;
