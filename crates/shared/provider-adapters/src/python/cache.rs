//! Read-only inventory for content-addressed Python environment caches.
//!
//! Inventory never executes Python, invokes uv, follows symlinks, or mutates a
//! cache entry. It classifies every visible entry so later prune and repair
//! operations can make decisions from one shared model.

use std::{
    fs, io,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use serde::Serialize;
use sha2::{Digest, Sha256};
use thiserror::Error;

use super::{
    environment::{PythonRuntimeFingerprint, PythonWheelTag},
    materializer::{READY_FILE, READY_SCHEMA_VERSION, ReadyMarker},
};

#[path = "cache_prune.rs"]
mod prune;
pub use prune::{
    PythonEnvironmentPruneCandidate, PythonEnvironmentPruneError, PythonEnvironmentPruneOutcome,
    PythonEnvironmentPrunePlan, PythonEnvironmentPrunePolicy, PythonEnvironmentPruneReport,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PythonEnvironmentCacheState {
    Ready,
    Incomplete,
    Invalid,
    Staging,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PythonEnvironmentCacheMetadata {
    pub schema_version: u32,
    pub environment_key: String,
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

impl From<ReadyMarker> for PythonEnvironmentCacheMetadata {
    fn from(marker: ReadyMarker) -> Self {
        Self {
            schema_version: marker.schema_version,
            environment_key: marker.environment_key,
            plan_version: marker.plan_version,
            dependency_count: marker.dependency_count,
            runtime: marker.runtime,
            sdk_wheel_tag: marker.sdk_wheel_tag,
            sdk_wheel_sha256: marker.sdk_wheel_sha256,
            uv_version: marker.uv_version,
            lock_sha256: marker.lock_sha256,
            provider_source_sha256: marker.provider_source_sha256,
            input_plan_key: marker.input_plan_key,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PythonEnvironmentCacheEntry {
    pub directory: PathBuf,
    pub key: Option<String>,
    pub plan_directory_version: Option<u32>,
    pub state: PythonEnvironmentCacheState,
    pub size_bytes: u64,
    pub file_count: u64,
    pub modified_unix_seconds: Option<u64>,
    pub metadata: Option<PythonEnvironmentCacheMetadata>,
    pub issue: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PythonEnvironmentCacheSummary {
    pub ready: usize,
    pub incomplete: usize,
    pub invalid: usize,
    pub staging: usize,
    pub total_size_bytes: u64,
    pub total_file_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PythonEnvironmentCacheInventory {
    pub root: PathBuf,
    pub entries: Vec<PythonEnvironmentCacheEntry>,
    pub summary: PythonEnvironmentCacheSummary,
}

#[derive(Debug, Error)]
pub enum PythonEnvironmentCacheError {
    #[error("Python environment cache I/O failed at {}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("Python environment cache root is not a real directory: {}", path.display())]
    UnsafeRoot { path: PathBuf },
}

#[derive(Debug, Clone)]
pub struct PythonEnvironmentCache {
    root: PathBuf,
}

impl PythonEnvironmentCache {
    pub fn new(cache_root: impl Into<PathBuf>) -> Self {
        Self {
            root: cache_root.into().join("python"),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn inventory(
        &self,
    ) -> Result<PythonEnvironmentCacheInventory, PythonEnvironmentCacheError> {
        let root_metadata = match fs::symlink_metadata(&self.root) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(PythonEnvironmentCacheInventory {
                    root: self.root.clone(),
                    entries: Vec::new(),
                    summary: PythonEnvironmentCacheSummary::default(),
                });
            }
            Err(source) => {
                return Err(PythonEnvironmentCacheError::Io {
                    path: self.root.clone(),
                    source,
                });
            }
        };
        if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
            return Err(PythonEnvironmentCacheError::UnsafeRoot {
                path: self.root.clone(),
            });
        }

        let mut version_directories = read_paths(&self.root)?;
        version_directories.sort();
        let mut entries = Vec::new();
        for version_directory in version_directories {
            let version_metadata = symlink_metadata(&version_directory)?;
            if version_metadata.file_type().is_symlink() || !version_metadata.is_dir() {
                entries.push(classify_non_directory(
                    &version_directory,
                    None,
                    version_metadata,
                ));
                continue;
            }
            let plan_directory_version = version_directory
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.strip_prefix('v'))
                .and_then(|version| version.parse::<u32>().ok());
            let mut children = read_paths(&version_directory)?;
            children.sort();
            for child in children {
                entries.push(inspect_entry(&child, plan_directory_version)?);
            }
        }
        entries.sort_by(|left, right| left.directory.cmp(&right.directory));
        let summary = summarize(&entries);
        Ok(PythonEnvironmentCacheInventory {
            root: self.root.clone(),
            entries,
            summary,
        })
    }
}

fn inspect_entry(
    directory: &Path,
    plan_directory_version: Option<u32>,
) -> Result<PythonEnvironmentCacheEntry, PythonEnvironmentCacheError> {
    let metadata = symlink_metadata(directory)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Ok(classify_non_directory(
            directory,
            plan_directory_version,
            metadata,
        ));
    }
    let stats = tree_stats(directory)?;
    let name = directory
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if name.starts_with('.')
        && (name.contains(".tmp-")
            || name.contains(".prune-")
            || name.contains(".repair-")
            || name.contains(".update-"))
    {
        let issue = if name.contains(".prune-") {
            "temporary prune quarantine"
        } else if name.contains(".repair-") {
            "temporary repair quarantine"
        } else if name.contains(".update-") {
            "temporary update candidate"
        } else {
            "temporary materialization directory"
        };
        return Ok(entry(
            directory,
            None,
            plan_directory_version,
            PythonEnvironmentCacheState::Staging,
            stats,
            None,
            Some(issue.to_owned()),
        ));
    }

    let key = Some(name.to_owned());
    let marker_path = directory.join(READY_FILE);
    let marker_metadata = match fs::symlink_metadata(&marker_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(entry(
                directory,
                key,
                plan_directory_version,
                PythonEnvironmentCacheState::Incomplete,
                stats,
                None,
                Some("readiness marker is missing".to_owned()),
            ));
        }
        Err(source) => {
            return Err(PythonEnvironmentCacheError::Io {
                path: marker_path,
                source,
            });
        }
    };
    if marker_metadata.file_type().is_symlink() || !marker_metadata.is_file() {
        return Ok(entry(
            directory,
            key,
            plan_directory_version,
            PythonEnvironmentCacheState::Invalid,
            stats,
            None,
            Some("readiness marker is not a regular file".to_owned()),
        ));
    }
    let marker_bytes =
        fs::read(&marker_path).map_err(|source| PythonEnvironmentCacheError::Io {
            path: marker_path,
            source,
        })?;
    let marker: ReadyMarker = match serde_json::from_slice(&marker_bytes) {
        Ok(marker) => marker,
        Err(error) => {
            return Ok(entry(
                directory,
                key,
                plan_directory_version,
                PythonEnvironmentCacheState::Invalid,
                stats,
                None,
                Some(format!("readiness marker is invalid: {error}")),
            ));
        }
    };
    let cache_metadata = PythonEnvironmentCacheMetadata::from(marker);
    let issue = validate_ready_entry(directory, name, plan_directory_version, &cache_metadata)?;
    let state = if issue.is_some() {
        PythonEnvironmentCacheState::Invalid
    } else {
        PythonEnvironmentCacheState::Ready
    };
    Ok(entry(
        directory,
        key,
        plan_directory_version,
        state,
        stats,
        Some(cache_metadata),
        issue,
    ))
}

fn validate_ready_entry(
    directory: &Path,
    directory_key: &str,
    plan_directory_version: Option<u32>,
    metadata: &PythonEnvironmentCacheMetadata,
) -> Result<Option<String>, PythonEnvironmentCacheError> {
    if metadata.schema_version != READY_SCHEMA_VERSION {
        return Ok(Some(format!(
            "unsupported readiness schema version {}; expected {READY_SCHEMA_VERSION}",
            metadata.schema_version
        )));
    }
    if metadata.environment_key != directory_key {
        return Ok(Some(
            "readiness marker key does not match directory name".to_owned(),
        ));
    }
    if plan_directory_version != Some(metadata.plan_version) {
        return Ok(Some(
            "readiness marker plan version does not match version directory".to_owned(),
        ));
    }
    let python = environment_python_path(directory);
    let python_metadata = match fs::metadata(&python) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(Some("prepared Python interpreter is missing".to_owned()));
        }
        Err(source) => {
            return Err(PythonEnvironmentCacheError::Io {
                path: python,
                source,
            });
        }
    };
    if !python_metadata.is_file() {
        return Ok(Some(
            "prepared Python interpreter is not a regular file".to_owned(),
        ));
    }
    let lockfile = directory.join("uv.lock");
    let lock_metadata = match fs::symlink_metadata(&lockfile) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(Some("uv.lock is missing".to_owned()));
        }
        Err(source) => {
            return Err(PythonEnvironmentCacheError::Io {
                path: lockfile,
                source,
            });
        }
    };
    if lock_metadata.file_type().is_symlink() || !lock_metadata.is_file() {
        return Ok(Some("uv.lock is not a regular file".to_owned()));
    }
    let lock = fs::read(&lockfile).map_err(|source| PythonEnvironmentCacheError::Io {
        path: lockfile,
        source,
    })?;
    if sha256_hex(&lock) != metadata.lock_sha256 {
        return Ok(Some(
            "uv.lock digest does not match readiness marker".to_owned(),
        ));
    }
    Ok(None)
}

fn classify_non_directory(
    path: &Path,
    plan_directory_version: Option<u32>,
    metadata: fs::Metadata,
) -> PythonEnvironmentCacheEntry {
    let file_type = if metadata.file_type().is_symlink() {
        "symbolic link"
    } else {
        "non-directory entry"
    };
    entry(
        path,
        path.file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned),
        plan_directory_version,
        PythonEnvironmentCacheState::Invalid,
        EntryStats::from_metadata(&metadata),
        None,
        Some(format!("cache entry is a {file_type}")),
    )
}

fn entry(
    directory: &Path,
    key: Option<String>,
    plan_directory_version: Option<u32>,
    state: PythonEnvironmentCacheState,
    stats: EntryStats,
    metadata: Option<PythonEnvironmentCacheMetadata>,
    issue: Option<String>,
) -> PythonEnvironmentCacheEntry {
    PythonEnvironmentCacheEntry {
        directory: directory.to_path_buf(),
        key,
        plan_directory_version,
        state,
        size_bytes: stats.size_bytes,
        file_count: stats.file_count,
        modified_unix_seconds: stats.modified_unix_seconds,
        metadata,
        issue,
    }
}

fn summarize(entries: &[PythonEnvironmentCacheEntry]) -> PythonEnvironmentCacheSummary {
    let mut summary = PythonEnvironmentCacheSummary::default();
    for entry in entries {
        match entry.state {
            PythonEnvironmentCacheState::Ready => summary.ready += 1,
            PythonEnvironmentCacheState::Incomplete => summary.incomplete += 1,
            PythonEnvironmentCacheState::Invalid => summary.invalid += 1,
            PythonEnvironmentCacheState::Staging => summary.staging += 1,
        }
        summary.total_size_bytes = summary.total_size_bytes.saturating_add(entry.size_bytes);
        summary.total_file_count = summary.total_file_count.saturating_add(entry.file_count);
    }
    summary
}

#[derive(Debug, Clone, Copy, Default)]
struct EntryStats {
    size_bytes: u64,
    file_count: u64,
    modified_unix_seconds: Option<u64>,
}

impl EntryStats {
    fn from_metadata(metadata: &fs::Metadata) -> Self {
        Self {
            size_bytes: metadata.len(),
            file_count: u64::from(metadata.is_file()),
            modified_unix_seconds: modified_unix_seconds(metadata),
        }
    }

    fn merge(&mut self, other: Self) {
        self.size_bytes = self.size_bytes.saturating_add(other.size_bytes);
        self.file_count = self.file_count.saturating_add(other.file_count);
        self.modified_unix_seconds = self.modified_unix_seconds.max(other.modified_unix_seconds);
    }
}

fn tree_stats(path: &Path) -> Result<EntryStats, PythonEnvironmentCacheError> {
    let mut stats = EntryStats::default();
    let mut pending = vec![path.to_path_buf()];
    while let Some(current) = pending.pop() {
        let metadata = symlink_metadata(&current)?;
        stats.merge(EntryStats::from_metadata(&metadata));
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            continue;
        }
        pending.extend(read_paths(&current)?);
    }
    Ok(stats)
}

fn read_paths(path: &Path) -> Result<Vec<PathBuf>, PythonEnvironmentCacheError> {
    fs::read_dir(path)
        .map_err(|source| PythonEnvironmentCacheError::Io {
            path: path.to_path_buf(),
            source,
        })?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|source| PythonEnvironmentCacheError::Io {
                    path: path.to_path_buf(),
                    source,
                })
        })
        .collect()
}

fn symlink_metadata(path: &Path) -> Result<fs::Metadata, PythonEnvironmentCacheError> {
    fs::symlink_metadata(path).map_err(|source| PythonEnvironmentCacheError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn modified_unix_seconds(metadata: &fs::Metadata) -> Option<u64> {
    metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
}

fn environment_python_path(directory: &Path) -> PathBuf {
    let unix = directory.join(".venv/bin/python");
    if fs::symlink_metadata(&unix).is_ok() {
        unix
    } else {
        directory.join(".venv/Scripts/python.exe")
    }
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

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;
