use std::collections::HashMap;
use std::future::Future;
use std::path::Path;

use async_trait::async_trait;
use bollard::query_parameters::{
    ListContainersOptions, ListImagesOptions, ListNetworksOptions, ListVolumesOptions,
};
use bollard::{API_DEFAULT_VERSION, Docker};
use soma_fleet::{HostEndpoint, HostId, HostRecord, TopologyRevision};
use tokio_util::sync::CancellationToken;

use crate::docker_map::{
    array_field, map_container_inspect, map_container_summary, map_image, map_network,
    map_system_info, map_volume, parse_error,
};
use crate::{
    ContainerInspect, ContainerListOptions, ContainerReader, ContainerSummary, DockerSystemInfo,
    DockerSystemReader, ImageListOptions, ImageReader, ImageSummary, InfraError, InfraResult,
    NetworkReader, NetworkSummary, VolumeReader, VolumeSummary,
};

const CLIENT_TIMEOUT_SECONDS: u64 = 120;

/// Local Bollard implementation of the neutral Docker read contracts.
pub struct BollardReadClient {
    docker: Docker,
    host: HostId,
    revision: TopologyRevision,
}

impl BollardReadClient {
    /// Connects to a local Docker socket and binds the client to one host revision.
    pub fn connect_local(host: &HostRecord, socket: Option<&Path>) -> InfraResult<Self> {
        if !matches!(host.endpoint(), HostEndpoint::Local) {
            return Err(InfraError::UnsupportedTarget {
                domain: "docker",
                host: host.id().clone(),
            });
        }
        let docker = match socket {
            Some(path) => Docker::connect_with_socket(
                path.to_string_lossy().as_ref(),
                CLIENT_TIMEOUT_SECONDS,
                API_DEFAULT_VERSION,
            ),
            None => Docker::connect_with_socket_defaults(),
        }
        .map_err(|error| InfraError::Docker(error.to_string()))?;
        Ok(Self {
            docker,
            host: host.id().clone(),
            revision: host.revision().clone(),
        })
    }

    fn validate_host(&self, host: &HostRecord) -> InfraResult<()> {
        if host.id() == &self.host && host.revision() == &self.revision {
            Ok(())
        } else {
            Err(InfraError::InvalidRequest {
                domain: "docker",
                message: format!(
                    "client is bound to {}@{}, received {}@{}",
                    self.host,
                    self.revision,
                    host.id(),
                    host.revision()
                ),
            })
        }
    }
}

#[async_trait]
impl DockerSystemReader for BollardReadClient {
    async fn system_info(
        &self,
        host: &HostRecord,
        cancellation: &CancellationToken,
    ) -> InfraResult<DockerSystemInfo> {
        self.validate_host(host)?;
        let value = serde_json::to_value(cancellable(cancellation, self.docker.info()).await?)
            .map_err(|error| parse_error(error.to_string()))?;
        map_system_info(host, &value)
    }
}

#[async_trait]
impl ContainerReader for BollardReadClient {
    async fn list_containers(
        &self,
        host: &HostRecord,
        options: &ContainerListOptions,
        cancellation: &CancellationToken,
    ) -> InfraResult<Vec<ContainerSummary>> {
        self.validate_host(host)?;
        let mut query = ListContainersOptions {
            all: options.all,
            ..Default::default()
        };
        if let Some(label) = options.label.as_deref() {
            validate_filter(label)?;
            let mut filters = HashMap::new();
            filters.insert("label".to_owned(), vec![label.to_owned()]);
            query.filters = Some(filters);
        }
        let rows = cancellable(cancellation, self.docker.list_containers(Some(query))).await?;
        rows.into_iter()
            .map(|row| serde_json::to_value(row).map_err(|error| parse_error(error.to_string())))
            .map(|value| value.and_then(|value| map_container_summary(host, &value)))
            .filter(|result| {
                result.as_ref().map_or(true, |row| {
                    options
                        .state
                        .as_ref()
                        .is_none_or(|state| &row.state == state)
                })
            })
            .collect()
    }

    async fn inspect_container(
        &self,
        host: &HostRecord,
        container: &str,
        cancellation: &CancellationToken,
    ) -> InfraResult<ContainerInspect> {
        self.validate_host(host)?;
        validate_identifier("container", container)?;
        let row = cancellable(cancellation, self.docker.inspect_container(container, None)).await?;
        let value = serde_json::to_value(row).map_err(|error| parse_error(error.to_string()))?;
        map_container_inspect(host, &value)
    }
}

#[async_trait]
impl ImageReader for BollardReadClient {
    async fn list_images(
        &self,
        host: &HostRecord,
        options: &ImageListOptions,
        cancellation: &CancellationToken,
    ) -> InfraResult<Vec<ImageSummary>> {
        self.validate_host(host)?;
        let mut query = ListImagesOptions {
            all: options.all,
            ..Default::default()
        };
        if options.dangling_only {
            let mut filters = HashMap::new();
            filters.insert("dangling".to_owned(), vec!["true".to_owned()]);
            query.filters = Some(filters);
        }
        let rows = cancellable(cancellation, self.docker.list_images(Some(query))).await?;
        rows.into_iter()
            .map(|row| serde_json::to_value(row).map_err(|error| parse_error(error.to_string())))
            .map(|value| value.and_then(|value| map_image(host, &value)))
            .collect()
    }
}

#[async_trait]
impl NetworkReader for BollardReadClient {
    async fn list_networks(
        &self,
        host: &HostRecord,
        cancellation: &CancellationToken,
    ) -> InfraResult<Vec<NetworkSummary>> {
        self.validate_host(host)?;
        let rows = cancellable(
            cancellation,
            self.docker.list_networks(None::<ListNetworksOptions>),
        )
        .await?;
        rows.into_iter()
            .map(|row| serde_json::to_value(row).map_err(|error| parse_error(error.to_string())))
            .map(|value| value.and_then(|value| map_network(host, &value)))
            .collect()
    }
}

#[async_trait]
impl VolumeReader for BollardReadClient {
    async fn list_volumes(
        &self,
        host: &HostRecord,
        cancellation: &CancellationToken,
    ) -> InfraResult<Vec<VolumeSummary>> {
        self.validate_host(host)?;
        let response = cancellable(
            cancellation,
            self.docker.list_volumes(None::<ListVolumesOptions>),
        )
        .await?;
        let value =
            serde_json::to_value(response).map_err(|error| parse_error(error.to_string()))?;
        match array_field(&value, &["Volumes", "volumes"]) {
            Some(values) => values.iter().map(|value| map_volume(host, value)).collect(),
            None => Ok(Vec::new()),
        }
    }
}

async fn cancellable<T, F>(cancellation: &CancellationToken, future: F) -> InfraResult<T>
where
    F: Future<Output = Result<T, bollard::errors::Error>>,
{
    tokio::select! {
        () = cancellation.cancelled() => Err(soma_fleet::FleetError::Cancelled.into()),
        result = future => result.map_err(|error| InfraError::Docker(error.to_string())),
    }
}

fn validate_filter(value: &str) -> InfraResult<()> {
    if value.is_empty() || value.len() > 1024 || value.chars().any(char::is_control) {
        Err(InfraError::InvalidRequest {
            domain: "docker",
            message: "label filter must contain 1-1024 printable characters".into(),
        })
    } else {
        Ok(())
    }
}

fn validate_identifier(kind: &str, value: &str) -> InfraResult<()> {
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        Err(InfraError::InvalidRequest {
            domain: "docker",
            message: format!("invalid {kind} identifier"),
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "bollard_driver_tests.rs"]
mod tests;
