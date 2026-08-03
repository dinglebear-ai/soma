use async_trait::async_trait;
use soma_fleet::{CommandExecutor, CommandRequest, HostRecord};
use soma_ops::{MutationSendState, ProgressEvent, Timestamp};
use tokio_util::sync::CancellationToken;

use crate::{
    CommandComposeInspector, ComposePullMutator, ComposePullReceipt, ComposePullRequest,
    InfraError, MutationFailure, MutationProgressReporter, MutationResult,
};

const PULL_OUTPUT_LIMIT: usize = 4 * 1024 * 1024;
const MAX_PROGRESS_ERRORS: usize = 16;

#[async_trait]
impl<E> ComposePullMutator for CommandComposeInspector<E>
where
    E: CommandExecutor,
{
    async fn pull_compose_images(
        &self,
        host: &HostRecord,
        request: &ComposePullRequest,
        progress: &dyn MutationProgressReporter,
        cancellation: &CancellationToken,
    ) -> MutationResult<ComposePullReceipt> {
        ensure_admitted(request.deadline(), cancellation)?;
        let mut progress_delivery_errors = Vec::new();
        report_progress(
            progress,
            request,
            1,
            format!(
                "pulling images for Compose project {}",
                request.project().name()
            ),
            &mut progress_delivery_errors,
        );
        let mut args = vec![
            "compose".into(),
            "-f".into(),
            request
                .project()
                .config_file()
                .to_string_lossy()
                .into_owned(),
            "pull".into(),
        ];
        if let Some(service) = request.service() {
            args.extend(["--".into(), service.to_owned()]);
        }
        let command = CommandRequest::new("docker", args, request.deadline())
            .map_err(soma_fleet::FleetError::from)
            .and_then(|request| {
                request
                    .with_output_limits(PULL_OUTPUT_LIMIT, PULL_OUTPUT_LIMIT)
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
                    domain: "compose-pull",
                    host: host.id().clone(),
                    exit_code: output.exit_code(),
                    stderr: String::from_utf8_lossy(output.stderr()).trim().to_owned(),
                },
            ));
        }
        report_progress(
            progress,
            request,
            2,
            format!(
                "Compose image pull completed for {}",
                request.project().name()
            ),
            &mut progress_delivery_errors,
        );
        Ok(ComposePullReceipt {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            project: request.project().name().to_owned(),
            service: request.service().map(str::to_owned),
            send_state: MutationSendState::Sent,
            progress_delivery_errors,
            output_truncated: output.truncated(),
        })
    }
}

fn report_progress(
    sink: &dyn MutationProgressReporter,
    request: &ComposePullRequest,
    sequence: u64,
    message: String,
    errors: &mut Vec<String>,
) {
    let event = ProgressEvent::new(
        request.operation_id().clone(),
        request.operation().clone(),
        sequence,
        Timestamp::now(),
        "pull",
    )
    .and_then(|event| event.with_message(message));
    if let Ok(event) = event
        && let Err(error) = sink.report(&event)
        && errors.len() < MAX_PROGRESS_ERRORS
    {
        errors.push(error);
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
#[path = "process_compose_pull_tests.rs"]
mod tests;
