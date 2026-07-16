use std::path::PathBuf;

use tokio::process::Command;

use crate::ToolError;

use super::output::cap_output;
use super::safety::safe_git_env;

#[derive(Debug, Clone)]
pub struct GitCommand {
    cwd: PathBuf,
    args: Vec<String>,
}

impl GitCommand {
    pub fn new(cwd: impl Into<PathBuf>, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            cwd: cwd.into(),
            args: args.into_iter().map(Into::into).collect(),
        }
    }

    pub async fn run(&self) -> Result<String, ToolError> {
        let mut command = Command::new("git");
        command.current_dir(&self.cwd).args(&self.args).env_clear();
        for (key, value) in safe_git_env() {
            command.env(key, value);
        }
        let output = command
            .output()
            .await
            .map_err(|err| ToolError::internal_message(format!("git failed: {err}")))?;
        if !output.status.success() {
            let stderr = cap_output(&output.stderr, 8 * 1024);
            return Err(ToolError::Sdk {
                sdk_kind: "upstream_error".to_string(),
                message: if stderr.trim().is_empty() {
                    format!("git exited with status {}", output.status)
                } else {
                    format!("git exited with status {}: {stderr}", output.status)
                },
            });
        }
        Ok(cap_output(&output.stdout, 64 * 1024))
    }

    pub fn args(&self) -> &[String] {
        &self.args
    }
}
