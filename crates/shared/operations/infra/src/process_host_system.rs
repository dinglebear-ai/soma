use std::path::{Component, Path};
use std::sync::Arc;

use async_trait::async_trait;
use soma_fleet::{CommandExecutor, CommandOutput, CommandRequest, HostRecord};
use soma_ops::Timestamp;
use tokio_util::sync::CancellationToken;

use crate::host_system_parse::{
    parse_mounts, parse_network, parse_ports, parse_services, parse_usage,
};
use crate::{
    DoctorCheck, DoctorReport, FilesystemUsage, HostSystemInspector, InfraError, InfraResult,
    MountInfo, NetworkInterface, PortInfo, PortListRequest, ServiceListRequest, ServiceStatus,
};

const HOST_SYSTEM_OUTPUT_LIMIT: usize = 4 * 1024 * 1024;

/// Host-system inspector backed by a fleet command executor.
pub struct CommandHostSystemInspector<E> {
    executor: Arc<E>,
}

impl<E> CommandHostSystemInspector<E> {
    /// Creates an inspector using the supplied executor.
    #[must_use]
    pub fn new(executor: Arc<E>) -> Self {
        Self { executor }
    }
}

#[async_trait]
impl<E> HostSystemInspector for CommandHostSystemInspector<E>
where
    E: CommandExecutor,
{
    async fn services(
        &self,
        host: &HostRecord,
        request: &ServiceListRequest,
        cancellation: &CancellationToken,
    ) -> InfraResult<Vec<ServiceStatus>> {
        let raw = self
            .run(
                host,
                "systemctl",
                vec![
                    "list-units".into(),
                    "--type=service".into(),
                    "--all".into(),
                    "--no-legend".into(),
                    "--no-pager".into(),
                ],
                request.deadline(),
                cancellation,
            )
            .await?;
        Ok(parse_services(&raw)
            .into_iter()
            .filter(|row| {
                request
                    .service()
                    .is_none_or(|value| row.unit.contains(value))
                    && request.state().is_none_or(|value| row.active == value)
            })
            .collect())
    }

    async fn network(
        &self,
        host: &HostRecord,
        deadline: Timestamp,
        cancellation: &CancellationToken,
    ) -> InfraResult<Vec<NetworkInterface>> {
        let raw = self
            .run(
                host,
                "ip",
                vec!["-j".into(), "address".into()],
                deadline,
                cancellation,
            )
            .await?;
        parse_network(&raw)
    }

    async fn mounts(
        &self,
        host: &HostRecord,
        deadline: Timestamp,
        cancellation: &CancellationToken,
    ) -> InfraResult<Vec<MountInfo>> {
        let raw = self
            .run(
                host,
                "findmnt",
                vec![
                    "-J".into(),
                    "-b".into(),
                    "-o".into(),
                    "TARGET,SOURCE,FSTYPE,OPTIONS,SIZE,USED,AVAIL".into(),
                ],
                deadline,
                cancellation,
            )
            .await?;
        parse_mounts(&raw)
    }

    async fn ports(
        &self,
        host: &HostRecord,
        request: &PortListRequest,
        cancellation: &CancellationToken,
    ) -> InfraResult<Vec<PortInfo>> {
        let mut args = vec!["-H".into(), "-l".into(), "-n".into(), "-p".into()];
        match request.protocol() {
            Some(protocol) => args.push(protocol.as_ss_filter().into()),
            None => {
                args.push("-t".into());
                args.push("-u".into());
            }
        }
        let raw = self
            .run(host, "ss", args, request.deadline(), cancellation)
            .await?;
        Ok(parse_ports(&raw)
            .into_iter()
            .skip(request.offset() as usize)
            .take(request.limit() as usize)
            .collect())
    }

    async fn filesystem_usage(
        &self,
        host: &HostRecord,
        path: Option<&str>,
        deadline: Timestamp,
        cancellation: &CancellationToken,
    ) -> InfraResult<FilesystemUsage> {
        let mut args = vec![
            "-B1".into(),
            "--output=source,fstype,size,used,avail,pcent,target".into(),
        ];
        if let Some(path) = path {
            validate_path(path)?;
            args.push(path.to_owned());
        }
        let raw = self.run(host, "df", args, deadline, cancellation).await?;
        parse_usage(&raw)
    }

    async fn doctor(
        &self,
        host: &HostRecord,
        deadline: Timestamp,
        cancellation: &CancellationToken,
    ) -> InfraResult<DoctorReport> {
        let mut checks = Vec::new();
        checks.push(check(
            "network",
            self.network(host, deadline, cancellation)
                .await
                .map(|rows| format!("{} interface(s)", rows.len())),
        ));
        checks.push(check(
            "services",
            self.services(host, &ServiceListRequest::new(deadline), cancellation)
                .await
                .map(|rows| format!("{} service(s)", rows.len())),
        ));
        checks.push(check(
            "storage",
            self.filesystem_usage(host, None, deadline, cancellation)
                .await
                .map(|usage| format!("{}% used on {}", usage.usage_percent, usage.target)),
        ));
        let all_ok = checks.iter().all(|check| check.ok);
        Ok(DoctorReport {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            overall: if all_ok { "ok" } else { "degraded" }.into(),
            checks,
        })
    }
}

impl<E> CommandHostSystemInspector<E>
where
    E: CommandExecutor,
{
    async fn run(
        &self,
        host: &HostRecord,
        program: &str,
        args: Vec<String>,
        deadline: Timestamp,
        cancellation: &CancellationToken,
    ) -> InfraResult<String> {
        let request = CommandRequest::new(program, args, deadline)
            .map_err(soma_fleet::FleetError::from)?
            .with_output_limits(HOST_SYSTEM_OUTPUT_LIMIT, HOST_SYSTEM_OUTPUT_LIMIT)
            .map_err(soma_fleet::FleetError::from)?;
        let output = self.executor.execute(host, &request, cancellation).await?;
        checked_output(host, "host-system", output)
    }
}

fn checked_output(
    host: &HostRecord,
    domain: &'static str,
    output: CommandOutput,
) -> InfraResult<String> {
    if output.exit_code() != Some(0) {
        return Err(InfraError::CommandFailed {
            domain,
            host: host.id().clone(),
            exit_code: output.exit_code(),
            stderr: String::from_utf8_lossy(output.stderr()).trim().to_owned(),
        });
    }
    if output.truncated() {
        return Err(InfraError::Parse {
            domain,
            message: "bounded command output was truncated".into(),
        });
    }
    String::from_utf8(output.stdout().to_vec()).map_err(|error| InfraError::Parse {
        domain,
        message: format!("command output was not UTF-8: {error}"),
    })
}

fn validate_path(value: &str) -> InfraResult<()> {
    let path = Path::new(value);
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        Err(InfraError::InvalidRequest {
            domain: "filesystem",
            message: format!("path must be absolute and normalized: {value}"),
        })
    } else {
        Ok(())
    }
}

fn check(name: &str, result: InfraResult<String>) -> DoctorCheck {
    match result {
        Ok(summary) => DoctorCheck {
            name: name.into(),
            ok: true,
            summary,
        },
        Err(error) => DoctorCheck {
            name: name.into(),
            ok: false,
            summary: error.to_string(),
        },
    }
}

#[cfg(test)]
#[path = "process_host_system_tests.rs"]
mod tests;
