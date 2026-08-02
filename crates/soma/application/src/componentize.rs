//! Experimental Python-to-component pipeline layered on the stable graduation workflow.

use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use atomicwrites::{AllowOverwrite, AtomicFile};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::graduation::{
    GraduationState, WorkspaceLock, ensure_no_transaction, read_state, validate_state_paths,
};

mod build;

const STATE_SCHEMA_VERSION: u32 = 1;
const POLICY_VERSION: &str = "soma-componentize-v1";
const COMPONENTIZE_PY_VERSION: &str = "0.25.0";
const MAX_SOURCE_BYTES: usize = 1024 * 1024;
const MAX_REPORT_BYTES: usize = 2 * 1024 * 1024;
const MAX_WHEELS: usize = 64;
const MAX_BINDING_FILES: usize = 10_000;
const MAX_BINDING_BYTES: usize = 32 * 1024 * 1024;
const STATE_FILE: &str = "componentize.json";
const REPORT_FILE: &str = "componentize-report.json";

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct ComponentizeWheel {
    pub path: PathBuf,
    pub filename: String,
    pub sha256: String,
    pub distribution: Option<String>,
    pub version: Option<String>,
    pub modules: Vec<String>,
    pub pure_python: bool,
    pub record_verified: bool,
    pub entries: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ScannerReport {
    schema_version: u32,
    policy_version: String,
    componentize_py_version: String,
    experimental: bool,
    compatible: bool,
    requires_build_validation: bool,
    filename: String,
    source_sha256: String,
    imports: Vec<String>,
    external_imports: Vec<String>,
    import_distributions: BTreeMap<String, String>,
    wheel_files: Vec<PathBuf>,
    wheel_evidence: Vec<ComponentizeWheel>,
    findings: Vec<Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct ComponentizeArtifact {
    pub path: PathBuf,
    pub sha256: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ComponentizeState {
    pub schema_version: u32,
    pub policy_version: String,
    pub componentize_py_version: String,
    pub source: PathBuf,
    pub source_sha256: String,
    pub wheelhouse: Option<PathBuf>,
    pub wheels: Vec<ComponentizeWheel>,
    pub report_sha256: String,
    pub compatible: bool,
    pub bindings: Option<ComponentizeArtifact>,
    pub component: Option<ComponentizeArtifact>,
    pub graduation_candidate: Option<ComponentizeArtifact>,
    pub verified: bool,
    pub verified_unix_ms: Option<u64>,
}

pub(crate) fn scan(
    workspace: &Path,
    wheelhouse: Option<&Path>,
    provider_root: &Path,
) -> anyhow::Result<Value> {
    let _lock = WorkspaceLock::acquire(workspace)?;
    ensure_no_transaction(workspace)?;
    let graduation = graduation_state(workspace, provider_root)?;
    let source = read_bounded(&graduation.source, MAX_SOURCE_BYTES, "componentize source")?;
    let source_text = std::str::from_utf8(&source)
        .map_err(|_| anyhow::anyhow!("componentize source must be UTF-8"))?;
    let wheels = collect_wheels(wheelhouse)?;
    let report = run_scanner(workspace, &graduation.source, source_text, &wheels)?;
    validate_report(&report, &graduation, &wheels)?;

    let report_bytes = serde_json::to_vec_pretty(&report)?;
    let report_sha256 = digest(&report_bytes);
    atomic_write(&workspace.join(REPORT_FILE), &report_bytes)?;
    let state = ComponentizeState {
        schema_version: STATE_SCHEMA_VERSION,
        policy_version: POLICY_VERSION.to_owned(),
        componentize_py_version: COMPONENTIZE_PY_VERSION.to_owned(),
        source: graduation.source,
        source_sha256: report.source_sha256.clone(),
        wheelhouse: wheelhouse.map(Path::to_path_buf),
        wheels: report.wheel_evidence.clone(),
        report_sha256,
        compatible: report.compatible,
        bindings: None,
        component: None,
        graduation_candidate: None,
        verified: false,
        verified_unix_ms: None,
    };
    write_state(workspace, &state)?;
    Ok(serde_json::to_value(report)?)
}

pub(crate) fn bindings(workspace: &Path, provider_root: &Path) -> anyhow::Result<Value> {
    let _lock = WorkspaceLock::acquire(workspace)?;
    ensure_no_transaction(workspace)?;
    let graduation = graduation_state(workspace, provider_root)?;
    let mut state = load_valid_state(workspace, &graduation, true)?;
    let program = componentize_program()?;
    verify_componentize_version(&program)?;

    let staging = tempfile::Builder::new()
        .prefix(".componentize-bindings-")
        .tempdir_in(workspace)?;
    let wit = staging.path().join("world.wit");
    fs::write(
        &wit,
        include_str!("../../../../wit/soma-provider/world.wit"),
    )?;
    let output = staging.path().join("bindings");
    let status = Command::new(&program)
        .env_clear()
        .env("HOME", staging.path())
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .args(["-d", wit.to_string_lossy().as_ref(), "-w", "provider"])
        .args(["--world-module", "soma_wit", "bindings"])
        .arg(&output)
        .status()?;
    if !status.success() {
        anyhow::bail!("componentize-py bindings generation failed with status {status}");
    }
    for expected in [
        output.join("soma_wit/__init__.py"),
        output.join("soma_wit/imports/host.py"),
        output.join("componentize_py_types.py"),
    ] {
        if !expected.is_file() {
            anyhow::bail!(
                "componentize-py omitted generated binding {}",
                expected.display()
            );
        }
    }
    let binding_sha256 = directory_digest(&output)?;
    let destination = workspace
        .join("componentize")
        .join(format!("bindings-{binding_sha256}"));
    publish_directory(&output, &destination, &binding_sha256)?;
    state.bindings = Some(ComponentizeArtifact {
        path: destination.clone(),
        sha256: binding_sha256.clone(),
    });
    write_state(workspace, &state)?;
    Ok(json!({
        "ok": true,
        "bindings": destination,
        "sha256": binding_sha256,
        "componentize_py_version": COMPONENTIZE_PY_VERSION,
    }))
}

pub(crate) fn build(workspace: &Path, provider_root: &Path) -> anyhow::Result<Value> {
    build::build(workspace, provider_root)
}

pub(crate) fn validate(workspace: &Path, provider_root: &Path) -> anyhow::Result<Value> {
    build::validate(workspace, provider_root)
}

pub(crate) fn status(workspace: &Path, provider_root: &Path) -> anyhow::Result<Value> {
    let _lock = WorkspaceLock::acquire(workspace)?;
    let graduation = graduation_state(workspace, provider_root)?;
    let path = workspace.join(STATE_FILE);
    if !path.exists() {
        return Ok(json!({
            "configured": false,
            "policy_version": POLICY_VERSION,
            "componentize_py_version": COMPONENTIZE_PY_VERSION,
        }));
    }
    let state = read_state_file(workspace)?;
    let validation_error = validate_state(workspace, &graduation, &state, false)
        .err()
        .map(|error| error.to_string());
    Ok(json!({
        "configured": true,
        "valid": validation_error.is_none(),
        "validation_error": validation_error,
        "state": state,
    }))
}

pub(super) fn graduation_state(
    workspace: &Path,
    provider_root: &Path,
) -> anyhow::Result<GraduationState> {
    let state = read_state(workspace)?;
    validate_state_paths(workspace, provider_root, &state)?;
    Ok(state)
}

pub(super) fn load_valid_state(
    workspace: &Path,
    graduation: &GraduationState,
    require_compatible: bool,
) -> anyhow::Result<ComponentizeState> {
    let state = read_state_file(workspace)?;
    validate_state(workspace, graduation, &state, require_compatible)?;
    Ok(state)
}

pub(super) fn validate_state(
    workspace: &Path,
    graduation: &GraduationState,
    state: &ComponentizeState,
    require_compatible: bool,
) -> anyhow::Result<()> {
    if state.schema_version != STATE_SCHEMA_VERSION
        || state.policy_version != POLICY_VERSION
        || state.componentize_py_version != COMPONENTIZE_PY_VERSION
    {
        anyhow::bail!("componentize state policy or schema is unsupported");
    }
    if state.source != graduation.source || state.source_sha256 != graduation.source_sha256 {
        anyhow::bail!("componentize state is bound to a different graduation source");
    }
    if digest_file(&state.source, MAX_SOURCE_BYTES)? != state.source_sha256 {
        anyhow::bail!("componentize source changed after compatibility scanning");
    }
    if require_compatible && !state.compatible {
        anyhow::bail!("componentize compatibility report contains blocking findings");
    }
    let report = workspace.join(REPORT_FILE);
    if digest_file(&report, MAX_REPORT_BYTES)? != state.report_sha256 {
        anyhow::bail!("componentize compatibility report digest mismatch");
    }
    if state.compatible {
        for wheel in &state.wheels {
            if !wheel.pure_python || !wheel.record_verified {
                anyhow::bail!("componentize wheel evidence is not pure and authenticated");
            }
            if digest_file(&wheel.path, 64 * 1024 * 1024)? != wheel.sha256 {
                anyhow::bail!("componentize wheel changed after compatibility scanning");
            }
            reject_symlink(&wheel.path)?;
        }
    }
    if let Some(bindings) = &state.bindings {
        validate_managed_path(workspace, &bindings.path, "componentize bindings")?;
        if directory_digest(&bindings.path)? != bindings.sha256 {
            anyhow::bail!("componentize generated bindings digest mismatch");
        }
    }
    for artifact in [&state.component, &state.graduation_candidate]
        .into_iter()
        .flatten()
    {
        validate_managed_path(workspace, &artifact.path, "componentize artifact")?;
        if digest_file(&artifact.path, 64 * 1024 * 1024)? != artifact.sha256 {
            anyhow::bail!("componentize artifact digest mismatch");
        }
    }
    Ok(())
}

pub(super) fn read_state_file(workspace: &Path) -> anyhow::Result<ComponentizeState> {
    let bytes = read_bounded(
        &workspace.join(STATE_FILE),
        MAX_REPORT_BYTES,
        "componentize state",
    )?;
    Ok(serde_json::from_slice(&bytes)?)
}

pub(super) fn write_state(workspace: &Path, state: &ComponentizeState) -> anyhow::Result<()> {
    atomic_write(
        &workspace.join(STATE_FILE),
        &serde_json::to_vec_pretty(state)?,
    )
}

pub(super) fn componentize_program() -> anyhow::Result<PathBuf> {
    let configured = std::env::var_os("SOMA_COMPONENTIZE_PY_PROGRAM")
        .unwrap_or_else(|| OsStr::new("componentize-py").to_owned());
    resolve_program(Path::new(&configured))
}

pub(super) fn verify_componentize_version(program: &Path) -> anyhow::Result<()> {
    let output = Command::new(program)
        .env_clear()
        .arg("--version")
        .output()?;
    if !output.status.success() {
        anyhow::bail!("componentize-py version probe failed");
    }
    let version = String::from_utf8(output.stdout)?.trim().to_owned();
    if version != COMPONENTIZE_PY_VERSION
        && version != format!("componentize-py {COMPONENTIZE_PY_VERSION}")
    {
        anyhow::bail!(
            "componentize-py version mismatch: expected {COMPONENTIZE_PY_VERSION}, got {version}"
        );
    }
    Ok(())
}

fn run_scanner(
    workspace: &Path,
    source_path: &Path,
    source: &str,
    wheels: &[PathBuf],
) -> anyhow::Result<ScannerReport> {
    let temporary = tempfile::Builder::new()
        .prefix(".componentize-scan-")
        .tempdir_in(workspace)?;
    let source_copy = temporary.path().join("provider.py");
    fs::write(&source_copy, source)?;
    let mut runner =
        include_str!("../../../../packages/python/python/soma_provider/_componentize.py")
            .to_owned();
    runner.push_str(
        "

import json
source=open(sys.argv[1],encoding='utf-8').read()
filename=sys.argv[2]
print(json.dumps(scan_componentize_compatibility(source,filename=filename,wheel_files=sys.argv[3:]),sort_keys=True,separators=(',',':')))
",
    );
    fs::write(temporary.path().join("run.py"), runner)?;
    let python = std::env::var_os("SOMA_COMPONENTIZE_PYTHON").unwrap_or_else(|| "python3".into());
    let mut command = Command::new(python);
    command
        .env_clear()
        .env("HOME", temporary.path())
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("PYTHONNOUSERSITE", "1")
        .args([
            "-I",
            temporary.path().join("run.py").to_string_lossy().as_ref(),
        ])
        .arg(&source_copy)
        .arg(source_path);
    command.args(wheels);
    let output = command.output()?;
    if !output.status.success() {
        anyhow::bail!(
            "componentize compatibility scan failed: {}",
            String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(1024)
                .collect::<String>()
        );
    }
    if output.stdout.len() > MAX_REPORT_BYTES {
        anyhow::bail!("componentize compatibility report exceeds {MAX_REPORT_BYTES} bytes");
    }
    let report: ScannerReport = serde_json::from_slice(&output.stdout)?;
    if source.len() > MAX_SOURCE_BYTES {
        anyhow::bail!("componentize source exceeds {MAX_SOURCE_BYTES} bytes");
    }
    Ok(report)
}

fn validate_report(
    report: &ScannerReport,
    graduation: &GraduationState,
    wheels: &[PathBuf],
) -> anyhow::Result<()> {
    if report.schema_version != 2
        || report.policy_version != POLICY_VERSION
        || report.componentize_py_version != COMPONENTIZE_PY_VERSION
        || !report.experimental
    {
        anyhow::bail!("componentize compatibility report contract mismatch");
    }
    if report.source_sha256 != graduation.source_sha256 {
        anyhow::bail!("componentize compatibility report source digest mismatch");
    }
    if report.wheel_files != wheels {
        anyhow::bail!("componentize compatibility report wheel set mismatch");
    }
    if report.compatible != report.requires_build_validation {
        anyhow::bail!("componentize report eligibility flags are inconsistent");
    }
    let has_error_finding = report
        .findings
        .iter()
        .any(|finding| finding.get("severity").and_then(Value::as_str) == Some("error"));
    if report.compatible == has_error_finding {
        anyhow::bail!("componentize report compatibility does not match its findings");
    }

    let evidence_paths = report
        .wheel_evidence
        .iter()
        .map(|wheel| wheel.path.as_path())
        .collect::<Vec<_>>();
    let evidence_set = evidence_paths.iter().copied().collect::<BTreeSet<_>>();
    let expected_set = wheels.iter().map(PathBuf::as_path).collect::<BTreeSet<_>>();
    if evidence_set.len() != evidence_paths.len() || !evidence_set.is_subset(&expected_set) {
        anyhow::bail!("componentize wheel evidence is not bound to the scanned wheel set");
    }

    if report.compatible {
        if evidence_set != expected_set {
            anyhow::bail!("componentize wheel evidence is not bound to the scanned wheel set");
        }
        if report.wheel_evidence.iter().any(|wheel| {
            !wheel.pure_python || !wheel.record_verified || wheel.entries > MAX_BINDING_FILES
        }) {
            anyhow::bail!("componentize wheel evidence violates the dependency policy");
        }
        if report
            .external_imports
            .iter()
            .any(|name| !report.import_distributions.contains_key(name))
        {
            anyhow::bail!("componentize external imports are not fully mapped to distributions");
        }
    }
    Ok(())
}

fn collect_wheels(wheelhouse: Option<&Path>) -> anyhow::Result<Vec<PathBuf>> {
    let Some(wheelhouse) = wheelhouse else {
        return Ok(Vec::new());
    };
    reject_symlink(wheelhouse)?;
    let root = wheelhouse.canonicalize()?;
    if !root.is_dir() {
        anyhow::bail!("componentize wheelhouse must be a directory");
    }
    let mut wheels = Vec::new();
    for entry in fs::read_dir(&root)? {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.file_type().is_symlink() {
            anyhow::bail!("componentize wheelhouse must not contain symlinks");
        }
        if metadata.is_file() && entry.path().extension() == Some(OsStr::new("whl")) {
            wheels.push(entry.path().canonicalize()?);
        }
    }
    wheels.sort();
    if wheels.len() > MAX_WHEELS {
        anyhow::bail!("componentize wheelhouse exceeds {MAX_WHEELS} wheels");
    }
    Ok(wheels)
}

pub(super) fn atomic_write(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("path requires a parent"))?;
    fs::create_dir_all(parent)?;
    AtomicFile::new(path, AllowOverwrite)
        .write(|file| {
            file.write_all(bytes)?;
            file.sync_all()
        })
        .map_err(|error| anyhow::anyhow!("atomic write failed for {}: {error}", path.display()))?;
    sync_parent(parent)
}

pub(super) fn read_bounded(path: &Path, limit: usize, label: &str) -> anyhow::Result<Vec<u8>> {
    reject_symlink(path)?;
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() || metadata.len() > limit as u64 {
        anyhow::bail!("{label} is not a regular bounded file");
    }
    let bytes = fs::read(path)?;
    if bytes.len() > limit {
        anyhow::bail!("{label} exceeds {limit} bytes");
    }
    Ok(bytes)
}

pub(super) fn digest_file(path: &Path, limit: usize) -> anyhow::Result<String> {
    Ok(digest(&read_bounded(path, limit, "digest input")?))
}

pub(super) fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(super) fn directory_digest(path: &Path) -> anyhow::Result<String> {
    reject_symlink(path)?;
    let root = path.canonicalize()?;
    let mut files = Vec::new();
    collect_files(&root, &root, &mut files)?;
    files.sort_by(|left, right| left.0.cmp(&right.0));
    let mut hash = Sha256::new();
    let mut total = 0usize;
    for (relative, path) in files {
        let bytes = read_bounded(&path, MAX_BINDING_BYTES, "generated binding")?;
        total = total.saturating_add(bytes.len());
        if total > MAX_BINDING_BYTES {
            anyhow::bail!("generated bindings exceed {MAX_BINDING_BYTES} bytes");
        }
        hash.update(relative.as_os_str().as_encoded_bytes());
        hash.update([0]);
        hash.update((bytes.len() as u64).to_be_bytes());
        hash.update(&bytes);
    }
    Ok(hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn collect_files(
    root: &Path,
    path: &Path,
    files: &mut Vec<(PathBuf, PathBuf)>,
) -> anyhow::Result<()> {
    if files.len() > MAX_BINDING_FILES {
        anyhow::bail!("generated bindings exceed {MAX_BINDING_FILES} files");
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.file_type().is_symlink() {
            anyhow::bail!("generated bindings must not contain symlinks");
        }
        if metadata.is_dir() {
            collect_files(root, &entry.path(), files)?;
        } else if metadata.is_file() {
            files.push((entry.path().strip_prefix(root)?.to_owned(), entry.path()));
            if files.len() > MAX_BINDING_FILES {
                anyhow::bail!("generated bindings exceed {MAX_BINDING_FILES} files");
            }
        }
    }
    Ok(())
}

fn publish_directory(source: &Path, destination: &Path, digest: &str) -> anyhow::Result<()> {
    if destination.exists() {
        if directory_digest(destination)? == digest {
            return Ok(());
        }
        anyhow::bail!("componentize digest path already contains different bindings");
    }
    fs::create_dir_all(
        destination
            .parent()
            .ok_or_else(|| anyhow::anyhow!("binding path requires a parent"))?,
    )?;
    fs::rename(source, destination)?;
    sync_parent(destination.parent().expect("binding parent checked"))
}

fn resolve_program(program: &Path) -> anyhow::Result<PathBuf> {
    if program.components().count() > 1 {
        return Ok(program.canonicalize()?);
    }
    let path = std::env::var_os("PATH").ok_or_else(|| anyhow::anyhow!("PATH is unavailable"))?;
    for directory in std::env::split_paths(&path) {
        let candidate = directory.join(program);
        if candidate.is_file() {
            return Ok(candidate.canonicalize()?);
        }
    }
    anyhow::bail!("componentize-py executable was not found")
}

fn validate_managed_path(workspace: &Path, path: &Path, label: &str) -> anyhow::Result<()> {
    reject_symlink(path)?;
    let workspace = workspace.canonicalize()?;
    let path = path.canonicalize()?;
    if !path.starts_with(&workspace) {
        anyhow::bail!("{label} escapes the componentize workspace");
    }
    Ok(())
}

fn reject_symlink(path: &Path) -> anyhow::Result<()> {
    if fs::symlink_metadata(path)?.file_type().is_symlink() {
        anyhow::bail!("managed componentize paths must not be symlinks");
    }
    Ok(())
}

fn sync_parent(parent: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        fs::File::open(parent)?.sync_all()?;
    }
    Ok(())
}

pub(super) fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
#[path = "componentize_tests.rs"]
mod tests;
