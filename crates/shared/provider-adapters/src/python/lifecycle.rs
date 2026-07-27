//! Provider-source preparation that composes PEP 723 discovery, deterministic
//! environment planning, and atomic uv materialization.
//!
//! The lifecycle is product-neutral: every path, digest, runtime identity, uv
//! version, and offline policy is supplied explicitly by the caller.

use std::{
    fs,
    path::{Path, PathBuf},
};

use thiserror::Error;

use super::{
    environment::{
        parse_pep723_metadata, plan_python_environment, PythonEnvironmentError,
        PythonRuntimeFingerprint,
    },
    materializer::{
        PreparedPythonEnvironment, PythonEnvironmentMaterializer, PythonMaterializationError,
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
                    sdk_wheel_sha256: &self.spec.sdk_wheel_sha256,
                    uv_version: &self.spec.uv_version,
                    offline: self.spec.offline,
                },
            )
            .map_err(Into::into)
    }
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
}

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod tests;
