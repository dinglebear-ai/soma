use std::sync::Arc;

use async_trait::async_trait;
use soma_fleet::{CommandExecutor, CommandOutput, CommandRequest, HostRecord};
use soma_ops::Timestamp;
use tokio_util::sync::CancellationToken;

use crate::compose_parse::{parse_config, parse_project_list, parse_status, validate_service};
use crate::{
    ComposeConfig, ComposeInspector, ComposeProject, ComposeProjectRef, ComposeStatus, InfraError,
    InfraResult,
};

const COMPOSE_OUTPUT_LIMIT: usize = 4 * 1024 * 1024;

/// Compose inspector backed by a `soma-fleet` command executor.
pub struct CommandComposeInspector<E> {
    executor: Arc<E>,
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
        parse_project_list(&raw)
    }

    async fn status(
        &self,
        host: &HostRecord,
        project: &ComposeProjectRef,
        service: Option<&str>,
        deadline: Timestamp,
        cancellation: &CancellationToken,
    ) -> InfraResult<ComposeStatus> {
        let config = project.config_file().to_string_lossy().into_owned();
        let mut args = vec![
            "compose".to_owned(),
            "-f".to_owned(),
            config,
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
                    project.config_file().to_string_lossy().into_owned(),
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
        let request = CommandRequest::new("docker", args, deadline)
            .map_err(soma_fleet::FleetError::from)?
            .with_output_limits(COMPOSE_OUTPUT_LIMIT, COMPOSE_OUTPUT_LIMIT)
            .map_err(soma_fleet::FleetError::from)?;
        let output = self.executor.execute(host, &request, cancellation).await?;
        checked_output(host, output)
    }
}

fn checked_output(host: &HostRecord, output: CommandOutput) -> InfraResult<String> {
    if output.exit_code() != Some(0) {
        return Err(InfraError::CommandFailed {
            domain: "compose",
            host: host.id().clone(),
            exit_code: output.exit_code(),
            stderr: String::from_utf8_lossy(output.stderr()).trim().to_owned(),
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
