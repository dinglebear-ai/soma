//! Conservative planning and application of Python cache cleanup.

use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use serde::Serialize;
use thiserror::Error;

use super::{
    PythonEnvironmentCache, PythonEnvironmentCacheEntry, PythonEnvironmentCacheError,
    PythonEnvironmentCacheState, inspect_entry,
};

static PRUNE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PythonEnvironmentPrunePolicy {
    pub stale_before_unix_seconds: u64,
    pub remove_incomplete: bool,
    pub remove_invalid: bool,
    pub remove_staging: bool,
}

impl PythonEnvironmentPrunePolicy {
    pub fn conservative(stale_before_unix_seconds: u64) -> Self {
        Self {
            stale_before_unix_seconds,
            remove_incomplete: true,
            remove_invalid: true,
            remove_staging: true,
        }
    }

    fn selects(self, entry: &PythonEnvironmentCacheEntry) -> bool {
        let selected_state = match entry.state {
            PythonEnvironmentCacheState::Ready => false,
            PythonEnvironmentCacheState::Incomplete => self.remove_incomplete,
            PythonEnvironmentCacheState::Invalid => self.remove_invalid,
            PythonEnvironmentCacheState::Staging => self.remove_staging,
        };
        selected_state
            && entry
                .modified_unix_seconds
                .is_some_and(|modified| modified <= self.stale_before_unix_seconds)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PythonEnvironmentPruneCandidate {
    pub entry: PythonEnvironmentCacheEntry,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PythonEnvironmentPrunePlan {
    pub root: PathBuf,
    pub policy: PythonEnvironmentPrunePolicy,
    pub candidates: Vec<PythonEnvironmentPruneCandidate>,
    pub reclaimable_size_bytes: u64,
    pub reclaimable_file_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PythonEnvironmentPruneOutcome {
    Removed {
        directory: PathBuf,
        reclaimed_size_bytes: u64,
        reclaimed_file_count: u64,
    },
    Missing {
        directory: PathBuf,
    },
    Changed {
        directory: PathBuf,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PythonEnvironmentPruneReport {
    pub root: PathBuf,
    pub outcomes: Vec<PythonEnvironmentPruneOutcome>,
    pub removed: usize,
    pub missing: usize,
    pub changed: usize,
    pub reclaimed_size_bytes: u64,
    pub reclaimed_file_count: u64,
}

#[derive(Debug, Error)]
pub enum PythonEnvironmentPruneError {
    #[error(transparent)]
    Inventory(#[from] PythonEnvironmentCacheError),
    #[error("Python prune plan root {} does not match cache root {}", plan.display(), cache.display())]
    RootMismatch { plan: PathBuf, cache: PathBuf },
    #[error("Python prune candidate is outside the managed cache: {}", path.display())]
    OutsideCache { path: PathBuf },
    #[error("Python prune plans may never delete ready environments: {}", path.display())]
    ReadyEnvironment { path: PathBuf },
    #[error("Python prune I/O failed at {}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl PythonEnvironmentCache {
    pub fn plan_prune(
        &self,
        policy: PythonEnvironmentPrunePolicy,
    ) -> Result<PythonEnvironmentPrunePlan, PythonEnvironmentPruneError> {
        let inventory = self.inventory()?;
        let candidates = inventory
            .entries
            .into_iter()
            .filter(|entry| policy.selects(entry))
            .map(|entry| PythonEnvironmentPruneCandidate {
                reason: entry
                    .issue
                    .clone()
                    .unwrap_or_else(|| "selected stale cache entry".to_owned()),
                entry,
            })
            .collect::<Vec<_>>();
        let reclaimable_size_bytes = candidates.iter().fold(0_u64, |total, candidate| {
            total.saturating_add(candidate.entry.size_bytes)
        });
        let reclaimable_file_count = candidates.iter().fold(0_u64, |total, candidate| {
            total.saturating_add(candidate.entry.file_count)
        });
        Ok(PythonEnvironmentPrunePlan {
            root: self.root.clone(),
            policy,
            candidates,
            reclaimable_size_bytes,
            reclaimable_file_count,
        })
    }

    pub fn apply_prune(
        &self,
        plan: &PythonEnvironmentPrunePlan,
    ) -> Result<PythonEnvironmentPruneReport, PythonEnvironmentPruneError> {
        if plan.root != self.root {
            return Err(PythonEnvironmentPruneError::RootMismatch {
                plan: plan.root.clone(),
                cache: self.root.clone(),
            });
        }
        for candidate in &plan.candidates {
            validate_candidate_path(&self.root, &candidate.entry)?;
            if candidate.entry.state == PythonEnvironmentCacheState::Ready {
                return Err(PythonEnvironmentPruneError::ReadyEnvironment {
                    path: candidate.entry.directory.clone(),
                });
            }
        }

        let current_inventory = self.inventory()?;
        let mut outcomes = Vec::with_capacity(plan.candidates.len());
        for candidate in &plan.candidates {
            let path = &candidate.entry.directory;
            let Some(current) = current_inventory
                .entries
                .iter()
                .find(|entry| entry.directory == *path)
            else {
                outcomes.push(PythonEnvironmentPruneOutcome::Missing {
                    directory: path.clone(),
                });
                continue;
            };
            if current != &candidate.entry || !plan.policy.selects(current) {
                outcomes.push(PythonEnvironmentPruneOutcome::Changed {
                    directory: path.clone(),
                });
                continue;
            }
            let revalidated = match fs::symlink_metadata(path) {
                Ok(_) => inspect_entry(path, current.plan_directory_version)?,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    outcomes.push(PythonEnvironmentPruneOutcome::Missing {
                        directory: path.clone(),
                    });
                    continue;
                }
                Err(source) => {
                    return Err(PythonEnvironmentPruneError::Io {
                        path: path.clone(),
                        source,
                    });
                }
            };
            if revalidated != *current || !plan.policy.selects(&revalidated) {
                outcomes.push(PythonEnvironmentPruneOutcome::Changed {
                    directory: path.clone(),
                });
                continue;
            }

            let quarantine = quarantine_path(path)?;
            match fs::rename(path, &quarantine) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    outcomes.push(PythonEnvironmentPruneOutcome::Missing {
                        directory: path.clone(),
                    });
                    continue;
                }
                Err(source) => {
                    return Err(PythonEnvironmentPruneError::Io {
                        path: path.clone(),
                        source,
                    });
                }
            }
            remove_quarantine(&quarantine)?;
            outcomes.push(PythonEnvironmentPruneOutcome::Removed {
                directory: path.clone(),
                reclaimed_size_bytes: current.size_bytes,
                reclaimed_file_count: current.file_count,
            });
        }

        let mut report = PythonEnvironmentPruneReport {
            root: self.root.clone(),
            outcomes,
            removed: 0,
            missing: 0,
            changed: 0,
            reclaimed_size_bytes: 0,
            reclaimed_file_count: 0,
        };
        for outcome in &report.outcomes {
            match outcome {
                PythonEnvironmentPruneOutcome::Removed {
                    reclaimed_size_bytes,
                    reclaimed_file_count,
                    ..
                } => {
                    report.removed += 1;
                    report.reclaimed_size_bytes = report
                        .reclaimed_size_bytes
                        .saturating_add(*reclaimed_size_bytes);
                    report.reclaimed_file_count = report
                        .reclaimed_file_count
                        .saturating_add(*reclaimed_file_count);
                }
                PythonEnvironmentPruneOutcome::Missing { .. } => report.missing += 1,
                PythonEnvironmentPruneOutcome::Changed { .. } => report.changed += 1,
            }
        }
        Ok(report)
    }
}

fn validate_candidate_path(
    root: &Path,
    entry: &PythonEnvironmentCacheEntry,
) -> Result<(), PythonEnvironmentPruneError> {
    let path = &entry.directory;
    let parent = path.parent();
    let managed = parent == Some(root)
        || parent
            .and_then(Path::parent)
            .is_some_and(|grandparent| grandparent == root);
    if managed {
        Ok(())
    } else {
        Err(PythonEnvironmentPruneError::OutsideCache { path: path.clone() })
    }
}

fn quarantine_path(path: &Path) -> Result<PathBuf, PythonEnvironmentPruneError> {
    let parent = path
        .parent()
        .ok_or_else(|| PythonEnvironmentPruneError::OutsideCache {
            path: path.to_path_buf(),
        })?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("environment");
    loop {
        let sequence = PRUNE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(".{name}.prune-{}-{sequence}", std::process::id()));
        match fs::symlink_metadata(&candidate) {
            Ok(_) => continue,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(candidate),
            Err(source) => {
                return Err(PythonEnvironmentPruneError::Io {
                    path: candidate,
                    source,
                });
            }
        }
    }
}

fn remove_quarantine(path: &Path) -> Result<(), PythonEnvironmentPruneError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|source| PythonEnvironmentPruneError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    let result = if metadata.file_type().is_symlink() || !metadata.is_dir() {
        fs::remove_file(path)
    } else {
        fs::remove_dir_all(path)
    };
    result.map_err(|source| PythonEnvironmentPruneError::Io {
        path: path.to_path_buf(),
        source,
    })
}
