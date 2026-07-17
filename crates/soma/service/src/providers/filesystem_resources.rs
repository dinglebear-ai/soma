//! Recursive discovery of `providers/resources/` files, with the drop-in
//! provider layout's trust-boundary enforcement: no resource file, symlink,
//! or nested path may resolve outside the provider root. Split out of
//! `filesystem.rs` to stay under the module size hard limit — see
//! `docs/contracts/drop-in-provider-layout.md`.

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{
    provider_registry::{DynamicResourceTemplate, Provider},
    providers::{
        resource_files::{ResourceFileError, ResourceFileProvider},
        resource_uri::display_with_forward_slashes,
    },
};
use soma_provider_core::{ProviderCatalog, ProviderKind};

use super::{FileProviderLoadError, ProviderFileInspection, ProviderFileInspectionStatus};

/// Files under `root/resources/`, recursively, as
/// `(absolute_path, path_relative_to_resources_dir)` pairs. A symlink whose
/// target escapes the canonicalized `resources/` root is a hard error for
/// the whole scan, not a skip — the caller's refresh path
/// (`ProviderRegistry::refresh_file_providers`) already keeps the last
/// valid snapshot active rather than tearing down the live server over it.
pub(super) fn resource_paths(
    root: &Path,
) -> Result<Vec<(PathBuf, PathBuf)>, FileProviderLoadError> {
    let Some(canonical_root) = canonical_resources_root(root)? else {
        return Ok(Vec::new());
    };
    let resources_dir = root.join("resources");
    let mut results = Vec::new();
    walk_resources_dir(&resources_dir, &canonical_root, Path::new(""), &mut results)?;
    results.sort();
    Ok(results)
}

/// The canonicalized `root/resources/` directory, or `None` if it doesn't
/// exist. Also the trust-boundary root every discovered file (at discovery
/// time) and every resource read (at read time, to close the TOCTOU window
/// between the two) is checked against. Verifies `resources/` itself
/// resolves inside the canonicalized provider `root` — a symlink replacing
/// the `resources/` directory entry itself, not just an entry inside it,
/// would otherwise defeat every `starts_with(canonical_root)` check that
/// follows, since they'd all be comparing against the wrong root.
pub(super) fn canonical_resources_root(
    root: &Path,
) -> Result<Option<PathBuf>, FileProviderLoadError> {
    let resources_dir = root.join("resources");
    if !resources_dir.is_dir() {
        return Ok(None);
    }
    let canonical_provider_root = root
        .canonicalize()
        .map_err(|source| FileProviderLoadError {
            path: root.to_path_buf(),
            message: format!("failed to canonicalize provider root: {source}"),
        })?;
    let canonical_root = resources_dir
        .canonicalize()
        .map_err(|source| FileProviderLoadError {
            path: resources_dir.clone(),
            message: format!("failed to canonicalize resources directory: {source}"),
        })?;
    if !canonical_root.starts_with(&canonical_provider_root) {
        return Err(FileProviderLoadError {
            path: resources_dir,
            message: "resources directory escapes the provider root".to_owned(),
        });
    }
    Ok(Some(canonical_root))
}

fn walk_resources_dir(
    dir: &Path,
    canonical_root: &Path,
    relative_prefix: &Path,
    results: &mut Vec<(PathBuf, PathBuf)>,
) -> Result<(), FileProviderLoadError> {
    let entries = fs::read_dir(dir).map_err(|source| FileProviderLoadError {
        path: dir.to_path_buf(),
        message: format!("failed to read resources directory: {source}"),
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| FileProviderLoadError {
            path: dir.to_path_buf(),
            message: format!("failed to read resources directory entry: {source}"),
        })?;
        let path = entry.path();
        let relative = relative_prefix.join(entry.file_name());
        let file_type = entry.file_type().map_err(|source| FileProviderLoadError {
            path: path.clone(),
            message: format!("failed to stat resources directory entry: {source}"),
        })?;

        if file_type.is_symlink() {
            // The only entry kind that can point outside the physically
            // nested directory tree — canonicalize and verify before trusting
            // it at all. Never recurse into a symlinked directory (avoids
            // symlink cycles); a symlinked file is accepted as a leaf only
            // if its resolved target is still inside the root.
            let canonical = path
                .canonicalize()
                .map_err(|source| FileProviderLoadError {
                    path: path.clone(),
                    message: format!("failed to resolve resource symlink: {source}"),
                })?;
            if !canonical.starts_with(canonical_root) {
                return Err(FileProviderLoadError {
                    path: path.clone(),
                    message: "resource path escapes the provider root via a symlink".to_owned(),
                });
            }
            if canonical.is_file() {
                results.push((path, relative));
            }
            continue;
        }

        if file_type.is_dir() {
            walk_resources_dir(&path, canonical_root, &relative, results)?;
            continue;
        }

        if file_type.is_file() {
            results.push((path, relative));
        }
    }
    Ok(())
}

/// Non-executing inspection of every discovered `resources/` file:
/// per-file `ProviderFileInspection` results, plus index-aligned catalogs
/// and dynamic resource templates for the caller's directory-wide
/// uniqueness pass. `dynamic_resource_templates()` (unlike `catalog()`)
/// isn't part of `ProviderCatalog` — it's derived from the filename, not
/// declared data — so it has to be captured here explicitly or a
/// dynamic-template collision (e.g. `service/[name].ts` vs
/// `service/[id].ts`) would be invisible to lint even though the live
/// `ResourceIndex::register` rejects it at real registry construction time.
pub(super) fn inspect_files(
    pairs: Vec<(PathBuf, PathBuf, PathBuf)>,
) -> (
    Vec<ProviderFileInspection>,
    Vec<Option<ProviderCatalog>>,
    Vec<Option<DynamicResourceTemplate>>,
) {
    let mut files = Vec::with_capacity(pairs.len());
    let mut catalogs = Vec::with_capacity(pairs.len());
    let mut templates = Vec::with_capacity(pairs.len());
    for (absolute, relative, canonical_root) in pairs {
        let file_name = display_with_forward_slashes(&relative);
        match ResourceFileProvider::from_file(absolute.clone(), &relative, &canonical_root) {
            Ok(provider) => {
                let template = provider.dynamic_resource_templates().into_iter().next();
                let catalog = provider.catalog();
                files.push(ProviderFileInspection {
                    path: absolute,
                    file_name,
                    status: ProviderFileInspectionStatus::Loaded,
                    provider_id: Some(catalog.provider.name.clone()),
                    provider_kind: Some(catalog.provider.kind.as_str().to_owned()),
                    actions: Vec::new(),
                    error: None,
                });
                catalogs.push(Some(catalog));
                templates.push(template);
            }
            Err(ResourceFileError(message)) => {
                files.push(ProviderFileInspection {
                    path: absolute,
                    file_name,
                    status: ProviderFileInspectionStatus::Invalid,
                    provider_id: None,
                    provider_kind: Some(ProviderKind::StaticRust.as_str().to_owned()),
                    actions: Vec::new(),
                    error: Some(message),
                });
                catalogs.push(None);
                templates.push(None);
            }
        }
    }
    (files, catalogs, templates)
}

#[cfg(test)]
#[path = "filesystem_resources_tests.rs"]
mod tests;
