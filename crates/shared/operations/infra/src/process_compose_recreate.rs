use async_trait::async_trait;
use soma_fleet::{CommandExecutor, CommandRequest, HostRecord};
use soma_ops::{MutationSendState, Timestamp};
use tokio_util::sync::CancellationToken;

use crate::{
    CommandComposeInspector, ComposeRecreateMutator, ComposeRecreateReceipt,
    ComposeRecreateRequest, InfraError, MutationFailure, MutationResult,
};

const OUTPUT_LIMIT: usize = 4 * 1024 * 1024;

#[async_trait]
impl<E> ComposeRecreateMutator for CommandComposeInspector<E>
where
    E: CommandExecutor,
{
    async fn recreate_compose(
        &self,
        host: &HostRecord,
        request: &ComposeRecreateRequest,
        cancellation: &CancellationToken,
    ) -> MutationResult<ComposeRecreateReceipt> {
        ensure_admitted(request.deadline(), cancellation)?;
        let args = vec![
            "compose".into(),
            "-f".into(),
            request
                .project()
                .config_file()
                .to_string_lossy()
                .into_owned(),
            "up".into(),
            "-d".into(),
            "--force-recreate".into(),
        ];
        let command = CommandRequest::new("docker", args, request.deadline())
            .map_err(soma_fleet::FleetError::from)
            .and_then(|request| {
                request
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
                    domain: "compose-recreate",
                    host: host.id().clone(),
                    exit_code: output.exit_code(),
                    stderr: String::from_utf8_lossy(output.stderr()).trim().to_owned(),
                },
            ));
        }
        Ok(ComposeRecreateReceipt {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            project: request.project().name().to_owned(),
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
    if Timestamp::now() >= deadline {
        return Err(MutationFailure::new(
            MutationSendState::NotSent,
            soma_fleet::FleetError::DeadlineExceeded.into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "process_compose_recreate_tests.rs"]
mod tests;
