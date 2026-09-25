use async_trait::async_trait;
use soma_fleet::{CommandExecutor, CommandRequest, HostRecord};
use soma_ops::{MutationSendState, Timestamp};
use tokio_util::sync::CancellationToken;

use crate::{
    CommandComposeInspector, ComposeDownMutator, ComposeDownReceipt, ComposeDownRequest,
    InfraError, MutationFailure, MutationResult,
};

const OUTPUT_LIMIT: usize = 4 * 1024 * 1024;

#[async_trait]
impl<E> ComposeDownMutator for CommandComposeInspector<E>
where
    E: CommandExecutor,
{
    async fn down_compose(
        &self,
        host: &HostRecord,
        request: &ComposeDownRequest,
        cancellation: &CancellationToken,
    ) -> MutationResult<ComposeDownReceipt> {
        ensure_admitted(request.deadline(), cancellation)?;
        let mut args = vec![
            "compose".into(),
            "-f".into(),
            request
                .project()
                .config_file()
                .to_string_lossy()
                .into_owned(),
            "down".into(),
        ];
        if request.remove_volumes() {
            args.push("--volumes".into());
        }
        let command = CommandRequest::new("docker", args, request.deadline())
            .map_err(soma_fleet::FleetError::from)
            .and_then(|command| {
                command
                    .with_output_limits(OUTPUT_LIMIT, OUTPUT_LIMIT)
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
                    domain: "compose-down",
                    host: host.id().clone(),
                    exit_code: output.exit_code(),
                    stderr: String::from_utf8_lossy(output.stderr()).trim().to_owned(),
                },
            ));
        }
        Ok(ComposeDownReceipt {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            project: request.project().name().to_owned(),
            remove_volumes: request.remove_volumes(),
            send_state: MutationSendState::Sent,
            stdout: String::from_utf8_lossy(output.stdout()).trim().to_owned(),
            stderr: String::from_utf8_lossy(output.stderr()).trim().to_owned(),
            output_truncated: output.truncated(),
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
    if deadline <= Timestamp::now() {
        return Err(MutationFailure::new(
            MutationSendState::NotSent,
            soma_fleet::FleetError::DeadlineExceeded.into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "process_compose_down_tests.rs"]
mod tests;
