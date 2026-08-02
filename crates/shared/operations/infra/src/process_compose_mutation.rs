use async_trait::async_trait;
use soma_fleet::{CommandExecutor, CommandRequest, HostRecord};
use soma_ops::{MutationSendState, Timestamp};
use tokio_util::sync::CancellationToken;

use crate::{
    CommandComposeInspector, ComposeMutationAction, ComposeMutationReceipt, ComposeMutationRequest,
    ComposeMutator, InfraError, MutationFailure, MutationResult,
};

const MUTATION_OUTPUT_LIMIT: usize = 1024 * 1024;

#[async_trait]
impl<E> ComposeMutator for CommandComposeInspector<E>
where
    E: CommandExecutor,
{
    async fn mutate_compose(
        &self,
        host: &HostRecord,
        request: &ComposeMutationRequest,
        cancellation: &CancellationToken,
    ) -> MutationResult<ComposeMutationReceipt> {
        ensure_admitted(request.deadline(), cancellation)?;
        let mut args = vec![
            "compose".into(),
            "-f".into(),
            request
                .project()
                .config_file()
                .to_string_lossy()
                .into_owned(),
            request.action().action_label().into(),
        ];
        if request.action() == ComposeMutationAction::Up {
            args.push("-d".into());
        }
        let command = CommandRequest::new("docker", args, request.deadline())
            .map_err(soma_fleet::FleetError::from)
            .and_then(|request| {
                request
                    .with_output_limits(MUTATION_OUTPUT_LIMIT, MUTATION_OUTPUT_LIMIT)
                    .map_err(soma_fleet::FleetError::from)
            })
            .map_err(|error| {
                MutationFailure::new(MutationSendState::NotSent, InfraError::from(error))
            })?;
        let output = self
            .executor
            .execute(host, &command, cancellation)
            .await
            .map_err(|error| {
                MutationFailure::new(MutationSendState::Unknown, InfraError::from(error))
            })?;
        if output.exit_code() != Some(0) {
            return Err(MutationFailure::new(
                MutationSendState::Sent,
                InfraError::CommandFailed {
                    domain: "compose-mutation",
                    host: host.id().clone(),
                    exit_code: output.exit_code(),
                    stderr: String::from_utf8_lossy(output.stderr()).trim().to_owned(),
                },
            ));
        }
        Ok(ComposeMutationReceipt {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            project: request.project().name().to_owned(),
            action: request.action(),
            send_state: MutationSendState::Sent,
        })
    }
}

fn ensure_admitted(deadline: Timestamp, cancellation: &CancellationToken) -> MutationResult<()> {
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

#[cfg(test)]
#[path = "process_compose_mutation_tests.rs"]
mod tests;
