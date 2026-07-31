//! Honest Python-to-Rust/component graduation workflow.
//!
//! The workflow scaffolds adapters and verifies recorded behavior. It never
//! claims to translate arbitrary Python business logic. Candidate publication,
//! attestation, activation, and rollback are serialized and digest-bound.

use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use atomicwrites::{AllowOverwrite, AtomicFile};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use soma_provider_core::{ProviderCatalog, ProviderKind};

const STATE_SCHEMA_VERSION: u32 = 3;
const TRANSACTION_DIR: &str = ".graduation-transaction";
const MAX_SOURCE_BYTES: usize = 1024 * 1024;
const MAX_FIXTURE_BYTES: usize = 4 * 1024 * 1024;
const MAX_COMPONENT_BYTES: usize = 64 * 1024 * 1024;
const MAX_RECOVERY_DEPTH: usize = 8;
const MAX_RECOVERY_DIRECTORIES: usize = 4_096;
const MAX_RECOVERY_ENTRIES: usize = 4_096;

mod build;
mod comparison;
mod recovery;
mod state;
mod transaction;
use build::run_isolated_component_build;
pub use comparison::{ComparisonRequest, GraduationFixture, compare};
pub(crate) use comparison::{read_fixture_snapshot, read_fixtures};
pub use recovery::{recover, recover_all};
use state::{read_state, validate_state_paths, write_state, write_state_at};
use transaction::{
    AmbiguousCommitError, begin_transaction, finish_transaction, recover_transaction,
    remove_committed_tombstone,
};

/// Immutable component identity retained in the graduation workspace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GraduationArtifact {
    /// Immutable artifact path in the graduation workspace.
    pub path: PathBuf,
    /// Lowercase SHA-256 digest of the artifact bytes.
    pub sha256: String,
}

/// Successful, digest-bound conformance evidence required by activation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ConformanceAttestation {
    /// Digest of the candidate artifact that was exercised.
    pub artifact_sha256: String,
    /// Digest of the exact fixture corpus used for comparison.
    pub fixtures_sha256: String,
    /// Number of successfully matched fixtures.
    pub fixture_count: usize,
    /// Digest of the exact live Python source exercised during dual-run.
    pub source_sha256: String,
    /// Canonical provider contract digest exercised by both runtimes.
    pub catalog_sha256: String,
    /// Unix timestamp in milliseconds when comparison completed.
    pub verified_unix_ms: u64,
}

/// Durable state for one graduation workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraduationState {
    /// Version of this persisted state schema.
    pub schema_version: u32,
    /// Canonical path of the original Python provider.
    pub source: PathBuf,
    /// Digest of the Python source at scaffold time.
    pub source_sha256: String,
    /// Canonical digest of tools, schemas, annotations, and capabilities.
    pub catalog_sha256: String,
    /// Captured provider catalog used to preserve the public contract.
    pub catalog: ProviderCatalog,
    /// Built and verified component awaiting conformance and activation.
    pub candidate: Option<GraduationArtifact>,
    /// Component currently published in the live provider directory.
    pub active: Option<GraduationArtifact>,
    /// Previously active component retained for rollback.
    pub previous: Option<GraduationArtifact>,
    /// Backup path holding the original Python provider while Wasm is active.
    pub python_backup: Option<PathBuf>,
    /// Digest-bound proof that the candidate matches the fixture corpus.
    pub attestation: Option<ConformanceAttestation>,
}

struct WorkspaceLock(File);

impl WorkspaceLock {
    fn acquire(workspace: &Path) -> anyhow::Result<Self> {
        Self::acquire_before(workspace, Instant::now() + Duration::from_secs(30))
    }

    fn acquire_before(workspace: &Path, deadline: Instant) -> anyhow::Result<Self> {
        if !workspace.is_dir() {
            anyhow::bail!(
                "graduation workspace does not exist: {}",
                workspace.display()
            );
        }
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(workspace.join(".graduation.lock"))?;
        loop {
            match file.try_lock_exclusive() {
                Ok(()) => return Ok(Self(file)),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        anyhow::bail!("graduation workspace lock deadline expired");
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => return Err(error.into()),
            }
        }
    }
}

impl Drop for WorkspaceLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.0);
    }
}

/// Scaffold a reusable Rust core plus thin PyO3 and WIT adapters.
pub fn graduate(
    source: &Path,
    workspace: &Path,
    fixtures: Option<&Path>,
    mut catalog: ProviderCatalog,
    provider_root: &Path,
) -> anyhow::Result<Value> {
    if source.extension().and_then(|value| value.to_str()) != Some("py") || !source.is_file() {
        anyhow::bail!("graduation source must be an existing .py provider");
    }
    if workspace.exists() {
        anyhow::bail!(
            "graduation workspace already exists: {}",
            workspace.display()
        );
    }
    let source = source.canonicalize()?;
    let provider_root = provider_root.canonicalize()?;
    if !source.starts_with(&provider_root) {
        anyhow::bail!("graduation source is outside the managed provider root");
    }
    let source_bytes = read_bounded(&source, MAX_SOURCE_BYTES, "Python source")?;
    let source_sha256 = digest_bytes(&source_bytes);
    if !matches!(
        catalog.provider.kind,
        ProviderKind::Python | ProviderKind::Langchain | ProviderKind::Llamaindex
    ) {
        anyhow::bail!("graduation source is not an active Python provider");
    }
    catalog.provider.source = Some(source.display().to_string());
    let catalog_sha256 = catalog_contract_digest(&catalog)?;

    let parent = workspace
        .parent()
        .ok_or_else(|| anyhow::anyhow!("graduation workspace requires a parent"))?;
    fs::create_dir_all(parent)?;
    let staging = tempfile::Builder::new()
        .prefix(".soma-graduate-")
        .tempdir_in(parent)?;
    let staging_path = staging.path();
    fs::create_dir_all(staging_path.join("src"))?;
    fs::create_dir_all(staging_path.join("fixtures"))?;
    fs::create_dir_all(staging_path.join("artifacts"))?;
    fs::write(staging_path.join("source.py"), &source_bytes)?;
    let fixture_destination = staging_path.join("fixtures/conformance-v1.json");
    if let Some(fixtures) = fixtures {
        let corpus = read_fixtures(fixtures)?;
        fs::write(&fixture_destination, serde_json::to_vec_pretty(&corpus)?)?;
    } else {
        fs::write(&fixture_destination, b"[]\n")?;
    }
    fs::write(
        staging_path.join("fixtures/README.md"),
        "Record provider/action/arguments selectors and expected JSON results in \
         `conformance-v1.json`. Soma supplies the host-owned execution envelope \
         before comparing or activating a component.\n",
    )?;
    fs::create_dir_all(staging_path.join("wit"))?;
    fs::write(
        staging_path.join("wit/world.wit"),
        include_str!("../../../../wit/soma-provider/world.wit"),
    )?;
    fs::write(
        staging_path.join("Cargo.toml"),
        include_str!("../templates/graduation/Cargo.toml"),
    )?;
    fs::write(
        staging_path.join("src/core.rs"),
        include_str!("../templates/graduation/core.rs"),
    )?;
    fs::write(
        staging_path.join("src/lib.rs"),
        include_str!("../templates/graduation/lib.rs"),
    )?;
    fs::write(
        staging_path.join("src/component.rs"),
        include_str!("../templates/graduation/component.rs"),
    )?;
    fs::write(
        staging_path.join("src/python.rs"),
        include_str!("../templates/graduation/python.rs"),
    )?;
    write_state_at(
        staging_path,
        &GraduationState {
            schema_version: STATE_SCHEMA_VERSION,
            source: source.clone(),
            source_sha256,
            catalog_sha256,
            catalog,
            candidate: None,
            active: None,
            previous: None,
            python_backup: None,
            attestation: None,
        },
    )?;
    let lock_status = Command::new("cargo")
        .args([
            "generate-lockfile",
            "--manifest-path",
            &staging_path.join("Cargo.toml").to_string_lossy(),
        ])
        .status()?;
    if !lock_status.success() {
        anyhow::bail!("graduation lockfile generation failed with status {lock_status}");
    }
    let fetch_status = Command::new("cargo")
        .args([
            "fetch",
            "--locked",
            "--manifest-path",
            &staging_path.join("Cargo.toml").to_string_lossy(),
            "--target",
            "wasm32-wasip2",
        ])
        .status()?;
    if !fetch_status.success() {
        anyhow::bail!("graduation dependency fetch failed with status {fetch_status}");
    }
    let staging = staging.keep();
    fs::rename(&staging, workspace)?;
    sync_parent(workspace)?;
    Ok(json!({
        "ok": true,
        "workspace": workspace,
        "source": source,
        "manual_rewrite_required": true,
        "translated_business_logic": false,
        "fixtures_imported": fixtures.is_some(),
    }))
}

/// Build (or import), verify, and publish an immutable candidate artifact.
pub fn build_component(
    workspace: &Path,
    component: Option<&Path>,
    provider_root: &Path,
) -> anyhow::Result<Value> {
    let _lock = WorkspaceLock::acquire(workspace)?;
    ensure_no_transaction(workspace)?;
    let initial_state = fs::read(workspace.join("graduation.json"))?;
    validate_state_paths(workspace, provider_root, &read_state(workspace)?)?;
    let built_component;
    let component = if let Some(component) = component {
        component
    } else {
        let status = run_isolated_component_build(workspace)?;
        if !status.success() {
            anyhow::bail!("graduated component build failed with status {status}");
        }
        built_component = workspace.join("target/wasm32-wasip2/debug/graduated_soma_provider.wasm");
        &built_component
    };
    if fs::read(workspace.join("graduation.json"))? != initial_state {
        anyhow::bail!("graduation control state changed during component build");
    }
    ensure_no_transaction(workspace)?;
    soma_provider_adapters::wasm::verify_component_artifact(component)
        .map_err(anyhow::Error::msg)?;
    let bytes = read_bounded(component, MAX_COMPONENT_BYTES, "component artifact")?;
    let digest = digest_bytes(&bytes);
    let destination = workspace
        .join("artifacts")
        .join(format!("candidate-{digest}.wasm"));
    if let Ok(existing) = fs::read(&destination)
        && existing != bytes
    {
        anyhow::bail!("candidate digest path already contains different bytes");
    }
    if !destination.exists() {
        atomic_write(&destination, &bytes)?;
        set_read_only(&destination)?;
    }
    verify_artifact(&GraduationArtifact {
        path: destination.clone(),
        sha256: digest.clone(),
    })?;
    let mut state = read_state(workspace)?;
    state.candidate = Some(GraduationArtifact {
        path: destination.clone(),
        sha256: digest.clone(),
    });
    state.attestation = None;
    write_state(workspace, &state)?;
    Ok(json!({"ok": true, "candidate": destination, "sha256": digest}))
}

/// Validate a component artifact against the versioned WIT runtime.
pub fn verify_component(component: &Path) -> anyhow::Result<Value> {
    soma_provider_adapters::wasm::verify_component_artifact(component)
        .map_err(anyhow::Error::msg)?;
    Ok(json!({
        "ok": true,
        "component": component,
        "sha256": digest_file(component)?,
        "wit": "soma:provider@1.0.0"
    }))
}

/// Publish the attested candidate into the live provider directory.
pub fn activate(workspace: &Path, provider_root: &Path) -> anyhow::Result<Value> {
    let _lock = WorkspaceLock::acquire(workspace)?;
    ensure_no_transaction(workspace)?;
    let mut state = read_state(workspace)?;
    validate_state_paths(workspace, provider_root, &state)?;
    let candidate = state
        .candidate
        .clone()
        .ok_or_else(|| anyhow::anyhow!("no verified component candidate exists"))?;
    let attestation = state
        .attestation
        .as_ref()
        .filter(|proof| {
            proof.artifact_sha256 == candidate.sha256
                && proof.source_sha256 == state.source_sha256
                && proof.catalog_sha256 == state.catalog_sha256
        })
        .ok_or_else(|| anyhow::anyhow!("candidate lacks digest-bound conformance attestation"))?;
    if attestation.fixture_count == 0 {
        anyhow::bail!("candidate conformance attestation is empty");
    }
    verify_artifact(&candidate)?;

    let deployed_component = state.source.with_extension("wasm");
    let deployed_manifest = wasm_manifest_path(&deployed_component);
    let backup = state
        .python_backup
        .clone()
        .unwrap_or_else(|| state.source.with_extension("py.soma-backup"));
    begin_transaction(
        workspace,
        &state,
        &deployed_component,
        &deployed_manifest,
        &backup,
    )?;
    let result = (|| {
        if state.active.is_none() {
            if digest_file(&state.source)? != state.source_sha256 {
                anyhow::bail!("Python source changed since graduation was scaffolded");
            }
            if backup.exists() {
                anyhow::bail!("Python source backup already exists: {}", backup.display());
            }
            if deployed_component.exists() || deployed_manifest.exists() {
                anyhow::bail!("refusing to overwrite an existing provider component or manifest");
            }
            fs::rename(&state.source, &backup)?;
            sync_parent(&backup)?;
            state.python_backup = Some(backup.clone());
        }

        let bytes = read_bounded(&candidate.path, MAX_COMPONENT_BYTES, "candidate component")?;
        if digest_bytes(&bytes) != candidate.sha256 {
            anyhow::bail!("candidate component changed during activation");
        }
        let mut catalog = state.catalog.clone();
        catalog.provider.kind = ProviderKind::Wasm;
        catalog.provider.source = Some(deployed_component.display().to_string());
        catalog.provider.version = Some(format!("sha256:{}", candidate.sha256));
        if let Err(error) = atomic_write(&deployed_component, &bytes)
            .and_then(|()| atomic_write(&deployed_manifest, &serde_json::to_vec_pretty(&catalog)?))
        {
            if state.active.is_none() && !state.source.exists() {
                let _ = fs::rename(&backup, &state.source);
            }
            return Err(error);
        }
        let previous = state.active.replace(candidate.clone());
        state.previous = previous;
        state.candidate = None;
        state.attestation = None;
        write_state(workspace, &state)?;
        let response = json!({
            "ok": true,
            "active": candidate,
            "previous": state.previous,
            "deployed_component": deployed_component,
            "deployed_manifest": deployed_manifest,
            "live_provider_refresh_required": true
        });
        Ok(response)
    })();
    match result {
        Ok(value) => Ok(value),
        Err(error) => recover_after_error(workspace, provider_root, error),
    }
}

/// Reactivate the retained component, or restore the original Python source.
pub fn rollback(workspace: &Path, provider_root: &Path) -> anyhow::Result<Value> {
    let _lock = WorkspaceLock::acquire(workspace)?;
    ensure_no_transaction(workspace)?;
    let mut state = read_state(workspace)?;
    validate_state_paths(workspace, provider_root, &state)?;
    let active = state
        .active
        .clone()
        .ok_or_else(|| anyhow::anyhow!("no active graduated component exists"))?;
    let deployed_component = state.source.with_extension("wasm");
    let deployed_manifest = wasm_manifest_path(&deployed_component);
    let backup = state
        .python_backup
        .clone()
        .unwrap_or_else(|| state.source.with_extension("py.soma-backup"));
    begin_transaction(
        workspace,
        &state,
        &deployed_component,
        &deployed_manifest,
        &backup,
    )?;
    let result = (|| {
        if let Some(previous) = state.previous.clone() {
            verify_artifact(&previous)?;
            let bytes = read_bounded(&previous.path, MAX_COMPONENT_BYTES, "previous component")?;
            if digest_bytes(&bytes) != previous.sha256 {
                anyhow::bail!("previous component changed during rollback");
            }
            atomic_write(&deployed_component, &bytes)?;
            let mut catalog = state.catalog.clone();
            catalog.provider.kind = ProviderKind::Wasm;
            catalog.provider.source = Some(deployed_component.display().to_string());
            catalog.provider.version = Some(format!("sha256:{}", previous.sha256));
            atomic_write(&deployed_manifest, &serde_json::to_vec_pretty(&catalog)?)?;
            state.active = Some(previous.clone());
            state.previous = Some(active);
            write_state(workspace, &state)?;
            let response = json!({
                "ok": true,
                "active": previous,
                "previous": state.previous,
                "deployed_component": deployed_component,
                "live_provider_refresh_required": true
            });
            return Ok(response);
        }

        let backup = state
            .python_backup
            .clone()
            .ok_or_else(|| anyhow::anyhow!("original Python source backup is unavailable"))?;
        if state.source.exists() {
            anyhow::bail!("refusing to overwrite existing Python provider source");
        }
        fs::rename(&backup, &state.source)?;
        if deployed_component.exists() {
            fs::remove_file(&deployed_component)?;
        }
        if deployed_manifest.exists() {
            fs::remove_file(&deployed_manifest)?;
        }
        sync_parent(&state.source)?;
        state.candidate = Some(active);
        state.active = None;
        state.python_backup = None;
        state.attestation = None;
        write_state(workspace, &state)?;
        let response = json!({
            "ok": true,
            "active": "python",
            "source": state.source,
            "live_provider_refresh_required": true
        });
        Ok(response)
    })();
    match result {
        Ok(value) => Ok(value),
        Err(error) => recover_after_error(workspace, provider_root, error),
    }
}

/// Read-only operator status with integrity checks for referenced artifacts.
pub fn status(workspace: &Path, provider_root: &Path) -> anyhow::Result<Value> {
    let _lock = WorkspaceLock::acquire(workspace)?;
    let state = read_state(workspace)?;
    validate_state_paths(workspace, provider_root, &state)?;
    let candidate_valid = state
        .candidate
        .as_ref()
        .is_some_and(|artifact| verify_artifact(artifact).is_ok());
    let active_valid = state
        .active
        .as_ref()
        .is_some_and(|artifact| verify_artifact(artifact).is_ok());
    let previous_valid = state
        .previous
        .as_ref()
        .is_some_and(|artifact| verify_artifact(artifact).is_ok());
    let deployed_component = state.source.with_extension("wasm");
    let deployed_sha256 = deployed_component
        .is_file()
        .then(|| digest_file(&deployed_component).ok())
        .flatten();
    let deployed_matches_active = state
        .active
        .as_ref()
        .is_some_and(|artifact| deployed_sha256.as_deref() == Some(&artifact.sha256));
    Ok(json!({
        "schema_version": state.schema_version,
        "source": state.source,
        "candidate": state.candidate,
        "candidate_valid": candidate_valid,
        "active": state.active,
        "active_valid": active_valid,
        "previous": state.previous,
        "previous_valid": previous_valid,
        "attestation": state.attestation,
        "python_backup": state.python_backup,
        "recovery_required": workspace.join(TRANSACTION_DIR).exists(),
        "deployed_component": deployed_component,
        "deployed_sha256": deployed_sha256,
        "deployed_matches_active": deployed_matches_active,
    }))
}

pub(crate) fn identity_before(
    workspace: &Path,
    provider_root: &Path,
    deadline: Instant,
) -> anyhow::Result<GraduationState> {
    let _lock = WorkspaceLock::acquire_before(workspace, deadline)?;
    let state = read_state(workspace)?;
    validate_state_paths(workspace, provider_root, &state)?;
    Ok(state)
}

pub(crate) fn catalog_contract_digest(catalog: &ProviderCatalog) -> anyhow::Result<String> {
    let mut normalized = catalog.clone();
    normalized.provider.source = None;
    normalized.provider.version = None;
    Ok(digest_bytes(&serde_json::to_vec(&normalized)?))
}

/// Commit a live activation only after the caller has refreshed and verified
/// the active provider generation.
pub fn commit_transaction(workspace: &Path) -> anyhow::Result<()> {
    let _lock = WorkspaceLock::acquire(workspace)?;
    finish_transaction(workspace)
}

/// Whether commit crossed the atomic marker transition but failed its
/// directory durability sync. Such errors must never trigger rollback.
pub fn is_ambiguous_commit(error: &anyhow::Error) -> bool {
    error.downcast_ref::<AmbiguousCommitError>().is_some()
}

fn atomic_write(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("atomic destination requires a parent"))?;
    fs::create_dir_all(parent)?;
    AtomicFile::new(path, AllowOverwrite)
        .write(|file| {
            file.write_all(bytes)?;
            file.sync_all()
        })
        .map_err(|error| anyhow::anyhow!("atomic write failed for {}: {error}", path.display()))?;
    sync_parent(path)
}

fn verify_artifact(artifact: &GraduationArtifact) -> anyhow::Result<()> {
    if digest_file(&artifact.path)? != artifact.sha256 {
        anyhow::bail!(
            "component artifact digest mismatch: {}",
            artifact.path.display()
        );
    }
    soma_provider_adapters::wasm::verify_component_artifact(&artifact.path)
        .map_err(anyhow::Error::msg)
}

fn digest_file(path: &Path) -> anyhow::Result<String> {
    Ok(digest_bytes(&read_bounded(
        path,
        MAX_COMPONENT_BYTES,
        "digest input",
    )?))
}

fn read_bounded(path: &Path, limit: usize, label: &str) -> anyhow::Result<Vec<u8>> {
    let length = fs::metadata(path)?.len();
    if length > limit as u64 {
        anyhow::bail!("{label} exceeds {limit} bytes");
    }
    let bytes = fs::read(path)?;
    if bytes.len() > limit {
        anyhow::bail!("{label} exceeds {limit} bytes");
    }
    Ok(bytes)
}

fn recover_after_error<T>(
    workspace: &Path,
    provider_root: &Path,
    original: anyhow::Error,
) -> anyhow::Result<T> {
    match recover_transaction(workspace, provider_root) {
        Ok(()) => Err(original),
        Err(recovery) => {
            anyhow::bail!("{original}; automatic graduation recovery also failed: {recovery}")
        }
    }
}

fn ensure_no_transaction(workspace: &Path) -> anyhow::Result<()> {
    if workspace.join(TRANSACTION_DIR).exists() {
        anyhow::bail!(
            "graduation workspace has an in-progress or interrupted transaction; recover it before another operation"
        );
    }
    Ok(())
}

fn digest_bytes(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn wasm_manifest_path(component: &Path) -> PathBuf {
    component.with_file_name(format!(
        "{}.json",
        component
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("provider.wasm")
    ))
}

fn set_read_only(path: &Path) -> anyhow::Result<()> {
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_readonly(true);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(unix)]
fn sync_parent(path: &Path) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("path requires a parent"))?;
    File::open(parent)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_parent(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_templates_keep_manual_rewrite_contract() {
        assert!(
            include_str!("../templates/graduation/core.rs").contains("manual rewrite required")
        );
        assert!(include_str!("../templates/graduation/component.rs").contains("export!"));
        assert!(include_str!("../templates/graduation/python.rs").contains("#[pymodule]"));
    }

    #[test]
    fn workspace_lock_contention_honors_the_absolute_deadline() {
        let workspace = tempfile::tempdir().expect("workspace");
        let held = WorkspaceLock::acquire(workspace.path()).expect("held lock");
        let error = WorkspaceLock::acquire_before(
            workspace.path(),
            Instant::now() + Duration::from_millis(20),
        )
        .err()
        .expect("contended lock must time out");
        assert!(error.to_string().contains("deadline"));
        drop(held);
        WorkspaceLock::acquire_before(workspace.path(), Instant::now() + Duration::from_secs(1))
            .expect("released lock");
    }

    #[test]
    fn interrupted_live_transaction_restores_source_files_and_state() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let workspace = temp.path().join("graduated");
        fs::create_dir(&workspace).expect("workspace");
        let source = temp.path().join("provider.py");
        fs::write(&source, b"original python").expect("source");
        let backup = source.with_extension("py.soma-backup");
        let component = source.with_extension("wasm");
        let manifest = wasm_manifest_path(&component);
        let state = GraduationState {
            schema_version: STATE_SCHEMA_VERSION,
            source: source.clone(),
            source_sha256: digest_file(&source).expect("digest"),
            catalog_sha256: catalog_contract_digest(
                &serde_json::from_value(json!({
                    "schema_version": 1,
                    "provider": {"name": "transaction-test", "kind": "python", "source": source},
                    "tools": []
                }))
                .expect("catalog"),
            )
            .expect("catalog digest"),
            catalog: serde_json::from_value(json!({
                "schema_version": 1,
                "provider": {"name": "transaction-test", "kind": "python", "source": source},
                "tools": []
            }))
            .expect("catalog"),
            candidate: None,
            active: None,
            previous: None,
            python_backup: None,
            attestation: None,
        };
        write_state(&workspace, &state).expect("state");
        begin_transaction(&workspace, &state, &component, &manifest, &backup).expect("transaction");
        fs::rename(&source, &backup).expect("move source");
        fs::write(&component, b"partial component").expect("component");
        fs::write(&manifest, b"partial manifest").expect("manifest");
        let mut changed = state.clone();
        changed.python_backup = Some(backup.clone());
        write_state(&workspace, &changed).expect("changed state");

        recover_transaction(&workspace, temp.path()).expect("recovery");

        assert_eq!(
            fs::read(&source).expect("restored source"),
            b"original python"
        );
        assert!(!backup.exists());
        assert!(!component.exists());
        assert!(!manifest.exists());
        assert_eq!(
            read_state(&workspace)
                .expect("restored state")
                .source_sha256,
            state.source_sha256
        );
        assert!(!workspace.join(TRANSACTION_DIR).exists());
    }

    #[test]
    fn status_reports_interrupted_transaction_without_mutating_it() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let workspace = temp.path().join("graduated");
        fs::create_dir(&workspace).expect("workspace");
        let source = temp.path().join("provider.py");
        fs::write(&source, b"original python").expect("source");
        let backup = source.with_extension("py.soma-backup");
        let component = source.with_extension("wasm");
        let manifest = wasm_manifest_path(&component);
        let state = GraduationState {
            schema_version: STATE_SCHEMA_VERSION,
            source: source.clone(),
            source_sha256: digest_file(&source).expect("digest"),
            catalog_sha256: catalog_contract_digest(
                &serde_json::from_value(json!({
                    "schema_version": 1,
                    "provider": {"name": "status-test", "kind": "python", "source": source},
                    "tools": []
                }))
                .expect("catalog"),
            )
            .expect("catalog digest"),
            catalog: serde_json::from_value(json!({
                "schema_version": 1,
                "provider": {"name": "status-test", "kind": "python", "source": source},
                "tools": []
            }))
            .expect("catalog"),
            candidate: None,
            active: None,
            previous: None,
            python_backup: None,
            attestation: None,
        };
        write_state(&workspace, &state).expect("state");
        begin_transaction(&workspace, &state, &component, &manifest, &backup).expect("transaction");

        let report = status(&workspace, temp.path()).expect("status");

        assert_eq!(report["recovery_required"], true);
        assert!(workspace.join(TRANSACTION_DIR).is_dir());
    }

    #[test]
    fn startup_recovery_finds_nested_workspaces() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let workspace = temp.path().join("teams/python/graduated");
        let source = create_interrupted_workspace(temp.path(), &workspace);

        assert_eq!(
            recover_all(temp.path(), &temp.path().join("providers")).expect("recursive recovery"),
            1
        );
        assert_eq!(
            fs::read(source).expect("restored source"),
            b"original python"
        );
        assert!(!workspace.join(TRANSACTION_DIR).exists());
    }

    #[cfg(unix)]
    #[test]
    fn startup_recovery_does_not_follow_symlinked_directories() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().expect("recovery root");
        let external = tempfile::tempdir().expect("external root");
        let workspace = external.path().join("graduated");
        create_interrupted_workspace(external.path(), &workspace);
        symlink(external.path(), root.path().join("external-link")).expect("symlink");

        assert_eq!(
            recover_all(root.path(), root.path()).expect("bounded recovery"),
            0
        );
        assert!(workspace.join(TRANSACTION_DIR).exists());
    }

    #[cfg(unix)]
    #[test]
    fn startup_recovery_rejects_symlinked_transaction_directories() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().expect("recovery root");
        let external = tempfile::tempdir().expect("external transaction");
        let workspace = root.path().join("graduated");
        fs::create_dir(&workspace).expect("workspace");
        symlink(external.path(), workspace.join(TRANSACTION_DIR)).expect("transaction symlink");

        assert!(
            recover_all(root.path(), root.path())
                .expect_err("symlinked transaction must fail closed")
                .to_string()
                .contains("must not be a symlink")
        );
    }

    #[cfg(unix)]
    #[test]
    fn direct_recovery_rejects_symlinked_transaction_directory() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().expect("root");
        let external = tempfile::tempdir().expect("external transaction");
        let workspace = root.path().join("graduated");
        fs::create_dir(&workspace).expect("workspace");
        symlink(external.path(), workspace.join(TRANSACTION_DIR)).expect("transaction symlink");

        assert!(
            recover(&workspace, root.path())
                .expect_err("direct recovery must reject the symlink")
                .to_string()
                .contains("must be a real directory")
        );
    }

    #[test]
    fn persisted_state_cannot_escape_managed_roots() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let provider_root = temp.path().join("providers");
        let workspace = temp.path().join("graduated");
        fs::create_dir_all(workspace.join("artifacts")).expect("workspace");
        fs::create_dir(&provider_root).expect("provider root");
        let external = temp.path().join("external.py");
        fs::write(&external, b"forged").expect("external source");
        let state = GraduationState {
            schema_version: STATE_SCHEMA_VERSION,
            source: external,
            source_sha256: "00".repeat(32),
            catalog_sha256: catalog_contract_digest(
                &serde_json::from_value(json!({
                    "schema_version": 1,
                    "provider": {"name": "forged", "kind": "python"},
                    "tools": []
                }))
                .expect("catalog"),
            )
            .expect("catalog digest"),
            catalog: serde_json::from_value(json!({
                "schema_version": 1,
                "provider": {"name": "forged", "kind": "python"},
                "tools": []
            }))
            .expect("catalog"),
            candidate: None,
            active: None,
            previous: None,
            python_backup: None,
            attestation: None,
        };
        write_state(&workspace, &state).expect("state");

        assert!(
            status(&workspace, &provider_root)
                .expect_err("forged source must fail closed")
                .to_string()
                .contains("outside the managed provider root")
        );
    }

    #[test]
    fn persisted_state_source_must_match_scaffolded_provider_identity() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let provider_root = temp.path().join("providers");
        let workspace = temp.path().join("graduated");
        fs::create_dir_all(workspace.join("artifacts")).expect("workspace");
        fs::create_dir(&provider_root).expect("provider root");
        let source = provider_root.join("target.py");
        let other = provider_root.join("other.py");
        fs::write(&source, b"target").expect("source");
        fs::write(&other, b"other").expect("other");
        let state = GraduationState {
            schema_version: STATE_SCHEMA_VERSION,
            source: other,
            source_sha256: "00".repeat(32),
            catalog_sha256: catalog_contract_digest(
                &serde_json::from_value(json!({
                    "schema_version": 1,
                    "provider": {"name": "target", "kind": "python", "source": source},
                    "tools": []
                }))
                .expect("catalog"),
            )
            .expect("catalog digest"),
            catalog: serde_json::from_value(json!({
                "schema_version": 1,
                "provider": {"name": "target", "kind": "python", "source": source},
                "tools": []
            }))
            .expect("catalog"),
            candidate: None,
            active: None,
            previous: None,
            python_backup: None,
            attestation: None,
        };
        write_state(&workspace, &state).expect("state");

        assert!(
            status(&workspace, &provider_root)
                .expect_err("in-root confused deputy must fail closed")
                .to_string()
                .contains("bound provider identity")
        );
    }

    #[test]
    fn recovery_rejects_forged_transaction_destinations() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let workspace = temp.path().join("graduated");
        create_interrupted_workspace(temp.path(), &workspace);
        let transaction_path = workspace.join(TRANSACTION_DIR).join("transaction.json");
        let mut transaction: Value =
            serde_json::from_slice(&fs::read(&transaction_path).expect("transaction"))
                .expect("transaction JSON");
        transaction["deployed_component"] =
            Value::String(temp.path().join("outside.wasm").display().to_string());
        fs::write(
            &transaction_path,
            serde_json::to_vec_pretty(&transaction).expect("encode transaction"),
        )
        .expect("forge transaction");

        assert!(
            recover(&workspace, &temp.path().join("providers"))
                .expect_err("forged recovery path must fail closed")
                .to_string()
                .contains("forged destination")
        );
    }

    #[test]
    fn startup_recovery_bounds_wide_directory_trees() {
        let root = tempfile::tempdir().expect("recovery root");
        for index in 0..=MAX_RECOVERY_ENTRIES {
            fs::write(root.path().join(format!("entry-{index}")), b"").expect("entry");
        }
        assert!(
            recover_all(root.path(), root.path())
                .expect_err("wide tree must fail closed")
                .to_string()
                .contains("entries")
        );
    }

    #[test]
    fn startup_recovery_rejects_and_preserves_untrusted_tombstones() {
        let temp = tempfile::tempdir().expect("recovery root");
        let workspace = temp.path().join("graduated");
        create_interrupted_workspace(temp.path(), &workspace);
        let tombstone = workspace.join(".graduation-transaction-complete-decoy");
        fs::rename(workspace.join(TRANSACTION_DIR), &tombstone).expect("tombstone");
        fs::write(tombstone.join("unexpected"), b"do not delete").expect("decoy");

        assert!(
            recover_all(temp.path(), &temp.path().join("providers"))
                .expect_err("untrusted tombstone must fail closed")
                .to_string()
                .contains("unexpected entry")
        );
        assert!(
            tombstone.join("unexpected").exists(),
            "recovery must preserve an untrusted directory for operator inspection"
        );
    }

    fn create_interrupted_workspace(root: &Path, workspace: &Path) -> PathBuf {
        fs::create_dir_all(workspace).expect("workspace");
        let source_dir = root.join("providers");
        fs::create_dir_all(&source_dir).expect("source directory");
        let source = source_dir.join("provider.py");
        fs::write(&source, b"original python").expect("source");
        let backup = source.with_extension("py.soma-backup");
        let component = source.with_extension("wasm");
        let manifest = wasm_manifest_path(&component);
        let state = GraduationState {
            schema_version: STATE_SCHEMA_VERSION,
            source: source.clone(),
            source_sha256: digest_file(&source).expect("digest"),
            catalog_sha256: catalog_contract_digest(
                &serde_json::from_value(json!({
                    "schema_version": 1,
                    "provider": {"name": "nested-recovery", "kind": "python", "source": source},
                    "tools": []
                }))
                .expect("catalog"),
            )
            .expect("catalog digest"),
            catalog: serde_json::from_value(json!({
                "schema_version": 1,
                "provider": {"name": "nested-recovery", "kind": "python", "source": source},
                "tools": []
            }))
            .expect("catalog"),
            candidate: None,
            active: None,
            previous: None,
            python_backup: None,
            attestation: None,
        };
        write_state(workspace, &state).expect("state");
        begin_transaction(workspace, &state, &component, &manifest, &backup).expect("transaction");
        fs::rename(&source, &backup).expect("move source");
        fs::write(&component, b"partial component").expect("component");
        source
    }
}
