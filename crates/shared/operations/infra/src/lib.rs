//! Product-neutral read-only infrastructure engines.
//!
//! `soma-infra` defines typed host, Docker, Compose, and filesystem read
//! contracts above `soma-fleet`. Product configuration, authorization,
//! Flux/Scout compatibility, and CLI/MCP/REST presentation remain outside.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod compose;
mod compose_parse;
mod docker;
mod docker_telemetry;
#[cfg(feature = "bollard-driver")]
mod docker_telemetry_map;
mod error;
mod filesystem;
mod host;
mod logs;
mod process;
mod zfs;

#[cfg(feature = "bollard-driver")]
mod bollard_driver;
#[cfg(feature = "bollard-driver")]
mod bollard_telemetry;
#[cfg(feature = "bollard-driver")]
mod docker_map;
#[cfg(all(feature = "linux-filesystem", target_os = "linux"))]
mod linux_filesystem;
#[cfg(feature = "process-driver")]
mod process_compose;
#[cfg(feature = "process-driver")]
mod process_logs;
#[cfg(feature = "process-driver")]
mod process_process;
#[cfg(feature = "process-driver")]
mod process_zfs;

#[cfg(feature = "bollard-driver")]
pub use bollard_driver::BollardReadClient;
pub use compose::{
    ComposeConfig, ComposeInspector, ComposeLogRequest, ComposeLogs, ComposeProject,
    ComposeProjectRef, ComposeServiceConfig, ComposeServiceStatus, ComposeStatus,
};
pub use docker::{
    ContainerInspect, ContainerListOptions, ContainerReader, ContainerState, ContainerSummary,
    DockerReadClient, DockerSystemInfo, DockerSystemReader, ImageListOptions, ImageReader,
    ImageSummary, NetworkReader, NetworkSummary, VolumeReader, VolumeSummary,
};
pub use docker_telemetry::{
    ContainerLogOptions, ContainerLogs, ContainerStatsSnapshot, DockerDiskUsage, DockerLogStream,
    DockerTelemetryReader, DockerUsageCategory,
};
pub use error::{InfraError, InfraResult};
pub use filesystem::{
    FileHash, FileKind, FileMetadata, FilePreview, FileReadPolicy, FilesystemInspector,
};
pub use host::{
    HostIdentity, HostInspectRequest, HostInspection, HostInspector, HostLoadAverage, HostMemory,
    LinuxCommandHostInspector,
};
#[cfg(all(feature = "linux-filesystem", target_os = "linux"))]
pub use linux_filesystem::LinuxFilesystemInspector;
pub use logs::{
    JournalFilters, JournalPriority, LogPermissionDiagnostic, LogRead, LogReadRequest, LogReader,
    LogSource,
};
pub use process::{ProcessInspector, ProcessListRequest, ProcessRow, ProcessSnapshot, ProcessSort};
#[cfg(feature = "process-driver")]
pub use process_compose::CommandComposeInspector;
#[cfg(feature = "process-driver")]
pub use process_logs::CommandLogReader;
#[cfg(feature = "process-driver")]
pub use process_process::CommandProcessInspector;
#[cfg(feature = "process-driver")]
pub use process_zfs::CommandZfsInspector;
pub use zfs::{
    ZfsDatasetRequest, ZfsDatasetType, ZfsInspector, ZfsPoolRequest, ZfsSnapshotRequest, ZfsTable,
};
