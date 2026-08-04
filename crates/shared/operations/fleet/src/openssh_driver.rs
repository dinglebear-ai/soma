use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use crate::{
    CommandExecutor, CommandOutput, CommandRequest, ConnectionPool, FleetError, FleetResult,
    HostEndpoint, HostId, HostRecord, OpenSshConnection, OpenSshConnector, TopologySnapshot,
    io::drain_bounded,
};

/// Pooled strict OpenSSH command driver.
pub struct OpenSshDriver {
    pool: ConnectionPool<OpenSshConnector>,
}

impl Default for OpenSshDriver {
    fn default() -> Self {
        Self::new(OpenSshConnector::default())
    }
}

impl OpenSshDriver {
    /// Creates a driver with an empty revision-aware connection pool.
    #[must_use]
    pub fn new(connector: OpenSshConnector) -> Self {
        Self {
            pool: ConnectionPool::new(Arc::new(connector)),
        }
    }

    /// Invalidates every cached revision for one host.
    pub async fn invalidate_host(&self, host: &HostId) -> FleetResult<usize> {
        self.pool.invalidate_host(host).await
    }

    /// Evicts cached sessions absent from the supplied snapshot.
    pub async fn retain_snapshot(&self, snapshot: &TopologySnapshot) -> FleetResult<usize> {
        self.pool.retain_snapshot(snapshot).await
    }

    /// Closes all cached sessions.
    pub async fn shutdown(&self) -> FleetResult<usize> {
        self.pool.shutdown().await
    }

    /// Returns the exact pooled connection for forwarding adapters.
    pub async fn connection(
        &self,
        host: &HostRecord,
        cancellation: &CancellationToken,
    ) -> FleetResult<Arc<OpenSshConnection>> {
        self.pool.get_or_connect(host, cancellation).await
    }
}

#[async_trait]
impl CommandExecutor for OpenSshDriver {
    async fn execute(
        &self,
        host: &HostRecord,
        request: &CommandRequest,
        cancellation: &CancellationToken,
    ) -> FleetResult<CommandOutput> {
        if !matches!(host.endpoint(), HostEndpoint::Ssh(_)) {
            return Err(FleetError::Command {
                host: host.id().clone(),
                message: "OpenSSH driver requires an SSH endpoint".into(),
            });
        }
        if request.working_dir().is_some() {
            return Err(FleetError::Command {
                host: host.id().clone(),
                message:
                    "remote working directories are unsupported without a typed remote launcher"
                        .into(),
            });
        }
        if cancellation.is_cancelled() {
            return Err(FleetError::Cancelled);
        }
        request.validate_at(soma_ops::Timestamp::now())?;
        let connection = self.connection(host, cancellation).await?;
        if connection.revision() != host.revision() {
            return Err(FleetError::StaleTopology {
                host: host.id().clone(),
                expected: connection.revision().clone(),
                actual: host.revision().clone(),
            });
        }
        let permit_timeout = remaining(request.deadline())?;
        let permit = tokio::select! {
            () = cancellation.cancelled() => return Err(FleetError::Cancelled),
            result = tokio::time::timeout(permit_timeout, connection.acquire_permit()) => match result {
                Err(_) => return Err(FleetError::DeadlineExceeded),
                Ok(Err(_)) => return Err(FleetError::Connection {
                    host: host.id().clone(),
                    message: "OpenSSH execution semaphore is closed".into(),
                }),
                Ok(Ok(permit)) => permit,
            }
        };
        let session = connection.session().await?;
        let mut command = session.arc_command(request.program().to_owned());
        command.args(request.args());
        command
            .stdin(if request.stdin().is_some() {
                openssh::Stdio::piped()
            } else {
                openssh::Stdio::null()
            })
            .stdout(openssh::Stdio::piped())
            .stderr(openssh::Stdio::piped());
        let mut child = command.spawn().await.map_err(|error| FleetError::Command {
            host: host.id().clone(),
            message: format!("OpenSSH spawn failed: {error}"),
        })?;
        let input = match request.stdin() {
            Some(bytes) => Some((
                child.stdin().take().ok_or_else(|| FleetError::Command {
                    host: host.id().clone(),
                    message: "OpenSSH stdin pipe unavailable".into(),
                })?,
                bytes.to_vec(),
            )),
            None => None,
        };
        let stdout = child.stdout().take().ok_or_else(|| FleetError::Command {
            host: host.id().clone(),
            message: "OpenSSH stdout pipe unavailable".into(),
        })?;
        let stderr = child.stderr().take().ok_or_else(|| FleetError::Command {
            host: host.id().clone(),
            message: "OpenSSH stderr pipe unavailable".into(),
        })?;
        let timeout = remaining(request.deadline())?;
        let completion = async move {
            let streams = async {
                tokio::try_join!(
                    drain_bounded(stdout, request.max_stdout_bytes()),
                    drain_bounded(stderr, request.max_stderr_bytes())
                )
                .map_err(openssh::Error::ChildIo)
            };
            let input = async move {
                if let Some((mut stdin, bytes)) = input {
                    stdin
                        .write_all(&bytes)
                        .await
                        .map_err(openssh::Error::ChildIo)?;
                    stdin.shutdown().await.map_err(openssh::Error::ChildIo)?;
                }
                Ok::<_, openssh::Error>(())
            };
            let (status, (stdout, stderr), ()) = tokio::try_join!(child.wait(), streams, input)?;
            Ok::<_, openssh::Error>((status, stdout, stderr))
        };
        let mut completion = Box::pin(completion);
        let result = tokio::select! {
            () = cancellation.cancelled() => Err(FleetError::RemoteCommandDetached {
                host: host.id().clone(),
                reason: "cancellation",
            }),
            result = tokio::time::timeout(timeout, &mut completion) => match result {
                Err(_) => Err(FleetError::RemoteCommandDetached {
                    host: host.id().clone(),
                    reason: "deadline",
                }),
                Ok(Err(error)) => Err(FleetError::Command {
                    host: host.id().clone(),
                    message: format!("OpenSSH command I/O failed: {error}"),
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
        };
        drop(completion);
        drop(permit);
        if result.is_err() {
            let _ = self.invalidate_host(host.id()).await;
        }
        result
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
#[path = "openssh_driver_tests.rs"]
mod tests;
