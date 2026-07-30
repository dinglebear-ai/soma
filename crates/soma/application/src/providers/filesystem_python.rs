//! Python-provider concerns for the file-backed provider source: interpreter
//! selection, immutable prepared-environment selections, dependency scanning,
//! and environment fingerprinting. Split out of `filesystem.rs` to stay under
//! the module size hard limit.
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime},
};

use async_trait::async_trait;
use serde_json::Value;
use sha2::{Digest, Sha256};
use soma_provider_adapters::python::{
    PythonInterpreter,
    lifecycle::PythonEnvironmentLifecycle,
    materializer::{PreparedPythonEnvironment, UvRunner},
};
use soma_provider_core::{ProviderCatalog, ProviderOutput};

use super::{FileProviderLoadError, FileProviderSource};
use crate::{
    provider_errors::ProviderError,
    provider_registry::{Provider, ProviderCall},
};

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

const MAX_GENERATION_FILES: usize = 4_096;
const MAX_GENERATION_BYTES: u64 = 64 * 1024 * 1024;
const STALE_GENERATION_STORE_AGE: Duration = Duration::from_secs(24 * 60 * 60);
static GENERATION_LEASES: OnceLock<Mutex<HashMap<PathBuf, Weak<PythonGenerationLease>>>> =
    OnceLock::new();
static GENERATION_STORE_INITIALIZED: OnceLock<()> = OnceLock::new();

pub(super) struct ImmutablePythonSource {
    pub(super) path: PathBuf,
    pub(super) lease: Arc<PythonGenerationLease>,
}

pub(super) struct PythonGenerationLease {
    generation: PathBuf,
}

impl Drop for PythonGenerationLease {
    fn drop(&mut self) {
        if let Some(leases) = GENERATION_LEASES.get() {
            let mut leases = leases
                .lock()
                .expect("Python generation lease lock should not be poisoned");
            leases.remove(&self.generation);
            let _ = fs::remove_dir_all(&self.generation);
            return;
        }
        let _ = fs::remove_dir_all(&self.generation);
    }
}

struct SnapshotRetainedProvider {
    inner: Arc<dyn Provider>,
    _lease: Arc<PythonGenerationLease>,
}

pub(super) fn retain_python_snapshot(
    inner: Arc<dyn Provider>,
    lease: Arc<PythonGenerationLease>,
) -> Arc<dyn Provider> {
    Arc::new(SnapshotRetainedProvider {
        inner,
        _lease: lease,
    })
}

#[async_trait]
impl Provider for SnapshotRetainedProvider {
    fn catalog(&self) -> ProviderCatalog {
        self.inner.catalog()
    }

    async fn call(&self, call: ProviderCall) -> Result<ProviderOutput, ProviderError> {
        self.inner.call(call).await
    }

    async fn retire(&self) {
        self.inner.retire().await;
    }

    async fn suspend(&self) {
        self.inner.suspend().await;
    }

    fn runtime_status(&self) -> Option<Value> {
        self.inner.runtime_status()
    }

    fn cancel_active(&self) -> bool {
        self.inner.cancel_active()
    }

    async fn reset_quarantine(&self) {
        self.inner.reset_quarantine().await;
    }

    fn deactivate(&self) {
        self.inner.deactivate();
    }

    fn activate(&self) {
        self.inner.activate();
    }

    fn acquire_dispatch(&self) -> bool {
        self.inner.acquire_dispatch()
    }

    fn release_dispatch(&self) {
        self.inner.release_dispatch();
    }
}

pub(super) fn immutable_python_source(
    provider_root: &Path,
    path: &Path,
) -> Result<ImmutablePythonSource, FileProviderLoadError> {
    static STAGING_SEQUENCE: AtomicU64 = AtomicU64::new(1);
    let relative_source = path
        .strip_prefix(provider_root)
        .map_err(|_| FileProviderLoadError {
            path: path.to_path_buf(),
            message: "Python provider source is outside its managed root".to_owned(),
        })?;
    let mut paths = BTreeSet::new();
    collect_python_dependency_paths(provider_root, &mut paths)?;
    validate_generation_limits(provider_root, &paths)?;
    let digest = python_tree_digest(provider_root, &paths)?;
    let generation_store =
        std::env::temp_dir().join(format!("soma-python-generations.{}", std::process::id()));
    fs::create_dir_all(&generation_store).map_err(|error| FileProviderLoadError {
        path: generation_store.clone(),
        message: format!("failed to create Python generation store: {error}"),
    })?;
    secure_generation_store(&generation_store)?;
    GENERATION_STORE_INITIALIZED.get_or_init(|| {
        if let Ok(entries) = fs::read_dir(&generation_store) {
            for entry in entries.flatten() {
                let path = entry.path();
                let _ = if path.is_dir() {
                    fs::remove_dir_all(path)
                } else {
                    fs::remove_file(path)
                };
            }
        }
    });
    cleanup_stale_generation_stores(&generation_store);
    let generation = generation_store.join(&digest);
    let snapshot = generation.join("tree").join(relative_source);
    if generation.exists() {
        verify_immutable_generation(&generation, &digest)?;
        return Ok(ImmutablePythonSource {
            path: snapshot,
            lease: generation_lease(generation)?,
        });
    }
    let staging = generation_store.join(format!(
        ".{digest}.{}.{}.staging",
        std::process::id(),
        STAGING_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let staging_tree = staging.join("tree");
    let staged = (|| {
        for source in &paths {
            let relative = source
                .strip_prefix(provider_root)
                .expect("collected dependency must remain under provider root");
            let destination = staging_tree.join(relative);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).map_err(|error| FileProviderLoadError {
                    path: parent.to_path_buf(),
                    message: format!("failed to create Python generation directory: {error}"),
                })?;
            }
            fs::copy(source, &destination).map_err(|error| FileProviderLoadError {
                path: destination,
                message: format!("failed to snapshot Python generation source: {error}"),
            })?;
        }
        verify_immutable_generation(&staging, &digest)?;
        Ok::<(), FileProviderLoadError>(())
    })();
    if let Err(error) = staged {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    match fs::rename(&staging, &generation) {
        Ok(()) => Ok(ImmutablePythonSource {
            path: snapshot,
            lease: generation_lease(generation)?,
        }),
        Err(_) if generation.exists() => {
            let _ = fs::remove_dir_all(&staging);
            verify_immutable_generation(&generation, &digest)?;
            Ok(ImmutablePythonSource {
                path: snapshot,
                lease: generation_lease(generation)?,
            })
        }
        Err(error) => {
            let _ = fs::remove_dir_all(&staging);
            Err(FileProviderLoadError {
                path: generation,
                message: format!("failed to publish Python generation snapshot: {error}"),
            })
        }
    }
}

fn generation_lease(
    generation: PathBuf,
) -> Result<Arc<PythonGenerationLease>, FileProviderLoadError> {
    let mut leases = GENERATION_LEASES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .expect("Python generation lease lock should not be poisoned");
    if let Some(lease) = leases.get(&generation).and_then(Weak::upgrade) {
        return Ok(lease);
    }
    if !generation.is_dir() {
        return Err(FileProviderLoadError {
            path: generation,
            message: "Python generation snapshot disappeared during acquisition".to_owned(),
        });
    }
    let lease = Arc::new(PythonGenerationLease {
        generation: generation.clone(),
    });
    leases.insert(generation, Arc::downgrade(&lease));
    Ok(lease)
}

fn cleanup_stale_generation_stores(active_store: &Path) {
    let Some(parent) = active_store.parent() else {
        return;
    };
    let Ok(entries) = fs::read_dir(parent) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        if path == active_store
            || !entry
                .file_name()
                .to_string_lossy()
                .starts_with("soma-python-generations.")
        {
            continue;
        }
        let stale = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age >= STALE_GENERATION_STORE_AGE);
        if stale && stale_store_process_is_gone(&entry.file_name().to_string_lossy()) {
            let _ = fs::remove_dir_all(path);
        }
    }
}

#[cfg(target_os = "linux")]
fn stale_store_process_is_gone(name: &str) -> bool {
    let Some(pid) = name
        .strip_prefix("soma-python-generations.")
        .and_then(|pid| pid.parse::<u32>().ok())
    else {
        return false;
    };
    !Path::new("/proc").join(pid.to_string()).exists()
}

#[cfg(not(target_os = "linux"))]
fn stale_store_process_is_gone(_name: &str) -> bool {
    false
}

fn validate_generation_limits(
    root: &Path,
    paths: &BTreeSet<PathBuf>,
) -> Result<(), FileProviderLoadError> {
    if paths.len() > MAX_GENERATION_FILES {
        return Err(FileProviderLoadError {
            path: root.to_path_buf(),
            message: format!(
                "Python generation tree exceeds the {MAX_GENERATION_FILES}-file limit"
            ),
        });
    }
    let mut total = 0_u64;
    for path in paths {
        total = total.saturating_add(
            fs::metadata(path)
                .map_err(|error| FileProviderLoadError {
                    path: path.clone(),
                    message: format!("failed to inspect Python generation input: {error}"),
                })?
                .len(),
        );
        if total > MAX_GENERATION_BYTES {
            return Err(FileProviderLoadError {
                path: root.to_path_buf(),
                message: format!(
                    "Python generation tree exceeds the {MAX_GENERATION_BYTES}-byte limit"
                ),
            });
        }
    }
    Ok(())
}

fn secure_generation_store(path: &Path) -> Result<(), FileProviderLoadError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| FileProviderLoadError {
        path: path.to_path_buf(),
        message: format!("failed to inspect Python generation store: {error}"),
    })?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
        return Err(FileProviderLoadError {
            path: path.to_path_buf(),
            message: "Python generation store is not a private directory".to_owned(),
        });
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|error| {
            FileProviderLoadError {
                path: path.to_path_buf(),
                message: format!("failed to secure Python generation store: {error}"),
            }
        })?;
    }
    Ok(())
}

fn python_tree_digest(
    root: &Path,
    paths: &BTreeSet<PathBuf>,
) -> Result<String, FileProviderLoadError> {
    let mut hasher = Sha256::new();
    for path in paths {
        let relative = path.strip_prefix(root).map_err(|_| FileProviderLoadError {
            path: path.clone(),
            message: "Python dependency is outside its managed root".to_owned(),
        })?;
        let bytes = fs::read(path).map_err(|error| FileProviderLoadError {
            path: path.clone(),
            message: format!("failed to read Python generation source: {error}"),
        })?;
        let label = relative.to_string_lossy();
        hasher.update(label.len().to_le_bytes());
        hasher.update(label.as_bytes());
        hasher.update(bytes.len().to_le_bytes());
        hasher.update(bytes);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn verify_immutable_generation(
    generation: &Path,
    expected: &str,
) -> Result<(), FileProviderLoadError> {
    let tree = generation.join("tree");
    let mut paths = BTreeSet::new();
    collect_python_dependency_paths(&tree, &mut paths)?;
    validate_generation_limits(&tree, &paths)?;
    let actual = python_tree_digest(&tree, &paths)?;
    if actual != expected {
        return Err(FileProviderLoadError {
            path: generation.to_path_buf(),
            message: "Python generation snapshot digest mismatch".to_owned(),
        });
    }
    Ok(())
}

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
        let metadata = fs::symlink_metadata(&path).map_err(|source| FileProviderLoadError {
            path: path.clone(),
            message: format!("failed to inspect provider dependency entry: {source}"),
        })?;
        if metadata.file_type().is_symlink() {
            return Err(FileProviderLoadError {
                path,
                message: "Python generation inputs must not contain symbolic links".to_owned(),
            });
        }
        if metadata.is_dir() {
            if should_scan_dependency_dir(&path) {
                collect_python_dependency_paths_inner(&path, paths)?;
            }
            continue;
        }
        if metadata.is_file() {
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
