use serde_json::{json, Value};

use crate::ToolError;

use super::path::state_root;
use super::path::VirtualPath;
use super::workspace::{FileEdit, StateWorkspace};

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
            "append_file" => {
                let path = VirtualPath::parse(string_param(&params, "path")?)?;
                Ok(json!(
                    self.workspace
                        .append_file(&path, string_param(&params, "content")?)
                        .await?
                ))
            }
            "read_file" => {
                let path = VirtualPath::parse(string_param(&params, "path")?)?;
                Ok(json!(self.workspace.read_file(&path).await?))
            }
            "read_json" => {
                let path = VirtualPath::parse(string_param(&params, "path")?)?;
                Ok(json!(self.workspace.read_json(&path).await?))
            }
            "write_json" => {
                let path = VirtualPath::parse(string_param(&params, "path")?)?;
                self.workspace
                    .write_json(
                        &path,
                        value_param(&params, "value")?,
                        bool_param(&params, "pretty", false),
                    )
                    .await?;
                Ok(json!({"ok": true}))
            }
            "hash_file" => {
                let path = VirtualPath::parse(string_param(&params, "path")?)?;
                Ok(json!(
                    self.workspace
                        .hash_file(&path, string_param_default(&params, "algorithm", "sha256"))
                        .await?
                ))
            }
            "detect_file" => {
                let path = VirtualPath::parse(string_param(&params, "path")?)?;
                Ok(json!(self.workspace.detect_file(&path).await?))
            }
            "exists" => {
                let path =
                    VirtualPath::parse_read_scope(string_param_default(&params, "path", ""))?;
                Ok(json!(self.workspace.exists(&path).await?))
            }
            "stat" => {
                let path =
                    VirtualPath::parse_read_scope(string_param_default(&params, "path", ""))?;
                Ok(json!(self.workspace.stat(&path).await?))
            }
            "mkdir" => {
                let path = VirtualPath::parse(string_param(&params, "path")?)?;
                Ok(json!(self.workspace.mkdir(&path).await?))
            }
            "remove" => {
                let path = VirtualPath::parse(string_param(&params, "path")?)?;
                Ok(json!(
                    self.workspace
                        .remove(&path, bool_param(&params, "recursive", false))
                        .await?
                ))
            }
            "copy" => {
                let from = VirtualPath::parse(string_param(&params, "from")?)?;
                let to = VirtualPath::parse(string_param(&params, "to")?)?;
                Ok(json!(self.workspace.copy(&from, &to).await?))
            }
            "move" => {
                let from = VirtualPath::parse(string_param(&params, "from")?)?;
                let to = VirtualPath::parse(string_param(&params, "to")?)?;
                Ok(json!(self.workspace.move_path(&from, &to).await?))
            }
            "walk_tree" => {
                let path =
                    VirtualPath::parse_read_scope(string_param_default(&params, "path", ""))?;
                Ok(json!(
                    self.workspace
                        .walk_tree(&path, usize_param(&params, "limit", 200))
                        .await?
                ))
            }
            "list" => {
                let path =
                    VirtualPath::parse_read_scope(string_param_default(&params, "path", ""))?;
                Ok(json!(self.workspace.list(&path).await?))
            }
            "glob" => Ok(json!(
                self.workspace
                    .glob(
                        string_param(&params, "pattern")?,
                        usize_param(&params, "limit", 200)
                    )
                    .await?
            )),
            "search_files" => Ok(json!(
                self.workspace
                    .search_files(
                        string_param(&params, "pattern")?,
                        string_param(&params, "query")?,
                        usize_param(&params, "limit", 200)
                    )
                    .await?
            )),
            "replace_in_files" => Ok(json!(
                self.workspace
                    .replace_in_files(
                        string_param(&params, "pattern")?,
                        string_param(&params, "search")?,
                        string_param(&params, "replace")?,
                        bool_param(&params, "dry_run", true)
                    )
                    .await?
            )),
            "plan_edits" => Ok(json!(
                self.workspace
                    .plan_edits(edits_param(&params, "edits")?)
                    .await?
            )),
            "apply_edit_plan" => Ok(json!(
                self.workspace
                    .apply_edit_plan(string_param(&params, "plan_id")?)
                    .await?
            )),
            "status" => Ok(json!({"root": self.workspace.root().display().to_string()})),
            _ => Err(ToolError::UnknownAction {
                message: format!("unknown state method `{method}`"),
                valid: state_methods().iter().map(ToString::to_string).collect(),
                hint: None,
            }),
        }
    }
}

fn state_methods() -> &'static [&'static str] {
    &[
        "append_file",
        "apply_edit_plan",
        "copy",
        "detect_file",
        "exists",
        "glob",
        "hash_file",
        "list",
        "mkdir",
        "move",
        "plan_edits",
        "read_file",
        "read_json",
        "remove",
        "replace_in_files",
        "search_files",
        "stat",
        "status",
        "walk_tree",
        "write_file",
        "write_json",
    ]
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

fn string_param_default<'a>(params: &'a Value, key: &str, default: &'a str) -> &'a str {
    params.get(key).and_then(Value::as_str).unwrap_or(default)
}

fn bool_param(params: &Value, key: &str, default: bool) -> bool {
    params.get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn usize_param(params: &Value, key: &str, default: usize) -> usize {
    params
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(default)
}

fn value_param<'a>(params: &'a Value, key: &str) -> Result<&'a Value, ToolError> {
    params.get(key).ok_or_else(|| ToolError::MissingParam {
        message: format!("missing `{key}`"),
        param: key.to_string(),
    })
}

fn edits_param(params: &Value, key: &str) -> Result<Vec<FileEdit>, ToolError> {
    let value = value_param(params, key)?;
    serde_json::from_value(value.clone()).map_err(|err| ToolError::InvalidParam {
        message: format!("invalid state edit list: {err}"),
        param: key.to_string(),
    })
}
