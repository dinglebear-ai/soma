use std::sync::Arc;

use async_trait::async_trait;
use soma_fleet::{CommandExecutor, CommandOutput, CommandRequest, HostRecord};
use soma_ops::Timestamp;
use tokio_util::sync::CancellationToken;

use crate::compose_parse::{parse_config, parse_project_list, parse_status, validate_service};
use crate::{
    ComposeConfig, ComposeInspector, ComposeLogRequest, ComposeLogs, ComposeProject,
    ComposeProjectRef, ComposeStatus, InfraError, InfraResult,
};

const COMPOSE_OUTPUT_LIMIT: usize = 4 * 1024 * 1024;

/// Compose inspector backed by a `soma-fleet` command executor.
pub struct CommandComposeInspector<E> {
    pub(crate) executor: Arc<E>,
}

impl<E> CommandComposeInspector<E> {
    /// Creates a Compose inspector using the supplied fleet executor.
    #[must_use]
    pub fn new(executor: Arc<E>) -> Self {
        Self { executor }
    }
}

#[async_trait]
impl<E> ComposeInspector for CommandComposeInspector<E>
where
    E: CommandExecutor,
{
    async fn list_projects(
        &self,
        host: &HostRecord,
        deadline: Timestamp,
        cancellation: &CancellationToken,
    ) -> InfraResult<Vec<ComposeProject>> {
        let raw = self
            .run(
                host,
                ["compose", "ls", "--format", "json"],
                deadline,
                cancellation,
            )
            .await?;
        parse_project_list(host, &raw)
    }

    async fn status(
        &self,
        host: &HostRecord,
        project: &ComposeProjectRef,
        service: Option<&str>,
        deadline: Timestamp,
        cancellation: &CancellationToken,
    ) -> InfraResult<ComposeStatus> {
        let config = project
            .config_file()
            .to_str()
            .expect("ComposeProjectRef validates UTF-8 paths")
            .to_owned();
        let mut args = vec![
            "compose".to_owned(),
            "-f".to_owned(),
            config,
            "--project-name".to_owned(),
            project.name().to_owned(),
            "ps".to_owned(),
            "--format".to_owned(),
            "json".to_owned(),
        ];
        if let Some(service) = service {
            validate_service(service)?;
            args.push("--".into());
            args.push(service.to_owned());
        }
        let raw = self.run_owned(host, args, deadline, cancellation).await?;
        parse_status(host, project, &raw)
    }

    async fn config(
        &self,
        host: &HostRecord,
        project: &ComposeProjectRef,
        deadline: Timestamp,
        cancellation: &CancellationToken,
    ) -> InfraResult<ComposeConfig> {
        let raw = self
            .run_owned(
                host,
                vec![
                    "compose".into(),
                    "-f".into(),
                    project
                        .config_file()
                        .to_str()
                        .expect("ComposeProjectRef validates UTF-8 paths")
                        .to_owned(),
                    "--project-name".into(),
                    project.name().to_owned(),
                    "config".into(),
                    "--format".into(),
                    "json".into(),
                ],
                deadline,
                cancellation,
            )
            .await?;
        parse_config(host, project, &raw)
    }

    async fn logs(
        &self,
        host: &HostRecord,
        project: &ComposeProjectRef,
        request: &ComposeLogRequest,
        cancellation: &CancellationToken,
    ) -> InfraResult<ComposeLogs> {
        let mut args = vec![
            "compose".into(),
            "-f".into(),
            project
                .config_file()
                .to_str()
                .expect("ComposeProjectRef validates UTF-8 paths")
                .to_owned(),
            "--project-name".into(),
            project.name().to_owned(),
            "logs".into(),
            "--no-color".into(),
            "--tail".into(),
            request.lines().to_string(),
        ];
        if let Some(since) = request.since() {
            args.extend(["--since".into(), since.to_owned()]);
        }
        if let Some(service) = request.service() {
            validate_service(service)?;
            args.extend(["--".into(), service.to_owned()]);
        }
        let output = self
            .execute_owned(host, args, request.deadline(), cancellation)
            .await?;
        if output.exit_code() != Some(0) {
            return Err(InfraError::CommandFailed {
                domain: "compose",
                host: host.id().clone(),
                exit_code: output.exit_code(),
                stderr: crate::error::public_diagnostic(output.stderr()),
            });
        }
        let text = std::str::from_utf8(output.stdout()).map_err(|error| InfraError::Parse {
            domain: "compose",
            message: format!("Compose log output was not UTF-8: {error}"),
        })?;
        Ok(ComposeLogs {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            project: project.name().to_owned(),
            lines: text.lines().map(str::to_owned).collect(),
            truncated: output.truncated(),
        })
    }
}

impl<E> CommandComposeInspector<E>
where
    E: CommandExecutor,
{
    async fn run<const N: usize>(
        &self,
        host: &HostRecord,
        args: [&str; N],
        deadline: Timestamp,
        cancellation: &CancellationToken,
    ) -> InfraResult<String> {
        self.run_owned(
            host,
            args.into_iter().map(str::to_owned).collect(),
            deadline,
            cancellation,
        )
        .await
    }

    async fn run_owned(
        &self,
        host: &HostRecord,
        args: Vec<String>,
        deadline: Timestamp,
        cancellation: &CancellationToken,
    ) -> InfraResult<String> {
        let output = self
            .execute_owned(host, args, deadline, cancellation)
            .await?;
        checked_output(host, output)
    }

    async fn execute_owned(
        &self,
        host: &HostRecord,
        args: Vec<String>,
        deadline: Timestamp,
        cancellation: &CancellationToken,
    ) -> InfraResult<CommandOutput> {
        let request = CommandRequest::new("docker", args, deadline)
            .map_err(soma_fleet::FleetError::from)?
            .with_output_limits(COMPOSE_OUTPUT_LIMIT, COMPOSE_OUTPUT_LIMIT)
            .map_err(soma_fleet::FleetError::from)?;
        self.executor
            .execute(host, &request, cancellation)
            .await
            .map_err(InfraError::from)
    }
}

fn checked_output(host: &HostRecord, output: CommandOutput) -> InfraResult<String> {
    if output.exit_code() != Some(0) {
        return Err(InfraError::CommandFailed {
            domain: "compose",
            host: host.id().clone(),
            exit_code: output.exit_code(),
            stderr: crate::error::public_diagnostic(output.stderr()),
        });
    }
    if output.truncated() {
        return Err(InfraError::Parse {
            domain: "compose",
            message: "bounded Compose output was truncated".into(),
        });
    }
    String::from_utf8(output.stdout().to_vec()).map_err(|error| InfraError::Parse {
        domain: "compose",
        message: format!("Compose output was not UTF-8: {error}"),
    })
}

#[cfg(test)]
#[path = "process_compose_tests.rs"]
mod tests;
