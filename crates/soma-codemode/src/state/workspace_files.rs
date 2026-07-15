use std::path::Path;

use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::path_safety::reject_existing_symlink_ancestors;
use crate::ToolError;

use super::path::VirtualPath;
use super::workspace::{
    internal_io, not_found_or_internal, serialize_error, DetectFileResult, HashFileResult,
    JsonReadResult, MutationResult, ReadFileResult, StateWorkspace,
};

impl StateWorkspace {
    pub async fn write_file(&self, path: &VirtualPath, content: &str) -> Result<(), ToolError> {
        if content.len() > self.limits().max_file_bytes {
            return Err(ToolError::InvalidParam {
                message: format!(
                    "state file content is {} bytes; maximum is {}",
                    content.len(),
                    self.limits().max_file_bytes
                ),
                param: "content".to_string(),
            });
        }
        let destination = self.resolve(path);
        self.ensure_path_allowed(&destination).await?;
        self.check_write_quota(&destination, content.len() as u64)
            .await?;
        if let Some(parent) = destination.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(internal_io("create state directory"))?;
        }
        self.ensure_path_allowed(&destination).await?;

        let tmp = self.create_temp_path().await?;
        let mut file = tokio::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&tmp)
            .await
            .map_err(internal_io("create state temp file"))?;
        file.write_all(content.as_bytes())
            .await
            .map_err(internal_io("write state temp file"))?;
        file.flush()
            .await
            .map_err(internal_io("flush state temp file"))?;
        drop(file);
        let tmp_metadata = tokio::fs::symlink_metadata(&tmp)
            .await
            .map_err(internal_io("inspect state temp file"))?;
        if !tmp_metadata.is_file() || tmp_metadata.file_type().is_symlink() {
            drop(tokio::fs::remove_file(&tmp).await);
            return Err(ToolError::Sdk {
                sdk_kind: "permission_denied".to_string(),
                message: "state temp path is not a regular file".to_string(),
            });
        }
        reject_existing_symlink_ancestors(self.root(), &destination)?;
        self.reject_existing_symlink_path(&destination).await?;
        tokio::fs::rename(&tmp, &destination)
            .await
            .map_err(internal_io("move state temp file"))?;
        Ok(())
    }

    pub async fn read_file(&self, path: &VirtualPath) -> Result<ReadFileResult, ToolError> {
        let destination = self.resolve(path);
        self.ensure_path_allowed(&destination).await?;
        let file = tokio::fs::File::open(&destination)
            .await
            .map_err(not_found_or_internal("open state file"))?;
        let mut content = String::new();
        file.take(self.limits().max_result_bytes as u64 + 1)
            .read_to_string(&mut content)
            .await
            .map_err(internal_io("read state file"))?;
        if content.len() > self.limits().max_result_bytes {
            return Err(ToolError::Sdk {
                sdk_kind: "response_too_large".to_string(),
                message: "state read result exceeded max result bytes".to_string(),
            });
        }
        Ok(ReadFileResult {
            path: path.as_str().to_string(),
            bytes: content.len(),
            content,
        })
    }

    pub async fn append_file(
        &self,
        path: &VirtualPath,
        content: &str,
    ) -> Result<MutationResult, ToolError> {
        let existing = match self.read_file(path).await {
            Ok(file) => file.content,
            Err(err) if err.kind() == "not_found" => String::new(),
            Err(err) => return Err(err),
        };
        let next = format!("{existing}{content}");
        self.write_file(path, &next).await?;
        Ok(MutationResult {
            ok: true,
            path: path.as_str().to_string(),
        })
    }

    pub async fn read_json(&self, path: &VirtualPath) -> Result<JsonReadResult, ToolError> {
        let file = self.read_file(path).await?;
        let value = serde_json::from_str(&file.content).map_err(|err| ToolError::InvalidParam {
            message: format!("state file is not valid JSON: {err}"),
            param: "path".to_string(),
        })?;
        Ok(JsonReadResult {
            path: path.as_str().to_string(),
            value,
        })
    }

    pub async fn write_json(
        &self,
        path: &VirtualPath,
        value: &serde_json::Value,
        pretty: bool,
    ) -> Result<(), ToolError> {
        let mut content = if pretty {
            serde_json::to_string_pretty(value).map_err(serialize_error)?
        } else {
            serde_json::to_string(value).map_err(serialize_error)?
        };
        content.push('\n');
        self.write_file(path, &content).await
    }

    pub async fn hash_file(
        &self,
        path: &VirtualPath,
        algorithm: &str,
    ) -> Result<HashFileResult, ToolError> {
        if algorithm != "sha256" {
            return Err(ToolError::InvalidParam {
                message: "state hashFile only supports sha256".to_string(),
                param: "algorithm".to_string(),
            });
        }
        let bytes = self.read_file_bytes(path).await?;
        Ok(HashFileResult {
            path: path.as_str().to_string(),
            algorithm: algorithm.to_string(),
            hex: hex::encode(Sha256::digest(&bytes)),
            bytes: bytes.len(),
        })
    }

    pub async fn detect_file(&self, path: &VirtualPath) -> Result<DetectFileResult, ToolError> {
        let bytes = self.read_file_bytes(path).await?;
        let extension = Path::new(path.as_str())
            .extension()
            .map(|value| value.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let text = std::str::from_utf8(&bytes).is_ok();
        let json =
            extension == "json" || serde_json::from_slice::<serde_json::Value>(&bytes).is_ok();
        Ok(DetectFileResult {
            path: path.as_str().to_string(),
            extension,
            text,
            json,
            bytes: bytes.len(),
        })
    }

    pub(crate) async fn read_file_bytes(&self, path: &VirtualPath) -> Result<Vec<u8>, ToolError> {
        let destination = self.resolve(path);
        self.ensure_path_allowed(&destination).await?;
        self.read_bounded_path(&destination, "open state file")
            .await
    }

    pub(crate) async fn read_bounded_path(
        &self,
        path: &Path,
        action: &'static str,
    ) -> Result<Vec<u8>, ToolError> {
        let file = tokio::fs::File::open(path)
            .await
            .map_err(not_found_or_internal(action))?;
        let mut bytes = Vec::new();
        file.take(self.limits().max_file_bytes as u64 + 1)
            .read_to_end(&mut bytes)
            .await
            .map_err(internal_io("read state file"))?;
        if bytes.len() > self.limits().max_file_bytes {
            return Err(ToolError::Sdk {
                sdk_kind: "response_too_large".to_string(),
                message: "state file exceeded max readable bytes".to_string(),
            });
        }
        Ok(bytes)
    }
}
