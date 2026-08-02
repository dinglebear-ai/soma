use serde_json::{Value, json};
use soma_infra::{FileFindRequest, PathReadRequest};
use soma_ops::OperationName;
use tokio_util::sync::CancellationToken;

use crate::runtime_params::{bool_or, optional_str, required_path, required_str, u32_or};
use crate::runtime_result::{compare, file_content, items, metrics};
use crate::{ExecutionError, SynapseReadRuntime};

impl SynapseReadRuntime {
    pub(crate) async fn execute_files(
        &self,
        operation: &OperationName,
        parameters: &Value,
        cancellation: &CancellationToken,
    ) -> Result<Value, ExecutionError> {
        match operation.as_str() {
            "files.read" => {
                let host = self.resolve_host(parameters).await?;
                let tree = bool_or(parameters, "tree", false)?;
                let mut request = PathReadRequest::new(self.deadline());
                if tree {
                    request = request.with_tree(u32_or(parameters, "depth", 3)? as u8)?;
                }
                let value = self
                    .ports
                    .filesystem
                    .read_path(
                        &host,
                        &required_path(parameters, "path")?,
                        &request,
                        cancellation,
                    )
                    .await?;
                Ok(file_content(value, tree))
            }
            "files.find" => {
                let host = self.resolve_host(parameters).await?;
                let requested_limit = u32_or(parameters, "limit", 500)?;
                let request =
                    FileFindRequest::new(required_str(parameters, "pattern")?, self.deadline())?
                        .with_depth(u32_or(parameters, "depth", 10)? as u8)?
                        .with_limit(requested_limit.min(500))?;
                let result = self
                    .ports
                    .filesystem
                    .find(
                        &host,
                        &required_path(parameters, "path")?,
                        &request,
                        cancellation,
                    )
                    .await?;
                let count = result.items.len();
                items(
                    result
                        .items
                        .into_iter()
                        .map(|path| json!({"path": path}))
                        .collect::<Vec<_>>(),
                    count,
                    result.truncated || requested_limit > 500,
                )
            }
            "filesystem.usage" => {
                let host = self.resolve_host(parameters).await?;
                metrics(
                    self.ports
                        .host_system
                        .filesystem_usage(
                            &host,
                            optional_str(parameters, "path")?,
                            self.deadline(),
                            cancellation,
                        )
                        .await?,
                )
            }
            "files.compare" => {
                let source_host = self
                    .resolve_host_name(required_str(parameters, "source_host")?)
                    .await?;
                let source_path = required_path(parameters, "source_path")?;
                let source = self
                    .ports
                    .filesystem
                    .read_path(
                        &source_host,
                        &source_path,
                        &PathReadRequest::new(self.deadline()),
                        cancellation,
                    )
                    .await?;
                let (target, target_label) =
                    if let Some(content) = optional_str(parameters, "content")? {
                        (content.as_bytes().to_vec(), "inline".to_owned())
                    } else {
                        let target_host = self
                            .resolve_host_name(required_str(parameters, "target_host")?)
                            .await?;
                        let target_path = required_path(parameters, "target_path")?;
                        let value = self
                            .ports
                            .filesystem
                            .read_path(
                                &target_host,
                                &target_path,
                                &PathReadRequest::new(self.deadline()),
                                cancellation,
                            )
                            .await?;
                        (
                            value.content,
                            format!("{}:{}", target_host.id(), target_path.display()),
                        )
                    };
                Ok(compare(
                    &source.content,
                    &target,
                    &format!("{}:{}", source_host.id(), source_path.display()),
                    &target_label,
                ))
            }
            _ => Err(ExecutionError::UnsupportedOperation(operation.clone())),
        }
    }
}
