use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use soma_fleet::{ConnectionPool, HostEndpoint, HostId, HostRecord, OpenSshConnector};
use tokio_util::sync::CancellationToken;

use crate::{
    BollardReadClient, ContainerExecClientProvider, ContainerExecMutator, ContainerRecreateClient,
    ContainerRecreateClientProvider, DockerArtifactClient, DockerArtifactClientProvider,
    DockerClientProvider, DockerMutationClient, DockerMutationClientProvider, DockerReadClient,
    InfraError, InfraResult,
};

const DEFAULT_REMOTE_SOCKET: &str = "/var/run/docker.sock";

/// Revision-aware Bollard provider for local and strict-SSH hosts.
pub struct BollardClientProvider {
    pool: Arc<ConnectionPool<OpenSshConnector>>,
    remote_sockets: BTreeMap<HostId, PathBuf>,
}

impl BollardClientProvider {
    /// Creates a provider backed by the shared strict-OpenSSH pool.
    #[must_use]
    pub fn new(pool: Arc<ConnectionPool<OpenSshConnector>>) -> Self {
        Self {
            pool,
            remote_sockets: BTreeMap::new(),
        }
    }

    /// Configures an explicit remote Docker socket for one SSH host.
    pub fn with_remote_socket(
        mut self,
        host: HostId,
        path: impl Into<PathBuf>,
    ) -> InfraResult<Self> {
        self.remote_sockets
            .insert(host, validate_socket(path.into())?);
        Ok(self)
    }

    fn plan<'a>(&'a self, host: &'a HostRecord) -> InfraResult<SocketPlan<'a>> {
        match host.endpoint() {
            HostEndpoint::Local => Ok(SocketPlan::Local),
            HostEndpoint::Ssh(_) => Ok(SocketPlan::Remote(
                self.remote_sockets
                    .get(host.id())
                    .map(PathBuf::as_path)
                    .unwrap_or_else(|| Path::new(DEFAULT_REMOTE_SOCKET)),
            )),
            HostEndpoint::Http(_) => Err(InfraError::UnsupportedTarget {
                domain: "docker",
                host: host.id().clone(),
            }),
        }
    }
}

#[async_trait]
impl DockerClientProvider for BollardClientProvider {
    async fn client(
        &self,
        host: &HostRecord,
        cancellation: &CancellationToken,
    ) -> InfraResult<Arc<dyn DockerReadClient>> {
        match self.plan(host)? {
            SocketPlan::Local => Ok(Arc::new(BollardReadClient::connect_local(host)?)),
            SocketPlan::Remote(socket) => {
                let connection = self.pool.get_or_connect(host, cancellation).await?;
                Ok(Arc::new(
                    BollardReadClient::connect_remote(connection, host, socket, cancellation)
                        .await?,
                ))
            }
        }
    }
}

#[async_trait]
impl DockerMutationClientProvider for BollardClientProvider {
    async fn mutation_client(
        &self,
        host: &HostRecord,
        cancellation: &CancellationToken,
    ) -> InfraResult<Arc<dyn DockerMutationClient>> {
        match self.plan(host)? {
            SocketPlan::Local => Ok(Arc::new(BollardReadClient::connect_local(host)?)),
            SocketPlan::Remote(socket) => {
                let connection = self.pool.get_or_connect(host, cancellation).await?;
                Ok(Arc::new(
                    BollardReadClient::connect_remote(connection, host, socket, cancellation)
                        .await?,
                ))
            }
        }
    }
}

#[async_trait]
impl ContainerExecClientProvider for BollardClientProvider {
    async fn exec_client(
        &self,
        host: &HostRecord,
        cancellation: &CancellationToken,
    ) -> InfraResult<Arc<dyn ContainerExecMutator>> {
        match self.plan(host)? {
            SocketPlan::Local => Ok(Arc::new(BollardReadClient::connect_local(host)?)),
            SocketPlan::Remote(socket) => {
                let connection = self.pool.get_or_connect(host, cancellation).await?;
                Ok(Arc::new(
                    BollardReadClient::connect_remote(connection, host, socket, cancellation)
                        .await?,
                ))
            }
        }
    }
}

#[async_trait]
impl ContainerRecreateClientProvider for BollardClientProvider {
    async fn recreate_client(
        &self,
        host: &HostRecord,
        cancellation: &CancellationToken,
    ) -> InfraResult<Arc<dyn ContainerRecreateClient>> {
        match self.plan(host)? {
            SocketPlan::Local => Ok(Arc::new(BollardReadClient::connect_local(host)?)),
            SocketPlan::Remote(socket) => {
                let connection = self.pool.get_or_connect(host, cancellation).await?;
                Ok(Arc::new(
                    BollardReadClient::connect_remote(connection, host, socket, cancellation)
                        .await?,
                ))
            }
        }
    }
}

#[async_trait]
impl DockerArtifactClientProvider for BollardClientProvider {
    async fn artifact_client(
        &self,
        host: &HostRecord,
        cancellation: &CancellationToken,
    ) -> InfraResult<Arc<dyn DockerArtifactClient>> {
        match self.plan(host)? {
            SocketPlan::Local => Ok(Arc::new(BollardReadClient::connect_local(host)?)),
            SocketPlan::Remote(socket) => {
                let connection = self.pool.get_or_connect(host, cancellation).await?;
                Ok(Arc::new(
                    BollardReadClient::connect_remote(connection, host, socket, cancellation)
                        .await?,
                ))
            }
        }
    }
}

enum SocketPlan<'a> {
    Local,
    Remote(&'a Path),
}

fn validate_socket(path: PathBuf) -> InfraResult<PathBuf> {
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        Err(InfraError::InvalidRequest {
            domain: "docker",
            message: format!(
                "Docker socket path must be absolute and normalized: {}",
                path.display()
            ),
        })
    } else {
        Ok(path)
    }
}

#[cfg(test)]
#[path = "bollard_provider_tests.rs"]
mod tests;
