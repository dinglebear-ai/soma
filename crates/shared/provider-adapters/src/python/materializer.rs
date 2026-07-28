//! Atomic materialization and frozen startup for planned Python environments.

use std::{
    ffi::{OsStr, OsString},
    fs, io,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use super::environment::{
    Pep723Metadata, PythonEnvironmentPlan, PythonRuntimeFingerprint, PythonWheelTag,
};

pub(super) const READY_FILE: &str = "soma-environment.json";
pub(super) const READY_SCHEMA_VERSION: u32 = 3;
static STAGING_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[path = "materializer_repair.rs"]
mod repair;
pub use repair::{
    PythonEnvironmentRepairError, PythonEnvironmentRepairOutcome, PythonEnvironmentRepairReport,
};
#[path = "materializer_update.rs"]
mod update;
pub use update::{
    PythonEnvironmentUpdateError, PythonEnvironmentUpdateOutcome, PythonEnvironmentUpdateReport,
    PythonEnvironmentUpdateRequest,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedPythonEnvironment {
    pub key: String,
    pub directory: PathBuf,
    pub python: PathBuf,
    pub lockfile: PathBuf,
    pub plan_version: u32,
    pub dependency_count: usize,
    pub runtime: PythonRuntimeFingerprint,
    pub sdk_wheel_tag: PythonWheelTag,
    pub sdk_wheel_sha256: String,
    pub uv_version: String,
    pub lock_sha256: String,
    pub provider_source_sha256: Option<String>,
    pub input_plan_key: Option<String>,
}

impl PreparedPythonEnvironment {
    pub fn environment_plan(&self) -> PythonEnvironmentPlan {
        PythonEnvironmentPlan {
            key: self.key.clone(),
            directory: self.directory.clone(),
            plan_version: self.plan_version,
            dependency_count: self.dependency_count,
            runtime: self.runtime.clone(),
            sdk_wheel_tag: self.sdk_wheel_tag.clone(),
            sdk_wheel_sha256: self.sdk_wheel_sha256.clone(),
            uv_version: self.uv_version.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PythonMaterializationRequest<'a> {
    pub metadata: Option<&'a Pep723Metadata>,
    pub python_executable: &'a Path,
    pub sdk_wheel: &'a Path,
    pub offline: bool,
}

#[derive(Debug, Error)]
pub enum PythonMaterializationError {
    #[error("Python environment is not cached for offline startup: {0}")]
    OfflineCacheMiss(String),
    #[error("SDK wheel digest does not match the environment plan")]
    SdkDigestMismatch,
    #[error("Python environment cache entry is incomplete: {0}")]
    IncompleteCache(String),
    #[error("uv command failed during {operation}: {message}")]
    Uv {
        operation: &'static str,
        message: String,
    },
    #[error("Python environment I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("Python environment marker is invalid: {0}")]
    InvalidMarker(String),
}

pub trait UvRunner: Send + Sync {
    fn run(&self, program: &Path, args: &[OsString], current_dir: &Path) -> Result<(), String>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemUvRunner;

impl UvRunner for SystemUvRunner {
    fn run(&self, program: &Path, args: &[OsString], current_dir: &Path) -> Result<(), String> {
        let output = Command::new(program)
            .args(args)
            .current_dir(current_dir)
            .env("UV_NO_PROGRESS", "1")
            .output()
            .map_err(|error| error.to_string())?;
        if output.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(stderr.trim().to_owned())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ReadyMarker {
    pub(super) schema_version: u32,
    pub(super) environment_key: String,
    pub(super) plan_version: u32,
    pub(super) dependency_count: usize,
    pub(super) runtime: PythonRuntimeFingerprint,
    pub(super) sdk_wheel_tag: PythonWheelTag,
    pub(super) sdk_wheel_sha256: String,
    pub(super) uv_version: String,
    pub(super) lock_sha256: String,
    #[serde(default)]
    pub(super) provider_source_sha256: Option<String>,
    #[serde(default)]
    pub(super) input_plan_key: Option<String>,
}

pub struct PythonEnvironmentMaterializer<R = SystemUvRunner> {
    uv_program: PathBuf,
    runner: R,
}

impl PythonEnvironmentMaterializer<SystemUvRunner> {
    pub fn new(uv_program: impl Into<PathBuf>) -> Self {
        Self {
            uv_program: uv_program.into(),
            runner: SystemUvRunner,
        }
    }
}

impl<R: UvRunner> PythonEnvironmentMaterializer<R> {
    pub fn with_runner(uv_program: impl Into<PathBuf>, runner: R) -> Self {
        Self {
            uv_program: uv_program.into(),
            runner,
        }
    }

    pub fn open_frozen(
        &self,
        plan: &PythonEnvironmentPlan,
    ) -> Result<PreparedPythonEnvironment, PythonMaterializationError> {
        open_ready(plan)?
            .ok_or_else(|| PythonMaterializationError::OfflineCacheMiss(plan.key.clone()))
    }

    pub fn validate_prepared(
        &self,
        expected: &PreparedPythonEnvironment,
    ) -> Result<PreparedPythonEnvironment, PythonMaterializationError> {
        let reopened = self.open_frozen(&expected.environment_plan())?;
        if reopened != *expected {
            return Err(PythonMaterializationError::InvalidMarker(
                "prepared environment identity changed since it was selected".to_owned(),
            ));
        }
        Ok(reopened)
    }

    pub fn prepare(
        &self,
        plan: &PythonEnvironmentPlan,
        request: PythonMaterializationRequest<'_>,
    ) -> Result<PreparedPythonEnvironment, PythonMaterializationError> {
        if let Some(environment) = open_ready(plan)? {
            return Ok(environment);
        }
        if request.offline {
            return Err(PythonMaterializationError::OfflineCacheMiss(
                plan.key.clone(),
            ));
        }
        verify_sdk_digest(request.sdk_wheel, &plan.sdk_wheel_sha256)?;

        let parent = plan.directory.parent().ok_or_else(|| {
            PythonMaterializationError::IncompleteCache("cache plan has no parent".to_owned())
        })?;
        fs::create_dir_all(parent)?;
        let staging = staging_path(&plan.directory);
        fs::create_dir(&staging)?;

        let result = self.materialize_staging(&staging, plan, request);
        if let Err(error) = result {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }

        match fs::rename(&staging, &plan.directory) {
            Ok(()) => self.open_frozen(plan),
            Err(_) if plan.directory.exists() => {
                let _ = fs::remove_dir_all(&staging);
                self.open_frozen(plan)
            }
            Err(error) => {
                let _ = fs::remove_dir_all(&staging);
                Err(error.into())
            }
        }
    }

    fn materialize_staging(
        &self,
        staging: &Path,
        plan: &PythonEnvironmentPlan,
        request: PythonMaterializationRequest<'_>,
    ) -> Result<(), PythonMaterializationError> {
        fs::write(
            staging.join("pyproject.toml"),
            render_project(request.metadata),
        )?;
        self.uv(
            "lock",
            staging,
            [
                OsString::from("lock"),
                OsString::from("--project"),
                OsString::from("."),
                OsString::from("--python"),
                request.python_executable.as_os_str().to_owned(),
            ],
        )?;
        self.uv(
            "sync",
            staging,
            [
                OsString::from("sync"),
                OsString::from("--project"),
                OsString::from("."),
                OsString::from("--locked"),
                OsString::from("--no-install-project"),
                OsString::from("--python"),
                request.python_executable.as_os_str().to_owned(),
            ],
        )?;
        let python = environment_python(staging);
        self.uv(
            "SDK install",
            staging,
            [
                OsString::from("pip"),
                OsString::from("install"),
                OsString::from("--python"),
                python.as_os_str().to_owned(),
                OsString::from("--offline"),
                OsString::from("--no-deps"),
                request.sdk_wheel.as_os_str().to_owned(),
            ],
        )?;
        let lockfile = staging.join("uv.lock");
        if !python.is_file() || !lockfile.is_file() {
            return Err(PythonMaterializationError::IncompleteCache(
                "uv did not create both .venv Python and uv.lock".to_owned(),
            ));
        }
        let lock_sha256 = sha256_hex(&fs::read(&lockfile)?);
        let marker = ReadyMarker {
            schema_version: READY_SCHEMA_VERSION,
            environment_key: plan.key.clone(),
            plan_version: plan.plan_version,
            dependency_count: plan.dependency_count,
            runtime: plan.runtime.clone(),
            sdk_wheel_tag: plan.sdk_wheel_tag.clone(),
            sdk_wheel_sha256: plan.sdk_wheel_sha256.clone(),
            uv_version: plan.uv_version.clone(),
            lock_sha256,
            provider_source_sha256: None,
            input_plan_key: None,
        };
        fs::write(
            staging.join(READY_FILE),
            serde_json::to_vec_pretty(&marker).unwrap(),
        )?;
        Ok(())
    }

    fn uv<const N: usize>(
        &self,
        operation: &'static str,
        current_dir: &Path,
        args: [OsString; N],
    ) -> Result<(), PythonMaterializationError> {
        self.runner
            .run(&self.uv_program, &args, current_dir)
            .map_err(|message| PythonMaterializationError::Uv { operation, message })
    }
}

fn open_ready(
    plan: &PythonEnvironmentPlan,
) -> Result<Option<PreparedPythonEnvironment>, PythonMaterializationError> {
    let directory_metadata = match fs::symlink_metadata(&plan.directory) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if directory_metadata.file_type().is_symlink() || !directory_metadata.is_dir() {
        return Err(PythonMaterializationError::IncompleteCache(
            "cache path is not a real directory".to_owned(),
        ));
    }
    let marker_path = plan.directory.join(READY_FILE);
    let marker_metadata = match fs::symlink_metadata(&marker_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(PythonMaterializationError::IncompleteCache(
                "cache directory exists without readiness marker".to_owned(),
            ));
        }
        Err(error) => return Err(error.into()),
    };
    if marker_metadata.file_type().is_symlink() || !marker_metadata.is_file() {
        return Err(PythonMaterializationError::IncompleteCache(
            "readiness marker is not a regular file".to_owned(),
        ));
    }
    let marker_bytes = fs::read(&marker_path)?;
    let marker: ReadyMarker = serde_json::from_slice(&marker_bytes)
        .map_err(|error| PythonMaterializationError::InvalidMarker(error.to_string()))?;
    if marker.schema_version != READY_SCHEMA_VERSION {
        return Err(PythonMaterializationError::InvalidMarker(format!(
            "unsupported readiness schema version {}; expected {READY_SCHEMA_VERSION}",
            marker.schema_version
        )));
    }
    if !marker_matches_plan(&marker, plan) {
        return Err(PythonMaterializationError::IncompleteCache(
            "readiness marker does not match the plan".to_owned(),
        ));
    }
    let python = environment_python(&plan.directory);
    if !python.is_file() {
        return Err(PythonMaterializationError::IncompleteCache(
            "readiness marker exists without .venv Python".to_owned(),
        ));
    }
    let lockfile = plan.directory.join("uv.lock");
    let lock_metadata = match fs::symlink_metadata(&lockfile) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(PythonMaterializationError::IncompleteCache(
                "readiness marker exists without uv.lock".to_owned(),
            ));
        }
        Err(error) => return Err(error.into()),
    };
    if lock_metadata.file_type().is_symlink() || !lock_metadata.is_file() {
        return Err(PythonMaterializationError::IncompleteCache(
            "uv.lock is not a regular file".to_owned(),
        ));
    }
    let lock_sha256 = sha256_hex(&fs::read(&lockfile)?);
    if lock_sha256 != marker.lock_sha256 {
        return Err(PythonMaterializationError::IncompleteCache(
            "uv.lock digest does not match the readiness marker".to_owned(),
        ));
    }
    Ok(Some(PreparedPythonEnvironment {
        key: plan.key.clone(),
        directory: plan.directory.clone(),
        python,
        lockfile,
        plan_version: plan.plan_version,
        dependency_count: plan.dependency_count,
        runtime: plan.runtime.clone(),
        sdk_wheel_tag: plan.sdk_wheel_tag.clone(),
        sdk_wheel_sha256: plan.sdk_wheel_sha256.clone(),
        uv_version: plan.uv_version.clone(),
        lock_sha256,
        provider_source_sha256: marker.provider_source_sha256,
        input_plan_key: marker.input_plan_key,
    }))
}

fn marker_matches_plan(marker: &ReadyMarker, plan: &PythonEnvironmentPlan) -> bool {
    marker.environment_key == plan.key
        && marker.plan_version == plan.plan_version
        && marker.dependency_count == plan.dependency_count
        && marker.runtime == plan.runtime
        && marker.sdk_wheel_tag == plan.sdk_wheel_tag
        && marker.sdk_wheel_sha256 == plan.sdk_wheel_sha256
        && marker.uv_version == plan.uv_version
}

fn verify_sdk_digest(path: &Path, expected: &str) -> Result<(), PythonMaterializationError> {
    let actual = sha256_hex(&fs::read(path)?);
    if actual != expected.trim().to_ascii_lowercase() {
        return Err(PythonMaterializationError::SdkDigestMismatch);
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

fn staging_path(target: &Path) -> PathBuf {
    let sequence = STAGING_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let name = target
        .file_name()
        .unwrap_or_else(|| OsStr::new("environment"));
    target.with_file_name(format!(
        ".{}.tmp-{}-{sequence}",
        name.to_string_lossy(),
        std::process::id()
    ))
}

fn environment_python(directory: &Path) -> PathBuf {
    let unix = directory.join(".venv/bin/python");
    if unix.is_file() {
        unix
    } else {
        directory.join(".venv/Scripts/python.exe")
    }
}

fn render_project(metadata: Option<&Pep723Metadata>) -> String {
    let metadata = metadata.cloned().unwrap_or_default();
    let mut project =
        String::from("[project]\nname = \"soma-provider-environment\"\nversion = \"0\"\n");
    if let Some(requires_python) = metadata.requires_python {
        project.push_str(&format!(
            "requires-python = {}\n",
            toml::Value::String(requires_python)
        ));
    }
    project.push_str("dependencies = [\n");
    for dependency in metadata.dependencies {
        project.push_str(&format!("  {},\n", toml::Value::String(dependency)));
    }
    project.push_str("]\n");
    if let Some(uv) = metadata.uv {
        project.push_str("\n[tool.uv]\n");
        if let Some(table) = uv.as_table() {
            for (key, value) in table {
                project.push_str(&format!("{key} = {value}\n"));
            }
        }
    }
    project
}

#[cfg(test)]
#[path = "materializer_tests.rs"]
mod tests;
