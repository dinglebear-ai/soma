use serde_json::{Value, json};
use soma_infra::{
    ComposeLogRequest, JournalFilters, JournalPriority, LogReadRequest, LogSource,
    ProcessListRequest, ProcessSort, ZfsDatasetRequest, ZfsDatasetType, ZfsPoolRequest,
    ZfsSnapshotRequest,
};
use soma_ops::OperationName;
use tokio_util::sync::CancellationToken;

use crate::runtime_params::{bool_or, optional_str, required_str, u32_or};
use crate::runtime_result::{items, status, text};
use crate::{ExecutionError, SynapseReadRuntime};

impl SynapseReadRuntime {
    pub(crate) async fn execute_observability(
        &self,
        operation: &OperationName,
        parameters: &Value,
        cancellation: &CancellationToken,
    ) -> Result<Value, ExecutionError> {
        match operation.as_str() {
            "compose.list" => {
                let host = self.resolve_host(parameters).await?;
                let rows = self
                    .ports
                    .compose
                    .list_projects(&host, self.deadline(), cancellation)
                    .await?;
                let count = rows.len();
                items(rows, count, false)
            }
            "compose.status" => {
                let host = self.resolve_host(parameters).await?;
                let project_name = required_str(parameters, "project")?;
                let project = self
                    .resolve_project(&host, project_name, cancellation)
                    .await?;
                let result = self
                    .ports
                    .compose
                    .status(
                        &host,
                        &project,
                        optional_str(parameters, "service")?,
                        self.deadline(),
                        cancellation,
                    )
                    .await?;
                let state = if result.services.is_empty() {
                    "empty"
                } else if result
                    .services
                    .iter()
                    .all(|service| service.state.as_deref() == Some("running"))
                {
                    "running"
                } else {
                    "degraded"
                };
                Ok(status(
                    state,
                    serde_json::to_value(result)
                        .map_err(|error| ExecutionError::Serialization(error.to_string()))?,
                ))
            }
            "compose.logs" => {
                let host = self.resolve_host(parameters).await?;
                let project_name = required_str(parameters, "project")?;
                let project = self
                    .resolve_project(&host, project_name, cancellation)
                    .await?;
                let mut request = ComposeLogRequest::new(self.deadline())
                    .with_lines(u32_or(parameters, "lines", 100)?)?;
                if let Some(since) = optional_str(parameters, "since")? {
                    request = request.with_since(since)?;
                }
                if let Some(service) = optional_str(parameters, "service")? {
                    request = request.with_service(service)?;
                }
                let logs = self
                    .ports
                    .compose
                    .logs(&host, &project, &request, cancellation)
                    .await?;
                let body = logs.lines.join("\n");
                Ok(text(
                    body.as_bytes(),
                    logs.truncated,
                    Some(logs.lines.len()),
                ))
            }
            "compose.refresh" => {
                let host = self.resolve_host(parameters).await?;
                let projects = self
                    .ports
                    .compose
                    .list_projects(&host, self.deadline(), cancellation)
                    .await?;
                Ok(status(
                    "refreshed",
                    json!({
                        "host": host.id(),
                        "topology_revision": host.revision(),
                        "project_count": projects.len(),
                        "projects": projects
                    }),
                ))
            }
            "processes.list" => {
                let host = self.resolve_host(parameters).await?;
                let requested_limit = u32_or(parameters, "limit", 50)?;
                let mut request = ProcessListRequest::new(self.deadline())
                    .with_sort(match optional_str(parameters, "sort")? {
                        Some("mem") => ProcessSort::Memory,
                        Some("pid") => ProcessSort::Pid,
                        Some("time") => ProcessSort::Time,
                        _ => ProcessSort::Cpu,
                    })
                    .with_limit(requested_limit.min(500))?;
                if let Some(grep) = optional_str(parameters, "grep")?
                    && !grep.is_empty()
                {
                    request = request.with_grep(grep)?;
                }
                if let Some(user) = optional_str(parameters, "user")? {
                    request = request.with_user(user)?;
                }
                let result = self
                    .ports
                    .processes
                    .list_processes(&host, &request, cancellation)
                    .await?;
                let count = result.rows.len();
                items(
                    result.rows,
                    count,
                    result.truncated || requested_limit > 500,
                )
            }
            "zfs.pools" => {
                let host = self.resolve_host(parameters).await?;
                let mut request = ZfsPoolRequest::new(self.deadline());
                if let Some(pool) = optional_str(parameters, "pool")? {
                    request = request.with_pool(pool)?;
                }
                let result = self.ports.zfs.pools(&host, &request, cancellation).await?;
                let count = result.rows.len();
                items(result.rows, count, result.truncated)
            }
            "zfs.datasets" => {
                let host = self.resolve_host(parameters).await?;
                let mut request = ZfsDatasetRequest::new(self.deadline()).recursive(bool_or(
                    parameters,
                    "recursive",
                    false,
                )?);
                if let Some(pool) = optional_str(parameters, "pool")? {
                    request = request.with_pool(pool)?;
                }
                if let Some(kind) = optional_str(parameters, "dataset_type")? {
                    request = request.with_type(match kind {
                        "volume" => ZfsDatasetType::Volume,
                        "snapshot" => ZfsDatasetType::Snapshot,
                        "bookmark" => ZfsDatasetType::Bookmark,
                        "all" => ZfsDatasetType::All,
                        _ => ZfsDatasetType::Filesystem,
                    });
                }
                let result = self
                    .ports
                    .zfs
                    .datasets(&host, &request, cancellation)
                    .await?;
                let count = result.rows.len();
                items(result.rows, count, result.truncated)
            }
            "zfs.snapshots" => {
                let host = self.resolve_host(parameters).await?;
                let requested_limit = u32_or(parameters, "limit", 5_000)?;
                let mut request = ZfsSnapshotRequest::new(self.deadline())
                    .with_limit(requested_limit.min(5_000))?;
                if let Some(pool) = optional_str(parameters, "pool")? {
                    request = request.with_pool(pool)?;
                }
                if let Some(dataset) = optional_str(parameters, "dataset")? {
                    request = request.with_dataset(dataset)?;
                }
                let result = self
                    .ports
                    .zfs
                    .snapshots(&host, &request, cancellation)
                    .await?;
                let count = result.rows.len();
                items(result.rows, count, result.truncated)
            }
            "logs.syslog" | "logs.journal" | "logs.kernel" | "logs.auth" => {
                self.execute_log(operation, parameters, cancellation).await
            }
            _ => Err(ExecutionError::UnsupportedOperation(operation.clone())),
        }
    }

    async fn execute_log(
        &self,
        operation: &OperationName,
        parameters: &Value,
        cancellation: &CancellationToken,
    ) -> Result<Value, ExecutionError> {
        let host = self.resolve_host(parameters).await?;
        let source = match operation.as_str() {
            "logs.journal" => LogSource::Journal,
            "logs.kernel" => LogSource::Dmesg,
            "logs.auth" => LogSource::Auth,
            _ => LogSource::Syslog,
        };
        let mut request = LogReadRequest::new(source, self.deadline())
            .with_lines(u32_or(parameters, "lines", 100)?)?;
        if let Some(grep) = optional_str(parameters, "grep")?
            && !grep.is_empty()
        {
            request = request.with_grep(grep)?;
        }
        if source == LogSource::Journal {
            let mut filters = JournalFilters::default();
            if let Some(unit) = optional_str(parameters, "unit")? {
                filters = filters.with_unit(unit)?;
            }
            if let Some(priority) = optional_str(parameters, "priority")? {
                filters = filters.with_priority(parse_priority(priority)?);
            }
            if let Some(since) = optional_str(parameters, "since")? {
                filters = filters.with_since(since)?;
            }
            if let Some(until) = optional_str(parameters, "until")? {
                filters = filters.with_until(until)?;
            }
            request = request.with_journal_filters(filters)?;
        }
        let result = self
            .ports
            .logs
            .read_logs(&host, &request, cancellation)
            .await?;
        let body = if let Some(permission) = result.permission {
            format!("{}\n{}", permission.message, permission.help)
        } else {
            result.lines.join("\n")
        };
        Ok(text(
            body.as_bytes(),
            result.truncated,
            Some(result.lines.len()),
        ))
    }
}

fn parse_priority(value: &str) -> Result<JournalPriority, ExecutionError> {
    Ok(match value {
        "emerg" | "0" => JournalPriority::Emerg,
        "alert" | "1" => JournalPriority::Alert,
        "crit" | "2" => JournalPriority::Crit,
        "err" | "3" => JournalPriority::Err,
        "warning" | "4" => JournalPriority::Warning,
        "notice" | "5" => JournalPriority::Notice,
        "info" | "6" => JournalPriority::Info,
        "debug" | "7" => JournalPriority::Debug,
        other => {
            return Err(ExecutionError::InvalidParameter {
                field: "priority".into(),
                message: format!("unsupported journal priority {other}"),
            });
        }
    })
}
