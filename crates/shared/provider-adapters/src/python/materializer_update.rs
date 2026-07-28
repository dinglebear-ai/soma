//! Explicit dependency update into an immutable candidate environment.

use std::{
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use serde::Serialize;
use thiserror::Error;

use super::{
    environment_python, open_ready, render_project, sha256_hex, verify_sdk_digest,
    PreparedPythonEnvironment, PythonEnvironmentMaterializer, PythonMaterializationError,
    PythonMaterializationRequest, ReadyMarker, UvRunner, READY_FILE, READY_SCHEMA_VERSION,
};
use crate::python::environment::{Pep723Metadata, PythonEnvironmentPlan};

const UPDATE_PLAN_VERSION: u32 = 3;
static UPDATE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy)]
pub struct PythonEnvironmentUpdateRequest<'a> {
    pub materialization: PythonMaterializationRequest<'a>,
    pub provider_source_sha256: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PythonEnvironmentUpdateOutcome {
    Prepared,
    Reused,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonEnvironmentUpdateReport {
    pub outcome: PythonEnvironmentUpdateOutcome,
    pub current: Option<PreparedPythonEnvironment>,
    pub candidate: PreparedPythonEnvironment,
}

#[derive(Debug, Error)]
pub enum PythonEnvironmentUpdateError {
    #[error("provider source SHA-256 must contain exactly 64 hexadecimal characters")]
    InvalidSourceDigest,
    #[error("current Python environment must be repaired before update: {0}")]
    CurrentInvalid(String),
    #[error("resolved Python update candidate is invalid: {0}")]
    CandidateInvalid(String),
    #[error("Python update cache plan has no managed cache root")]
    MissingCacheRoot,
    #[error("Python update cache path is not a real directory: {}", path.display())]
    UnsafeCachePath { path: PathBuf },
    #[error("uv command failed during update {operation}: {message}")]
    Uv {
        operation: &'static str,
        message: String,
    },
    #[error(transparent)]
    Materialization(#[from] PythonMaterializationError),
    #[error("Python update I/O failed: {0}")]
    Io(#[from] io::Error),
}

impl<R: UvRunner> PythonEnvironmentMaterializer<R> {
    pub fn update(
        &self,
        plan: &PythonEnvironmentPlan,
        request: PythonEnvironmentUpdateRequest<'_>,
    ) -> Result<PythonEnvironmentUpdateReport, PythonEnvironmentUpdateError> {
        let source_sha256 = normalize_digest(request.provider_source_sha256)?;
        let current = match open_ready(plan) {
            Ok(environment) => environment,
            Err(PythonMaterializationError::IncompleteCache(message))
            | Err(PythonMaterializationError::InvalidMarker(message)) => {
                return Err(PythonEnvironmentUpdateError::CurrentInvalid(message));
            }
            Err(error) => return Err(error.into()),
        };
        verify_sdk_digest(request.materialization.sdk_wheel, &plan.sdk_wheel_sha256)?;

        let python_cache_root = plan
            .directory
            .parent()
            .and_then(Path::parent)
            .ok_or(PythonEnvironmentUpdateError::MissingCacheRoot)?;
        ensure_real_directory(python_cache_root, true)?;
        let candidate_parent = python_cache_root.join(format!("v{UPDATE_PLAN_VERSION}"));
        ensure_real_directory(&candidate_parent, false)?;
        let staging = update_staging_path(&candidate_parent, &plan.key)?;
        fs::create_dir(&staging)?;

        let result = self.resolve_and_prepare_update(
            plan,
            request,
            &source_sha256,
            &candidate_parent,
            &staging,
        );
        if result.is_err() {
            let _ = fs::remove_dir_all(&staging);
        }
        let (outcome, candidate) = result?;
        Ok(PythonEnvironmentUpdateReport {
            outcome,
            current,
            candidate,
        })
    }

    fn resolve_and_prepare_update(
        &self,
        plan: &PythonEnvironmentPlan,
        request: PythonEnvironmentUpdateRequest<'_>,
        source_sha256: &str,
        candidate_parent: &Path,
        staging: &Path,
    ) -> Result<
        (PythonEnvironmentUpdateOutcome, PreparedPythonEnvironment),
        PythonEnvironmentUpdateError,
    > {
        fs::write(
            staging.join("pyproject.toml"),
            render_project(request.materialization.metadata),
        )?;
        let mut lock_args = vec![
            OsString::from("lock"),
            OsString::from("--upgrade"),
            OsString::from("--project"),
            OsString::from("."),
            OsString::from("--python"),
            request
                .materialization
                .python_executable
                .as_os_str()
                .to_owned(),
        ];
        if request.materialization.offline {
            lock_args.push(OsString::from("--offline"));
        }
        self.update_uv("lock", staging, &lock_args)?;

        let lockfile = staging.join("uv.lock");
        let lock = fs::read(&lockfile)?;
        let lock_sha256 = sha256_hex(&lock);
        let candidate_key = resolved_candidate_key(
            plan,
            request.materialization.metadata,
            source_sha256,
            &lock_sha256,
        );
        let candidate_plan = PythonEnvironmentPlan {
            key: candidate_key.clone(),
            directory: candidate_parent.join(&candidate_key),
            plan_version: UPDATE_PLAN_VERSION,
            dependency_count: plan.dependency_count,
            runtime: plan.runtime.clone(),
            sdk_wheel_tag: plan.sdk_wheel_tag.clone(),
            sdk_wheel_sha256: plan.sdk_wheel_sha256.clone(),
            uv_version: plan.uv_version.clone(),
        };

        match open_ready(&candidate_plan) {
            Ok(Some(candidate)) => {
                validate_candidate_identity(&candidate, source_sha256, &plan.key)?;
                fs::remove_dir_all(staging)?;
                return Ok((PythonEnvironmentUpdateOutcome::Reused, candidate));
            }
            Ok(None) => {}
            Err(error) => {
                return Err(PythonEnvironmentUpdateError::CandidateInvalid(
                    error.to_string(),
                ));
            }
        }

        let mut sync_args = vec![
            OsString::from("sync"),
            OsString::from("--project"),
            OsString::from("."),
            OsString::from("--locked"),
            OsString::from("--no-install-project"),
            OsString::from("--python"),
            request
                .materialization
                .python_executable
                .as_os_str()
                .to_owned(),
        ];
        if request.materialization.offline {
            sync_args.push(OsString::from("--offline"));
        }
        self.update_uv("sync", staging, &sync_args)?;
        let python = environment_python(staging);
        self.update_uv(
            "SDK install",
            staging,
            &[
                OsString::from("pip"),
                OsString::from("install"),
                OsString::from("--python"),
                python.as_os_str().to_owned(),
                OsString::from("--offline"),
                OsString::from("--no-deps"),
                request.materialization.sdk_wheel.as_os_str().to_owned(),
            ],
        )?;
        if !python.is_file() || !lockfile.is_file() {
            return Err(PythonMaterializationError::IncompleteCache(
                "update did not create both .venv Python and uv.lock".to_owned(),
            )
            .into());
        }
        let marker = ReadyMarker {
            schema_version: READY_SCHEMA_VERSION,
            environment_key: candidate_plan.key.clone(),
            plan_version: candidate_plan.plan_version,
            dependency_count: candidate_plan.dependency_count,
            runtime: candidate_plan.runtime.clone(),
            sdk_wheel_tag: candidate_plan.sdk_wheel_tag.clone(),
            sdk_wheel_sha256: candidate_plan.sdk_wheel_sha256.clone(),
            uv_version: candidate_plan.uv_version.clone(),
            lock_sha256,
            provider_source_sha256: Some(source_sha256.to_owned()),
            input_plan_key: Some(plan.key.clone()),
        };
        fs::write(
            staging.join(READY_FILE),
            serde_json::to_vec_pretty(&marker).expect("update marker is serializable"),
        )?;

        match fs::rename(staging, &candidate_plan.directory) {
            Ok(()) => {}
            Err(_) if fs::symlink_metadata(&candidate_plan.directory).is_ok() => {
                fs::remove_dir_all(staging)?;
            }
            Err(error) => return Err(error.into()),
        }
        let candidate = open_ready(&candidate_plan)?.ok_or(
            PythonEnvironmentUpdateError::CandidateInvalid(candidate_key),
        )?;
        validate_candidate_identity(&candidate, source_sha256, &plan.key)?;
        Ok((PythonEnvironmentUpdateOutcome::Prepared, candidate))
    }

    fn update_uv(
        &self,
        operation: &'static str,
        current_dir: &Path,
        args: &[OsString],
    ) -> Result<(), PythonEnvironmentUpdateError> {
        self.runner
            .run(&self.uv_program, args, current_dir)
            .map_err(|message| PythonEnvironmentUpdateError::Uv { operation, message })
    }
}

fn ensure_real_directory(
    path: &Path,
    create_parents: bool,
) -> Result<(), PythonEnvironmentUpdateError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(PythonEnvironmentUpdateError::UnsafeCachePath {
                    path: path.to_path_buf(),
                });
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            if create_parents {
                fs::create_dir_all(path)?;
            } else {
                fs::create_dir(path)?;
            }
            let metadata = fs::symlink_metadata(path)?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(PythonEnvironmentUpdateError::UnsafeCachePath {
                    path: path.to_path_buf(),
                });
            }
        }
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn validate_candidate_identity(
    candidate: &PreparedPythonEnvironment,
    source_sha256: &str,
    input_plan_key: &str,
) -> Result<(), PythonEnvironmentUpdateError> {
    if candidate.provider_source_sha256.as_deref() != Some(source_sha256)
        || candidate.input_plan_key.as_deref() != Some(input_plan_key)
    {
        return Err(PythonEnvironmentUpdateError::CandidateInvalid(
            "readiness identity does not match update request".to_owned(),
        ));
    }
    Ok(())
}

fn normalize_digest(value: &str) -> Result<String, PythonEnvironmentUpdateError> {
    let value = value.trim().to_ascii_lowercase();
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(PythonEnvironmentUpdateError::InvalidSourceDigest);
    }
    Ok(value)
}

#[derive(Serialize)]
struct ResolvedCandidateFingerprint<'a> {
    policy_version: u32,
    provider_source_sha256: &'a str,
    input_plan_key: &'a str,
    metadata: Option<&'a Pep723Metadata>,
    runtime: &'a crate::python::environment::PythonRuntimeFingerprint,
    sdk_wheel_tag: &'a crate::python::environment::PythonWheelTag,
    sdk_wheel_sha256: &'a str,
    uv_version: &'a str,
    lock_sha256: &'a str,
}

fn resolved_candidate_key(
    plan: &PythonEnvironmentPlan,
    metadata: Option<&Pep723Metadata>,
    source_sha256: &str,
    lock_sha256: &str,
) -> String {
    let fingerprint = ResolvedCandidateFingerprint {
        policy_version: UPDATE_PLAN_VERSION,
        provider_source_sha256: source_sha256,
        input_plan_key: &plan.key,
        metadata,
        runtime: &plan.runtime,
        sdk_wheel_tag: &plan.sdk_wheel_tag,
        sdk_wheel_sha256: &plan.sdk_wheel_sha256,
        uv_version: &plan.uv_version,
        lock_sha256,
    };
    sha256_hex(&serde_json::to_vec(&fingerprint).expect("candidate fingerprint is serializable"))
}

fn update_staging_path(parent: &Path, input_plan_key: &str) -> io::Result<PathBuf> {
    loop {
        let sequence = UPDATE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(
            ".{input_plan_key}.update-{}-{sequence}",
            std::process::id()
        ));
        match fs::symlink_metadata(&candidate) {
            Ok(_) => continue,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(candidate),
            Err(error) => return Err(error),
        }
    }
}
