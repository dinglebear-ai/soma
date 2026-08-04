use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use soma_fleet::{HostId, HostRecord, TopologyRevision};
use soma_ops::{MutationSendState, OperationId, OperationName, Timestamp};
use tokio_util::sync::CancellationToken;

use crate::{InfraError, InfraResult, MutationResult};

const MAX_COMMAND_ARGUMENTS: usize = 256;
const MAX_ARGUMENT_CHARS: usize = 4096;
const MAX_OUTPUT_BYTES: usize = 96 * 1024;

/// One non-interactive bounded Docker exec request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerExecRequest {
    operation_id: OperationId,
    operation: OperationName,
    container: String,
    command: Vec<String>,
    user: Option<String>,
    working_dir: Option<PathBuf>,
    deadline: Timestamp,
}

impl ContainerExecRequest {
    /// Creates a one-shot non-TTY exec request.
    pub fn new(
        operation_id: OperationId,
        operation: OperationName,
        container: impl Into<String>,
        command: Vec<String>,
        user: Option<String>,
        working_dir: Option<PathBuf>,
        deadline: Timestamp,
    ) -> InfraResult<Self> {
        let container = container.into();
        validate_text("container", &container, 256)?;
        if command.is_empty() || command.len() > MAX_COMMAND_ARGUMENTS {
            return Err(invalid(format!(
                "container exec requires 1-{MAX_COMMAND_ARGUMENTS} command arguments"
            )));
        }
        for argument in &command {
            validate_text("command argument", argument, MAX_ARGUMENT_CHARS)?;
        }
        if let Some(user) = &user {
            validate_text("exec user", user, 256)?;
        }
        let working_dir = working_dir.map(validate_working_dir).transpose()?;
        if deadline <= Timestamp::now() {
            return Err(invalid("container exec deadline must be in the future"));
        }
        Ok(Self {
            operation_id,
            operation,
            container,
            command,
            user,
            working_dir,
            deadline,
        })
    }

    /// Returns the operation identity.
    #[must_use]
    pub fn operation_id(&self) -> &OperationId {
        &self.operation_id
    }
    /// Returns the canonical operation name.
    #[must_use]
    pub fn operation(&self) -> &OperationName {
        &self.operation
    }
    /// Returns the target container identifier.
    #[must_use]
    pub fn container(&self) -> &str {
        &self.container
    }
    /// Returns direct exec argv.
    #[must_use]
    pub fn command(&self) -> &[String] {
        &self.command
    }
    /// Returns the optional Docker exec user.
    #[must_use]
    pub fn user(&self) -> Option<&str> {
        self.user.as_deref()
    }
    /// Returns the optional absolute container working directory.
    #[must_use]
    pub fn working_dir(&self) -> Option<&Path> {
        self.working_dir.as_deref()
    }
    /// Returns the absolute deadline.
    #[must_use]
    pub const fn deadline(&self) -> Timestamp {
        self.deadline
    }
    /// Returns the stdout byte ceiling.
    #[must_use]
    pub const fn max_stdout_bytes(&self) -> usize {
        MAX_OUTPUT_BYTES
    }
    /// Returns the stderr byte ceiling.
    #[must_use]
    pub const fn max_stderr_bytes(&self) -> usize {
        MAX_OUTPUT_BYTES
    }
}

/// Completed non-interactive Docker exec.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerExecReceipt {
    /// Target host.
    pub host: HostId,
    /// Exact topology revision.
    pub topology_revision: TopologyRevision,
    /// Target container.
    pub container: String,
    /// Direct command argv.
    pub command: Vec<String>,
    /// Optional exec user.
    pub user: Option<String>,
    /// Optional container working directory.
    pub working_dir: Option<PathBuf>,
    /// Bounded stdout.
    pub stdout: String,
    /// Bounded stderr.
    pub stderr: String,
    /// Docker exec exit code when available.
    pub exit_code: Option<i64>,
    /// Whether either output stream exceeded its ceiling.
    pub truncated: bool,
    /// Whether UTF-8 replacement was required.
    pub encoding_lossy: bool,
    /// Backend send state.
    pub send_state: MutationSendState,
}

/// Product-neutral non-interactive Docker exec driver.
#[async_trait]
pub trait ContainerExecMutator: Send + Sync {
    /// Executes one direct argv command without a shell or TTY.
    async fn exec_container(
        &self,
        host: &HostRecord,
        request: &ContainerExecRequest,
        cancellation: &CancellationToken,
    ) -> MutationResult<ContainerExecReceipt>;
}

/// Supplies one host-bound Docker exec client.
#[async_trait]
pub trait ContainerExecClientProvider: Send + Sync {
    /// Creates an exec client bound to the exact host revision.
    async fn exec_client(
        &self,
        host: &HostRecord,
        cancellation: &CancellationToken,
    ) -> InfraResult<Arc<dyn ContainerExecMutator>>;
}

fn validate_text(field: &'static str, value: &str, max: usize) -> InfraResult<()> {
    let count = value.chars().count();
    if count == 0 || count > max || value.as_bytes().contains(&0) {
        Err(invalid(format!("invalid {field}")))
    } else {
        Ok(())
    }
}

fn validate_working_dir(path: PathBuf) -> InfraResult<PathBuf> {
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
        || path.to_string_lossy().as_bytes().contains(&0)
    {
        Err(invalid(format!(
            "container working directory must be absolute and normalized: {}",
            path.display()
        )))
    } else {
        Ok(path)
    }
}

fn invalid(message: impl Into<String>) -> InfraError {
    InfraError::InvalidRequest {
        domain: "container-exec",
        message: message.into(),
    }
}

#[cfg(test)]
#[path = "container_exec_tests.rs"]
mod tests;
