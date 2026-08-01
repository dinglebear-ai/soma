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
mod error;
mod filesystem;
mod host;

#[cfg(feature = "bollard-driver")]
mod bollard_driver;
#[cfg(feature = "bollard-driver")]
mod docker_map;
#[cfg(all(feature = "linux-filesystem", target_os = "linux"))]
mod linux_filesystem;
#[cfg(feature = "process-driver")]
mod process_compose;

#[cfg(feature = "bollard-driver")]
pub use bollard_driver::BollardReadClient;
pub use compose::{
    ComposeConfig, ComposeInspector, ComposeProject, ComposeProjectRef, ComposeServiceConfig,
    ComposeServiceStatus, ComposeStatus,
};
pub use docker::{
    ContainerInspect, ContainerListOptions, ContainerReader, ContainerState, ContainerSummary,
    DockerReadClient, DockerSystemInfo, DockerSystemReader, ImageListOptions, ImageReader,
    ImageSummary, NetworkReader, NetworkSummary, VolumeReader, VolumeSummary,
};
pub use error::{InfraError, InfraResult};
pub use filesystem::{
    FileHash, FileKind, FileMetadata, FilePreview, FileReadPolicy, FilesystemInspector,
};
pub use host::{
    CommandHostInspector, HostIdentity, HostInspectRequest, HostInspection, HostInspector,
    HostLoadAverage, HostMemory,
};
#[cfg(all(feature = "linux-filesystem", target_os = "linux"))]
pub use linux_filesystem::LinuxFilesystemInspector;
#[cfg(feature = "process-driver")]
pub use process_compose::CommandComposeInspector;
