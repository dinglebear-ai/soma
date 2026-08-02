use std::collections::BTreeMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use soma_fleet::{HostId, HostRecord, TopologyRevision};
use tokio_util::sync::CancellationToken;

use crate::InfraResult;

/// Neutral Docker daemon information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DockerSystemInfo {
    /// Target host.
    pub host: HostId,
    /// Exact topology revision.
    pub topology_revision: TopologyRevision,
    /// Docker daemon identity.
    pub daemon_id: Option<String>,
    /// Daemon name.
    pub name: Option<String>,
    /// Server version.
    pub server_version: Option<String>,
    /// Operating-system description.
    pub operating_system: Option<String>,
    /// Architecture.
    pub architecture: Option<String>,
    /// Kernel version.
    pub kernel_version: Option<String>,
    /// Storage driver.
    pub storage_driver: Option<String>,
    /// Total containers known to the daemon.
    pub containers: u64,
    /// Running containers.
    pub containers_running: u64,
    /// Paused containers.
    pub containers_paused: u64,
    /// Stopped containers.
    pub containers_stopped: u64,
    /// Images known to the daemon.
    pub images: u64,
    /// Logical CPU count.
    pub cpus: u64,
    /// Total host memory reported by Docker.
    pub memory_total_bytes: u64,
}

/// Closed container-list options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerListOptions {
    /// Include stopped containers.
    pub all: bool,
    /// Optional exact runtime-state filter.
    pub state: Option<ContainerState>,
    /// Optional Docker label selector.
    pub label: Option<String>,
}

impl Default for ContainerListOptions {
    fn default() -> Self {
        Self {
            all: true,
            state: None,
            label: None,
        }
    }
}

/// Neutral container runtime state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContainerState {
    /// Created but not started.
    Created,
    /// Running.
    Running,
    /// Paused.
    Paused,
    /// Restarting.
    Restarting,
    /// Removing.
    Removing,
    /// Exited.
    Exited,
    /// Dead.
    Dead,
    /// Driver supplied an unrecognized state.
    Unknown(String),
}

impl ContainerState {
    #[cfg(any(feature = "bollard-driver", test))]
    pub(crate) fn from_text(value: Option<&str>) -> Self {
        match value.unwrap_or_default().to_ascii_lowercase().as_str() {
            "created" => Self::Created,
            "running" => Self::Running,
            "paused" => Self::Paused,
            "restarting" => Self::Restarting,
            "removing" => Self::Removing,
            "exited" => Self::Exited,
            "dead" => Self::Dead,
            other => Self::Unknown(other.to_owned()),
        }
    }
}

/// Neutral Docker container summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerSummary {
    /// Target host.
    pub host: HostId,
    /// Exact topology revision.
    pub topology_revision: TopologyRevision,
    /// Container ID.
    pub id: Option<String>,
    /// Container names without Docker's leading slash normalization requirement.
    pub names: Vec<String>,
    /// Configured image reference.
    pub image: Option<String>,
    /// Image content ID.
    pub image_id: Option<String>,
    /// Configured command.
    pub command: Option<String>,
    /// Creation time in Unix seconds.
    pub created_unix_seconds: Option<i64>,
    /// Runtime state.
    pub state: ContainerState,
    /// Engine status text.
    pub status: Option<String>,
    /// Container labels.
    pub labels: BTreeMap<String, String>,
}

/// Selected neutral container inspection fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerInspect {
    /// Target host.
    pub host: HostId,
    /// Exact topology revision.
    pub topology_revision: TopologyRevision,
    /// Container ID.
    pub id: Option<String>,
    /// Container name.
    pub name: Option<String>,
    /// Creation timestamp text reported by Docker.
    pub created: Option<String>,
    /// Executable path.
    pub path: Option<String>,
    /// Process arguments.
    pub args: Vec<String>,
    /// Image content ID.
    pub image: Option<String>,
    /// Current state.
    pub state: ContainerState,
    /// Process ID when running.
    pub pid: Option<i64>,
    /// Exit code when reported.
    pub exit_code: Option<i64>,
    /// Restart count.
    pub restart_count: Option<i64>,
    /// Config labels.
    pub labels: BTreeMap<String, String>,
}

/// Closed image-list options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ImageListOptions {
    /// Include intermediate images.
    pub all: bool,
    /// Return dangling images only.
    pub dangling_only: bool,
}

/// Neutral Docker image summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageSummary {
    /// Target host.
    pub host: HostId,
    /// Exact topology revision.
    pub topology_revision: TopologyRevision,
    /// Image ID.
    pub id: String,
    /// Repository tags.
    pub repo_tags: Vec<String>,
    /// Repository digests.
    pub repo_digests: Vec<String>,
    /// Creation time in Unix seconds.
    pub created_unix_seconds: i64,
    /// Image size in bytes.
    pub size_bytes: i64,
    /// Number of containers referencing the image when reported.
    pub containers: i64,
    /// Image labels.
    pub labels: BTreeMap<String, String>,
}

/// Neutral Docker network summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkSummary {
    /// Target host.
    pub host: HostId,
    /// Exact topology revision.
    pub topology_revision: TopologyRevision,
    /// Network ID.
    pub id: Option<String>,
    /// Network name.
    pub name: Option<String>,
    /// Driver name.
    pub driver: Option<String>,
    /// Scope.
    pub scope: Option<String>,
    /// Whether the network is internal.
    pub internal: Option<bool>,
    /// Whether containers may attach manually.
    pub attachable: Option<bool>,
    /// Network labels.
    pub labels: BTreeMap<String, String>,
}

/// Neutral Docker volume summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VolumeSummary {
    /// Target host.
    pub host: HostId,
    /// Exact topology revision.
    pub topology_revision: TopologyRevision,
    /// Volume name.
    pub name: String,
    /// Volume driver.
    pub driver: String,
    /// Host mountpoint.
    pub mountpoint: String,
    /// Volume scope.
    pub scope: Option<String>,
    /// Volume labels.
    pub labels: BTreeMap<String, String>,
}

/// Docker system-level read operations.
#[async_trait]
pub trait DockerSystemReader: Send + Sync {
    /// Reads Docker daemon information.
    async fn system_info(
        &self,
        host: &HostRecord,
        cancellation: &CancellationToken,
    ) -> InfraResult<DockerSystemInfo>;
}

/// Docker container read operations.
#[async_trait]
pub trait ContainerReader: Send + Sync {
    /// Lists containers.
    async fn list_containers(
        &self,
        host: &HostRecord,
        options: &ContainerListOptions,
        cancellation: &CancellationToken,
    ) -> InfraResult<Vec<ContainerSummary>>;

    /// Inspects one container.
    async fn inspect_container(
        &self,
        host: &HostRecord,
        container: &str,
        cancellation: &CancellationToken,
    ) -> InfraResult<ContainerInspect>;
}

/// Docker image read operations.
#[async_trait]
pub trait ImageReader: Send + Sync {
    /// Lists images.
    async fn list_images(
        &self,
        host: &HostRecord,
        options: &ImageListOptions,
        cancellation: &CancellationToken,
    ) -> InfraResult<Vec<ImageSummary>>;
}

/// Docker network read operations.
#[async_trait]
pub trait NetworkReader: Send + Sync {
    /// Lists networks.
    async fn list_networks(
        &self,
        host: &HostRecord,
        cancellation: &CancellationToken,
    ) -> InfraResult<Vec<NetworkSummary>>;
}

/// Docker volume read operations.
#[async_trait]
pub trait VolumeReader: Send + Sync {
    /// Lists volumes.
    async fn list_volumes(
        &self,
        host: &HostRecord,
        cancellation: &CancellationToken,
    ) -> InfraResult<Vec<VolumeSummary>>;
}

/// Complete neutral Docker read surface.
pub trait DockerReadClient:
    DockerSystemReader + ContainerReader + ImageReader + NetworkReader + VolumeReader
{
}

impl<T> DockerReadClient for T where
    T: DockerSystemReader + ContainerReader + ImageReader + NetworkReader + VolumeReader
{
}

#[cfg(test)]
#[path = "docker_tests.rs"]
mod tests;
