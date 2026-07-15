use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::path_safety::{reject_existing_symlink_ancestors, rel_to_unix_string};
use crate::ToolError;

use super::path::{is_reserved_metadata_path, VirtualPath};
use super::quota::StateWorkspaceLimits;

#[derive(Debug, Clone)]
pub struct StateWorkspace {
    root: PathBuf,
    limits: StateWorkspaceLimits,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReadFileResult {
    pub path: String,
    pub content: String,
    pub bytes: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListResult {
    pub entries: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MutationResult {
    pub ok: bool,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExistsResult {
    pub path: String,
    pub exists: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatResult {
    pub path: String,
    pub kind: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct WalkEntry {
    pub path: String,
    pub kind: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct WalkTreeResult {
    pub entries: Vec<WalkEntry>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct JsonReadResult {
    pub path: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct HashFileResult {
    pub path: String,
    pub algorithm: String,
    pub hex: String,
    pub bytes: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct DetectFileResult {
    pub path: String,
    pub extension: String,
    pub text: bool,
    pub json: bool,
    pub bytes: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArchiveCreateResult {
    pub ok: bool,
    pub destination: String,
    pub entries: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArchiveListResult {
    pub path: String,
    pub entries: Vec<String>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct GlobResult {
    pub matches: Vec<String>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchMatch {
    pub path: String,
    pub line: usize,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchFilesResult {
    pub matches: Vec<SearchMatch>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReplaceInFilesResult {
    pub changed: Vec<String>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FileEdit {
    pub path: String,
    pub search: String,
    pub replace: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EditPlanResult {
    pub plan_id: String,
    pub edits: Vec<FileEdit>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApplyEditPlanResult {
    pub ok: bool,
    pub changed: Vec<String>,
}

pub(crate) struct WalkFilesResult {
    pub(crate) files: Vec<String>,
    pub(crate) truncated: bool,
}

struct WorkspaceUsage {
    bytes: u64,
    entries: u64,
}

pub fn default_search_limit() -> usize {
    200
}

pub fn default_true() -> bool {
    true
}

impl StateWorkspace {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            limits: StateWorkspaceLimits::default(),
        }
    }

    pub fn with_limits(root: impl Into<PathBuf>, limits: StateWorkspaceLimits) -> Self {
        Self {
            root: root.into(),
            limits,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn root_path(&self) -> &PathBuf {
        &self.root
    }

    pub(crate) fn limits(&self) -> StateWorkspaceLimits {
        self.limits
    }

    pub(crate) fn resolve(&self, path: &VirtualPath) -> PathBuf {
        self.root.join(path.as_str())
    }

    pub(crate) async fn ensure_path_allowed(&self, path: &Path) -> Result<(), ToolError> {
        reject_existing_symlink_ancestors(&self.root, path)?;
        self.reject_existing_symlink_path(path).await
    }

    pub(crate) async fn reject_existing_symlink_path(&self, path: &Path) -> Result<(), ToolError> {
        match tokio::fs::symlink_metadata(path).await {
            Ok(metadata) if metadata.file_type().is_symlink() => Err(ToolError::Sdk {
                sdk_kind: "symlink_rejected".to_string(),
                message: "state path is denied because it is a symlink".to_string(),
            }),
            Ok(_) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(internal_io("read state path metadata")(err)),
        }
    }

    pub(crate) async fn create_temp_path(&self) -> Result<PathBuf, ToolError> {
        let dir = self.root.join(".soma-state").join("tmp");
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(internal_io("create state temp directory"))?;
        reject_existing_symlink_ancestors(&self.root, &dir)?;
        let metadata = tokio::fs::symlink_metadata(&dir)
            .await
            .map_err(internal_io("inspect state temp directory"))?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(ToolError::Sdk {
                sdk_kind: "permission_denied".to_string(),
                message: "state temp directory is not a directory".to_string(),
            });
        }
        Ok(dir.join(format!("{}.tmp", ulid::Ulid::new())))
    }

    pub(crate) async fn check_write_quota(
        &self,
        destination: &Path,
        next_file_bytes: u64,
    ) -> Result<(), ToolError> {
        let current_file_bytes = match tokio::fs::metadata(destination).await {
            Ok(metadata) if metadata.is_file() => metadata.len(),
            Ok(_) | Err(_) => 0,
        };
        let usage = workspace_usage(&self.root).await?;
        let projected = usage
            .bytes
            .saturating_sub(current_file_bytes)
            .saturating_add(next_file_bytes);
        if projected > self.limits.max_total_bytes {
            return Err(quota_error(format!(
                "state workspace would be {projected} bytes; maximum is {}",
                self.limits.max_total_bytes
            )));
        }
        let projected_entries = usage
            .entries
            .saturating_add(missing_entry_count(&self.root, destination).await?);
        if projected_entries > self.limits.max_entries {
            return Err(quota_error(format!(
                "state workspace would have {projected_entries} entries; maximum is {}",
                self.limits.max_entries
            )));
        }
        Ok(())
    }

    pub async fn enforce_total_bytes(&self) -> Result<(), ToolError> {
        self.enforce_total_limits().await
    }

    pub(crate) async fn enforce_total_limits(&self) -> Result<(), ToolError> {
        let usage = workspace_usage(&self.root).await?;
        if usage.bytes > self.limits.max_total_bytes {
            return Err(quota_error(format!(
                "state workspace is {} bytes; maximum is {}",
                usage.bytes, self.limits.max_total_bytes
            )));
        }
        if usage.entries > self.limits.max_entries {
            return Err(quota_error(format!(
                "state workspace has {} entries; maximum is {}",
                usage.entries, self.limits.max_entries
            )));
        }
        Ok(())
    }

    pub(crate) async fn check_entry_quota_for_path(
        &self,
        destination: &Path,
    ) -> Result<(), ToolError> {
        let usage = workspace_usage(&self.root).await?;
        let projected = usage
            .entries
            .saturating_add(missing_entry_count(&self.root, destination).await?);
        if projected > self.limits.max_entries {
            return Err(quota_error(format!(
                "state workspace would have {projected} entries; maximum is {}",
                self.limits.max_entries
            )));
        }
        Ok(())
    }

    pub(crate) fn plan_path(&self, plan_id: &str) -> PathBuf {
        self.root
            .join(".soma-state")
            .join("plans")
            .join(format!("{plan_id}.json"))
    }

    pub(crate) async fn walk_files(&self, limit: usize) -> Result<WalkFilesResult, ToolError> {
        let mut files = Vec::new();
        let mut truncated = false;
        let mut stack = vec![self.root.clone()];
        while let Some(dir) = stack.pop() {
            let mut read_dir = match tokio::fs::read_dir(&dir).await {
                Ok(read_dir) => read_dir,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(internal_io("walk state workspace")(error)),
            };
            while let Some(entry) = read_dir
                .next_entry()
                .await
                .map_err(internal_io("walk state workspace entry"))?
            {
                let path = entry.path();
                let relative = match path.strip_prefix(&self.root) {
                    Ok(relative) => relative,
                    Err(_) => continue,
                };
                let virtual_path = rel_to_unix_string(relative);
                if is_reserved_metadata_path(&virtual_path) {
                    continue;
                }
                let metadata = tokio::fs::symlink_metadata(&path)
                    .await
                    .map_err(internal_io("read state workspace metadata"))?;
                if metadata.file_type().is_symlink() {
                    continue;
                }
                if metadata.is_dir() {
                    stack.push(path);
                } else if metadata.is_file() {
                    files.push(virtual_path);
                    if files.len() > limit {
                        truncated = true;
                        break;
                    }
                }
            }
            if truncated {
                break;
            }
        }
        files.sort();
        if truncated {
            files.truncate(limit);
        }
        Ok(WalkFilesResult { files, truncated })
    }
}

pub(crate) fn internal_io(action: &'static str) -> impl FnOnce(std::io::Error) -> ToolError {
    move |err| ToolError::Sdk {
        sdk_kind: "internal_error".to_string(),
        message: format!("failed to {action}: {err}"),
    }
}

pub(crate) fn not_found_or_internal(
    action: &'static str,
) -> impl FnOnce(std::io::Error) -> ToolError {
    move |err| ToolError::Sdk {
        sdk_kind: if err.kind() == std::io::ErrorKind::NotFound {
            "not_found"
        } else {
            "internal_error"
        }
        .to_string(),
        message: format!("failed to {action}: {err}"),
    }
}

pub(crate) fn serialize_error(err: serde_json::Error) -> ToolError {
    ToolError::Sdk {
        sdk_kind: "internal_error".to_string(),
        message: format!("failed to serialize state value: {err}"),
    }
}

pub(crate) async fn cleanup_file_after_quota_error(
    path: &Path,
    original: ToolError,
    label: &str,
) -> Result<(), ToolError> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Err(original),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Err(original),
        Err(err) => Err(ToolError::Sdk {
            sdk_kind: "quota_cleanup_failed".to_string(),
            message: format!("{label} exceeded quota and cleanup failed: {err}"),
        }),
    }
}

fn quota_error(message: String) -> ToolError {
    ToolError::Sdk {
        sdk_kind: "quota_exceeded".to_string(),
        message,
    }
}

async fn workspace_usage(root: &Path) -> Result<WorkspaceUsage, ToolError> {
    let mut usage = WorkspaceUsage {
        bytes: 0,
        entries: 0,
    };
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut read_dir = match tokio::fs::read_dir(&dir).await {
            Ok(read_dir) => read_dir,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(internal_io("scan state workspace")(error)),
        };
        while let Some(entry) = read_dir
            .next_entry()
            .await
            .map_err(internal_io("scan state workspace entry"))?
        {
            let path = entry.path();
            let metadata = tokio::fs::symlink_metadata(&path)
                .await
                .map_err(internal_io("read state workspace metadata"))?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            if path.strip_prefix(root).is_ok() {
                usage.entries = usage.entries.saturating_add(1);
            }
            if metadata.is_dir() {
                stack.push(path);
            } else if metadata.is_file() {
                usage.bytes = usage.bytes.saturating_add(metadata.len());
            }
        }
    }
    Ok(usage)
}

async fn missing_entry_count(root: &Path, destination: &Path) -> Result<u64, ToolError> {
    let relative = destination.strip_prefix(root).map_err(|_| ToolError::Sdk {
        sdk_kind: "path_traversal".to_string(),
        message: "state path escapes the workspace".to_string(),
    })?;
    let virtual_path = rel_to_unix_string(relative);
    if is_reserved_metadata_path(&virtual_path) {
        return Ok(0);
    }
    let parts = relative
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_owned()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let mut current = root.to_path_buf();
    for (index, part) in parts.iter().enumerate() {
        current.push(part);
        match tokio::fs::symlink_metadata(&current).await {
            Ok(_) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Ok((parts.len() - index) as u64);
            }
            Err(err) => return Err(internal_io("read state path metadata")(err)),
        }
    }
    Ok(0)
}
