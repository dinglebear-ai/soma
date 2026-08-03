//! Product-neutral infrastructure read and verified mutation engines.
//!
//! `soma-infra` defines typed host, Docker, Compose, filesystem, process, log,
//! and ZFS contracts above `soma-fleet`, plus bounded mutation coordinators that
//! preserve send state and verify postconditions. Product configuration,
//! authorization, and CLI/MCP/REST presentation remain outside.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod compose;
mod compose_mutation;
mod compose_parse;
mod compose_pull;
mod compose_pull_engine;
mod container_mutation;
mod container_mutation_engine;
mod docker;
mod docker_provider;
mod docker_telemetry;
#[cfg(feature = "bollard-driver")]
mod docker_telemetry_map;
mod error;
mod filesystem;
mod filesystem_query;
mod host;
mod host_system;
#[cfg(feature = "process-driver")]
mod host_system_parse;
mod image_pull;
mod image_pull_engine;
mod logs;
mod mutation;
mod process;
mod progress_sink;
mod zfs;

#[cfg(feature = "bollard-driver")]
mod bollard_driver;
#[cfg(feature = "bollard-driver")]
mod bollard_image_pull;
#[cfg(feature = "bollard-driver")]
mod bollard_mutation;
#[cfg(feature = "remote-bollard")]
mod bollard_provider;
#[cfg(feature = "bollard-driver")]
mod bollard_telemetry;
#[cfg(feature = "bollard-driver")]
mod docker_map;
#[cfg(all(feature = "linux-filesystem", target_os = "linux"))]
mod linux_filesystem;
#[cfg(feature = "process-driver")]
mod process_compose;
#[cfg(feature = "process-driver")]
mod process_compose_mutation;
#[cfg(feature = "process-driver")]
mod process_compose_pull;
#[cfg(feature = "process-driver")]
mod process_filesystem;
#[cfg(feature = "process-driver")]
mod process_host_system;
#[cfg(feature = "process-driver")]
mod process_logs;
#[cfg(feature = "process-driver")]
mod process_process;
#[cfg(feature = "process-driver")]
mod process_zfs;

#[cfg(feature = "bollard-driver")]
pub use bollard_driver::BollardReadClient;
#[cfg(feature = "remote-bollard")]
pub use bollard_provider::BollardClientProvider;
pub use compose::{
    ComposeConfig, ComposeInspector, ComposeLogRequest, ComposeLogs, ComposeProject,
    ComposeProjectRef, ComposeServiceConfig, ComposeServiceStatus, ComposeStatus,
};
pub use compose_mutation::{
    ComposeMutationAction, ComposeMutationClient, ComposeMutationEngine, ComposeMutationOutcome,
    ComposeMutationReceipt, ComposeMutationRequest, ComposeMutator,
};
pub use compose_pull::{
    ComposePullClient, ComposePullMutator, ComposePullOutcome, ComposePullReceipt,
    ComposePullRequest, ComposePulledImage,
};
pub use compose_pull_engine::ComposePullEngine;
pub use container_mutation::{
    ContainerLifecycleAction, ContainerLifecycleMutator, ContainerLifecycleOutcome,
    ContainerLifecycleRequest, ContainerMutationReceipt, DockerMutationClient,
    DockerMutationClientProvider, MutationVerificationPolicy,
};
pub use container_mutation_engine::ContainerLifecycleEngine;
pub use docker::{
    ContainerInspect, ContainerListOptions, ContainerProcessTable, ContainerReader, ContainerState,
    ContainerSummary, DockerSystemInfo, DockerSystemReader, ImageListOptions, ImageReader,
    ImageSummary, NetworkReader, NetworkSummary, VolumeReader, VolumeSummary,
};
pub use docker_provider::{DockerClientProvider, DockerReadClient};
pub use docker_telemetry::{
    ContainerLogOptions, ContainerLogs, ContainerStatsSnapshot, DockerDiskUsage, DockerLogStream,
    DockerTelemetryReader, DockerUsageCategory,
};
pub use error::{InfraError, InfraResult};
pub use filesystem::{
    FileHash, FileKind, FileMetadata, FilePreview, FileReadPolicy, FilesystemInspector,
};
pub use filesystem_query::{
    FileFindRequest, FileSearch, FileTail, FileTailRequest, FilesystemQueryInspector, PathRead,
    PathReadRequest,
};
pub use host::{
    HostIdentity, HostInspectRequest, HostInspection, HostInspector, HostLoadAverage, HostMemory,
    LinuxCommandHostInspector,
};
pub use host_system::{
    DoctorCheck, DoctorReport, FilesystemUsage, HostSystemInspector, MountInfo, NetworkAddress,
    NetworkInterface, PortInfo, PortListRequest, PortProtocol, ServiceListRequest, ServiceStatus,
};
pub use image_pull::{
    DockerArtifactClient, DockerArtifactClientProvider, ImageIdentity, ImagePullMutator,
    ImagePullOutcome, ImagePullProgressFrame, ImagePullReceipt, ImagePullRequest,
    canonical_image_reference,
};
pub use image_pull_engine::ImagePullEngine;
#[cfg(all(feature = "linux-filesystem", target_os = "linux"))]
pub use linux_filesystem::LinuxFilesystemInspector;
pub use logs::{
    JournalFilters, JournalPriority, LogPermissionDiagnostic, LogRead, LogReadRequest, LogReader,
    LogSource,
};
pub use mutation::{MutationFailure, MutationResult, MutationVerification};
pub use process::{ProcessInspector, ProcessListRequest, ProcessRow, ProcessSnapshot, ProcessSort};
#[cfg(feature = "process-driver")]
pub use process_compose::CommandComposeInspector;
#[cfg(feature = "process-driver")]
pub use process_filesystem::CommandFilesystemQueryInspector;
#[cfg(feature = "process-driver")]
pub use process_host_system::CommandHostSystemInspector;
#[cfg(feature = "process-driver")]
pub use process_logs::CommandLogReader;
#[cfg(feature = "process-driver")]
pub use process_process::CommandProcessInspector;
#[cfg(feature = "process-driver")]
pub use process_zfs::CommandZfsInspector;
pub use progress_sink::MutationProgressReporter;
pub use zfs::{
    ZfsDatasetRequest, ZfsDatasetType, ZfsInspector, ZfsPoolRequest, ZfsSnapshotRequest, ZfsTable,
};
