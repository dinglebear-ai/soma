use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use openssh::{ForwardType, Session, Socket};
use tokio_util::sync::CancellationToken;

use crate::{
    FleetError, FleetResult, HostRecord, OpenSshConnection, request::validate_absolute_path,
    runtime::secure_runtime_subdir,
};

const SOCKET_WAIT: Duration = Duration::from_secs(2);
const SOCKET_POLL: Duration = Duration::from_millis(20);
static FORWARD_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// Returns a private deterministic-shape local path for a forwarded Unix socket.
pub fn forwarded_socket_path(host: &HostRecord) -> FleetResult<PathBuf> {
    let sequence = FORWARD_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let revision = host.revision().as_str();
    let prefix = revision.get(..16).unwrap_or(revision);
    Ok(secure_runtime_subdir("forward")?
        .join(format!("{prefix}-{}-{sequence}.sock", std::process::id())))
}

/// RAII guard over one local-to-remote Unix socket forward.
pub struct ForwardedUnixSocket {
    session: Arc<Session>,
    local_path: PathBuf,
    remote_path: PathBuf,
    closed: bool,
}

impl ForwardedUnixSocket {
    /// Opens and secures a local Unix socket forwarding to a remote absolute path.
    pub async fn open(
        connection: &OpenSshConnection,
        host: &HostRecord,
        remote_path: impl Into<PathBuf>,
        cancellation: &CancellationToken,
    ) -> FleetResult<Self> {
        if cancellation.is_cancelled() {
            return Err(FleetError::Cancelled);
        }
        if connection.revision() != host.revision() {
            return Err(FleetError::StaleTopology {
                host: host.id().clone(),
                expected: connection.revision().clone(),
                actual: host.revision().clone(),
            });
        }
        let remote_path = validate_absolute_path(remote_path.into())?;
        let local_path = forwarded_socket_path(host)?;
        if let Ok(metadata) = std::fs::symlink_metadata(&local_path) {
            if metadata.file_type().is_symlink() {
                return Err(FleetError::Connection {
                    host: host.id().clone(),
                    message: "forward socket path is a symbolic link".into(),
                });
            }
            std::fs::remove_file(&local_path).map_err(|error| FleetError::Connection {
                host: host.id().clone(),
                message: format!("remove stale forward socket failed: {error}"),
            })?;
        }
        let session = connection.session().await?;
        let request = session.request_port_forward(
            ForwardType::Local,
            Socket::UnixSocket {
                path: local_path.as_path().into(),
            },
            Socket::UnixSocket {
                path: remote_path.as_path().into(),
            },
        );
        tokio::select! {
            () = cancellation.cancelled() => return Err(FleetError::Cancelled),
            result = tokio::time::timeout(SOCKET_WAIT, request) => match result {
                Err(_) => return Err(FleetError::DeadlineExceeded),
                Ok(Err(error)) => return Err(FleetError::Connection {
                    host: host.id().clone(),
                    message: format!("open Unix socket forward failed: {error}"),
                }),
                Ok(Ok(())) => {}
            }
        }
        if let Err(error) = secure_socket(&local_path, host).await {
            let _ = session
                .close_port_forward(
                    ForwardType::Local,
                    Socket::UnixSocket {
                        path: local_path.as_path().into(),
                    },
                    Socket::UnixSocket {
                        path: remote_path.as_path().into(),
                    },
                )
                .await;
            let _ = std::fs::remove_file(&local_path);
            return Err(error);
        }
        Ok(Self {
            session,
            local_path,
            remote_path,
            closed: false,
        })
    }

    /// Returns the private local socket path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.local_path
    }

    /// Explicitly closes the forward and removes the local socket.
    pub async fn close(mut self) -> FleetResult<()> {
        self.closed = true;
        let result = self
            .session
            .close_port_forward(
                ForwardType::Local,
                Socket::UnixSocket {
                    path: self.local_path.as_path().into(),
                },
                Socket::UnixSocket {
                    path: self.remote_path.as_path().into(),
                },
            )
            .await;
        let _ = std::fs::remove_file(&self.local_path);
        result.map_err(|error| FleetError::Connection {
            host: crate::HostId::new("forward").expect("static host id"),
            message: format!("close Unix socket forward failed: {error}"),
        })
    }
}

impl Drop for ForwardedUnixSocket {
    fn drop(&mut self) {
        if self.closed {
            return;
        }
        let _ = std::fs::remove_file(&self.local_path);
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let session = Arc::clone(&self.session);
            let local_path = self.local_path.clone();
            let remote_path = self.remote_path.clone();
            handle.spawn(async move {
                let _ = session
                    .close_port_forward(
                        ForwardType::Local,
                        Socket::UnixSocket {
                            path: local_path.as_path().into(),
                        },
                        Socket::UnixSocket {
                            path: remote_path.as_path().into(),
                        },
                    )
                    .await;
            });
        }
    }
}

async fn secure_socket(path: &Path, host: &HostRecord) -> FleetResult<()> {
    let deadline = tokio::time::Instant::now() + SOCKET_WAIT;
    loop {
        match std::fs::symlink_metadata(path) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.file_type().is_socket() {
                    return Err(FleetError::Connection {
                        host: host.id().clone(),
                        message: "forward path is not a real Unix socket".into(),
                    });
                }
                let uid = rustix::process::getuid().as_raw();
                if metadata.uid() != uid {
                    return Err(FleetError::Connection {
                        host: host.id().clone(),
                        message: "forward socket is not owned by the current user".into(),
                    });
                }
                tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                    .await
                    .map_err(|error| FleetError::Connection {
                        host: host.id().clone(),
                        message: format!("chmod 0600 forward socket failed: {error}"),
                    })?;
                return Ok(());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if tokio::time::Instant::now() >= deadline {
                    return Err(FleetError::Connection {
                        host: host.id().clone(),
                        message: "forward socket did not appear before timeout".into(),
                    });
                }
                tokio::time::sleep(SOCKET_POLL).await;
            }
            Err(error) => {
                return Err(FleetError::Connection {
                    host: host.id().clone(),
                    message: format!("inspect forward socket failed: {error}"),
                });
            }
        }
    }
}

#[cfg(test)]
#[path = "forward_tests.rs"]
mod tests;
