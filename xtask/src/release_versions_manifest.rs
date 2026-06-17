use super::{Component, Manifest, VersionFile, VersionKind};
use anyhow::{bail, Result};
use std::collections::HashSet;

pub(super) fn validate_manifest(manifest: &Manifest) -> Result<()> {
    let mut component_ids = HashSet::new();
    let mut tag_prefixes: Vec<&str> = Vec::new();
    for component in &manifest.components {
        if component.id.trim().is_empty() {
            bail!("release manifest contains an empty component id");
        }
        if !component_ids.insert(component.id.as_str()) {
            bail!("duplicate release component id {}", component.id);
        }
        if component.tag_prefix.trim().is_empty() {
            bail!("{} has an empty tag_prefix", component.id);
        }
        if tag_prefixes.iter().any(|existing| {
            existing.starts_with(&component.tag_prefix)
                || component.tag_prefix.starts_with(*existing)
        }) {
            bail!("{} tag_prefix overlaps another component", component.id);
        }
        tag_prefixes.push(&component.tag_prefix);
        if component.shipping_paths.is_empty() {
            bail!("{} has no shipping_paths", component.id);
        }
        if !component.release_workflow.ends_with(".yml")
            && !component.release_workflow.ends_with(".yaml")
        {
            bail!("{} release_workflow must be a YAML workflow", component.id);
        }
        validate_version_file(component, "version_source", &component.version_source)?;
        for file in &component.version_files {
            validate_version_file(component, "version_files", file)?;
        }
        if !component
            .version_files
            .iter()
            .any(|file| same_version_file(file, &component.version_source))
        {
            bail!(
                "{} version_source is not listed in version_files",
                component.id
            );
        }
    }
    Ok(())
}

fn validate_version_file(component: &Component, field: &str, file: &VersionFile) -> Result<()> {
    match file.kind {
        VersionKind::CargoPackage | VersionKind::CargoLockPackage => {
            if file.package.as_deref().unwrap_or("").trim().is_empty() {
                bail!(
                    "{} {field} {} {:?} requires package",
                    component.id,
                    file.path,
                    file.kind
                );
            }
            if file.json_pointer.is_some() {
                bail!(
                    "{} {field} {} {:?} must not set json_pointer",
                    component.id,
                    file.path,
                    file.kind
                );
            }
        }
        VersionKind::JsonVersion | VersionKind::OciIdentifierVersion => {
            if file.package.is_some() {
                bail!(
                    "{} {field} {} {:?} must not set package",
                    component.id,
                    file.path,
                    file.kind
                );
            }
            let pointer = file.json_pointer.as_deref().unwrap_or("");
            if !pointer.starts_with('/') {
                bail!(
                    "{} {field} {} {:?} requires an absolute json_pointer",
                    component.id,
                    file.path,
                    file.kind
                );
            }
        }
        VersionKind::ChangelogHeading | VersionKind::JsonNoVersion => {
            if file.package.is_some() || file.json_pointer.is_some() {
                bail!(
                    "{} {field} {} {:?} must not set package/json_pointer",
                    component.id,
                    file.path,
                    file.kind
                );
            }
        }
    }
    Ok(())
}

fn same_version_file(left: &VersionFile, right: &VersionFile) -> bool {
    left.kind == right.kind
        && left.path == right.path
        && left.package == right.package
        && left.json_pointer == right.json_pointer
}
