use super::*;

fn state_path(workspace: &Path) -> PathBuf {
    workspace.join("graduation.json")
}

pub(super) fn read_state(workspace: &Path) -> anyhow::Result<GraduationState> {
    let state: GraduationState = serde_json::from_slice(&fs::read(state_path(workspace))?)?;
    if state.schema_version != STATE_SCHEMA_VERSION {
        anyhow::bail!(
            "unsupported graduation state schema {}; expected {}",
            state.schema_version,
            STATE_SCHEMA_VERSION
        );
    }
    Ok(state)
}

pub(super) fn validate_state_paths(
    workspace: &Path,
    provider_root: &Path,
    state: &GraduationState,
) -> anyhow::Result<()> {
    let workspace = workspace.canonicalize()?;
    let provider_root = provider_root.canonicalize()?;
    let source = canonicalize_existing_or_parent(&state.source)?;
    reject_symlink_if_present(&state.source)?;
    if !source.starts_with(&provider_root)
        || source.extension().and_then(|value| value.to_str()) != Some("py")
    {
        anyhow::bail!("graduation state source is outside the managed provider root");
    }
    let catalog_source = state
        .catalog
        .provider
        .source
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("graduation state lacks its bound provider source"))?;
    if canonicalize_existing_or_parent(Path::new(catalog_source))? != source {
        anyhow::bail!("graduation state source does not match its bound provider identity");
    }
    if state.catalog_sha256 != catalog_contract_digest(&state.catalog)? {
        anyhow::bail!("graduation state catalog digest does not match its bound provider contract");
    }
    let expected_backup = state.source.with_extension("py.soma-backup");
    if state
        .python_backup
        .as_ref()
        .is_some_and(|backup| backup != &expected_backup)
    {
        anyhow::bail!("graduation state contains an invalid Python backup path");
    }
    if let Some(backup) = &state.python_backup {
        reject_symlink_if_present(backup)?;
        if !canonicalize_existing_or_parent(backup)?.starts_with(&provider_root) {
            anyhow::bail!("graduation state backup is outside the managed provider root");
        }
    }
    let artifact_root = workspace.join("artifacts");
    for artifact in [
        state.candidate.as_ref(),
        state.active.as_ref(),
        state.previous.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        reject_symlink_if_present(&artifact.path)?;
        let path = canonicalize_existing_or_parent(&artifact.path)?;
        if !path.starts_with(&artifact_root)
            || artifact.path.extension().and_then(|value| value.to_str()) != Some("wasm")
        {
            anyhow::bail!("graduation state artifact is outside the workspace artifact store");
        }
    }
    Ok(())
}

fn reject_symlink_if_present(path: &Path) -> anyhow::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            anyhow::bail!(
                "managed graduation path must not be a symlink: {}",
                path.display()
            )
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn canonicalize_existing_or_parent(path: &Path) -> anyhow::Result<PathBuf> {
    match path.canonicalize() {
        Ok(path) => Ok(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let parent = path
                .parent()
                .ok_or_else(|| anyhow::anyhow!("managed path requires a parent"))?
                .canonicalize()?;
            Ok(parent.join(
                path.file_name()
                    .ok_or_else(|| anyhow::anyhow!("managed path requires a file name"))?,
            ))
        }
        Err(error) => Err(error.into()),
    }
}

pub(super) fn write_state(workspace: &Path, state: &GraduationState) -> anyhow::Result<()> {
    write_state_at(workspace, state)
}

pub(super) fn write_state_at(workspace: &Path, state: &GraduationState) -> anyhow::Result<()> {
    atomic_write(&state_path(workspace), &serde_json::to_vec_pretty(state)?)
}
