use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::ToolError;

const RUNNER_EXE_ENV: &str = "SOMA_CODE_MODE_RUNNER_EXE";

pub fn resolve_runner_exe() -> Result<PathBuf, ToolError> {
    let current = std::env::current_exe().map_err(|err| {
        ToolError::internal_message(format!(
            "failed to locate current executable for Code Mode runner: {err}"
        ))
    })?;
    let override_exe = std::env::var_os(RUNNER_EXE_ENV).map(PathBuf::from);
    resolve_runner_exe_from(current, override_exe)
}

pub fn resolve_runner_exe_from(
    current_exe: PathBuf,
    override_exe: Option<PathBuf>,
) -> Result<PathBuf, ToolError> {
    if let Some(path) = override_exe {
        let path = validate_operator_override(path)?;
        tracing::warn!(
            runner_exe = %path.display(),
            "using SOMA_CODE_MODE_RUNNER_EXE override for Code Mode runner"
        );
        return Ok(path);
    }

    if is_runner_binary_path(&current_exe) && is_usable_exe(&current_exe) {
        return Ok(current_exe);
    }
    for candidate in sibling_runner_candidates(&current_exe) {
        if is_usable_exe(&candidate) {
            return Ok(candidate);
        }
    }

    Err(ToolError::internal_message(format!(
        "Code Mode runner executable is stale or unavailable near `{}`; restart the soma service or set SOMA_CODE_MODE_RUNNER_EXE to a validated soma-codemode-runner binary",
        current_exe.display()
    )))
}

fn validate_operator_override(path: PathBuf) -> Result<PathBuf, ToolError> {
    if !path.is_absolute() {
        return Err(ToolError::Sdk {
            sdk_kind: "invalid_param".to_string(),
            message: "SOMA_CODE_MODE_RUNNER_EXE must be an absolute path".to_string(),
        });
    }
    let canonical = std::fs::canonicalize(&path).map_err(|err| {
        ToolError::internal_message(format!(
            "SOMA_CODE_MODE_RUNNER_EXE points at `{}`, but it cannot be resolved: {err}",
            path.display()
        ))
    })?;
    if !is_usable_exe(&canonical) {
        return Err(ToolError::internal_message(format!(
            "SOMA_CODE_MODE_RUNNER_EXE points at `{}`, but that file is not executable",
            canonical.display()
        )));
    }
    reject_untrusted_permissions(&canonical)?;
    Ok(canonical)
}

fn is_usable_exe(path: &Path) -> bool {
    if path.to_string_lossy().ends_with(" (deleted)") {
        return false;
    }
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn sibling_runner_candidates(current_exe: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(dir) = current_exe.parent() {
        candidates.push(dir.join(runner_binary_name()));
        if dir.file_name() == Some(OsStr::new("deps")) {
            if let Some(parent) = dir.parent() {
                candidates.push(parent.join(runner_binary_name()));
            }
        }
    }
    candidates
}

fn is_runner_binary_path(path: &Path) -> bool {
    path.file_name() == Some(OsStr::new(runner_binary_name()))
}

fn runner_binary_name() -> &'static str {
    if cfg!(windows) {
        "soma-codemode-runner.exe"
    } else {
        "soma-codemode-runner"
    }
}

fn reject_untrusted_permissions(path: &Path) -> Result<(), ToolError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let meta = std::fs::metadata(path).map_err(|err| {
            ToolError::internal_message(format!("failed to inspect `{}`: {err}", path.display()))
        })?;
        if meta.mode() & 0o022 != 0 {
            return Err(ToolError::internal_message(format!(
                "SOMA_CODE_MODE_RUNNER_EXE points at `{}`, but the file is group/world writable",
                path.display()
            )));
        }
        let current_uid = nix::unistd::Uid::current().as_raw();
        if meta.uid() != current_uid && meta.uid() != 0 {
            return Err(ToolError::internal_message(format!(
                "SOMA_CODE_MODE_RUNNER_EXE points at `{}`, but the file is not owned by the current user or root",
                path.display()
            )));
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}
