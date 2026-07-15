use std::path::{Component, Path, PathBuf};

use crate::ToolError;

pub fn reject_path_traversal(rel_path: &str) -> Result<(), ToolError> {
    let normalized = rel_path.replace('\\', "/");
    for component in Path::new(&normalized).components() {
        if matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        ) {
            return Err(path_error(format!(
                "path traversal rejected: `{rel_path}` must be relative"
            )));
        }
    }
    for denied in [".git", ".soma-state", legacy_state_dir()] {
        if normalized.split('/').any(|part| part == denied) {
            return Err(path_error(format!(
                "path `{rel_path}` targets reserved state"
            )));
        }
    }
    Ok(())
}

fn legacy_state_dir() -> &'static str {
    concat!(".la", "bby-state")
}

pub fn rel_to_unix_string(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

pub fn reject_existing_symlink_ancestors(
    write_root: &Path,
    target: &Path,
) -> Result<(), ToolError> {
    let root = normalize_lexical(write_root);
    let target = normalize_lexical(target);
    if !target.starts_with(&root) {
        return Err(path_error(format!(
            "target path `{}` escapes write root `{}`",
            target.display(),
            root.display()
        )));
    }
    let mut current = root.clone();
    reject_existing_symlink(&current)?;
    let relative = target.strip_prefix(&root).map_err(|_| {
        path_error(format!(
            "target path `{}` escapes write root `{}`",
            target.display(),
            root.display()
        ))
    })?;
    for component in relative.components() {
        current.push(component.as_os_str());
        reject_existing_symlink(&current)?;
    }
    Ok(())
}

pub fn reject_existing_symlinks_in_path(path: &Path) -> Result<(), ToolError> {
    let path = normalize_lexical(path);
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        reject_existing_symlink(&current)?;
    }
    Ok(())
}

fn reject_existing_symlink(path: &Path) -> Result<(), ToolError> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(symlink_error(format!(
            "refusing to operate through symlink `{}`",
            path.display()
        ))),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(ToolError::internal_message(format!(
            "lstat failed for `{}`: {error}",
            path.display()
        ))),
    }
}

fn normalize_lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn path_error(message: String) -> ToolError {
    ToolError::Sdk {
        sdk_kind: "path_traversal".into(),
        message,
    }
}

fn symlink_error(message: String) -> ToolError {
    ToolError::Sdk {
        sdk_kind: "symlink_rejected".into(),
        message,
    }
}
