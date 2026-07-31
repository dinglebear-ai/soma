use std::{fs, path::Path};

use super::*;

/// Restore the exact pre-operation provider files and graduation state.
pub fn recover(workspace: &Path, provider_root: &Path) -> anyhow::Result<()> {
    recover_before(
        workspace,
        provider_root,
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
}

fn recover_before(
    workspace: &Path,
    provider_root: &Path,
    deadline: std::time::Instant,
) -> anyhow::Result<()> {
    let _lock = WorkspaceLock::acquire_before(workspace, deadline)?;
    recover_transaction(workspace, provider_root)
}

/// Recover interrupted transactions beneath an operator-owned graduation
/// root before provider discovery observes partially promoted files.
pub fn recover_all(root: &Path, provider_root: &Path) -> anyhow::Result<usize> {
    if !root.exists() {
        return Ok(0);
    }
    let root = root.canonicalize()?;
    let mut recovered = 0;
    let mut visited = 0;
    let mut entries = 0;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let mut pending = vec![(root.clone(), 0usize)];
    while let Some((path, depth)) = pending.pop() {
        if std::time::Instant::now() >= deadline {
            anyhow::bail!("graduation recovery exceeded its global deadline");
        }
        visited += 1;
        if visited > MAX_RECOVERY_DIRECTORIES {
            anyhow::bail!(
                "graduation recovery exceeds {MAX_RECOVERY_DIRECTORIES} directories beneath {}",
                root.display()
            );
        }
        let transaction = path.join(TRANSACTION_DIR);
        match fs::symlink_metadata(&transaction) {
            Ok(metadata) if metadata.file_type().is_symlink() => anyhow::bail!(
                "graduation transaction directory must not be a symlink: {}",
                transaction.display()
            ),
            Ok(metadata) if metadata.file_type().is_dir() => {
                recover_before(&path, provider_root, deadline)?;
                recovered += 1;
            }
            Ok(_) => anyhow::bail!(
                "graduation transaction marker is not a directory: {}",
                transaction.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        if depth >= MAX_RECOVERY_DEPTH {
            ensure_leaf(&path, &root, &mut entries)?;
            continue;
        }
        for entry in fs::read_dir(&path)? {
            let entry = entry?;
            count_entry(&root, &mut entries)?;
            if entry.file_name() == TRANSACTION_DIR {
                continue;
            }
            if entry
                .file_name()
                .to_string_lossy()
                .starts_with(".graduation-transaction-complete-")
            {
                let metadata = fs::symlink_metadata(entry.path())?;
                if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
                    anyhow::bail!(
                        "graduation transaction tombstone is invalid: {}",
                        entry.path().display()
                    );
                }
                let removed = remove_committed_tombstone(&path, provider_root, &entry.path())?;
                entries = entries.saturating_add(removed);
                if entries > MAX_RECOVERY_ENTRIES {
                    anyhow::bail!(
                        "graduation recovery exceeds {MAX_RECOVERY_ENTRIES} entries beneath {}",
                        root.display()
                    );
                }
                continue;
            }
            let metadata = fs::symlink_metadata(entry.path())?;
            if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
                continue;
            }
            let child = entry.path().canonicalize()?;
            if !child.starts_with(&root) {
                anyhow::bail!(
                    "graduation recovery directory escapes configured root: {}",
                    child.display()
                );
            }
            pending.push((child, depth + 1));
        }
    }
    Ok(recovered)
}

fn ensure_leaf(path: &Path, root: &Path, entries: &mut usize) -> anyhow::Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        count_entry(root, entries)?;
        if entry.file_name() == TRANSACTION_DIR {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
            anyhow::bail!(
                "graduation recovery exceeds depth {MAX_RECOVERY_DEPTH} beneath {}",
                root.display()
            );
        }
    }
    Ok(())
}

fn count_entry(root: &Path, entries: &mut usize) -> anyhow::Result<()> {
    *entries += 1;
    if *entries > MAX_RECOVERY_ENTRIES {
        anyhow::bail!(
            "graduation recovery exceeds {MAX_RECOVERY_ENTRIES} entries beneath {}",
            root.display()
        );
    }
    Ok(())
}
