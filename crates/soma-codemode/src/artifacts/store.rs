use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::ToolError;

use super::path::{artifact_root, safe_artifact_path};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactReceipt {
    pub path: String,
    pub absolute_path: String,
    pub content_type: String,
    pub bytes: usize,
    pub sha256: String,
}

#[derive(Debug, Clone)]
pub struct ArtifactStore {
    run_id: String,
    max_bytes: usize,
}

impl ArtifactStore {
    pub fn new(run_id: impl Into<String>) -> Self {
        Self {
            run_id: run_id.into(),
            max_bytes: 8 * 1024 * 1024,
        }
    }

    pub fn with_max_bytes(mut self, max_bytes: usize) -> Self {
        self.max_bytes = max_bytes.max(1);
        self
    }

    pub fn root(&self) -> PathBuf {
        artifact_root(&self.run_id)
    }

    pub async fn write_text(
        &self,
        rel_path: &str,
        content: &str,
        content_type: Option<&str>,
    ) -> Result<ArtifactReceipt, ToolError> {
        if content.len() > self.max_bytes {
            return Err(ToolError::InvalidParam {
                message: "artifact content exceeded size limit".to_string(),
                param: "content".to_string(),
            });
        }
        let root = self.root();
        let target = safe_artifact_path(&root, rel_path)?;
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|err| {
                ToolError::internal_message(format!("create artifact dir: {err}"))
            })?;
        }
        let mut file = tokio::fs::File::create(&target)
            .await
            .map_err(|err| ToolError::internal_message(format!("create artifact: {err}")))?;
        file.write_all(content.as_bytes())
            .await
            .map_err(|err| ToolError::internal_message(format!("write artifact: {err}")))?;
        Ok(ArtifactReceipt {
            path: rel_path.to_string(),
            absolute_path: target.display().to_string(),
            content_type: content_type.unwrap_or("text/plain").to_string(),
            bytes: content.len(),
            sha256: hex::encode(Sha256::digest(content.as_bytes())),
        })
    }
}
