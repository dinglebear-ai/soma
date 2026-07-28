//! Python-provider concerns for the file-backed provider source: interpreter
//! selection, immutable prepared-environment selections, dependency scanning,
//! and environment fingerprinting. Split out of `filesystem.rs` to stay under
//! the module size hard limit.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};
use soma_provider_adapters::python::{
    lifecycle::PythonEnvironmentLifecycle,
    materializer::{PreparedPythonEnvironment, UvRunner},
    PythonInterpreter,
};

use super::{FileProviderLoadError, FileProviderSource};

/// Prepares an immutable Python environment before a provider candidate is
/// imported for catalog discovery.
pub trait PythonProviderEnvironmentPreparer: Send + Sync {
    /// Returns the interpreter selected for `provider_path` after preparing its environment.
    fn prepare(&self, provider_path: &Path) -> Result<PythonInterpreter, String>;

    /// Revalidates an immutable candidate against the current provider source and cache.
    fn validate_candidate(
        &self,
        provider_path: &Path,
        candidate: &PreparedPythonEnvironment,
    ) -> Result<PythonInterpreter, String>;
}

impl<R> PythonProviderEnvironmentPreparer for PythonEnvironmentLifecycle<R>
where
    R: UvRunner + 'static,
{
    fn prepare(&self, provider_path: &Path) -> Result<PythonInterpreter, String> {
        self.prepare_provider(provider_path)
            .map(|prepared| PythonInterpreter::prepared(&prepared))
            .map_err(|error| error.to_string())
    }

    fn validate_candidate(
        &self,
        provider_path: &Path,
        candidate: &PreparedPythonEnvironment,
    ) -> Result<PythonInterpreter, String> {
        self.validate_provider_candidate(provider_path, candidate)
            .map(|prepared| PythonInterpreter::prepared(&prepared))
            .map_err(|error| error.to_string())
    }
}

/// Immutable prepared-environment selections keyed by managed Python provider path.
pub type PythonProviderEnvironmentSelections = BTreeMap<PathBuf, PreparedPythonEnvironment>;

impl FileProviderSource {
    /// Resolves an exact managed `.py` provider path under this source.
    pub fn resolve_python_provider_path(
        &self,
        provider_path: &Path,
    ) -> Result<PathBuf, FileProviderLoadError> {
        let requested = if provider_path.is_absolute() {
            provider_path.to_path_buf()
        } else {
            self.root.join(provider_path)
        };
        self.provider_paths()?
            .into_iter()
            .find(|path| path == &requested && is_python_provider_source(path))
            .ok_or_else(|| FileProviderLoadError {
                path: requested,
                message: "Python provider path is not a managed provider source".to_owned(),
            })
    }

    pub(super) fn validate_python_environment_selections(
        &self,
        selections: &PythonProviderEnvironmentSelections,
    ) -> Result<(), FileProviderLoadError> {
        if selections.is_empty() {
            return Ok(());
        }
        let managed = self
            .provider_paths()?
            .into_iter()
            .filter(|path| is_python_provider_source(path))
            .collect::<BTreeSet<_>>();
        if let Some(path) = selections.keys().find(|path| !managed.contains(*path)) {
            return Err(FileProviderLoadError {
                path: path.clone(),
                message: "Python environment selection is not a managed provider source".to_owned(),
            });
        }
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn python_interpreter(
        &self,
        path: &Path,
    ) -> Result<PythonInterpreter, FileProviderLoadError> {
        self.python_interpreter_with_environments(path, &PythonProviderEnvironmentSelections::new())
    }

    pub(super) fn python_interpreter_with_environments(
        &self,
        path: &Path,
        selections: &PythonProviderEnvironmentSelections,
    ) -> Result<PythonInterpreter, FileProviderLoadError> {
        if !is_python_provider_source(path) {
            return Ok(PythonInterpreter::Ambient);
        }
        if let Some(candidate) = selections.get(path) {
            let preparer =
                self.python_environment_preparer
                    .as_ref()
                    .ok_or_else(|| FileProviderLoadError {
                        path: path.to_path_buf(),
                        message: "Python candidate validation requires an environment preparer"
                            .to_owned(),
                    })?;
            return preparer
                .validate_candidate(path, candidate)
                .map_err(|source| FileProviderLoadError {
                    path: path.to_path_buf(),
                    message: format!("failed to validate Python provider candidate: {source}"),
                });
        }
        self.python_environment_preparer.as_ref().map_or(
            Ok(PythonInterpreter::Ambient),
            |preparer| {
                preparer
                    .prepare(path)
                    .map_err(|source| FileProviderLoadError {
                        path: path.to_path_buf(),
                        message: format!("failed to prepare Python provider environment: {source}"),
                    })
            },
        )
    }
}

pub(super) fn collect_python_dependency_paths(
    root: &Path,
    paths: &mut BTreeSet<PathBuf>,
) -> Result<(), FileProviderLoadError> {
    if !root.exists() {
        return Ok(());
    }
    collect_python_dependency_paths_inner(root, paths)
}

fn collect_python_dependency_paths_inner(
    dir: &Path,
    paths: &mut BTreeSet<PathBuf>,
) -> Result<(), FileProviderLoadError> {
    let entries = fs::read_dir(dir).map_err(|source| FileProviderLoadError {
        path: dir.to_path_buf(),
        message: format!("failed to read provider dependency directory: {source}"),
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| FileProviderLoadError {
            path: dir.to_path_buf(),
            message: format!("failed to read provider dependency directory entry: {source}"),
        })?;
        let path = entry.path();
        if path.is_dir() {
            if should_scan_dependency_dir(&path) {
                collect_python_dependency_paths_inner(&path, paths)?;
            }
            continue;
        }
        if path.is_file() && is_python_dependency_file(&path) {
            paths.insert(path);
        }
    }
    Ok(())
}

fn should_scan_dependency_dir(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    !matches!(
        name,
        "__pycache__"
            | ".git"
            | ".mypy_cache"
            | ".pytest_cache"
            | ".ruff_cache"
            | ".venv"
            | "venv"
            | "node_modules"
            | "target"
            | "dist"
            | "build"
    )
}

fn is_python_dependency_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("py" | "pyi")
    )
}

pub(super) fn is_python_provider_source(path: &Path) -> bool {
    path.extension().and_then(|extension| extension.to_str()) == Some("py")
}

pub(super) fn fingerprint_python_environment(
    hasher: &mut Sha256,
    root: &Path,
    path: &Path,
    candidate: &PreparedPythonEnvironment,
) {
    let label = path
        .strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string();
    hasher.update(b"python-environment\0");
    hasher.update(label.as_bytes());
    hasher.update([0]);
    for value in [
        candidate.key.as_str(),
        candidate.lock_sha256.as_str(),
        candidate
            .provider_source_sha256
            .as_deref()
            .unwrap_or_default(),
        candidate.input_plan_key.as_deref().unwrap_or_default(),
    ] {
        hasher.update(value.len().to_le_bytes());
        hasher.update(value.as_bytes());
        hasher.update([0]);
    }
}

#[cfg(test)]
#[path = "filesystem_python_tests.rs"]
mod tests;
