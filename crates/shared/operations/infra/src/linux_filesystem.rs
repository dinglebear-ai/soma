use std::fs::{File, Metadata};
use std::io::{Read, Seek, SeekFrom};
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use async_trait::async_trait;
use rustix::fs::{Mode, OFlags, ResolveFlags, open, openat2};
use sha2::{Digest, Sha256};
use soma_fleet::{HostEndpoint, HostRecord};
use tokio_util::sync::CancellationToken;

use crate::{
    FileHash, FileKind, FileMetadata, FilePreview, FileReadPolicy, FilesystemInspector, InfraError,
    InfraResult,
};

/// Linux descriptor-confined filesystem reader.
#[derive(Debug, Clone)]
pub struct LinuxFilesystemInspector {
    policy: FileReadPolicy,
}

impl LinuxFilesystemInspector {
    /// Creates an inspector from an explicit read policy.
    #[must_use]
    pub fn new(policy: FileReadPolicy) -> Self {
        Self { policy }
    }

    /// Returns the active read policy.
    #[must_use]
    pub fn policy(&self) -> &FileReadPolicy {
        &self.policy
    }
}

#[async_trait]
impl FilesystemInspector for LinuxFilesystemInspector {
    async fn stat(
        &self,
        host: &HostRecord,
        path: &Path,
        cancellation: &CancellationToken,
    ) -> InfraResult<FileMetadata> {
        ensure_local(host)?;
        ensure_not_cancelled(cancellation)?;
        let policy = self.policy.clone();
        let path = path.to_path_buf();
        let host = host.clone();
        run_blocking(cancellation, move || {
            let bound = bind_read_path(&policy, &path)?;
            metadata_for(
                &host,
                &path,
                &bound
                    .file
                    .metadata()
                    .map_err(|error| fs_error("stat", &path, error))?,
            )
        })
        .await
    }

    async fn read(
        &self,
        host: &HostRecord,
        path: &Path,
        cancellation: &CancellationToken,
    ) -> InfraResult<FilePreview> {
        ensure_local(host)?;
        ensure_not_cancelled(cancellation)?;
        let policy = self.policy.clone();
        let path = path.to_path_buf();
        let host = host.clone();
        run_blocking(cancellation, move || {
            let mut bound = bind_read_path(&policy, &path)?;
            let metadata = bound
                .file
                .metadata()
                .map_err(|error| fs_error("read", &path, error))?;
            let typed = metadata_for(&host, &path, &metadata)?;
            if typed.kind != FileKind::File {
                return Err(InfraError::Filesystem {
                    operation: "read",
                    path,
                    message: "path is not a regular file".into(),
                });
            }
            let limit = policy.max_preview_bytes();
            let mut content = Vec::with_capacity(limit.min(8192));
            bound
                .file
                .by_ref()
                .take((limit as u64).saturating_add(1))
                .read_to_end(&mut content)
                .map_err(|error| fs_error("read", &typed.path, error))?;
            let truncated = content.len() > limit;
            content.truncate(limit);
            Ok(FilePreview {
                metadata: typed,
                content,
                truncated,
            })
        })
        .await
    }

    async fn hash(
        &self,
        host: &HostRecord,
        path: &Path,
        cancellation: &CancellationToken,
    ) -> InfraResult<FileHash> {
        ensure_local(host)?;
        ensure_not_cancelled(cancellation)?;
        let policy = self.policy.clone();
        let path = path.to_path_buf();
        let host = host.clone();
        run_blocking(cancellation, move || {
            let mut bound = bind_read_path(&policy, &path)?;
            let metadata = bound
                .file
                .metadata()
                .map_err(|error| fs_error("hash", &path, error))?;
            let typed = metadata_for(&host, &path, &metadata)?;
            if typed.kind != FileKind::File {
                return Err(InfraError::Filesystem {
                    operation: "hash",
                    path,
                    message: "path is not a regular file".into(),
                });
            }
            if typed.size_bytes > policy.max_hash_bytes() {
                return Err(InfraError::InvalidRequest {
                    domain: "filesystem",
                    message: format!(
                        "file is {} bytes; hash limit is {}",
                        typed.size_bytes,
                        policy.max_hash_bytes()
                    ),
                });
            }
            bound
                .file
                .seek(SeekFrom::Start(0))
                .map_err(|error| fs_error("hash", &typed.path, error))?;
            let mut hasher = Sha256::new();
            let mut buffer = [0_u8; 64 * 1024];
            let mut bytes_hashed = 0_u64;
            loop {
                let read = bound
                    .file
                    .read(&mut buffer)
                    .map_err(|error| fs_error("hash", &typed.path, error))?;
                if read == 0 {
                    break;
                }
                bytes_hashed = bytes_hashed.saturating_add(read as u64);
                hasher.update(&buffer[..read]);
            }
            Ok(FileHash {
                metadata: typed,
                sha256: format!("{:x}", hasher.finalize()),
                bytes_hashed,
            })
        })
        .await
    }
}

struct BoundFile {
    file: File,
}

fn bind_read_path(policy: &FileReadPolicy, path: &Path) -> InfraResult<BoundFile> {
    let (root, relative) = policy.resolve(path)?;
    let slash: OwnedFd = open(
        "/",
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|error| fs_error("open-root", path, error))?;
    let root_relative = root.strip_prefix("/").unwrap_or(root.as_path());
    let root_fd = openat2(
        &slash,
        if root_relative.as_os_str().is_empty() {
            Path::new(".")
        } else {
            root_relative
        },
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
        ResolveFlags::BENEATH | ResolveFlags::NO_SYMLINKS | ResolveFlags::NO_MAGICLINKS,
    )
    .map_err(|error| fs_error("open-root", path, error))?;
    let target = if relative.as_os_str().is_empty() {
        Path::new(".")
    } else {
        relative.as_path()
    };
    let fd = openat2(
        &root_fd,
        target,
        OFlags::RDONLY | OFlags::CLOEXEC,
        Mode::empty(),
        ResolveFlags::BENEATH | ResolveFlags::NO_SYMLINKS | ResolveFlags::NO_MAGICLINKS,
    )
    .map_err(|error| fs_error("open", path, error))?;
    Ok(BoundFile { file: fd.into() })
}

fn metadata_for(host: &HostRecord, path: &Path, metadata: &Metadata) -> InfraResult<FileMetadata> {
    let kind = if metadata.is_file() {
        FileKind::File
    } else if metadata.is_dir() {
        FileKind::Directory
    } else {
        return Err(InfraError::Filesystem {
            operation: "stat",
            path: path.to_path_buf(),
            message: "path is neither a regular file nor a directory".into(),
        });
    };
    let modified_unix_millis = metadata.modified().ok().and_then(system_time_millis);
    Ok(FileMetadata {
        host: host.id().clone(),
        topology_revision: host.revision().clone(),
        path: path.to_path_buf(),
        kind,
        size_bytes: if kind == FileKind::File {
            metadata.len()
        } else {
            0
        },
        readonly: metadata.permissions().readonly(),
        modified_unix_millis,
    })
}

fn system_time_millis(value: SystemTime) -> Option<i64> {
    let duration = value.duration_since(SystemTime::UNIX_EPOCH).ok()?;
    i64::try_from(duration.as_millis()).ok()
}

fn ensure_local(host: &HostRecord) -> InfraResult<()> {
    if matches!(host.endpoint(), HostEndpoint::Local) {
        Ok(())
    } else {
        Err(InfraError::UnsupportedTarget {
            domain: "filesystem",
            host: host.id().clone(),
        })
    }
}

fn ensure_not_cancelled(cancellation: &CancellationToken) -> InfraResult<()> {
    if cancellation.is_cancelled() {
        Err(soma_fleet::FleetError::Cancelled.into())
    } else {
        Ok(())
    }
}

async fn run_blocking<T, F>(cancellation: &CancellationToken, operation: F) -> InfraResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> InfraResult<T> + Send + 'static,
{
    let task = tokio::task::spawn_blocking(operation);
    tokio::select! {
        () = cancellation.cancelled() => Err(soma_fleet::FleetError::Cancelled.into()),
        result = task => result.map_err(|error| InfraError::Filesystem {
            operation: "join",
            path: PathBuf::new(),
            message: error.to_string(),
        })?,
    }
}

fn fs_error(operation: &'static str, path: &Path, error: impl std::fmt::Display) -> InfraError {
    InfraError::Filesystem {
        operation,
        path: path.to_path_buf(),
        message: error.to_string(),
    }
}

#[cfg(test)]
#[path = "linux_filesystem_tests.rs"]
mod tests;
