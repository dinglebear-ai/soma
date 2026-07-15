use sha2::{Digest, Sha256};

use crate::ToolError;

use super::path::VirtualPath;
use super::workspace::{
    cleanup_file_after_quota_error, internal_io, serialize_error, ApplyEditPlanResult,
    EditPlanResult, FileEdit, StateWorkspace,
};

impl StateWorkspace {
    pub async fn plan_edits(&self, edits: Vec<FileEdit>) -> Result<EditPlanResult, ToolError> {
        let edits = normalize_edits(edits)?;
        let canonical = serde_json::to_vec(&edits).map_err(serialize_error)?;
        if canonical.len() > self.limits().max_file_bytes {
            return Err(ToolError::Sdk {
                sdk_kind: "response_too_large".to_string(),
                message: "state edit plan exceeded max file bytes".to_string(),
            });
        }
        let plan_id = hex::encode(Sha256::digest(&canonical));
        let plan_path = self.plan_path(&plan_id);
        if let Some(parent) = plan_path.parent() {
            self.check_entry_quota_for_path(parent).await?;
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(internal_io("create state edit plan directory"))?;
        }
        self.check_write_quota(&plan_path, canonical.len() as u64)
            .await?;
        tokio::fs::write(&plan_path, canonical)
            .await
            .map_err(internal_io("write state edit plan"))?;
        if let Err(err) = self.enforce_total_limits().await {
            cleanup_file_after_quota_error(&plan_path, err, "state edit plan").await?;
        }
        Ok(EditPlanResult { plan_id, edits })
    }

    pub async fn apply_edit_plan(&self, plan_id: &str) -> Result<ApplyEditPlanResult, ToolError> {
        validate_plan_id(plan_id)?;
        let plan_path = self.plan_path(plan_id);
        let plan = self
            .read_bounded_path(&plan_path, "read state edit plan")
            .await?;
        let edits: Vec<FileEdit> = serde_json::from_slice(&plan).map_err(|err| ToolError::Sdk {
            sdk_kind: "internal_error".to_string(),
            message: format!("failed to parse state edit plan: {err}"),
        })?;
        let mut planned = Vec::new();
        for edit in edits {
            let path = VirtualPath::parse(&edit.path)?;
            let original = self.read_file(&path).await?;
            if !original.content.contains(&edit.search) {
                return Err(ToolError::Sdk {
                    sdk_kind: "edit_conflict".to_string(),
                    message: format!("state edit plan no longer matches `{}`", path.as_str()),
                });
            }
            let next = original.content.replace(&edit.search, &edit.replace);
            planned.push((path, original.content, next));
        }
        let mut changed = Vec::new();
        let mut originals = Vec::new();
        for (path, original, next) in planned {
            if let Err(err) = self.write_file(&path, &next).await {
                return Err(self.restore_originals_after_failure(&originals, err).await);
            }
            originals.push((path.clone(), original));
            changed.push(path.as_str().to_string());
        }
        Ok(ApplyEditPlanResult { ok: true, changed })
    }

    pub(super) async fn restore_originals_after_failure(
        &self,
        originals: &[(VirtualPath, String)],
        original_error: ToolError,
    ) -> ToolError {
        for (path, content) in originals.iter().rev() {
            if let Err(rollback_error) = self.write_file(path, content).await {
                return ToolError::Sdk {
                    sdk_kind: "rollback_failed".to_string(),
                    message: format!(
                        "state batch mutation failed with `{}` and rollback of `{}` failed with `{}`",
                        original_error.kind(),
                        path.as_str(),
                        rollback_error.kind()
                    ),
                };
            }
        }
        original_error
    }
}

fn normalize_edits(edits: Vec<FileEdit>) -> Result<Vec<FileEdit>, ToolError> {
    if edits.is_empty() {
        return Err(ToolError::InvalidParam {
            message: "state edit plan must include at least one edit".to_string(),
            param: "edits".to_string(),
        });
    }
    edits
        .into_iter()
        .map(|edit| {
            if edit.search.is_empty() {
                return Err(ToolError::InvalidParam {
                    message: "state edit search must not be empty".to_string(),
                    param: "search".to_string(),
                });
            }
            let path = VirtualPath::parse(&edit.path)?.as_str().to_string();
            Ok(FileEdit {
                path,
                search: edit.search,
                replace: edit.replace,
            })
        })
        .collect()
}

fn validate_plan_id(plan_id: &str) -> Result<(), ToolError> {
    let valid = plan_id.len() == 64 && plan_id.chars().all(|ch| ch.is_ascii_hexdigit());
    if valid {
        Ok(())
    } else {
        Err(ToolError::InvalidParam {
            message: "state edit plan id must be a sha256 hex string".to_string(),
            param: "planId".to_string(),
        })
    }
}
