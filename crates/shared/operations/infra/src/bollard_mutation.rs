use std::future::Future;
use std::time::Duration;

use async_trait::async_trait;
use bollard::query_parameters::{
    RestartContainerOptions, StartContainerOptions, StopContainerOptions,
};
use soma_fleet::{HostRecord, TopologyRevision};
use soma_ops::{MutationSendState, Timestamp};
use tokio_util::sync::CancellationToken;

use crate::{
    BollardReadClient, ContainerLifecycleAction, ContainerLifecycleMutator,
    ContainerLifecycleRequest, ContainerMutationReceipt, InfraError, MutationFailure,
    MutationResult,
};

#[async_trait]
impl ContainerLifecycleMutator for BollardReadClient {
    async fn mutate_container(
        &self,
        host: &HostRecord,
        request: &ContainerLifecycleRequest,
        cancellation: &CancellationToken,
    ) -> MutationResult<ContainerMutationReceipt> {
        self.validate_host(host)
            .map_err(|error| MutationFailure::new(MutationSendState::NotSent, error))?;
        ensure_not_expired(request.deadline(), cancellation)?;
        let result = match request.action() {
            ContainerLifecycleAction::Start => {
                await_send(
                    request.deadline(),
                    cancellation,
                    self.docker()
                        .start_container(request.container(), None::<StartContainerOptions>),
                )
                .await
            }
            ContainerLifecycleAction::Stop => {
                await_send(
                    request.deadline(),
                    cancellation,
                    self.docker()
                        .stop_container(request.container(), None::<StopContainerOptions>),
                )
                .await
            }
            ContainerLifecycleAction::Restart => {
                await_send(
                    request.deadline(),
                    cancellation,
                    self.docker()
                        .restart_container(request.container(), None::<RestartContainerOptions>),
                )
                .await
            }
            ContainerLifecycleAction::Pause => {
                await_send(
                    request.deadline(),
                    cancellation,
                    self.docker().pause_container(request.container()),
                )
                .await
            }
            ContainerLifecycleAction::Resume => {
                await_send(
                    request.deadline(),
                    cancellation,
                    self.docker().unpause_container(request.container()),
                )
                .await
            }
        };
        result?;
        Ok(ContainerMutationReceipt {
            host: host.id().clone(),
            topology_revision: TopologyRevision::clone(host.revision()),
            container: request.container().to_owned(),
            action: request.action(),
            send_state: MutationSendState::Sent,
        })
    }
}

fn ensure_not_expired(deadline: Timestamp, cancellation: &CancellationToken) -> MutationResult<()> {
    if cancellation.is_cancelled() {
        return Err(MutationFailure::new(
            MutationSendState::NotSent,
            soma_fleet::FleetError::Cancelled.into(),
        ));
    }
    if Timestamp::now() >= deadline {
        return Err(MutationFailure::new(
            MutationSendState::NotSent,
            soma_fleet::FleetError::DeadlineExceeded.into(),
        ));
    }
    Ok(())
}

async fn await_send<F>(
    deadline: Timestamp,
    cancellation: &CancellationToken,
    future: F,
) -> MutationResult<()>
where
    F: Future<Output = Result<(), bollard::errors::Error>>,
{
    let now = Timestamp::now().unix_millis();
    let remaining = deadline.unix_millis().saturating_sub(now);
    if remaining <= 0 {
        return Err(MutationFailure::new(
            MutationSendState::NotSent,
            soma_fleet::FleetError::DeadlineExceeded.into(),
        ));
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

#[cfg(test)]
#[path = "bollard_mutation_tests.rs"]
mod tests;
