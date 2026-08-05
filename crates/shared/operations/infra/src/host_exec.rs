use std::path::{Component, Path, PathBuf};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use soma_fleet::{HostId, HostRecord, TopologyRevision};
use soma_ops::{MutationSendState, OperationId, OperationName, Timestamp};
use tokio_util::sync::CancellationToken;

use crate::{InfraError, InfraResult, MutationResult};

const MAX_COMMAND_ARGUMENTS: usize = 256;
const MAX_ARGUMENT_CHARS: usize = 4096;
const MAX_OUTPUT_BYTES: usize = 96 * 1024;

/// Closed allowlist of host commands admitted by canonical Synapse execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostExecCommand {
    /// Concatenate files.
    Cat,
    /// Read the beginning of files.
    Head,
    /// Read the end of files.
    Tail,
    /// Search text with grep.
    Grep,
    /// Search text with ripgrep.
    Rg,
    /// List filesystem entries.
    Ls,
    /// Render a directory tree.
    Tree,
    /// Count bytes, words, or lines.
    Wc,
    /// Collapse adjacent duplicate lines.
    Uniq,
    /// Compare files.
    Diff,
    /// Read filesystem metadata.
    Stat,
    /// Identify file types.
    File,
    /// Summarize filesystem usage.
    Du,
    /// Report filesystem capacity.
    Df,
    /// Print the working directory.
    Pwd,
    /// Print the host name.
    Hostname,
    /// Print host uptime.
    Uptime,
    /// Print the effective user.
    Whoami,
}

impl HostExecCommand {
    /// Parses one canonical command name.
    pub fn parse(value: &str) -> InfraResult<Self> {
        match value {
            "cat" => Ok(Self::Cat),
            "head" => Ok(Self::Head),
            "tail" => Ok(Self::Tail),
            "grep" => Ok(Self::Grep),
            "rg" => Ok(Self::Rg),
            "ls" => Ok(Self::Ls),
            "tree" => Ok(Self::Tree),
            "wc" => Ok(Self::Wc),
            "uniq" => Ok(Self::Uniq),
            "diff" => Ok(Self::Diff),
            "stat" => Ok(Self::Stat),
            "file" => Ok(Self::File),
            "du" => Ok(Self::Du),
            "df" => Ok(Self::Df),
            "pwd" => Ok(Self::Pwd),
            "hostname" => Ok(Self::Hostname),
            "uptime" => Ok(Self::Uptime),
            "whoami" => Ok(Self::Whoami),
            _ => Err(invalid(format!("host command is not allowlisted: {value}"))),
        }
    }

    /// Returns the executable name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cat => "cat",
            Self::Head => "head",
            Self::Tail => "tail",
            Self::Grep => "grep",
            Self::Rg => "rg",
            Self::Ls => "ls",
            Self::Tree => "tree",
            Self::Wc => "wc",
            Self::Uniq => "uniq",
            Self::Diff => "diff",
            Self::Stat => "stat",
            Self::File => "file",
            Self::Du => "du",
            Self::Df => "df",
            Self::Pwd => "pwd",
            Self::Hostname => "hostname",
            Self::Uptime => "uptime",
            Self::Whoami => "whoami",
        }
    }
}

/// One bounded allowlisted host execution request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostExecRequest {
    operation_id: OperationId,
    operation: OperationName,
    command: HostExecCommand,
    args: Vec<String>,
    working_dir: Option<PathBuf>,
    deadline: Timestamp,
}

impl HostExecRequest {
    /// Creates a validated request with at most 256 direct arguments.
    pub fn new(
        operation_id: OperationId,
        operation: OperationName,
        command: HostExecCommand,
        args: Vec<String>,
        working_dir: Option<PathBuf>,
        deadline: Timestamp,
    ) -> InfraResult<Self> {
        if args.len() > MAX_COMMAND_ARGUMENTS {
            return Err(invalid(format!(
                "host command accepts at most {MAX_COMMAND_ARGUMENTS} arguments"
            )));
        }
        for argument in &args {
            let count = argument.chars().count();
            if count == 0 || count > MAX_ARGUMENT_CHARS || argument.as_bytes().contains(&0) {
                return Err(invalid(
                    "host command arguments must be 1-4096 characters without NUL",
                ));
            }
        }
        let working_dir = working_dir.map(validate_absolute_path).transpose()?;
        if deadline <= Timestamp::now() {
            return Err(invalid("host command deadline must be in the future"));
        }
        Ok(Self {
            operation_id,
            operation,
            command,
            args,
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
    /// Returns the allowlisted command.
    #[must_use]
    pub const fn command(&self) -> HostExecCommand {
        self.command
    }
    /// Returns positional arguments.
    #[must_use]
    pub fn args(&self) -> &[String] {
        &self.args
    }
    /// Returns the optional descriptor-bound working directory.
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

/// Completed bounded host execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostExecReceipt {
    /// Target host.
    pub host: HostId,
    /// Exact topology revision.
    pub topology_revision: TopologyRevision,
    /// Executed command.
    pub command: HostExecCommand,
    /// Positional arguments.
    pub args: Vec<String>,
    /// Optional descriptor-bound working directory.
    pub working_dir: Option<PathBuf>,
    /// Lossy UTF-8 stdout bounded by policy.
    pub stdout: String,
    /// Lossy UTF-8 stderr bounded by policy.
    pub stderr: String,
    /// Process exit code when available.
    pub exit_code: Option<i32>,
    /// Whether either stream exceeded its byte ceiling.
    pub truncated: bool,
    /// Whether UTF-8 replacement was required.
    pub encoding_lossy: bool,
    /// Backend send state.
    pub send_state: MutationSendState,
}

/// Product-neutral bounded host command driver.
#[async_trait]
pub trait HostExecMutator: Send + Sync {
    /// Executes one allowlisted command through a typed launcher.
    async fn exec_host(
        &self,
        host: &HostRecord,
        request: &HostExecRequest,
        cancellation: &CancellationToken,
    ) -> MutationResult<HostExecReceipt>;
}

fn validate_absolute_path(path: PathBuf) -> InfraResult<PathBuf> {
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
        || path.to_string_lossy().chars().any(char::is_control)
    {
        Err(invalid(format!(
            "working directory must be absolute and normalized: {}",
            path.display()
        )))
    } else {
        Ok(path)
    }
}

fn invalid(message: impl Into<String>) -> InfraError {
    InfraError::InvalidRequest {
        domain: "host-exec",
        message: message.into(),
    }
}

#[cfg(test)]
#[path = "host_exec_tests.rs"]
mod tests;
