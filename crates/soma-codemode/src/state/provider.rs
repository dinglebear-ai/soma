use serde_json::{json, Value};

use crate::ToolError;

use super::path::state_root;
use super::path::VirtualPath;
use super::workspace::StateWorkspace;

#[derive(Debug, Clone)]
pub struct StateProvider {
    workspace: StateWorkspace,
}

impl Default for StateProvider {
    fn default() -> Self {
        Self {
            workspace: StateWorkspace::new(state_root()),
        }
    }
}

impl StateProvider {
    pub fn new(workspace: StateWorkspace) -> Self {
        Self { workspace }
    }

    pub async fn dispatch(&self, method: &str, params: Value) -> Result<Value, ToolError> {
        match method {
            "write_file" => {
                let path = VirtualPath::parse(string_param(&params, "path")?)?;
                let content = string_param(&params, "content")?;
                self.workspace.write_file(&path, content).await?;
                Ok(json!({"ok": true}))
            }
            "read_file" => {
                let path = VirtualPath::parse(string_param(&params, "path")?)?;
                Ok(json!(self.workspace.read_file(&path).await?))
            }
            "status" => Ok(json!({"root": self.workspace.root().display().to_string()})),
            _ => Err(ToolError::UnknownAction {
                message: format!("unknown state method `{method}`"),
                valid: vec![
                    "read_file".to_string(),
                    "write_file".to_string(),
                    "status".to_string(),
                ],
                hint: None,
            }),
        }
    }
}

fn string_param<'a>(params: &'a Value, key: &str) -> Result<&'a str, ToolError> {
    params
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::MissingParam {
            message: format!("missing `{key}`"),
            param: key.to_string(),
        })
}
