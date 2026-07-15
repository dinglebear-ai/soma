use serde_json::Value;

use crate::ToolError;

use super::provider::GitProvider;

pub async fn dispatch_git(method: &str, params: Value) -> Result<Value, ToolError> {
    GitProvider::new(std::env::current_dir().map_err(|err| {
        ToolError::internal_message(format!("failed to resolve cwd for git provider: {err}"))
    })?)
    .dispatch(method, params)
    .await
}
