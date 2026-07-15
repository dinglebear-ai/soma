use std::path::{Component, Path, PathBuf};

use crate::ToolError;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VirtualPath(String);

impl VirtualPath {
    pub fn parse(raw: &str) -> Result<Self, ToolError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed == "/" {
            return Err(invalid_path(
                "state path must name a file or directory inside the workspace",
            ));
        }
        let normalized = normalize(trimmed, false)?;
        Ok(Self(normalized))
    }

    pub fn parse_read_scope(raw: &str) -> Result<Self, ToolError> {
        Ok(Self(normalize(raw, true)?))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub fn state_root() -> PathBuf {
    crate::soma_home().join(".soma-state")
}

fn normalize(raw: &str, allow_root: bool) -> Result<String, ToolError> {
    let trimmed = raw.trim();
    if allow_root && (trimmed.is_empty() || trimmed == "." || trimmed == "/") {
        return Ok(String::new());
    }
    let replaced = trimmed.replace('\\', "/");
    if has_windows_drive_prefix(&replaced) {
        return Err(path_traversal(raw));
    }
    let stripped = replaced.trim_start_matches('/');
    let mut parts = Vec::new();
    for component in Path::new(stripped).components() {
        match component {
            Component::Normal(value) => parts.push(value.to_string_lossy().to_string()),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(path_traversal(raw));
            }
        }
    }
    let value = parts.join("/");
    if value.is_empty() {
        return Err(invalid_path(
            "state path must name a file or directory inside the workspace",
        ));
    }
    reject_credential_like_path(&value)?;
    Ok(value)
}

pub fn is_reserved_metadata_path(path: &str) -> bool {
    path.split('/').any(|part| {
        part.eq_ignore_ascii_case(".git")
            || part.eq_ignore_ascii_case(".soma-state")
            || part == concat!(".la", "bby-state")
    })
}

fn reject_credential_like_path(path: &str) -> Result<(), ToolError> {
    let lower = path.to_ascii_lowercase();
    if is_reserved_metadata_path(&lower) {
        return Err(ToolError::Sdk {
            sdk_kind: "permission_denied".to_string(),
            message: "state path is reserved runtime metadata".to_string(),
        });
    }
    let denied = [
        ".env",
        ".ssh/",
        ".aws/",
        ".config/gcloud/",
        ".netrc",
        "id_rsa",
        "id_ed25519",
    ];
    if denied
        .iter()
        .any(|needle| lower == *needle || lower.contains(needle))
    {
        return Err(ToolError::Sdk {
            sdk_kind: "permission_denied".to_string(),
            message: "state path is denied because it looks credential-related".to_string(),
        });
    }
    Ok(())
}

fn has_windows_drive_prefix(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn invalid_path(message: &str) -> ToolError {
    ToolError::InvalidParam {
        message: message.to_string(),
        param: "path".to_string(),
    }
}

fn path_traversal(raw: &str) -> ToolError {
    ToolError::Sdk {
        sdk_kind: "path_traversal".to_string(),
        message: format!("state path `{raw}` escapes the workspace"),
    }
}
