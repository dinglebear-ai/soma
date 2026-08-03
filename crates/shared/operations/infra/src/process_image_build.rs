use std::sync::Arc;

use async_trait::async_trait;
use soma_fleet::{CommandExecutor, CommandRequest, HostRecord};
use soma_ops::{MutationSendState, ProgressEvent, Timestamp};
use tokio_util::sync::CancellationToken;

use crate::{
    ImageBuildMutator, ImageBuildReceipt, ImageBuildRequest, InfraError, MutationFailure,
    MutationProgressReporter, MutationResult,
};

const BUILD_OUTPUT_LIMIT: usize = 16 * 1024 * 1024;
const MAX_PROGRESS_ERRORS: usize = 16;

/// Process-backed Docker build driver.
pub struct CommandImageBuildMutator<E> {
    executor: Arc<E>,
}

impl<E> CommandImageBuildMutator<E> {
    /// Creates a build driver from a fleet command executor.
    #[must_use]
    pub fn new(executor: Arc<E>) -> Self {
        Self { executor }
    }
}

#[async_trait]
impl<E> ImageBuildMutator for CommandImageBuildMutator<E>
where
    E: CommandExecutor,
{
    async fn build_image(
        &self,
        host: &HostRecord,
        request: &ImageBuildRequest,
        progress: &dyn MutationProgressReporter,
        cancellation: &CancellationToken,
    ) -> MutationResult<ImageBuildReceipt> {
        ensure_admitted(request.deadline(), cancellation)?;
        let mut errors = Vec::new();
        report(
            progress,
            request,
            1,
            "build",
            "starting Docker image build",
            &mut errors,
        );
        let mut args = vec![
            "build".into(),
            "--progress=plain".into(),
            "-t".into(),
            request.tag().into(),
        ];
        if request.no_cache() {
            args.push("--no-cache".into());
        }
        if let Some(dockerfile) = request.dockerfile() {
            args.push("-f".into());
            args.push(
                request
                    .context()
                    .join(dockerfile)
                    .to_string_lossy()
                    .into_owned(),
            );
        }
        args.push(request.context().to_string_lossy().into_owned());
        let command = CommandRequest::new("docker", args, request.deadline())
            .map_err(soma_fleet::FleetError::from)
            .and_then(|request| {
                request
                    .with_output_limits(BUILD_OUTPUT_LIMIT, BUILD_OUTPUT_LIMIT)
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
            "Docker image build process completed",
            &mut errors,
        );
        if output.exit_code() != Some(0) {
            return Err(MutationFailure::new(
                MutationSendState::Sent,
                InfraError::CommandFailed {
                    domain: "image-build",
                    host: host.id().clone(),
                    exit_code: output.exit_code(),
                    stderr: String::from_utf8_lossy(output.stderr()).trim().to_owned(),
                },
            ));
        }
        Ok(ImageBuildReceipt {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            tag: request.tag().into(),
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
    request: &ImageBuildRequest,
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
#[path = "process_image_build_tests.rs"]
mod tests;
