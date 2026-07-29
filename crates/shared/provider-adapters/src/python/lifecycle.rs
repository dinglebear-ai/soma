//! Provider-source preparation that composes PEP 723 discovery, deterministic
//! environment planning, and atomic uv materialization.
//!
//! The lifecycle is product-neutral: every path, digest, runtime identity, uv
//! version, and offline policy is supplied explicitly by the caller.

use std::{
    fs,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};
use thiserror::Error;

use super::{
    environment::{
        PythonEnvironmentError, PythonRuntimeFingerprint, parse_pep723_metadata,
        plan_python_environment,
    },
    materializer::{
        PreparedPythonEnvironment, PythonEnvironmentMaterializer, PythonEnvironmentUpdateError,
        PythonEnvironmentUpdateReport, PythonEnvironmentUpdateRequest, PythonMaterializationError,
        PythonMaterializationRequest, SystemUvRunner, UvRunner,
    },
};

/// Immutable inputs used to plan and materialize one provider environment.
#[derive(Debug, Clone)]
pub struct PythonEnvironmentSpec {
    pub cache_root: PathBuf,
    pub runtime: PythonRuntimeFingerprint,
    pub python_executable: PathBuf,
    pub sdk_wheel: PathBuf,
    pub sdk_wheel_sha256: String,
    pub uv_version: String,
    pub offline: bool,
}

/// Composes metadata discovery, planning, and materialization for provider files.
pub struct PythonEnvironmentLifecycle<R = SystemUvRunner> {
    spec: PythonEnvironmentSpec,
    materializer: PythonEnvironmentMaterializer<R>,
}

impl PythonEnvironmentLifecycle<SystemUvRunner> {
    pub fn new(uv_program: impl Into<PathBuf>, spec: PythonEnvironmentSpec) -> Self {
        Self {
            spec,
            materializer: PythonEnvironmentMaterializer::new(uv_program),
        }
    }
}

impl<R: UvRunner> PythonEnvironmentLifecycle<R> {
    pub fn with_runner(
        uv_program: impl Into<PathBuf>,
        spec: PythonEnvironmentSpec,
        runner: R,
    ) -> Self {
        Self {
            spec,
            materializer: PythonEnvironmentMaterializer::with_runner(uv_program, runner),
        }
    }

    /// Prepares the immutable environment for `provider_path`.
    ///
    /// Metadata parsing happens directly from source and never imports provider
    /// code. A complete content-addressed cache is reopened without invoking uv.
    pub fn prepare_provider(
        &self,
        provider_path: &Path,
    ) -> Result<PreparedPythonEnvironment, PythonEnvironmentLifecycleError> {
        let source = fs::read_to_string(provider_path).map_err(|source| {
            PythonEnvironmentLifecycleError::ReadSource {
                path: provider_path.to_path_buf(),
                source,
            }
        })?;
        let metadata = parse_pep723_metadata(&source)?;
        let plan = plan_python_environment(
            &self.spec.cache_root,
            metadata.as_ref(),
            &self.spec.runtime,
            &self.spec.sdk_wheel,
            &self.spec.sdk_wheel_sha256,
            &self.spec.uv_version,
        )?;
        self.materializer
            .prepare(
                &plan,
                PythonMaterializationRequest {
                    metadata: metadata.as_ref(),
                    python_executable: &self.spec.python_executable,
                    sdk_wheel: &self.spec.sdk_wheel,
                    offline: self.spec.offline,
                },
            )
            .map_err(Into::into)
    }

    /// Resolves dependencies into a new immutable candidate generation.
    ///
    /// The current prepared environment is never replaced or activated here.
    pub fn validate_provider_candidate(
        &self,
        provider_path: &Path,
        candidate: &PreparedPythonEnvironment,
    ) -> Result<PreparedPythonEnvironment, PythonEnvironmentLifecycleError> {
        let source = fs::read_to_string(provider_path).map_err(|source| {
            PythonEnvironmentLifecycleError::ReadSource {
                path: provider_path.to_path_buf(),
                source,
            }
        })?;
        let metadata = parse_pep723_metadata(&source)?;
        let input_plan = plan_python_environment(
            &self.spec.cache_root,
            metadata.as_ref(),
            &self.spec.runtime,
            &self.spec.sdk_wheel,
            &self.spec.sdk_wheel_sha256,
            &self.spec.uv_version,
        )?;
        let source_sha256 = normalized_source_sha256(&source);
        if candidate.provider_source_sha256.as_deref() != Some(source_sha256.as_str()) {
            return Err(PythonEnvironmentLifecycleError::CandidateSourceMismatch);
        }
        if candidate.input_plan_key.as_deref() != Some(input_plan.key.as_str()) {
            return Err(PythonEnvironmentLifecycleError::CandidatePlanMismatch);
        }
        self.materializer
            .validate_prepared(candidate)
            .map_err(Into::into)
    }

    pub fn update_provider(
        &self,
        provider_path: &Path,
    ) -> Result<PythonEnvironmentUpdateReport, PythonEnvironmentLifecycleError> {
        let source = fs::read_to_string(provider_path).map_err(|source| {
            PythonEnvironmentLifecycleError::ReadSource {
                path: provider_path.to_path_buf(),
                source,
            }
        })?;
        let metadata = parse_pep723_metadata(&source)?;
        let plan = plan_python_environment(
            &self.spec.cache_root,
            metadata.as_ref(),
            &self.spec.runtime,
            &self.spec.sdk_wheel,
            &self.spec.sdk_wheel_sha256,
            &self.spec.uv_version,
        )?;
        let source_sha256 = normalized_source_sha256(&source);
        self.materializer
            .update(
                &plan,
                PythonEnvironmentUpdateRequest {
                    materialization: PythonMaterializationRequest {
                        metadata: metadata.as_ref(),
                        python_executable: &self.spec.python_executable,
                        sdk_wheel: &self.spec.sdk_wheel,
                        offline: self.spec.offline,
                    },
                    provider_source_sha256: &source_sha256,
                },
            )
            .map_err(Into::into)
    }
}

fn normalized_source_sha256(source: &str) -> String {
    let normalized = source.replace("\r\n", "\n").replace('\r', "\n");
    let digest = Sha256::digest(normalized.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[derive(Debug, Error)]
pub enum PythonEnvironmentLifecycleError {
    #[error("failed to read Python provider source {}: {source}", path.display())]
    ReadSource {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Environment(#[from] PythonEnvironmentError),
    #[error(transparent)]
    Materialization(#[from] PythonMaterializationError),
    #[error(transparent)]
    Update(#[from] PythonEnvironmentUpdateError),
    #[error("Python candidate source digest does not match the provider file")]
    CandidateSourceMismatch,
    #[error("Python candidate input plan does not match the provider file")]
    CandidatePlanMismatch,
}

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod tests;
