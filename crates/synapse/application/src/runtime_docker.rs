use serde_json::{Map, Value};
use soma_infra::{
    ContainerListOptions, ContainerLogOptions, ContainerState, DockerLogStream, ImageListOptions,
};
use soma_ops::OperationName;
use tokio_util::sync::CancellationToken;

use crate::runtime_params::{optional_str, parse_time_spec, required_str, u32_or};
use crate::runtime_result::{items, metrics, resource, text};
use crate::{ExecutionError, SynapseReadRuntime};

impl SynapseReadRuntime {
    pub(crate) async fn execute_docker(
        &self,
        operation: &OperationName,
        parameters: &Value,
        cancellation: &CancellationToken,
    ) -> Result<Value, ExecutionError> {
        let host = self.resolve_host(parameters).await?;
        let client = self.ports.docker.client(&host, cancellation).await?;
        match operation.as_str() {
            "docker.info" => resource(client.system_info(&host, cancellation).await?),
            "docker.df" => metrics(client.disk_usage(&host, cancellation).await?),
            "docker.images" => {
                let options = ImageListOptions {
                    all: false,
                    dangling_only: crate::runtime_params::bool_or(
                        parameters,
                        "dangling_only",
                        false,
                    )?,
                };
                let rows = client.list_images(&host, &options, cancellation).await?;
                let count = rows.len();
                items(rows, count, false)
            }
            "docker.networks" => {
                let rows = client.list_networks(&host, cancellation).await?;
                let count = rows.len();
                items(rows, count, false)
            }
            "docker.volumes" => {
                let rows = client.list_volumes(&host, cancellation).await?;
                let count = rows.len();
                items(rows, count, false)
            }
            "container.list" => {
                let options = ContainerListOptions {
                    all: true,
                    state: docker_state(optional_str(parameters, "state")?),
                    label: optional_str(parameters, "label_filter")?.map(str::to_owned),
                };
                let mut rows = client
                    .list_containers(&host, &options, cancellation)
                    .await?;
                filter_containers(
                    &mut rows,
                    optional_str(parameters, "name_filter")?,
                    optional_str(parameters, "image_filter")?,
                    None,
                );
                let count = rows.len();
                items(rows, count, false)
            }
            "container.inspect" => resource(
                client
                    .inspect_container(
                        &host,
                        required_str(parameters, "container_id")?,
                        cancellation,
                    )
                    .await?,
            ),
            "container.logs" => {
                let mut options =
                    ContainerLogOptions::default().with_lines(u32_or(parameters, "lines", 50)?)?;
                options = options.with_stream(match optional_str(parameters, "stream")? {
                    Some("stdout") => DockerLogStream::Stdout,
                    Some("stderr") => DockerLogStream::Stderr,
                    _ => DockerLogStream::Both,
                });
                if let Some(since) = optional_str(parameters, "since")? {
                    options = options.with_since(parse_time_spec(since)?)?;
                }
                if let Some(until) = optional_str(parameters, "until")? {
                    options = options.with_until(parse_time_spec(until)?)?;
                }
                if let Some(grep) = optional_str(parameters, "grep")?
                    && !grep.is_empty()
                {
                    options = options.with_grep(grep)?;
                }
                let logs = client
                    .container_logs(
                        &host,
                        required_str(parameters, "container_id")?,
                        &options,
                        cancellation,
                    )
                    .await?;
                let body = logs.lines.join("\n");
                Ok(text(
                    body.as_bytes(),
                    logs.truncated,
                    Some(logs.lines.len()),
                ))
            }
            "container.stats" => metrics(
                client
                    .container_stats(
                        &host,
                        required_str(parameters, "container_id")?,
                        cancellation,
                    )
                    .await?,
            ),
            "container.top" => {
                let table = client
                    .top_container(
                        &host,
                        required_str(parameters, "container_id")?,
                        cancellation,
                    )
                    .await?;
                let rows = table
                    .processes
                    .into_iter()
                    .map(|values| {
                        let mut row = Map::new();
                        for (index, title) in table.titles.iter().enumerate() {
                            row.insert(
                                title.clone(),
                                Value::String(values.get(index).cloned().unwrap_or_default()),
                            );
                        }
                        Value::Object(row)
                    })
                    .collect::<Vec<_>>();
                let count = rows.len();
                items(rows, count, false)
            }
            "container.search" => {
                let query = required_str(parameters, "query")?;
                let mut rows = client
                    .list_containers(&host, &ContainerListOptions::default(), cancellation)
                    .await?;
                filter_containers(&mut rows, None, None, Some(query));
                let count = rows.len();
                items(rows, count, false)
            }
            _ => Err(ExecutionError::UnsupportedOperation(operation.clone())),
        }
    }
}

fn docker_state(value: Option<&str>) -> Option<ContainerState> {
    match value {
        None | Some("all") => None,
        Some("running" | "active") => Some(ContainerState::Running),
        Some("paused") => Some(ContainerState::Paused),
        Some("restarting") => Some(ContainerState::Restarting),
        Some("exited" | "inactive" | "failed") => Some(ContainerState::Exited),
        Some(other) => Some(ContainerState::Unknown(other.to_owned())),
    }
}

fn filter_containers(
    rows: &mut Vec<soma_infra::ContainerSummary>,
    name: Option<&str>,
    image: Option<&str>,
    query: Option<&str>,
) {
    rows.retain(|row| {
        let name_matches =
            name.is_none_or(|needle| row.names.iter().any(|value| value.contains(needle)));
        let image_matches = image.is_none_or(|needle| {
            row.image
                .as_deref()
                .is_some_and(|value| value.contains(needle))
        });
        let query_matches = query.is_none_or(|needle| {
            let needle = needle.to_ascii_lowercase();
            row.names
                .iter()
                .any(|value| value.to_ascii_lowercase().contains(&needle))
                || row
                    .image
                    .as_deref()
                    .is_some_and(|value| value.to_ascii_lowercase().contains(&needle))
                || row
                    .command
                    .as_deref()
                    .is_some_and(|value| value.to_ascii_lowercase().contains(&needle))
                || row.labels.iter().any(|(key, value)| {
                    key.to_ascii_lowercase().contains(&needle)
                        || value.to_ascii_lowercase().contains(&needle)
                })
        });
        name_matches && image_matches && query_matches
    });
}
