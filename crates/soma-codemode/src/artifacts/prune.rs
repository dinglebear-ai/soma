use std::path::Path;

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
    for dir in dirs.into_iter().take(remove_count) {
        let _ = tokio::fs::remove_dir_all(dir).await;
    }
    Ok(remove_count)
}
