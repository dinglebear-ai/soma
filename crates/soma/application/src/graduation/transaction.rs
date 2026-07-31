use super::*;

#[derive(Debug)]
pub(super) struct AmbiguousCommitError {
    tombstone: PathBuf,
    source: String,
}

impl std::fmt::Display for AmbiguousCommitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "graduation publish completed but its transaction marker could not be durably committed; \
             retained tombstone {} for startup cleanup: {}",
            self.tombstone.display(),
            self.source
        )
    }
}

impl std::error::Error for AmbiguousCommitError {}

#[derive(Debug, Serialize, Deserialize)]
struct GraduationTransaction {
    prior_state: GraduationState,
    deployed_component: PathBuf,
    deployed_manifest: PathBuf,
    backup: PathBuf,
    component_existed: bool,
    manifest_existed: bool,
    source_existed: bool,
    backup_existed: bool,
    component_mode: Option<u32>,
    manifest_mode: Option<u32>,
    source_mode: Option<u32>,
    backup_mode: Option<u32>,
}

pub(super) fn begin_transaction(
    workspace: &Path,
    state: &GraduationState,
    deployed_component: &Path,
    deployed_manifest: &Path,
    backup: &Path,
) -> anyhow::Result<()> {
    let destination = workspace.join(TRANSACTION_DIR);
    if destination.exists() {
        anyhow::bail!("unrecovered graduation transaction already exists");
    }
    let staging = tempfile::Builder::new()
        .prefix(".graduation-transaction-")
        .tempdir_in(workspace)?;
    snapshot_file(deployed_component, &staging.path().join("component"))?;
    snapshot_file(deployed_manifest, &staging.path().join("manifest"))?;
    snapshot_file(&state.source, &staging.path().join("source"))?;
    snapshot_file(backup, &staging.path().join("backup"))?;
    let transaction = GraduationTransaction {
        prior_state: state.clone(),
        deployed_component: deployed_component.to_owned(),
        deployed_manifest: deployed_manifest.to_owned(),
        backup: backup.to_owned(),
        component_existed: deployed_component.exists(),
        manifest_existed: deployed_manifest.exists(),
        source_existed: state.source.exists(),
        backup_existed: backup.exists(),
        component_mode: file_mode(deployed_component),
        manifest_mode: file_mode(deployed_manifest),
        source_mode: file_mode(&state.source),
        backup_mode: file_mode(backup),
    };
    atomic_write(
        &staging.path().join("transaction.json"),
        &serde_json::to_vec_pretty(&transaction)?,
    )?;
    let staging = staging.keep();
    fs::rename(&staging, &destination)?;
    sync_parent(&destination)
}

pub(super) fn recover_transaction(workspace: &Path, provider_root: &Path) -> anyhow::Result<()> {
    let transaction_dir = workspace.join(TRANSACTION_DIR);
    match fs::symlink_metadata(&transaction_dir) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() => {
            anyhow::bail!(
                "graduation transaction marker must be a real directory: {}",
                transaction_dir.display()
            );
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    }
    for name in [
        "transaction.json",
        "component",
        "manifest",
        "source",
        "backup",
    ] {
        let path = transaction_dir.join(name);
        match fs::symlink_metadata(&path) {
            Ok(metadata)
                if metadata.file_type().is_symlink() || !metadata.file_type().is_file() =>
            {
                anyhow::bail!(
                    "graduation transaction snapshot must be a regular file: {}",
                    path.display()
                );
            }
            Ok(_) => {}
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound && name != "transaction.json" => {}
            Err(error) => return Err(error.into()),
        }
    }
    let transaction: GraduationTransaction = serde_json::from_slice(&read_bounded(
        &transaction_dir.join("transaction.json"),
        MAX_FIXTURE_BYTES,
        "graduation transaction",
    )?)?;
    validate_state_paths(workspace, provider_root, &transaction.prior_state)?;
    let expected_component = transaction.prior_state.source.with_extension("wasm");
    let expected_manifest = wasm_manifest_path(&expected_component);
    let expected_backup = transaction
        .prior_state
        .source
        .with_extension("py.soma-backup");
    if transaction.deployed_component != expected_component
        || transaction.deployed_manifest != expected_manifest
        || transaction.backup != expected_backup
    {
        anyhow::bail!("graduation transaction contains forged destination paths");
    }
    restore_snapshot(
        &transaction_dir.join("component"),
        &transaction.deployed_component,
        transaction.component_existed,
    )?;
    restore_mode(&transaction.deployed_component, transaction.component_mode)?;
    restore_snapshot(
        &transaction_dir.join("manifest"),
        &transaction.deployed_manifest,
        transaction.manifest_existed,
    )?;
    restore_mode(&transaction.deployed_manifest, transaction.manifest_mode)?;
    restore_snapshot(
        &transaction_dir.join("source"),
        &transaction.prior_state.source,
        transaction.source_existed,
    )?;
    restore_mode(&transaction.prior_state.source, transaction.source_mode)?;
    restore_snapshot(
        &transaction_dir.join("backup"),
        &transaction.backup,
        transaction.backup_existed,
    )?;
    restore_mode(&transaction.backup, transaction.backup_mode)?;
    write_state(workspace, &transaction.prior_state)?;
    finish_transaction(workspace)
}

pub(super) fn remove_committed_tombstone(
    workspace: &Path,
    provider_root: &Path,
    tombstone: &Path,
) -> anyhow::Result<usize> {
    let metadata = fs::symlink_metadata(tombstone)?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
        anyhow::bail!(
            "graduation transaction tombstone is invalid: {}",
            tombstone.display()
        );
    }
    let allowed = [
        "transaction.json",
        "component",
        "manifest",
        "source",
        "backup",
    ];
    let mut entries = 0;
    for entry in fs::read_dir(tombstone)? {
        let entry = entry?;
        entries += 1;
        let name = entry.file_name();
        if !allowed.iter().any(|allowed| name == *allowed) {
            anyhow::bail!(
                "graduation transaction tombstone contains unexpected entry: {}",
                entry.path().display()
            );
        }
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
            anyhow::bail!(
                "graduation transaction tombstone entry must be a regular file: {}",
                entry.path().display()
            );
        }
    }
    let transaction: GraduationTransaction = serde_json::from_slice(&read_bounded(
        &tombstone.join("transaction.json"),
        MAX_FIXTURE_BYTES,
        "graduation transaction tombstone",
    )?)?;
    validate_state_paths(workspace, provider_root, &transaction.prior_state)?;
    let expected_component = transaction.prior_state.source.with_extension("wasm");
    let expected_manifest = wasm_manifest_path(&expected_component);
    let expected_backup = transaction
        .prior_state
        .source
        .with_extension("py.soma-backup");
    if transaction.deployed_component != expected_component
        || transaction.deployed_manifest != expected_manifest
        || transaction.backup != expected_backup
    {
        anyhow::bail!("graduation transaction tombstone contains forged destination paths");
    }
    for name in allowed {
        let path = tombstone.join(name);
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound && name != "transaction.json" => {}
            Err(error) => return Err(error.into()),
        }
    }
    fs::remove_dir(tombstone)?;
    sync_parent(tombstone)?;
    Ok(entries)
}

pub(super) fn finish_transaction(workspace: &Path) -> anyhow::Result<()> {
    finish_transaction_with(workspace, sync_parent)
}

fn finish_transaction_with(
    workspace: &Path,
    sync: impl FnOnce(&Path) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let transaction_dir = workspace.join(TRANSACTION_DIR);
    if transaction_dir.exists() {
        let tombstone = workspace.join(format!(
            ".graduation-transaction-complete-{}-{}",
            std::process::id(),
            unix_ms()
        ));
        fs::rename(&transaction_dir, &tombstone)?;
        // A successful directory sync is the durable commit boundary. If it
        // fails, retain the tombstone and report an ambiguous commit; callers
        // must not attempt rollback after the active marker has moved.
        sync(&tombstone).map_err(|source| AmbiguousCommitError {
            tombstone: tombstone.clone(),
            source: source.to_string(),
        })?;
        // Only post-commit tombstone cleanup is best effort.
        let _ = fs::remove_dir_all(&tombstone);
    }
    Ok(())
}

fn snapshot_file(source: &Path, snapshot: &Path) -> anyhow::Result<()> {
    if source.exists() {
        let bytes = read_bounded(source, MAX_COMPONENT_BYTES, "transaction snapshot")?;
        fs::write(snapshot, bytes)?;
        File::open(snapshot)?.sync_all()?;
    }
    Ok(())
}

fn restore_snapshot(snapshot: &Path, destination: &Path, existed: bool) -> anyhow::Result<()> {
    if existed {
        atomic_write(
            destination,
            &read_bounded(snapshot, MAX_COMPONENT_BYTES, "transaction snapshot")?,
        )?;
    } else if destination.exists() {
        fs::remove_file(destination)?;
        sync_parent(destination)?;
    }
    Ok(())
}

#[cfg(unix)]
fn file_mode(path: &Path) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::metadata(path)
        .ok()
        .map(|metadata| metadata.permissions().mode())
}

#[cfg(not(unix))]
fn file_mode(_path: &Path) -> Option<u32> {
    None
}

#[cfg(unix)]
fn restore_mode(path: &Path, mode: Option<u32>) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    if let Some(mode) = mode
        && path.exists()
    {
        fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn restore_mode(_path: &Path, _mode: Option<u32>) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durability_failure_keeps_committed_tombstone_and_never_restores_active_marker() {
        let workspace = tempfile::tempdir().expect("workspace");
        fs::create_dir(workspace.path().join(TRANSACTION_DIR)).expect("transaction");
        let error = finish_transaction_with(workspace.path(), |_| {
            anyhow::bail!("injected directory sync failure")
        })
        .expect_err("sync failure must be surfaced");

        assert!(super::super::is_ambiguous_commit(&error));
        assert!(error.to_string().contains("could not be durably committed"));
        assert!(!workspace.path().join(TRANSACTION_DIR).exists());
        assert!(
            fs::read_dir(workspace.path())
                .expect("workspace entries")
                .filter_map(Result::ok)
                .any(|entry| entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".graduation-transaction-complete-"))
        );
    }
}
