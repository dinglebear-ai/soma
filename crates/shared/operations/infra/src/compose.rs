use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use soma_fleet::{HostId, HostRecord, TopologyRevision};
use soma_ops::Timestamp;
use tokio_util::sync::CancellationToken;

use crate::InfraResult;

/// Validated reference to one Compose project configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposeProjectRef {
    name: String,
    config_file: PathBuf,
}

impl ComposeProjectRef {
    /// Creates a project reference with an absolute normalized config path.
    pub fn new(name: impl Into<String>, config_file: impl Into<PathBuf>) -> InfraResult<Self> {
        let name = name.into();
        crate::compose_parse::validate_project_name(&name)?;
        let config_file = crate::compose_parse::validate_absolute_path(config_file.into())?;
        Ok(Self { name, config_file })
    }

    /// Returns the project name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the Compose config path.
    #[must_use]
    pub fn config_file(&self) -> &Path {
        &self.config_file
    }
}

/// Project row returned by `docker compose ls`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposeProject {
    /// Project name.
    pub name: String,
    /// Engine-reported status text.
    pub status: Option<String>,
    /// Referenced config files.
    pub config_files: Vec<PathBuf>,
}

/// Service row returned by `docker compose ps`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposeServiceStatus {
    /// Compose service name.
    pub service: String,
    /// Container name, when reported.
    pub container_name: Option<String>,
    /// Runtime state.
    pub state: Option<String>,
    /// Health state.
    pub health: Option<String>,
    /// Container exit code.
    pub exit_code: Option<i64>,
    /// Image reference.
    pub image: Option<String>,
}

/// Typed status for one Compose project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposeStatus {
    /// Target host.
    pub host: HostId,
    /// Exact topology revision.
    pub topology_revision: TopologyRevision,
    /// Project name.
    pub project: String,
    /// Service status rows.
    pub services: Vec<ComposeServiceStatus>,
}

/// Selected read-only service configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposeServiceConfig {
    /// Image reference, when configured.
    pub image: Option<String>,
    /// Build context, when represented as a string or object context.
    pub build_context: Option<String>,
    /// Enabled profiles.
    pub profiles: Vec<String>,
}

/// Typed read-only Compose configuration summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposeConfig {
    /// Target host.
    pub host: HostId,
    /// Exact topology revision.
    pub topology_revision: TopologyRevision,
    /// Project name.
    pub project: String,
    /// Service configurations keyed by service name.
    pub services: BTreeMap<String, ComposeServiceConfig>,
    /// Declared network names.
    pub networks: Vec<String>,
    /// Declared volume names.
    pub volumes: Vec<String>,
}

/// Product-neutral Compose inspection engine.
#[async_trait]
pub trait ComposeInspector: Send + Sync {
    /// Lists Compose projects visible on one host.
    async fn list_projects(
        &self,
        host: &HostRecord,
        deadline: Timestamp,
        cancellation: &CancellationToken,
    ) -> InfraResult<Vec<ComposeProject>>;

    /// Returns status for one project, optionally restricted to a service.
    async fn status(
        &self,
        host: &HostRecord,
        project: &ComposeProjectRef,
        service: Option<&str>,
        deadline: Timestamp,
        cancellation: &CancellationToken,
    ) -> InfraResult<ComposeStatus>;

    /// Returns selected normalized project configuration.
    async fn config(
        &self,
        host: &HostRecord,
        project: &ComposeProjectRef,
        deadline: Timestamp,
        cancellation: &CancellationToken,
    ) -> InfraResult<ComposeConfig>;
}
