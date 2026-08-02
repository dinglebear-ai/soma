use async_trait::async_trait;
use bollard::query_parameters::{DataUsageOptions, LogsOptions, StatsOptions};
use futures_util::StreamExt;
use soma_fleet::HostRecord;
use tokio_util::sync::CancellationToken;

use crate::bollard_driver::{BollardReadClient, cancellable};
use crate::docker_map::parse_error;
use crate::docker_telemetry_map::{map_container_stats, map_disk_usage};
use crate::{
    ContainerLogOptions, ContainerLogs, ContainerStatsSnapshot, DockerDiskUsage, DockerLogStream,
    DockerTelemetryReader, InfraError, InfraResult,
};

const MAX_LOG_BYTES: usize = 4 * 1024 * 1024;

#[async_trait]
impl DockerTelemetryReader for BollardReadClient {
    async fn disk_usage(
        &self,
        host: &HostRecord,
        cancellation: &CancellationToken,
    ) -> InfraResult<DockerDiskUsage> {
        self.validate_host(host)?;
        let response =
            cancellable(cancellation, self.docker().df(None::<DataUsageOptions>)).await?;
        let value =
            serde_json::to_value(response).map_err(|error| parse_error(error.to_string()))?;
        map_disk_usage(host, &value)
    }

    async fn container_logs(
        &self,
        host: &HostRecord,
        container: &str,
        options: &ContainerLogOptions,
        cancellation: &CancellationToken,
    ) -> InfraResult<ContainerLogs> {
        self.validate_host(host)?;
        validate_identifier(container)?;
        let (stdout, stderr) = match options.stream() {
            DockerLogStream::Stdout => (true, false),
            DockerLogStream::Stderr => (false, true),
            DockerLogStream::Both => (true, true),
        };
        let query = LogsOptions {
            follow: false,
            stdout,
            stderr,
            since: to_i32_time("since", options.since_unix_seconds())?,
            until: to_i32_time("until", options.until_unix_seconds())?,
            timestamps: false,
            tail: options.lines().to_string(),
        };
        let stream = self.docker().logs(container, Some(query));
        futures_util::pin_mut!(stream);
        let mut lines = Vec::new();
        let mut retained_bytes = 0_usize;
        let mut truncated = false;
        'frames: loop {
            let item = tokio::select! {
                () = cancellation.cancelled() => {
                    return Err(soma_fleet::FleetError::Cancelled.into());
                }
                item = stream.next() => item,
            };
            let Some(item) = item else {
                break;
            };
            let frame = item.map_err(|error| InfraError::Docker(error.to_string()))?;
            for line in frame
                .to_string()
                .lines()
                .map(|line| line.trim_end_matches(['\r', '\n']))
                .filter(|line| !line.is_empty())
                .filter(|line| options.grep().is_none_or(|pattern| line.contains(pattern)))
            {
                let next = retained_bytes.saturating_add(line.len());
                if next > MAX_LOG_BYTES {
                    truncated = true;
                    break 'frames;
                }
                retained_bytes = next;
                lines.push(line.to_owned());
            }
        }
        Ok(ContainerLogs {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            container: container.to_owned(),
            lines,
            truncated,
        })
    }

    async fn container_stats(
        &self,
        host: &HostRecord,
        container: &str,
        cancellation: &CancellationToken,
    ) -> InfraResult<ContainerStatsSnapshot> {
        self.validate_host(host)?;
        validate_identifier(container)?;
        let stream = self.docker().stats(
            container,
            Some(StatsOptions {
                stream: false,
                one_shot: true,
            }),
        );
        futures_util::pin_mut!(stream);
        let item = tokio::select! {
            () = cancellation.cancelled() => {
                return Err(soma_fleet::FleetError::Cancelled.into());
            }
            item = stream.next() => item,
        };
        let stats = item
            .ok_or_else(|| InfraError::Docker(format!("no stats frame for container {container}")))?
            .map_err(|error| InfraError::Docker(error.to_string()))?;
        let value = serde_json::to_value(stats).map_err(|error| parse_error(error.to_string()))?;
        map_container_stats(host, container, &value)
    }
}

fn validate_identifier(value: &str) -> InfraResult<()> {
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        Err(InfraError::InvalidRequest {
            domain: "docker",
            message: "invalid container identifier".into(),
        })
    } else {
        Ok(())
    }
}

fn to_i32_time(name: &'static str, value: Option<i64>) -> InfraResult<i32> {
    match value {
        None => Ok(0),
        Some(value) => i32::try_from(value).map_err(|_| InfraError::InvalidRequest {
            domain: "docker",
            message: format!("container log {name} is outside Docker's supported range"),
        }),
    }
}

#[cfg(test)]
#[path = "bollard_telemetry_tests.rs"]
mod tests;
