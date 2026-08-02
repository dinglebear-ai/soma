use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use soma_fleet::{CommandExecutor, CommandOutput, CommandRequest, HostRecord};
use tokio_util::sync::CancellationToken;

use crate::logs::filtered_tail;
use crate::{
    InfraError, InfraResult, LogPermissionDiagnostic, LogRead, LogReadRequest, LogReader, LogSource,
};

const LOG_OUTPUT_LIMIT: usize = 4 * 1024 * 1024;

/// Operating-system log reader backed by a fleet command executor.
pub struct CommandLogReader<E> {
    executor: Arc<E>,
}

impl<E> CommandLogReader<E> {
    /// Creates a reader using the supplied fleet executor.
    #[must_use]
    pub fn new(executor: Arc<E>) -> Self {
        Self { executor }
    }
}

#[async_trait]
impl<E> LogReader for CommandLogReader<E>
where
    E: CommandExecutor,
{
    async fn read_logs(
        &self,
        host: &HostRecord,
        request: &LogReadRequest,
        cancellation: &CancellationToken,
    ) -> InfraResult<LogRead> {
        match request.source() {
            LogSource::Syslog => {
                self.read_file_logs(
                    host,
                    request,
                    "/var/log/syslog",
                    "/var/log/messages",
                    cancellation,
                )
                .await
            }
            LogSource::Auth => {
                self.read_file_logs(
                    host,
                    request,
                    "/var/log/auth.log",
                    "/var/log/secure",
                    cancellation,
                )
                .await
            }
            LogSource::Journal => self.read_journal(host, request, cancellation).await,
            LogSource::Dmesg => self.read_dmesg(host, request, cancellation).await,
        }
    }
}

impl<E> CommandLogReader<E>
where
    E: CommandExecutor,
{
    async fn read_file_logs(
        &self,
        host: &HostRecord,
        request: &LogReadRequest,
        primary: &str,
        fallback: &str,
        cancellation: &CancellationToken,
    ) -> InfraResult<LogRead> {
        let line_count = request.lines().to_string();
        let primary_output = self
            .execute(
                host,
                "tail",
                vec!["-n".into(), line_count.clone(), primary.into()],
                request,
                cancellation,
            )
            .await?;
        let (output, source_path) = if primary_output.exit_code() == Some(0) {
            (primary_output, PathBuf::from(primary))
        } else if missing_file(&primary_output) {
            let fallback_output = self
                .execute(
                    host,
                    "tail",
                    vec!["-n".into(), line_count, fallback.into()],
                    request,
                    cancellation,
                )
                .await?;
            if fallback_output.exit_code() != Some(0) {
                return Err(command_error(host, "logs", &fallback_output));
            }
            (fallback_output, PathBuf::from(fallback))
        } else {
            return Err(command_error(host, "logs", &primary_output));
        };
        self.render(host, request, output, Some(source_path), None)
    }

    async fn read_journal(
        &self,
        host: &HostRecord,
        request: &LogReadRequest,
        cancellation: &CancellationToken,
    ) -> InfraResult<LogRead> {
        let mut args = vec![
            "-n".into(),
            request.lines().to_string(),
            "--no-pager".into(),
        ];
        let filters = request.journal();
        if let Some(unit) = filters.unit() {
            args.extend(["-u".into(), unit.into()]);
        }
        if let Some(priority) = filters.priority() {
            args.extend(["-p".into(), priority.as_arg().into()]);
        }
        if let Some(since) = filters.since() {
            args.extend(["--since".into(), since.into()]);
        }
        if let Some(until) = filters.until() {
            args.extend(["--until".into(), until.into()]);
        }
        let output = self
            .execute(host, "journalctl", args, request, cancellation)
            .await?;
        if output.exit_code() != Some(0) {
            return Err(command_error(host, "logs", &output));
        }
        self.render(host, request, output, None, None)
    }

    async fn read_dmesg(
        &self,
        host: &HostRecord,
        request: &LogReadRequest,
        cancellation: &CancellationToken,
    ) -> InfraResult<LogRead> {
        let fetch_lines = request.lines().saturating_mul(4).min(1000);
        let output = self
            .execute(
                host,
                "dmesg",
                vec![
                    "--color=never".into(),
                    "--lines".into(),
                    fetch_lines.to_string(),
                ],
                request,
                cancellation,
            )
            .await?;
        if output.exit_code() != Some(0) {
            let detail = String::from_utf8_lossy(output.stderr()).trim().to_owned();
            if permission_denied(&detail) {
                return Ok(LogRead {
                    host: host.id().clone(),
                    topology_revision: host.revision().clone(),
                    source: LogSource::Dmesg,
                    source_path: None,
                    lines: Vec::new(),
                    truncated: output.truncated(),
                    permission: Some(LogPermissionDiagnostic {
                        message: detail,
                        help: "dmesg requires root or CAP_SYSLOG on restricted kernels".into(),
                    }),
                });
            }
            return Err(command_error(host, "logs", &output));
        }
        self.render(host, request, output, None, None)
    }

    async fn execute(
        &self,
        host: &HostRecord,
        program: &str,
        args: Vec<String>,
        request: &LogReadRequest,
        cancellation: &CancellationToken,
    ) -> InfraResult<CommandOutput> {
        let command = CommandRequest::new(program, args, request.deadline())
            .map_err(soma_fleet::FleetError::from)?
            .with_output_limits(LOG_OUTPUT_LIMIT, LOG_OUTPUT_LIMIT)
            .map_err(soma_fleet::FleetError::from)?;
        self.executor
            .execute(host, &command, cancellation)
            .await
            .map_err(InfraError::from)
    }

    fn render(
        &self,
        host: &HostRecord,
        request: &LogReadRequest,
        output: CommandOutput,
        source_path: Option<PathBuf>,
        permission: Option<LogPermissionDiagnostic>,
    ) -> InfraResult<LogRead> {
        let text = std::str::from_utf8(output.stdout()).map_err(|error| InfraError::Parse {
            domain: "logs",
            message: format!("log output is not UTF-8: {error}"),
        })?;
        let (lines, line_truncated) = filtered_tail(text, request.grep(), request.lines());
        Ok(LogRead {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            source: request.source(),
            source_path,
            lines,
            truncated: output.truncated() || line_truncated,
            permission,
        })
    }
}

fn missing_file(output: &CommandOutput) -> bool {
    let stderr = String::from_utf8_lossy(output.stderr()).to_lowercase();
    stderr.contains("no such file") || stderr.contains("not found")
}

fn permission_denied(detail: &str) -> bool {
    let detail = detail.to_lowercase();
    detail.contains("operation not permitted")
        || detail.contains("permission denied")
        || detail.contains("read kernel buffer failed")
}

fn command_error(host: &HostRecord, domain: &'static str, output: &CommandOutput) -> InfraError {
    InfraError::CommandFailed {
        domain,
        host: host.id().clone(),
        exit_code: output.exit_code(),
        stderr: String::from_utf8_lossy(output.stderr()).trim().to_owned(),
    }
}

#[cfg(test)]
#[path = "process_logs_tests.rs"]
mod tests;
