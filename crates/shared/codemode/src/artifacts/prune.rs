use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock, PoisonError},
    time::{SystemTime, UNIX_EPOCH},
};

use futures::{StreamExt, stream};

#[derive(Debug, Clone)]
struct RunDir {
    name: String,
    path: PathBuf,
    modified: SystemTime,
}

fn active_runs() -> &'static Mutex<HashMap<String, usize>> {
    static ACTIVE: OnceLock<Mutex<HashMap<String, usize>>> = OnceLock::new();
    ACTIVE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Keeps one in-flight artifact directory out of concurrent retention passes.
#[derive(Debug)]
pub(crate) struct ActiveArtifactRun {
    run_id: String,
}

impl ActiveArtifactRun {
    pub(crate) fn register(run_id: &str) -> Self {
        let mut active = active_runs().lock().unwrap_or_else(PoisonError::into_inner);
        let count = active.entry(run_id.to_owned()).or_default();
        *count = count.saturating_add(1);
        Self {
            run_id: run_id.to_owned(),
        }
    }
}

impl Drop for ActiveArtifactRun {
    fn drop(&mut self) {
        let mut active = active_runs().lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(count) = active.get_mut(&self.run_id) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                active.remove(&self.run_id);
            }
        }
    }
}

pub(crate) fn active_artifact_runs_snapshot() -> HashSet<String> {
    active_runs()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .keys()
        .cloned()
        .collect()
}

/// Compatibility count-only pruning entrypoint retained for crate consumers.
pub async fn prune_old_runs(root: &Path, keep: usize) -> std::io::Result<usize> {
    if keep == 0 || !root.exists() {
        return Ok(0);
    }
    let mut entries = tokio::fs::read_dir(root).await?;
    let mut dirs = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        if entry.file_type().await?.is_dir() {
            dirs.push(entry.path());
        }
    }
    dirs.sort();
    let remove_count = dirs.len().saturating_sub(keep);
    let mut removed = 0;
    for dir in dirs.into_iter().take(remove_count) {
        tokio::fs::remove_dir_all(dir).await?;
        removed += 1;
    }
    Ok(removed)
}

/// Best-effort retention for the dedicated Code Mode artifact store.
///
/// The newest runs are kept while they fit both enabled policies: retain == 0
/// disables count pruning and max_store_bytes == 0 disables byte pruning.
/// Active runs are never removed. Only safe single-segment directory names that
/// could have been produced by ArtifactStore are considered.
pub(crate) async fn prune_artifact_runs_in(
    root: &Path,
    retain: usize,
    max_store_bytes: u64,
    active: &HashSet<String>,
) -> usize {
    let count_pruning = retain > 0;
    let byte_pruning = max_store_bytes > 0;
    if !count_pruning && !byte_pruning {
        return 0;
    }

    let mut entries = match tokio::fs::read_dir(root).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return 0,
        Err(error) => {
            tracing::warn!(
                error = %error,
                "code-mode artifact retention disabled: cannot read store directory"
            );
            return 0;
        }
    };

    let mut runs = Vec::new();
    loop {
        let entry = match entries.next_entry().await {
            Ok(Some(entry)) => entry,
            Ok(None) => break,
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    "code-mode artifact retention enumeration interrupted"
                );
                break;
            }
        };
        let Ok(file_type) = entry.file_type().await else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !is_managed_run_name(&name) {
            continue;
        }
        let modified = entry
            .metadata()
            .await
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .unwrap_or(UNIX_EPOCH);
        runs.push(RunDir {
            name,
            path: entry.path(),
            modified,
        });
    }

    // Modification time is the portable ordering source because Soma accepts a
    // caller execution id as the run id; unlike Labby, not every directory name
    // is necessarily a ULID. The name is a deterministic tie-breaker.
    runs.sort_by(|left, right| {
        right
            .modified
            .cmp(&left.modified)
            .then_with(|| right.name.cmp(&left.name))
    });

    let sizes = if byte_pruning {
        const SIZE_WALK_CONCURRENCY: usize = 8;
        stream::iter(runs.iter().map(|run| dir_size_bytes(run.path.clone())))
            .buffered(SIZE_WALK_CONCURRENCY)
            .collect::<Vec<_>>()
            .await
    } else {
        Vec::new()
    };

    let mut cumulative = 0u64;
    let mut remove = Vec::new();
    for (index, run) in runs.iter().enumerate() {
        if byte_pruning {
            cumulative = cumulative.saturating_add(sizes[index]);
        }
        let within_count = !count_pruning || index < retain;
        let within_bytes = !byte_pruning || cumulative <= max_store_bytes;
        if (within_count && within_bytes) || active.contains(&run.name) {
            continue;
        }
        remove.push(run.path.clone());
    }

    let mut removed = 0usize;
    for path in remove {
        match tokio::fs::remove_dir_all(&path).await {
            Ok(()) => removed = removed.saturating_add(1),
            Err(error) => tracing::debug!(
                path = %path.display(),
                error = %error,
                "failed to prune old code-mode artifact directory"
            ),
        }
    }
    removed
}

async fn dir_size_bytes(path: PathBuf) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![path];
    while let Some(directory) = stack.pop() {
        let Ok(mut entries) = tokio::fs::read_dir(directory).await else {
            continue;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            match entry.file_type().await {
                Ok(file_type) if file_type.is_dir() => stack.push(entry.path()),
                Ok(file_type) if file_type.is_file() => {
                    if let Ok(metadata) = entry.metadata().await {
                        total = total.saturating_add(metadata.len());
                    }
                }
                _ => {}
            }
        }
    }
    total
}

fn is_managed_run_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && name.len() <= 128
        && name.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
}
