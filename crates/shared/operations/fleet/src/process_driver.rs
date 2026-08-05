use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use crate::{
    CommandExecutor, CommandOutput, CommandRequest, FleetError, FleetResult, HostEndpoint,
    HostRecord, io::drain_bounded,
};

/// Local process-backed command driver used for conformance and local hosts.
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalProcessDriver;

#[async_trait]
impl CommandExecutor for LocalProcessDriver {
    async fn execute(
        &self,
        host: &HostRecord,
        request: &CommandRequest,
        cancellation: &CancellationToken,
    ) -> FleetResult<CommandOutput> {
        if !matches!(host.endpoint(), HostEndpoint::Local) {
            return Err(FleetError::Command {
                host: host.id().clone(),
                message: "local process driver requires a local endpoint".into(),
            });
        }
        if cancellation.is_cancelled() {
            return Err(FleetError::Cancelled);
        }
        request.validate_at(soma_ops::Timestamp::now())?;
        let timeout = remaining(request.deadline())?;

        let mut command = Command::new(request.program());
        command
            .args(request.args())
            .stdin(if request.stdin().is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some(directory) = request.working_dir() {
            command.current_dir(directory);
        }
        let mut child = command.spawn().map_err(|error| FleetError::Command {
            host: host.id().clone(),
            message: format!("spawn failed: {error}"),
        })?;
        let input = match request.stdin() {
            Some(bytes) => Some((
                child.stdin.take().ok_or_else(|| FleetError::Command {
                    host: host.id().clone(),
                    message: "stdin pipe unavailable".into(),
                })?,
                bytes.to_vec(),
            )),
            None => None,
        };
        let stdout = child.stdout.take().ok_or_else(|| FleetError::Command {
            host: host.id().clone(),
            message: "stdout pipe unavailable".into(),
        })?;
        let stderr = child.stderr.take().ok_or_else(|| FleetError::Command {
            host: host.id().clone(),
            message: "stderr pipe unavailable".into(),
        })?;

        let input = async move {
            if let Some((mut stdin, bytes)) = input {
                stdin.write_all(&bytes).await?;
                stdin.shutdown().await?;
            }
            Ok::<_, std::io::Error>(())
        };
        let completion = async {
            let (status, (stdout, stderr), ()) = tokio::try_join!(
                child.wait(),
                async {
                    tokio::try_join!(
                        drain_bounded(stdout, request.max_stdout_bytes()),
                        drain_bounded(stderr, request.max_stderr_bytes())
                    )
                },
                input
            )?;
            Ok::<_, std::io::Error>((status, stdout, stderr))
        };

        tokio::select! {
            () = cancellation.cancelled() => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                Err(FleetError::Cancelled)
            }
            result = tokio::time::timeout(timeout, completion) => {
                match result {
                    Err(_) => {
                        let _ = child.kill().await;
                        let _ = child.wait().await;
                        Err(FleetError::DeadlineExceeded)
                    }
                    Ok(Err(error)) => Err(FleetError::Command {
                        host: host.id().clone(),
                        message: format!("process I/O failed: {error}"),
                    }),
                    Ok(Ok((status, (stdout, stdout_truncated), (stderr, stderr_truncated)))) => {
                        Ok(CommandOutput::new(
                            stdout,
                            stderr,
                            status.code(),
                            stdout_truncated || stderr_truncated,
                        ))
                    }
                }
            }
        }
    }
}

fn remaining(deadline: soma_ops::Timestamp) -> FleetResult<Duration> {
    let millis = deadline
        .unix_millis()
        .saturating_sub(soma_ops::Timestamp::now().unix_millis());
    if millis <= 0 {
        Err(FleetError::DeadlineExceeded)
    } else {
        Ok(Duration::from_millis(millis as u64))
    }
}

#[cfg(test)]
#[path = "process_driver_tests.rs"]
mod tests;
