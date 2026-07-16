use std::path::{Path, PathBuf};

use crate::path_safety::{reject_existing_symlink_ancestors, reject_path_traversal};
use crate::ToolError;

pub fn artifact_root(run_id: &str) -> PathBuf {
    crate::soma_home().join("code-mode-artifacts").join(run_id)
}

pub fn safe_artifact_path(root: &Path, rel_path: &str) -> Result<PathBuf, ToolError> {
    reject_path_traversal(rel_path)?;
    let target = root.join(rel_path);
    reject_existing_symlink_ancestors(root, &target)?;
    Ok(target)
}
