use std::future::Future;
use std::time::Duration;

use async_trait::async_trait;
use bollard::container::LogOutput;
use bollard::exec::{StartExecOptions, StartExecResults};
use bollard::models::ExecConfig;
use futures_util::StreamExt;
use soma_fleet::{HostRecord, TopologyRevision};
use soma_ops::{MutationSendState, Timestamp};
use tokio_util::sync::CancellationToken;

use crate::{
    BollardReadClient, ContainerExecMutator, ContainerExecReceipt, ContainerExecRequest,
    InfraError, MutationFailure, MutationResult,
};

#[async_trait]
impl ContainerExecMutator for BollardReadClient {
    async fn exec_container(
        &self,
        host: &HostRecord,
        request: &ContainerExecRequest,
        cancellation: &CancellationToken,
    ) -> MutationResult<ContainerExecReceipt> {
        self.validate_host(host)
            .map_err(|error| MutationFailure::new(MutationSendState::NotSent, error))?;
        ensure_before_start(request.deadline(), cancellation)?;
        let created = await_pre_start(
            request.deadline(),
            cancellation,
            self.docker().create_exec(
                request.container(),
                ExecConfig {
                    cmd: Some(request.command().to_vec()),
                    user: request.user().map(str::to_owned),
                    working_dir: request
                        .working_dir()
                        .map(|path| path.to_string_lossy().into_owned()),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    tty: Some(false),
                    ..Default::default()
                },
            ),
        )
        .await?;

        let started = await_post_start(
            request.deadline(),
            cancellation,
            self.docker().start_exec(
                &created.id,
                Some(StartExecOptions {
                    detach: false,
                    tty: false,
                    ..Default::default()
                }),
            ),
        )
        .await?;
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let (mut stdout_truncated, mut stderr_truncated) = (false, false);
        match started {
            StartExecResults::Attached { mut output, .. } => loop {
                let timeout = remaining(request.deadline(), MutationSendState::Unknown)?;
                let next = tokio::select! {
                    () = cancellation.cancelled() => return Err(MutationFailure::new(
                        MutationSendState::Unknown,
                        soma_fleet::FleetError::Cancelled.into(),
                    )),
                    result = tokio::time::timeout(timeout, output.next()) => match result {
                        Err(_) => return Err(MutationFailure::new(
                            MutationSendState::Unknown,
                            soma_fleet::FleetError::DeadlineExceeded.into(),
                        )),
                        Ok(value) => value,
                    }
                };
                let Some(frame) = next else { break };
                match frame.map_err(|error| {
                    MutationFailure::new(
                        MutationSendState::Unknown,
                        InfraError::Docker(error.to_string()),
                    )
                })? {
                    LogOutput::StdOut { message } | LogOutput::Console { message } => {
                        stdout_truncated |=
                            append_bounded(&mut stdout, &message, request.max_stdout_bytes());
                    }
                    LogOutput::StdErr { message } => {
                        stderr_truncated |=
                            append_bounded(&mut stderr, &message, request.max_stderr_bytes());
                    }
                    _ => {}
                }
            },
            StartExecResults::Detached => {
                return Err(MutationFailure::new(
                    MutationSendState::Unknown,
                    InfraError::Docker(
                        "container exec unexpectedly detached; completion is unknown".into(),
                    ),
                ));
            }
        }
        let inspected = await_post_start(
            request.deadline(),
            cancellation,
            self.docker().inspect_exec(&created.id),
        )
        .await?;
        let stdout_text = String::from_utf8_lossy(&stdout);
        let stderr_text = String::from_utf8_lossy(&stderr);
        Ok(ContainerExecReceipt {
            host: host.id().clone(),
            topology_revision: TopologyRevision::clone(host.revision()),
            container: request.container().to_owned(),
            command: request.command().to_vec(),
            user: request.user().map(str::to_owned),
            working_dir: request.working_dir().map(ToOwned::to_owned),
            stdout: stdout_text.into_owned(),
            stderr: stderr_text.into_owned(),
            exit_code: inspected.exit_code,
            truncated: stdout_truncated || stderr_truncated,
            encoding_lossy: std::str::from_utf8(&stdout).is_err()
                || std::str::from_utf8(&stderr).is_err(),
            send_state: MutationSendState::Sent,
        })
    }
}

fn ensure_before_start(
    deadline: Timestamp,
    cancellation: &CancellationToken,
) -> MutationResult<()> {
    if cancellation.is_cancelled() {
        return Err(MutationFailure::new(
            MutationSendState::NotSent,
            soma_fleet::FleetError::Cancelled.into(),
        ));
    }
    remaining(deadline, MutationSendState::NotSent).map(|_| ())
}

async fn await_pre_start<T, F>(
    deadline: Timestamp,
    cancellation: &CancellationToken,
    future: F,
) -> MutationResult<T>
where
    F: Future<Output = Result<T, bollard::errors::Error>>,
{
    let timeout = remaining(deadline, MutationSendState::NotSent)?;
    tokio::select! {
        () = cancellation.cancelled() => Err(MutationFailure::new(
            MutationSendState::NotSent,
            soma_fleet::FleetError::Cancelled.into(),
        )),
        result = tokio::time::timeout(timeout, future) => match result {
            Err(_) => Err(MutationFailure::new(
                MutationSendState::NotSent,
                soma_fleet::FleetError::DeadlineExceeded.into(),
            )),
            Ok(Err(error)) => Err(MutationFailure::new(
                MutationSendState::NotSent,
                InfraError::Docker(error.to_string()),
            )),
            Ok(Ok(value)) => Ok(value),
        }
    }
}

async fn await_post_start<T, F>(
    deadline: Timestamp,
    cancellation: &CancellationToken,
    future: F,
) -> MutationResult<T>
where
    F: Future<Output = Result<T, bollard::errors::Error>>,
{
    let timeout = remaining(deadline, MutationSendState::Unknown)?;
    tokio::select! {
        () = cancellation.cancelled() => Err(MutationFailure::new(
            MutationSendState::Unknown,
            soma_fleet::FleetError::Cancelled.into(),
        )),
        result = tokio::time::timeout(timeout, future) => match result {
            Err(_) => Err(MutationFailure::new(
                MutationSendState::Unknown,
                soma_fleet::FleetError::DeadlineExceeded.into(),
            )),
            Ok(Err(error)) => Err(MutationFailure::new(
                MutationSendState::Unknown,
                InfraError::Docker(error.to_string()),
            )),
            Ok(Ok(value)) => Ok(value),
        }
    }
}

fn remaining(deadline: Timestamp, send_state: MutationSendState) -> MutationResult<Duration> {
    let millis = deadline
        .unix_millis()
        .saturating_sub(Timestamp::now().unix_millis());
    if millis <= 0 {
        Err(MutationFailure::new(
            send_state,
            soma_fleet::FleetError::DeadlineExceeded.into(),
        ))
    } else {
        Ok(Duration::from_millis(millis as u64))
    }
}

fn append_bounded(destination: &mut Vec<u8>, bytes: &[u8], limit: usize) -> bool {
    let remaining = limit.saturating_sub(destination.len());
    let retained = remaining.min(bytes.len());
    destination.extend_from_slice(&bytes[..retained]);
    retained < bytes.len()
}

#[cfg(test)]
#[path = "bollard_exec_tests.rs"]
mod tests;
