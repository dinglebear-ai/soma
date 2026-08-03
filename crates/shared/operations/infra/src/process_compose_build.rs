use std::sync::Arc;

use async_trait::async_trait;
use soma_fleet::{CommandExecutor, CommandRequest, HostRecord};
use soma_ops::{MutationSendState, ProgressEvent, Timestamp};
use tokio_util::sync::CancellationToken;

use crate::{
    ComposeBuildMutator, ComposeBuildReceipt, ComposeBuildRequest, InfraError, MutationFailure,
    MutationProgressReporter, MutationResult,
};

const OUTPUT_LIMIT: usize = 16 * 1024 * 1024;
const MAX_PROGRESS_ERRORS: usize = 16;

/// Process-backed Compose build driver.
pub struct CommandComposeBuildMutator<E> {
    executor: Arc<E>,
}
impl<E> CommandComposeBuildMutator<E> {
    /// Creates a driver from a fleet command executor.
    #[must_use]
    pub fn new(executor: Arc<E>) -> Self {
        Self { executor }
    }
}

#[async_trait]
impl<E> ComposeBuildMutator for CommandComposeBuildMutator<E>
where
    E: CommandExecutor,
{
    async fn build_compose(
        &self,
        host: &HostRecord,
        request: &ComposeBuildRequest,
        progress: &dyn MutationProgressReporter,
        cancellation: &CancellationToken,
    ) -> MutationResult<ComposeBuildReceipt> {
        ensure_admitted(request.deadline(), cancellation)?;
        let mut errors = Vec::new();
        report(
            progress,
            request,
            1,
            "build",
            "starting Compose image build",
            &mut errors,
        );
        let mut args = vec![
            "compose".into(),
            "--progress".into(),
            "plain".into(),
            "-f".into(),
            request
                .project()
                .config_file()
                .to_string_lossy()
                .into_owned(),
            "build".into(),
        ];
        if let Some(service) = request.service() {
            args.extend(["--".into(), service.into()]);
        }
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
        report(
            progress,
            request,
            2,
            "build",
            "Compose image build process completed",
            &mut errors,
        );
        if output.exit_code() != Some(0) {
            return Err(MutationFailure::new(
                MutationSendState::Sent,
                InfraError::CommandFailed {
                    domain: "compose-build",
                    host: host.id().clone(),
                    exit_code: output.exit_code(),
                    stderr: String::from_utf8_lossy(output.stderr()).trim().to_owned(),
                },
            ));
        }
        Ok(ComposeBuildReceipt {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            project: request.project().name().into(),
            service: request.service().map(str::to_owned),
            send_state: MutationSendState::Sent,
            stdout: String::from_utf8_lossy(output.stdout()).into_owned(),
            stderr: String::from_utf8_lossy(output.stderr()).into_owned(),
            output_truncated: output.truncated(),
            progress_delivery_errors: errors,
        })
    }
}
fn report(
    sink: &dyn MutationProgressReporter,
    request: &ComposeBuildRequest,
    sequence: u64,
    phase: &str,
    message: &str,
    errors: &mut Vec<String>,
) {
    let event = ProgressEvent::new(
        request.operation_id().clone(),
        request.operation().clone(),
        sequence,
        Timestamp::now(),
        phase,
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
#[path = "process_compose_build_tests.rs"]
mod tests;
