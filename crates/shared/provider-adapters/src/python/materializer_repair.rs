//! Atomic repair of one exact planned Python environment.

use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use thiserror::Error;

use super::{
    PreparedPythonEnvironment, PythonEnvironmentMaterializer, PythonMaterializationError,
    PythonMaterializationRequest, UvRunner, open_ready, verify_sdk_digest,
};
use crate::python::environment::PythonEnvironmentPlan;

static REPAIR_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PythonEnvironmentRepairOutcome {
    Healthy,
    Prepared,
    Rebuilt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonEnvironmentRepairReport {
    pub outcome: PythonEnvironmentRepairOutcome,
    pub environment: PreparedPythonEnvironment,
    pub replaced_error: Option<String>,
    pub cleanup_pending: Option<PathBuf>,
}

#[derive(Debug, Error)]
pub enum PythonEnvironmentRepairError {
    #[error(transparent)]
    Materialization(#[from] PythonMaterializationError),
    #[error("failed to quarantine Python environment {}: {source}", path.display())]
    Quarantine {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(
        "Python environment rebuild failed ({build_error}) and the original cache could not be restored from {} to {}: {source}",
        quarantine.display(),
        original.display()
    )]
    Restore {
        original: PathBuf,
        quarantine: PathBuf,
        build_error: String,
        #[source]
        source: io::Error,
    },
    #[error(
        "Python environment rebuild failed ({build_error}); recovery is preserved at {} because {} is occupied",
        quarantine.display(),
        original.display()
    )]
    RestoreBlocked {
        original: PathBuf,
        quarantine: PathBuf,
        build_error: String,
    },
}

impl<R: UvRunner> PythonEnvironmentMaterializer<R> {
    pub fn repair(
        &self,
        plan: &PythonEnvironmentPlan,
        request: PythonMaterializationRequest<'_>,
    ) -> Result<PythonEnvironmentRepairReport, PythonEnvironmentRepairError> {
        let replaced_error = match open_ready(plan) {
            Ok(Some(environment)) => {
                return Ok(PythonEnvironmentRepairReport {
                    outcome: PythonEnvironmentRepairOutcome::Healthy,
                    environment,
                    replaced_error: None,
                    cleanup_pending: None,
                });
            }
            Ok(None) => {
                let environment = self.prepare(plan, request)?;
                return Ok(PythonEnvironmentRepairReport {
                    outcome: PythonEnvironmentRepairOutcome::Prepared,
                    environment,
                    replaced_error: None,
                    cleanup_pending: None,
                });
            }
            Err(error @ PythonMaterializationError::IncompleteCache(_))
            | Err(error @ PythonMaterializationError::InvalidMarker(_)) => error.to_string(),
            Err(error) => return Err(error.into()),
        };

        if request.offline {
            return Err(PythonMaterializationError::OfflineCacheMiss(plan.key.clone()).into());
        }
        verify_sdk_digest(request.sdk_wheel, &plan.sdk_wheel_sha256)?;

        let quarantine = repair_quarantine_path(&plan.directory)?;
        match fs::rename(&plan.directory, &quarantine) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let environment = self.prepare(plan, request)?;
                return Ok(PythonEnvironmentRepairReport {
                    outcome: PythonEnvironmentRepairOutcome::Prepared,
                    environment,
                    replaced_error: None,
                    cleanup_pending: None,
                });
            }
            Err(source) => {
                return Err(PythonEnvironmentRepairError::Quarantine {
                    path: plan.directory.clone(),
                    source,
                });
            }
        }

        match self.prepare(plan, request) {
            Ok(environment) => {
                let cleanup_pending = remove_repair_quarantine(&quarantine)
                    .err()
                    .map(|_| quarantine.clone());
                Ok(PythonEnvironmentRepairReport {
                    outcome: PythonEnvironmentRepairOutcome::Rebuilt,
                    environment,
                    replaced_error: Some(replaced_error),
                    cleanup_pending,
                })
            }
            Err(build_error) => {
                restore_after_failed_rebuild(&plan.directory, &quarantine, build_error)
            }
        }
    }
}

fn restore_after_failed_rebuild(
    original: &Path,
    quarantine: &Path,
    build_error: PythonMaterializationError,
) -> Result<PythonEnvironmentRepairReport, PythonEnvironmentRepairError> {
    let build_message = build_error.to_string();
    match fs::symlink_metadata(original) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::rename(quarantine, original).map_err(|source| {
                PythonEnvironmentRepairError::Restore {
                    original: original.to_path_buf(),
                    quarantine: quarantine.to_path_buf(),
                    build_error: build_message,
                    source,
                }
            })?;
            Err(PythonEnvironmentRepairError::Materialization(build_error))
        }
        Ok(_) => Err(PythonEnvironmentRepairError::RestoreBlocked {
            original: original.to_path_buf(),
            quarantine: quarantine.to_path_buf(),
            build_error: build_message,
        }),
        Err(source) => Err(PythonEnvironmentRepairError::Restore {
            original: original.to_path_buf(),
            quarantine: quarantine.to_path_buf(),
            build_error: build_message,
            source,
        }),
    }
}

fn repair_quarantine_path(path: &Path) -> Result<PathBuf, PythonEnvironmentRepairError> {
    let parent = path
        .parent()
        .ok_or_else(|| PythonEnvironmentRepairError::Quarantine {
            path: path.to_path_buf(),
            source: io::Error::new(io::ErrorKind::InvalidInput, "cache plan has no parent"),
        })?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("environment");
    loop {
        let sequence = REPAIR_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(".{name}.repair-{}-{sequence}", std::process::id()));
        match fs::symlink_metadata(&candidate) {
            Ok(_) => continue,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(candidate),
            Err(source) => {
                return Err(PythonEnvironmentRepairError::Quarantine {
                    path: candidate,
                    source,
                });
            }
        }
    }
}

fn remove_repair_quarantine(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        fs::remove_file(path)
    } else {
        fs::remove_dir_all(path)
    }
}
