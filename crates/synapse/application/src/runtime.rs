use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use soma_fleet::{HostEndpoint, HostId, HostRecord, HostRepository, TopologySnapshot};
use soma_infra::{
    ComposeInspector, ComposeProjectRef, DockerClientProvider, FilesystemQueryInspector,
    HostInspector, HostSystemInspector, LogReader, ProcessInspector, ZfsInspector,
};
use soma_ops::{AccessClass, OperationName, Timestamp};
use tokio_util::sync::CancellationToken;

use crate::runtime_params::optional_str;
use crate::{ExecutionError, SynapseCatalog};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Product-owned ports required to execute every canonical read operation.
pub struct SynapseReadPorts {
    /// Immutable fleet topology source.
    pub hosts: Arc<dyn HostRepository>,
    /// Core host identity and resource inspection.
    pub host: Arc<dyn HostInspector>,
    /// Host services, network, mounts, ports, usage, and doctor checks.
    pub host_system: Arc<dyn HostSystemInspector>,
    /// Revision-bound Docker client provider.
    pub docker: Arc<dyn DockerClientProvider>,
    /// Compose inspection engine.
    pub compose: Arc<dyn ComposeInspector>,
    /// Bounded filesystem read, tree, find, and tail engine.
    pub filesystem: Arc<dyn FilesystemQueryInspector>,
    /// Process inspection engine.
    pub processes: Arc<dyn ProcessInspector>,
    /// Operating-system log reader.
    pub logs: Arc<dyn LogReader>,
    /// ZFS inspection engine.
    pub zfs: Arc<dyn ZfsInspector>,
}

/// Canonical Synapse read-operation runtime.
pub struct SynapseReadRuntime {
    pub(crate) catalog: &'static SynapseCatalog,
    pub(crate) ports: SynapseReadPorts,
    default_host: Option<HostId>,
    timeout: Duration,
}

impl SynapseReadRuntime {
    /// Creates a runtime using the checked-in canonical catalog.
    #[must_use]
    pub fn new(ports: SynapseReadPorts) -> Self {
        Self {
            catalog: SynapseCatalog::embedded(),
            ports,
            default_host: None,
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// Sets the product default host used when a schema permits omission.
    #[must_use]
    pub fn with_default_host(mut self, host: HostId) -> Self {
        self.default_host = Some(host);
        self
    }

    /// Sets the per-operation command deadline budget.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        if !timeout.is_zero() {
            self.timeout = timeout;
        }
        self
    }

    /// Executes one schema-validated canonical read operation.
    pub async fn execute(
        &self,
        operation: &OperationName,
        parameters: &Value,
        cancellation: &CancellationToken,
    ) -> Result<Value, ExecutionError> {
        let spec = self
            .catalog
            .operation(operation)
            .ok_or_else(|| crate::CompatibilityError::UnknownOperation(operation.clone()))?;
        if spec.access() != AccessClass::Read {
            return Err(ExecutionError::UnsupportedOperation(operation.clone()));
        }
        self.catalog.validate_parameters(operation, parameters)?;
        let result = match operation.as_str().split('.').next().unwrap_or_default() {
            "product" => self.execute_product(operation, parameters)?,
            "docker" | "container" => {
                self.execute_docker(operation, parameters, cancellation)
                    .await?
            }
            "host" | "fleet" => {
                self.execute_host(operation, parameters, cancellation)
                    .await?
            }
            "compose" | "processes" | "zfs" | "logs" => {
                self.execute_observability(operation, parameters, cancellation)
                    .await?
            }
            "files" | "filesystem" => {
                self.execute_files(operation, parameters, cancellation)
                    .await?
            }
            _ => return Err(ExecutionError::UnsupportedOperation(operation.clone())),
        };
        self.catalog.validate_result(operation, &result)?;
        Ok(result)
    }

    fn execute_product(
        &self,
        operation: &OperationName,
        parameters: &Value,
    ) -> Result<Value, ExecutionError> {
        if operation.as_str() != "product.help" {
            return Err(ExecutionError::UnsupportedOperation(operation.clone()));
        }
        let topic = optional_str(parameters, "topic")?;
        let operations = self
            .catalog
            .operations()
            .filter(|spec| topic.is_none_or(|topic| spec.name().as_str().starts_with(topic)))
            .map(|spec| {
                json!({
                    "name": spec.name().as_str(),
                    "summary": format!("Canonical {} operation", spec.name())
                })
            })
            .collect::<Vec<_>>();
        let mut names = self
            .catalog
            .operations()
            .filter_map(|spec| spec.name().as_str().split('.').next())
            .collect::<std::collections::BTreeSet<_>>();
        if let Some(topic) = topic {
            names.retain(|name| name.starts_with(topic) || topic.starts_with(name));
        }
        let topics = names
            .into_iter()
            .map(|name| json!({"name": name, "summary": format!("{name} operations")}))
            .collect::<Vec<_>>();
        Ok(json!({"topics": topics, "operations": operations}))
    }

    pub(crate) fn deadline(&self) -> Timestamp {
        let millis = i64::try_from(self.timeout.as_millis()).unwrap_or(i64::MAX);
        Timestamp::from_unix_millis(Timestamp::now().unix_millis().saturating_add(millis))
    }

    pub(crate) async fn resolve_host(
        &self,
        parameters: &Value,
    ) -> Result<HostRecord, ExecutionError> {
        let snapshot = self.ports.hosts.snapshot().await?;
        if let Some(name) = optional_str(parameters, "host")? {
            return self.resolve_host_name_from_snapshot(&snapshot, name);
        }
        if let Some(default) = &self.default_host {
            return snapshot
                .get(default)
                .cloned()
                .ok_or_else(|| ExecutionError::HostNotFound(default.to_string()));
        }
        if snapshot.len() == 1 {
            return Ok(snapshot.hosts().next().expect("single host exists").clone());
        }
        let local = snapshot
            .hosts()
            .filter(|host| matches!(host.endpoint(), HostEndpoint::Local))
            .collect::<Vec<_>>();
        if local.len() == 1 {
            return Ok(local[0].clone());
        }
        Err(ExecutionError::HostRequired)
    }

    pub(crate) async fn resolve_host_name(&self, name: &str) -> Result<HostRecord, ExecutionError> {
        let snapshot = self.ports.hosts.snapshot().await?;
        self.resolve_host_name_from_snapshot(&snapshot, name)
    }

    fn resolve_host_name_from_snapshot(
        &self,
        snapshot: &TopologySnapshot,
        name: &str,
    ) -> Result<HostRecord, ExecutionError> {
        let id = HostId::new(name).map_err(|error| ExecutionError::InvalidParameter {
            field: "host".into(),
            message: error.to_string(),
        })?;
        snapshot
            .get(&id)
            .cloned()
            .ok_or_else(|| ExecutionError::HostNotFound(name.to_owned()))
    }

    pub(crate) async fn resolve_project(
        &self,
        host: &HostRecord,
        name: &str,
        cancellation: &CancellationToken,
    ) -> Result<ComposeProjectRef, ExecutionError> {
        let projects = self
            .ports
            .compose
            .list_projects(host, self.deadline(), cancellation)
            .await?;
        let project = projects
            .into_iter()
            .find(|project| project.name == name)
            .ok_or_else(|| ExecutionError::ProjectNotFound {
                host: host.id().to_string(),
                project: name.to_owned(),
            })?;
        let config = project.config_files.into_iter().next().ok_or_else(|| {
            ExecutionError::ProjectNotFound {
                host: host.id().to_string(),
                project: name.to_owned(),
            }
        })?;
        Ok(ComposeProjectRef::new(name, config)?)
    }

    pub(crate) async fn topology_items(&self) -> Result<Value, ExecutionError> {
        let snapshot = self.ports.hosts.snapshot().await?;
        let items = snapshot
            .hosts()
            .map(|host| {
                json!({
                    "id": host.id(),
                    "revision": host.revision(),
                    "endpoint": host.endpoint(),
                    "labels": host.labels().collect::<Vec<_>>(),
                    "capabilities": host.capabilities().collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();
        crate::runtime_result::items(items, snapshot.len(), false)
    }
}
