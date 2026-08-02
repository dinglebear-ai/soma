use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use crate::{FleetError, FleetResult};

/// Creates an owner-only runtime subdirectory for control and forwarding sockets.
pub(crate) fn secure_runtime_subdir(name: &str) -> FleetResult<PathBuf> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(FleetError::Connection {
            host: crate::HostId::new("runtime").expect("static host id"),
            message: "invalid runtime subdirectory name".into(),
        });
    }
    let uid = rustix::process::getuid().as_raw();
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join(format!("soma-fleet-{uid}")));
    let root = base.join("soma-fleet");
    let directory = root.join(name);
    secure_directory(&root, uid)?;
    secure_directory(&directory, uid)?;
    Ok(directory)
}

fn secure_directory(path: &Path, uid: u32) -> FleetResult<()> {
    if let Ok(metadata) = std::fs::symlink_metadata(path)
        && metadata.file_type().is_symlink()
    {
        return runtime_error(path, "runtime directory is a symbolic link");
    }
    std::fs::create_dir_all(path)
        .map_err(|error| runtime_error_value(path, format!("create failed: {error}")))?;
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| runtime_error_value(path, format!("metadata failed: {error}")))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return runtime_error(path, "runtime path is not a real directory");
    }
    if metadata.uid() != uid {
        return runtime_error(path, "runtime directory is not owned by the current user");
    }
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .map_err(|error| runtime_error_value(path, format!("chmod 0700 failed: {error}")))?;
    Ok(())
}

fn runtime_error<T>(path: &Path, message: &str) -> FleetResult<T> {
    Err(runtime_error_value(path, message.to_owned()))
}

fn runtime_error_value(path: &Path, message: String) -> FleetError {
    FleetError::Connection {
        host: crate::HostId::new("runtime").expect("static host id"),
        message: format!("{}: {message}", path.display()),
    }
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
