use std::sync::Arc;

use async_trait::async_trait;
use soma_fleet::{CommandExecutor, CommandOutput, CommandRequest, HostRecord};
use soma_ops::Timestamp;
use tokio_util::sync::CancellationToken;

use crate::zfs::parse_zfs_table;
use crate::{
    InfraError, InfraResult, ZfsDatasetRequest, ZfsInspector, ZfsPoolRequest, ZfsSnapshotRequest,
    ZfsTable,
};

const ZFS_OUTPUT_LIMIT: usize = 2 * 1024 * 1024;

/// ZFS inspector backed by a fleet command executor.
pub struct CommandZfsInspector<E> {
    executor: Arc<E>,
}

impl<E> CommandZfsInspector<E> {
    /// Creates an inspector using the supplied fleet executor.
    #[must_use]
    pub fn new(executor: Arc<E>) -> Self {
        Self { executor }
    }
}

#[async_trait]
impl<E> ZfsInspector for CommandZfsInspector<E>
where
    E: CommandExecutor,
{
    async fn pools(
        &self,
        host: &HostRecord,
        request: &ZfsPoolRequest,
        cancellation: &CancellationToken,
    ) -> InfraResult<ZfsTable> {
        let mut args = vec!["list".to_owned()];
        if let Some(pool) = request.pool() {
            args.push(pool.to_owned());
        }
        let output = self
            .execute(host, "zpool", args, request.deadline(), cancellation)
            .await?;
        parse_zfs_table(host, &output, None)
    }

    async fn datasets(
        &self,
        host: &HostRecord,
        request: &ZfsDatasetRequest,
        cancellation: &CancellationToken,
    ) -> InfraResult<ZfsTable> {
        let mut args = vec!["list".to_owned()];
        if let Some(dataset_type) = request.dataset_type() {
            args.extend(["-t".into(), dataset_type.as_arg().into()]);
        }
        if request.is_recursive() || request.pool().is_some() {
            args.push("-r".into());
        }
        if let Some(pool) = request.pool() {
            args.push(pool.to_owned());
        }
        let output = self
            .execute(host, "zfs", args, request.deadline(), cancellation)
            .await?;
        parse_zfs_table(host, &output, None)
    }

    async fn snapshots(
        &self,
        host: &HostRecord,
        request: &ZfsSnapshotRequest,
        cancellation: &CancellationToken,
    ) -> InfraResult<ZfsTable> {
        let mut args = vec!["list".into(), "-t".into(), "snapshot".into()];
        if let Some(target) = request.dataset().or_else(|| request.pool()) {
            args.extend(["-r".into(), target.to_owned()]);
        }
        let output = self
            .execute(host, "zfs", args, request.deadline(), cancellation)
            .await?;
        parse_zfs_table(host, &output, Some(request.limit()))
    }
}

impl<E> CommandZfsInspector<E>
where
    E: CommandExecutor,
{
    async fn execute(
        &self,
        host: &HostRecord,
        program: &str,
        args: Vec<String>,
        deadline: Timestamp,
        cancellation: &CancellationToken,
    ) -> InfraResult<String> {
        let request = CommandRequest::new(program, args, deadline)
            .map_err(soma_fleet::FleetError::from)?
            .with_output_limits(ZFS_OUTPUT_LIMIT, ZFS_OUTPUT_LIMIT)
            .map_err(soma_fleet::FleetError::from)?;
        let output = self.executor.execute(host, &request, cancellation).await?;
        validate_output(host, program, &output)?;
        std::str::from_utf8(output.stdout())
            .map(str::to_owned)
            .map_err(|error| InfraError::Parse {
                domain: "zfs",
                message: format!("ZFS output is not UTF-8: {error}"),
            })
    }
}

fn validate_output(host: &HostRecord, program: &str, output: &CommandOutput) -> InfraResult<()> {
    if output.truncated() {
        return Err(InfraError::InvalidRequest {
            domain: "zfs",
            message: format!("{program} output exceeded {ZFS_OUTPUT_LIMIT} bytes"),
        });
    }
    if output.exit_code() != Some(0) {
        return Err(InfraError::CommandFailed {
            domain: "zfs",
            host: host.id().clone(),
            exit_code: output.exit_code(),
            stderr: String::from_utf8_lossy(output.stderr()).trim().to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
#[path = "process_zfs_tests.rs"]
mod tests;
