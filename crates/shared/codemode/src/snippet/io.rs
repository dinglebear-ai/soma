use std::path::{Path, PathBuf};

use crate::path_safety::reject_path_traversal;
use crate::ToolError;

pub fn snippet_path(root: &Path, rel_path: &str) -> Result<PathBuf, ToolError> {
    reject_path_traversal(rel_path)?;
    Ok(root.join(rel_path))
}

pub async fn read_snippet_file(root: &Path, rel_path: &str) -> Result<String, ToolError> {
    let path = snippet_path(root, rel_path)?;
    tokio::fs::read_to_string(&path).await.map_err(|err| {
        ToolError::internal_message(format!("read snippet `{}`: {err}", path.display()))
    })
}
