use serde_json::{json, Value};

use crate::ToolError;

use super::command::GitCommand;
use super::safety::validate_ref;

#[derive(Debug, Clone)]
pub struct GitProvider {
    cwd: std::path::PathBuf,
}

impl GitProvider {
    pub fn new(cwd: impl Into<std::path::PathBuf>) -> Self {
        Self { cwd: cwd.into() }
    }

    pub async fn dispatch(&self, method: &str, params: Value) -> Result<Value, ToolError> {
        match method {
            "status" => Ok(json!({
                "stdout": GitCommand::new(&self.cwd, ["status", "--short"]).run().await?
            })),
            "show_ref" => {
                let reference = params.get("ref").and_then(Value::as_str).ok_or_else(|| {
                    ToolError::MissingParam {
                        message: "missing `ref`".to_string(),
                        param: "ref".to_string(),
                    }
                })?;
                validate_ref(reference)?;
                let resolved = GitCommand::new(
                    &self.cwd,
                    vec![
                        "rev-parse".to_string(),
                        "--verify".to_string(),
                        "--quiet".to_string(),
                        format!("{reference}^{{object}}"),
                    ],
                )
                .run()
                .await?;
                Ok(json!({"ref": reference, "oid": resolved.trim()}))
            }
            _ => Err(ToolError::UnknownAction {
                message: format!("unknown git method `{method}`"),
                valid: vec!["status".to_string(), "show_ref".to_string()],
                hint: None,
            }),
        }
    }
}
