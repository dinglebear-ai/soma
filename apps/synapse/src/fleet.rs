use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use soma_fleet::{
    CommandExecutor, CommandOutput, CommandRequest, FleetError, FleetResult, HostEndpoint, HostId,
    HostRecord, HostRepository, LocalProcessDriver, OpenSshDriver, SshEndpoint, TopologySnapshot,
};
use soma_infra::{
    BuildContextFingerprint, BuildContextInspector, FileFindRequest, FileSearch, FileTail,
    FileTailRequest, FilesystemQueryInspector, InfraError, InfraResult, PathRead, PathReadRequest,
};
use soma_ops::Timestamp;
use tokio_util::sync::CancellationToken;

use crate::config::{EndpointConfig, SynapseConfig};

#[derive(Clone)]
pub struct StaticHostRepository {
    snapshot: TopologySnapshot,
}

impl StaticHostRepository {
    pub fn new(snapshot: TopologySnapshot) -> Self {
        Self { snapshot }
    }
}

#[async_trait]
impl HostRepository for StaticHostRepository {
    async fn snapshot(&self) -> FleetResult<TopologySnapshot> {
        Ok(self.snapshot.clone())
    }
}

pub struct RoutedCommandExecutor {
    local: LocalProcessDriver,
    ssh: Arc<OpenSshDriver>,
}

impl RoutedCommandExecutor {
    pub fn new(ssh: Arc<OpenSshDriver>) -> Self {
        Self {
            local: LocalProcessDriver,
            ssh,
        }
    }
}

#[async_trait]
impl CommandExecutor for RoutedCommandExecutor {
    async fn execute(
        &self,
        host: &HostRecord,
        request: &CommandRequest,
        cancellation: &CancellationToken,
    ) -> FleetResult<CommandOutput> {
        match host.endpoint() {
            HostEndpoint::Local => self.local.execute(host, request, cancellation).await,
            HostEndpoint::Ssh(_) => self.ssh.execute(host, request, cancellation).await,
            HostEndpoint::Http(_) => Err(FleetError::Command {
                host: host.id().clone(),
                message: "HTTP fleet endpoints cannot execute process commands".into(),
            }),
        }
    }
}

pub struct PerHostFilesystem {
    drivers: BTreeMap<HostId, Arc<dyn FilesystemQueryInspector>>,
}

impl PerHostFilesystem {
    pub fn new(drivers: BTreeMap<HostId, Arc<dyn FilesystemQueryInspector>>) -> Self {
        Self { drivers }
    }

    fn driver(&self, host: &HostRecord) -> InfraResult<&dyn FilesystemQueryInspector> {
        self.drivers
            .get(host.id())
            .map(Arc::as_ref)
            .ok_or_else(|| policy_error("filesystem", host))
    }
}

#[async_trait]
impl FilesystemQueryInspector for PerHostFilesystem {
    async fn read_path(
        &self,
        host: &HostRecord,
        path: &Path,
        request: &PathReadRequest,
        cancellation: &CancellationToken,
    ) -> InfraResult<PathRead> {
        self.driver(host)?
            .read_path(host, path, request, cancellation)
            .await
    }

    async fn find(
        &self,
        host: &HostRecord,
        path: &Path,
        request: &FileFindRequest,
        cancellation: &CancellationToken,
    ) -> InfraResult<FileSearch> {
        self.driver(host)?
            .find(host, path, request, cancellation)
            .await
    }

    async fn tail(
        &self,
        host: &HostRecord,
        path: &Path,
        request: &FileTailRequest,
        cancellation: &CancellationToken,
    ) -> InfraResult<FileTail> {
        self.driver(host)?
            .tail(host, path, request, cancellation)
            .await
    }
}

pub struct PerHostBuildContext {
    drivers: BTreeMap<HostId, Arc<dyn BuildContextInspector>>,
}

impl PerHostBuildContext {
    pub fn new(drivers: BTreeMap<HostId, Arc<dyn BuildContextInspector>>) -> Self {
        Self { drivers }
    }
}

#[async_trait]
impl BuildContextInspector for PerHostBuildContext {
    async fn fingerprint(
        &self,
        host: &HostRecord,
        path: &Path,
        deadline: Timestamp,
        cancellation: &CancellationToken,
    ) -> InfraResult<BuildContextFingerprint> {
        self.drivers
            .get(host.id())
            .ok_or_else(|| policy_error("build-context", host))?
            .fingerprint(host, path, deadline, cancellation)
            .await
    }
}

pub fn topology(config: &SynapseConfig) -> anyhow::Result<TopologySnapshot> {
    let hosts = config
        .hosts
        .iter()
        .map(host_record)
        .collect::<anyhow::Result<Vec<_>>>()?;
    TopologySnapshot::new(hosts).map_err(Into::into)
}

fn host_record(config: &crate::config::HostConfig) -> anyhow::Result<HostRecord> {
    let id = HostId::new(&config.id)?;
    let endpoint = match &config.endpoint {
        EndpointConfig::Local => HostEndpoint::Local,
        EndpointConfig::Ssh {
            host,
            port,
            user,
            identity_file,
            config_file,
            known_hosts_file,
        } => {
            let mut endpoint = SshEndpoint::new(host)?.with_port(*port)?;
            if let Some(user) = user {
                endpoint = endpoint.with_user(user)?;
            }
            if let Some(path) = identity_file {
                endpoint = endpoint.with_identity_file(path)?;
            }
            if let Some(path) = config_file {
                endpoint = endpoint.with_config_file(path)?;
            }
            if let Some(path) = known_hosts_file {
                endpoint = endpoint.with_known_hosts_file(path)?;
            }
            HostEndpoint::Ssh(endpoint)
        }
    };
    let mut record = HostRecord::new(id, endpoint);
    for label in &config.labels {
        record = record.with_label(label)?;
    }
    Ok(record)
}

fn policy_error(domain: &'static str, host: &HostRecord) -> InfraError {
    InfraError::InvalidRequest {
        domain,
        message: format!("{domain} policy is not configured for {}", host.id()),
    }
}

#[cfg(test)]
#[path = "fleet_tests.rs"]
mod tests;
