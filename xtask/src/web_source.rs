use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::{command_exists, run_cmd};

const WEB_APP: &str = "apps/web";
const WEB_BUNDLE: &str = "crates/soma/web/assets/source";
const AURORA_COMPONENTS: &[&str] = &[
    "https://aurora.tootie.tv/r/aurora-tokens.json",
    "@aurora/aurora-badge",
    "@aurora/aurora-button",
    "@aurora/aurora-card",
    "@aurora/aurora-input",
    "@aurora/aurora-progress",
    "@aurora/aurora-separator",
    "@aurora/aurora-skeleton",
    "@aurora/aurora-tabs",
];

pub(crate) fn sync() -> Result<()> {
    let source = Path::new(WEB_APP);
    let destination = Path::new(WEB_BUNDLE);
    ensure_app_dir_exists(source)?;

    if destination.exists() {
        fs::remove_dir_all(destination)
            .with_context(|| format!("failed to remove {}", destination.display()))?;
    }
    fs::create_dir_all(destination)
        .with_context(|| format!("failed to create {}", destination.display()))?;

    for entry in app_entries(source)? {
        copy_entry(source, destination, &entry)?;
    }

    println!(
        "==> sync-web-source: copied {} to {}",
        source.display(),
        destination.display()
    );
    Ok(())
}

pub(crate) fn check() -> Result<()> {
    let source = Path::new(WEB_APP);
    let destination = Path::new(WEB_BUNDLE);
    ensure_app_dir_exists(source)?;
    ensure_app_dir_exists(destination)?;

    let mut web_app_entries = app_entries(source)?;
    let mut bundle_entries = app_entries(destination)?;
    web_app_entries.sort();
    bundle_entries.sort();

    if web_app_entries != bundle_entries {
        report_entry_drift(&web_app_entries, &bundle_entries);
        bail!("web source bundle is out of sync; run `cargo xtask sync-web-source`");
    }

    let mut mismatches = Vec::new();
    for relative in &web_app_entries {
        let source_path = source.join(relative);
        let destination_path = destination.join(relative);
        if !entries_match(&source_path, &destination_path)
            .with_context(|| format!("failed to compare {}", relative.display()))?
        {
            mismatches.push(relative.clone());
        }
    }

    if !mismatches.is_empty() {
        println!("==> check-web-source-sync: content drift:");
        for path in mismatches.iter().take(25) {
            println!("  DIFF  {}", path.display());
        }
        if mismatches.len() > 25 {
            println!("  ... {} more", mismatches.len() - 25);
        }
        bail!("web source bundle is out of sync; run `cargo xtask sync-web-source`");
    }

    println!("==> check-web-source-sync: bundled web source matches apps/web");
    Ok(())
}

pub(crate) fn update_aurora() -> Result<()> {
    if !command_exists("pnpm") {
        bail!("pnpm is not installed; install it before updating Aurora components");
    }

    for component in AURORA_COMPONENTS {
        println!("==> update-aurora-web: refreshing {component}");
        run_cmd(
            "pnpm",
            &[
                "--dir",
                WEB_APP,
                "dlx",
                "shadcn@latest",
                "add",
                "--yes",
                "--overwrite",
                component,
            ],
        )
        .with_context(|| format!("failed to refresh Aurora component {component}"))?;
    }

    println!("==> update-aurora-web: validating apps/web");
    run_cmd("pnpm", &["--dir", WEB_APP, "validate"])
        .context("apps/web validation failed after Aurora refresh")?;

    sync().context("failed to sync refreshed Aurora web source")?;
    Ok(())
}

fn ensure_app_dir_exists(path: &Path) -> Result<()> {
    if !path.is_dir() {
        bail!("{} does not exist or is not a directory", path.display());
    }
    Ok(())
}

fn app_entries(root: &Path) -> Result<Vec<PathBuf>> {
    let mut entries = Vec::new();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| !is_ignored(entry.path().strip_prefix(root).unwrap_or(entry.path())))
    {
        let entry = entry.with_context(|| format!("failed to walk {}", root.display()))?;
        let relative = entry
            .path()
            .strip_prefix(root)
            .with_context(|| format!("walk entry was outside {}", root.display()))?;
        if relative.as_os_str().is_empty() || entry.file_type().is_dir() {
            continue;
        }
        entries.push(relative.to_path_buf());
    }
    Ok(entries)
}

fn is_ignored(relative: &Path) -> bool {
    relative.components().any(|component| {
        let name = component.as_os_str().to_string_lossy();
        matches!(name.as_ref(), ".next" | "node_modules" | "out")
    }) || relative
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| matches!(name, "tsconfig.tsbuildinfo" | ".DS_Store"))
}

fn copy_entry(source: &Path, destination: &Path, relative: &Path) -> Result<()> {
    let source_path = source.join(relative);
    let destination_path = destination.join(relative);
    if let Some(parent) = destination_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let metadata = fs::symlink_metadata(&source_path)
        .with_context(|| format!("failed to stat {}", source_path.display()))?;
    if metadata.file_type().is_symlink() {
        let target = fs::read_link(&source_path)
            .with_context(|| format!("failed to read symlink {}", source_path.display()))?;
        symlink_file(&target, &destination_path)
            .with_context(|| format!("failed to create symlink {}", destination_path.display()))?;
    } else {
        fs::copy(&source_path, &destination_path).with_context(|| {
            format!(
                "failed to copy {} to {}",
                source_path.display(),
                destination_path.display()
            )
        })?;
    }
    Ok(())
}

fn entries_match(left: &Path, right: &Path) -> Result<bool> {
    let left_meta =
        fs::symlink_metadata(left).with_context(|| format!("failed to stat {}", left.display()))?;
    let right_meta = fs::symlink_metadata(right)
        .with_context(|| format!("failed to stat {}", right.display()))?;

    if left_meta.file_type().is_symlink() || right_meta.file_type().is_symlink() {
        return Ok(left_meta.file_type().is_symlink()
            && right_meta.file_type().is_symlink()
            && fs::read_link(left)? == fs::read_link(right)?);
    }

    if left_meta.len() != right_meta.len() {
        return Ok(false);
    }
    Ok(fs::read(left)? == fs::read(right)?)
}

fn report_entry_drift(app_entries: &[PathBuf], bundle_entries: &[PathBuf]) {
    println!("==> check-web-source-sync: file list drift:");
    for path in app_entries
        .iter()
        .filter(|path| !bundle_entries.contains(path))
        .take(25)
    {
        println!("  MISSING  {}", path.display());
    }
    for path in bundle_entries
        .iter()
        .filter(|path| !app_entries.contains(path))
        .take(25)
    {
        println!("  EXTRA    {}", path.display());
    }
}

#[cfg(unix)]
fn symlink_file(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn symlink_file(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_file(target, link)
}
