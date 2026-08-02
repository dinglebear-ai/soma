use std::sync::Arc;

use async_trait::async_trait;
use soma_fleet::{CommandExecutor, CommandRequest, HostRecord};
use tokio_util::sync::CancellationToken;

use crate::process::parse_process_rows;
use crate::{InfraError, InfraResult, ProcessInspector, ProcessListRequest, ProcessSnapshot};

const PROCESS_OUTPUT_LIMIT: usize = 4 * 1024 * 1024;

/// Process inspector backed by a fleet command executor.
pub struct CommandProcessInspector<E> {
    executor: Arc<E>,
}

impl<E> CommandProcessInspector<E> {
    /// Creates an inspector using the supplied fleet executor.
    #[must_use]
    pub fn new(executor: Arc<E>) -> Self {
        Self { executor }
    }
}

#[async_trait]
impl<E> ProcessInspector for CommandProcessInspector<E>
where
    E: CommandExecutor,
{
    async fn list_processes(
        &self,
        host: &HostRecord,
        request: &ProcessListRequest,
        cancellation: &CancellationToken,
    ) -> InfraResult<ProcessSnapshot> {
        let command = CommandRequest::new(
            "ps",
            ["aux", "--sort", request.sort().ps_argument()],
            request.deadline(),
        )
        .map_err(soma_fleet::FleetError::from)?
        .with_output_limits(PROCESS_OUTPUT_LIMIT, PROCESS_OUTPUT_LIMIT)
        .map_err(soma_fleet::FleetError::from)?;
        let output = self.executor.execute(host, &command, cancellation).await?;
        if output.truncated() {
            return Err(InfraError::InvalidRequest {
                domain: "process",
                message: format!("process output exceeded {PROCESS_OUTPUT_LIMIT} bytes"),
            });
        }
        if output.exit_code() != Some(0) {
            return Err(InfraError::CommandFailed {
                domain: "process",
                host: host.id().clone(),
                exit_code: output.exit_code(),
                stderr: String::from_utf8_lossy(output.stderr()).trim().to_owned(),
            });
        }
        let stdout = std::str::from_utf8(output.stdout()).map_err(|error| InfraError::Parse {
            domain: "process",
            message: format!("process output is not UTF-8: {error}"),
        })?;
        let mut lines = stdout.lines();
        let _header = lines.next();
        parse_process_rows(
            host,
            request,
            &lines.collect::<Vec<_>>().join(
                "
",
            ),
        )
    }
}

#[cfg(test)]
#[path = "process_process_tests.rs"]
mod tests;
