use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use base64::Engine;
use soma_fleet::{
    CommandExecutor, CommandRequest, FileTransfer, FleetError, FleetResult, HostId, HostRecord,
    TransferLifecycle, TransferReceipt, TransferRequest,
};
use tokio_util::sync::CancellationToken;

use crate::file_transfer::identity_from_bytes;
use crate::{
    FileTransferInspector, FileTransferPathRole, FileTransferPolicy, InfraError, InfraResult,
    TransferFileIdentity,
};

const STDERR_LIMIT: usize = 64 * 1024;
const PY_BOOTSTRAP: &str =
    "import base64,sys;exec(compile(base64.b64decode(sys.argv[1]),'<soma-transfer>','exec'))";
const READ_SOURCE: &str = r#"import os, stat, sys
root, rel, cap, optional = sys.argv[2], sys.argv[3], int(sys.argv[4]), sys.argv[5] == '1'
parts = [part for part in root.split('/') if part] + [part for part in rel.split('/') if part]
fd = os.open('/', os.O_RDONLY | os.O_DIRECTORY)
try:
    for index, part in enumerate(parts):
        flags = os.O_RDONLY | os.O_NOFOLLOW
        if index < len(parts) - 1: flags |= os.O_DIRECTORY
        try:
            nxt = os.open(part, flags, dir_fd=fd)
        except FileNotFoundError:
            if optional: sys.exit(3)
            raise
        os.close(fd); fd = nxt
    meta = os.fstat(fd)
    if not stat.S_ISREG(meta.st_mode): raise RuntimeError('path is not a regular file')
    if meta.st_size > cap: raise RuntimeError('file exceeds transfer byte limit')
    while True:
        data = os.read(fd, 65536)
        if not data: break
        sys.stdout.buffer.write(data)
finally:
    os.close(fd)
"#;
const WRITE_SOURCE: &str = r#"import os, sys
root, rel, cap = sys.argv[2], sys.argv[3], int(sys.argv[4])
parts = [part for part in root.split('/') if part] + [part for part in rel.split('/') if part]
if not parts: raise RuntimeError('destination must name a file')
fd = os.open('/', os.O_RDONLY | os.O_DIRECTORY)
try:
    for part in parts[:-1]:
        nxt = os.open(part, os.O_RDONLY | os.O_NOFOLLOW | os.O_DIRECTORY, dir_fd=fd)
        os.close(fd); fd = nxt
    out = os.open(parts[-1], os.O_WRONLY | os.O_CREAT | os.O_TRUNC | os.O_NOFOLLOW, 0o600, dir_fd=fd)
    try:
        total = 0
        while True:
            data = sys.stdin.buffer.read(65536)
            if not data: break
            total += len(data)
            if total > cap: raise RuntimeError('destination exceeded transfer byte limit')
            view = memoryview(data)
            while view:
                written = os.write(out, view)
                view = view[written:]
        os.fsync(out)
    finally:
        os.close(out)
finally:
    os.close(fd)
"#;

/// Descriptor-confined local or strict-SSH file transfer driver.
pub struct CommandFileTransfer {
    executor: Arc<dyn CommandExecutor>,
    policies: BTreeMap<HostId, FileTransferPolicy>,
}

#[derive(Debug, Clone, Copy)]
struct BoundReadOptions {
    role: FileTransferPathRole,
    optional: bool,
    max_bytes: usize,
    deadline: soma_ops::Timestamp,
}

impl CommandFileTransfer {
    /// Creates a transfer driver with no admitted hosts.
    #[must_use]
    pub fn new(executor: Arc<dyn CommandExecutor>) -> Self {
        Self {
            executor,
            policies: BTreeMap::new(),
        }
    }

    /// Adds or replaces one host policy.
    #[must_use]
    pub fn with_policy(mut self, host: HostId, policy: FileTransferPolicy) -> Self {
        self.policies.insert(host, policy);
        self
    }

    fn policy(&self, host: &HostRecord) -> InfraResult<&FileTransferPolicy> {
        self.policies
            .get(host.id())
            .ok_or_else(|| InfraError::InvalidRequest {
                domain: "file-transfer",
                message: format!("file transfer is disabled for {}", host.id()),
            })
    }

    async fn read_bound(
        &self,
        host: &HostRecord,
        path: &Path,
        options: BoundReadOptions,
        cancellation: &CancellationToken,
    ) -> FleetResult<Option<Vec<u8>>> {
        let policy = self
            .policy(host)
            .map_err(|error| command_error(host, error))?;
        let (root, relative) = match options.role {
            FileTransferPathRole::Source => policy.resolve_source(path),
            FileTransferPathRole::Destination => policy.resolve_destination(path),
        }
        .map_err(|error| command_error(host, error))?;
        let args = vec![
            "-c".into(),
            PY_BOOTSTRAP.into(),
            encoded(READ_SOURCE),
            root.to_string_lossy().into_owned(),
            relative.to_string_lossy().into_owned(),
            options.max_bytes.to_string(),
            if options.optional {
                "1".into()
            } else {
                "0".into()
            },
        ];
        let command = CommandRequest::new("python3", args, options.deadline)?
            .with_output_limits(options.max_bytes, STDERR_LIMIT)?;
        let output = self.executor.execute(host, &command, cancellation).await?;
        if options.optional && output.exit_code() == Some(3) {
            return Ok(None);
        }
        if output.exit_code() != Some(0) || output.truncated() {
            return Err(FleetError::Command {
                host: host.id().clone(),
                message: format!(
                    "descriptor-bound read failed: {}",
                    String::from_utf8_lossy(output.stderr()).trim()
                ),
            });
        }
        Ok(Some(output.stdout().to_vec()))
    }

    async fn write_bound(
        &self,
        host: &HostRecord,
        path: &Path,
        bytes: &[u8],
        max_bytes: usize,
        deadline: soma_ops::Timestamp,
        cancellation: &CancellationToken,
    ) -> FleetResult<()> {
        let policy = self
            .policy(host)
            .map_err(|error| command_error(host, error))?;
        let (root, relative) = policy
            .resolve_destination(path)
            .map_err(|error| command_error(host, error))?;
        let args = vec![
            "-c".into(),
            PY_BOOTSTRAP.into(),
            encoded(WRITE_SOURCE),
            root.to_string_lossy().into_owned(),
            relative.to_string_lossy().into_owned(),
            max_bytes.to_string(),
        ];
        let command = CommandRequest::new("python3", args, deadline)?
            .with_stdin(bytes.to_vec())?
            .with_output_limits(STDERR_LIMIT, STDERR_LIMIT)?;
        let output = self.executor.execute(host, &command, cancellation).await?;
        if output.exit_code() != Some(0) {
            return Err(FleetError::Command {
                host: host.id().clone(),
                message: format!(
                    "descriptor-bound write failed: {}",
                    String::from_utf8_lossy(output.stderr()).trim()
                ),
            });
        }
        Ok(())
    }
}

#[async_trait]
impl FileTransferInspector for CommandFileTransfer {
    async fn inspect_transfer_file(
        &self,
        host: &HostRecord,
        path: &Path,
        role: FileTransferPathRole,
        optional: bool,
        cancellation: &CancellationToken,
    ) -> InfraResult<Option<TransferFileIdentity>> {
        let deadline = soma_ops::Timestamp::from_unix_millis(
            soma_ops::Timestamp::now().unix_millis() + 30_000,
        );
        self.read_bound(
            host,
            path,
            BoundReadOptions {
                role,
                optional,
                max_bytes: crate::MAX_FILE_TRANSFER_BYTES as usize,
                deadline,
            },
            cancellation,
        )
        .await
        .map(|bytes| bytes.map(|bytes| identity_from_bytes(path, &bytes)))
        .map_err(InfraError::from)
    }
}

#[async_trait]
impl FileTransfer for CommandFileTransfer {
    async fn transfer(
        &self,
        source: &HostRecord,
        destination: &HostRecord,
        request: &TransferRequest,
        cancellation: &CancellationToken,
    ) -> FleetResult<TransferReceipt> {
        if source.id() != request.source_host() || destination.id() != request.destination_host() {
            return Err(FleetError::Transfer {
                source_host: request.source_host().clone(),
                destination_host: request.destination_host().clone(),
                message: "transfer host identities do not match request".into(),
            });
        }
        request.validate_at(soma_ops::Timestamp::now())?;
        let max_bytes = usize::try_from(request.max_bytes()).map_err(|_| FleetError::Transfer {
            source_host: source.id().clone(),
            destination_host: destination.id().clone(),
            message: "transfer byte limit does not fit this platform".into(),
        })?;
        let bytes = self
            .read_bound(
                source,
                request.source_path(),
                BoundReadOptions {
                    role: FileTransferPathRole::Source,
                    optional: false,
                    max_bytes,
                    deadline: request.deadline(),
                },
                cancellation,
            )
            .await?
            .ok_or_else(|| FleetError::Transfer {
                source_host: source.id().clone(),
                destination_host: destination.id().clone(),
                message: "source file is absent".into(),
            })?;
        let (_lifecycle, mut guard) = TransferLifecycle::start(request);
        guard.record_chunk(bytes.len() as u64)?;
        let source_identity = identity_from_bytes(request.source_path(), &bytes);
        if let Some(expected) = request.expected_source_sha256()
            && source_identity.sha256 != expected
        {
            let error = FleetError::Transfer {
                source_host: source.id().clone(),
                destination_host: destination.id().clone(),
                message: "source content changed after planning".into(),
            };
            let _ = guard.fail(bounded_error(&error));
            return Err(error);
        }
        if let Err(error) = self
            .write_bound(
                destination,
                request.destination_path(),
                &bytes,
                max_bytes,
                request.deadline(),
                cancellation,
            )
            .await
        {
            let _ = guard.fail(bounded_error(&error));
            return Err(error);
        }
        let destination_bytes = match self
            .read_bound(
                destination,
                request.destination_path(),
                BoundReadOptions {
                    role: FileTransferPathRole::Destination,
                    optional: false,
                    max_bytes,
                    deadline: request.deadline(),
                },
                cancellation,
            )
            .await
        {
            Ok(Some(bytes)) => bytes,
            Ok(None) => {
                let error = FleetError::Transfer {
                    source_host: source.id().clone(),
                    destination_host: destination.id().clone(),
                    message: "destination is absent after write".into(),
                };
                let _ = guard.fail(bounded_error(&error));
                return Err(error);
            }
            Err(error) => {
                let _ = guard.fail(bounded_error(&error));
                return Err(error);
            }
        };
        let destination_identity =
            identity_from_bytes(request.destination_path(), &destination_bytes);
        let receipt = TransferReceipt::new(bytes.len() as u64)
            .with_digests(source_identity.sha256, destination_identity.sha256)?;
        guard.complete(receipt)
    }
}

fn encoded(source: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(source)
}

fn command_error(host: &HostRecord, error: InfraError) -> FleetError {
    FleetError::Command {
        host: host.id().clone(),
        message: error.to_string(),
    }
}

fn bounded_error(error: &FleetError) -> String {
    let text = error.to_string().replace(char::is_control, " ");
    text.chars().take(1024).collect()
}

#[cfg(test)]
#[path = "process_file_transfer_tests.rs"]
mod tests;
