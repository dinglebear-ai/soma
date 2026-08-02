use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use openssh::{KnownHosts, Session, SessionBuilder};
use tokio::sync::{RwLock, Semaphore};
use tokio_util::sync::CancellationToken;

use crate::{
    ConnectionFactory, FleetError, FleetResult, HostEndpoint, HostId, HostRecord, TopologyRevision,
    runtime::secure_runtime_subdir,
};

const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_SERVER_ALIVE_INTERVAL: Duration = Duration::from_secs(15);
const DEFAULT_EXEC_PERMITS: usize = 4;

/// Inspectable strict OpenSSH connection plan derived from one host record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenSshConnectPlan {
    host: HostId,
    revision: TopologyRevision,
    destination: String,
    port: u16,
    user: Option<String>,
    identity_file: Option<PathBuf>,
    config_file: Option<PathBuf>,
    known_hosts_file: Option<PathBuf>,
    connect_timeout: Duration,
    server_alive_interval: Duration,
    strict_known_hosts: bool,
}

impl OpenSshConnectPlan {
    /// Returns the target host identity.
    #[must_use]
    pub fn host(&self) -> &HostId {
        &self.host
    }
    /// Returns the topology revision bound to this plan.
    #[must_use]
    pub fn revision(&self) -> &TopologyRevision {
        &self.revision
    }
    /// Returns the SSH hostname or configuration alias.
    #[must_use]
    pub fn destination(&self) -> &str {
        &self.destination
    }
    /// Returns the SSH port.
    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }
    /// Returns the optional SSH user.
    #[must_use]
    pub fn user(&self) -> Option<&str> {
        self.user.as_deref()
    }
    /// Returns the optional identity-file path.
    #[must_use]
    pub fn identity_file(&self) -> Option<&Path> {
        self.identity_file.as_deref()
    }
    /// Returns the optional SSH config path.
    #[must_use]
    pub fn config_file(&self) -> Option<&Path> {
        self.config_file.as_deref()
    }
    /// Returns the optional explicit known-hosts path.
    #[must_use]
    pub fn known_hosts_file(&self) -> Option<&Path> {
        self.known_hosts_file.as_deref()
    }
    /// Returns the outer connection timeout.
    #[must_use]
    pub const fn connect_timeout(&self) -> Duration {
        self.connect_timeout
    }
    /// Returns the server-alive interval.
    #[must_use]
    pub const fn server_alive_interval(&self) -> Duration {
        self.server_alive_interval
    }
    /// Returns whether strict host-key verification is mandatory.
    #[must_use]
    pub const fn strict_known_hosts(&self) -> bool {
        self.strict_known_hosts
    }
}

/// Strict OpenSSH connection factory.
#[derive(Debug, Clone)]
pub struct OpenSshConnector {
    connect_timeout: Duration,
    server_alive_interval: Duration,
    exec_permits: usize,
}

impl Default for OpenSshConnector {
    fn default() -> Self {
        Self {
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            server_alive_interval: DEFAULT_SERVER_ALIVE_INTERVAL,
            exec_permits: DEFAULT_EXEC_PERMITS,
        }
    }
}

impl OpenSshConnector {
    /// Creates a connector with explicit nonzero bounds.
    pub fn new(
        connect_timeout: Duration,
        server_alive_interval: Duration,
        exec_permits: usize,
    ) -> FleetResult<Self> {
        if connect_timeout.is_zero() || server_alive_interval.is_zero() || exec_permits == 0 {
            return Err(FleetError::Connection {
                host: HostId::new("ssh").expect("static host id"),
                message: "OpenSSH timeouts and execution permits must be nonzero".into(),
            });
        }
        Ok(Self {
            connect_timeout,
            server_alive_interval,
            exec_permits,
        })
    }

    /// Derives the exact strict connection plan for one SSH host.
    pub fn plan(&self, host: &HostRecord) -> FleetResult<OpenSshConnectPlan> {
        let HostEndpoint::Ssh(endpoint) = host.endpoint() else {
            return Err(FleetError::Connection {
                host: host.id().clone(),
                message: "OpenSSH connector requires an SSH endpoint".into(),
            });
        };
        Ok(OpenSshConnectPlan {
            host: host.id().clone(),
            revision: host.revision().clone(),
            destination: endpoint.host().to_owned(),
            port: endpoint.port(),
            user: endpoint.user().map(str::to_owned),
            identity_file: endpoint.identity_file().map(Path::to_path_buf),
            config_file: endpoint.config_file().map(Path::to_path_buf),
            known_hosts_file: endpoint.known_hosts_file().map(Path::to_path_buf),
            connect_timeout: self.connect_timeout,
            server_alive_interval: self.server_alive_interval,
            strict_known_hosts: true,
        })
    }

    fn builder(&self, plan: &OpenSshConnectPlan) -> FleetResult<SessionBuilder> {
        let mut builder = SessionBuilder::default();
        builder
            .known_hosts_check(KnownHosts::Strict)
            .control_directory(secure_runtime_subdir("control")?)
            .connect_timeout(plan.connect_timeout)
            .server_alive_interval(plan.server_alive_interval)
            .port(plan.port);
        if let Some(user) = &plan.user {
            builder.user(user.clone());
        }
        if let Some(path) = &plan.identity_file {
            builder.keyfile(path);
        }
        if let Some(path) = &plan.config_file {
            builder.config_file(path);
        }
        if let Some(path) = &plan.known_hosts_file {
            builder.user_known_hosts_file(path);
        }
        Ok(builder)
    }
}

/// Revision-bound multiplexed OpenSSH session.
pub struct OpenSshConnection {
    host: HostId,
    revision: TopologyRevision,
    session: RwLock<Option<Arc<Session>>>,
    permits: Semaphore,
}

impl OpenSshConnection {
    /// Returns the host identity.
    #[must_use]
    pub fn host(&self) -> &HostId {
        &self.host
    }
    /// Returns the exact topology revision.
    #[must_use]
    pub fn revision(&self) -> &TopologyRevision {
        &self.revision
    }

    pub(crate) async fn acquire_permit(
        &self,
    ) -> Result<tokio::sync::SemaphorePermit<'_>, tokio::sync::AcquireError> {
        self.permits.acquire().await
    }

    pub(crate) async fn session(&self) -> FleetResult<Arc<Session>> {
        self.session
            .read()
            .await
            .as_ref()
            .map(Arc::clone)
            .ok_or_else(|| FleetError::Connection {
                host: self.host.clone(),
                message: "OpenSSH connection is closed".into(),
            })
    }

    async fn close(&self) -> FleetResult<()> {
        let session = self.session.write().await.take();
        if let Some(session) = session
            && let Ok(session) = Arc::try_unwrap(session)
        {
            session
                .close()
                .await
                .map_err(|error| FleetError::Connection {
                    host: self.host.clone(),
                    message: format!("close failed: {error}"),
                })?;
        }
        Ok(())
    }
}

#[async_trait]
impl ConnectionFactory for OpenSshConnector {
    type Connection = OpenSshConnection;

    async fn connect(
        &self,
        host: &HostRecord,
        cancellation: &CancellationToken,
    ) -> FleetResult<Self::Connection> {
        if cancellation.is_cancelled() {
            return Err(FleetError::Cancelled);
        }
        let plan = self.plan(host)?;
        let builder = self.builder(&plan)?;
        let connect = builder.connect_mux(&plan.destination);
        let session = tokio::select! {
            () = cancellation.cancelled() => return Err(FleetError::Cancelled),
            result = tokio::time::timeout(plan.connect_timeout, connect) => match result {
                Err(_) => return Err(FleetError::DeadlineExceeded),
                Ok(Err(error)) => return Err(FleetError::Connection {
                    host: host.id().clone(),
                    message: format!("strict OpenSSH connect failed: {error}"),
                }),
                Ok(Ok(session)) => session,
            }
        };
        Ok(OpenSshConnection {
            host: host.id().clone(),
            revision: host.revision().clone(),
            session: RwLock::new(Some(Arc::new(session))),
            permits: Semaphore::new(self.exec_permits),
        })
    }

    async fn close(&self, connection: &Self::Connection) -> FleetResult<()> {
        connection.close().await
    }
}

#[cfg(test)]
#[path = "openssh_connector_tests.rs"]
mod tests;
