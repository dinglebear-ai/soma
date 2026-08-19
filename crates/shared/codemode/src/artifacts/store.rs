use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::ToolError;

use super::path::{artifact_root, artifact_store_root, safe_artifact_path};
use super::prune::{ActiveArtifactRun, active_artifact_runs_snapshot, prune_artifact_runs_in};

const DEFAULT_MAX_FILE_BYTES: usize = 8 * 1024 * 1024;
const DEFAULT_MAX_RUN_BYTES: usize = 64 * 1024 * 1024;
const DEFAULT_MAX_FILES: usize = 128;
const DEFAULT_RETENTION_RUNS: usize = 200;
const DEFAULT_MAX_STORE_BYTES: u64 = 4096 * 1024 * 1024;

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
    max_run_bytes: usize,
    max_files: usize,
    retention_runs: usize,
    max_store_bytes: u64,
    prune_once: Arc<tokio::sync::OnceCell<()>>,
    _active_run: Arc<ActiveArtifactRun>,
    usage: Arc<Mutex<ArtifactUsage>>,
}

#[derive(Debug, Default)]
struct ArtifactUsage {
    bytes: usize,
    files: usize,
}

impl ArtifactStore {
    pub fn new(run_id: impl Into<String>) -> Result<Self, ToolError> {
        let run_id = validate_run_id(run_id.into())?;
        let active_run = Arc::new(ActiveArtifactRun::register(&run_id));
        Ok(Self {
            run_id,
            max_bytes: DEFAULT_MAX_FILE_BYTES,
            max_run_bytes: DEFAULT_MAX_RUN_BYTES,
            max_files: DEFAULT_MAX_FILES,
            retention_runs: DEFAULT_RETENTION_RUNS,
            max_store_bytes: DEFAULT_MAX_STORE_BYTES,
            prune_once: Arc::new(tokio::sync::OnceCell::new()),
            _active_run: active_run,
            usage: Arc::new(Mutex::new(ArtifactUsage::default())),
        })
    }

    pub fn with_max_bytes(mut self, max_bytes: usize) -> Self {
        self.max_bytes = max_bytes.max(1);
        self
    }

    pub fn with_run_limits(mut self, max_run_bytes: usize, max_files: usize) -> Self {
        self.max_run_bytes = max_run_bytes.max(1);
        self.max_files = max_files.max(1);
        self
    }

    pub fn with_retention_limits(mut self, retention_runs: usize, max_store_bytes: u64) -> Self {
        self.retention_runs = retention_runs;
        self.max_store_bytes = max_store_bytes;
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
        self.check_run_quota(content.len())?;
        self.prune_before_first_write().await;
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
        self.record_write(content.len())?;
        Ok(ArtifactReceipt {
            path: rel_path.to_string(),
            absolute_path: target.display().to_string(),
            content_type: content_type.unwrap_or("text/plain").to_string(),
            bytes: content.len(),
            sha256: hex::encode(Sha256::digest(content.as_bytes())),
        })
    }

    async fn prune_before_first_write(&self) {
        self.prune_once
            .get_or_init(|| async {
                let active = active_artifact_runs_snapshot();
                prune_artifact_runs_in(
                    &artifact_store_root(),
                    self.retention_runs,
                    self.max_store_bytes,
                    &active,
                )
                .await;
            })
            .await;
    }

    fn check_run_quota(&self, next_bytes: usize) -> Result<(), ToolError> {
        let usage = self
            .usage
            .lock()
            .map_err(|_| ToolError::internal_message("artifact usage lock poisoned"))?;
        if usage.files >= self.max_files
            || usage.bytes.saturating_add(next_bytes) > self.max_run_bytes
        {
            return Err(ToolError::InvalidParam {
                message: "artifact run quota exceeded".to_string(),
                param: "content".to_string(),
            });
        }
        Ok(())
    }

    fn record_write(&self, bytes: usize) -> Result<(), ToolError> {
        let mut usage = self
            .usage
            .lock()
            .map_err(|_| ToolError::internal_message("artifact usage lock poisoned"))?;
        usage.files = usage.files.saturating_add(1);
        usage.bytes = usage.bytes.saturating_add(bytes);
        Ok(())
    }
}

fn validate_run_id(run_id: String) -> Result<String, ToolError> {
    let trimmed = run_id.trim();
    let safe = !trimmed.is_empty()
        && trimmed != "."
        && trimmed != ".."
        && trimmed.len() <= 128
        && trimmed
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'));
    if safe {
        Ok(trimmed.to_string())
    } else {
        Err(ToolError::InvalidParam {
            message: "artifact run id must be a safe single path segment".to_string(),
            param: "execution_id".to_string(),
        })
    }
}
