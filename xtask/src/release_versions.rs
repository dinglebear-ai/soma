use anyhow::{bail, Context, Result};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

#[path = "release_versions_manifest.rs"]
mod manifest;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateMode {
    Pr,
    Main,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BumpLevel {
    Patch,
    Minor,
    Major,
}

#[derive(Debug, Clone, Serialize)]
pub struct ComponentPlan {
    pub id: String,
    pub name: String,
    pub changed: bool,
    pub version: String,
    pub candidate_tag: String,
    pub last_tag: Option<String>,
    pub release_workflow: String,
    pub shipping_paths: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Manifest {
    schema_version: u32,
    components: Vec<Component>,
}

#[derive(Debug, Deserialize)]
struct Component {
    id: String,
    name: String,
    tag_prefix: String,
    release_workflow: String,
    shipping_paths: Vec<String>,
    version_source: VersionFile,
    version_files: Vec<VersionFile>,
}

#[derive(Debug, Deserialize, Clone)]
struct VersionFile {
    kind: VersionKind,
    path: String,
    package: Option<String>,
    json_pointer: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum VersionKind {
    CargoPackage,
    CargoLockPackage,
    ChangelogHeading,
    JsonVersion,
    JsonNoVersion,
    OciIdentifierVersion,
}

pub fn check(
    root: &Path,
    base: Option<&str>,
    head: &str,
    mode: GateMode,
    json: bool,
) -> Result<()> {
    let manifest = load_manifest(root)?;
    let plans = build_plan(root, &manifest, base, head, mode)?;
    let mut errors = Vec::new();

    for (component, plan) in manifest.components.iter().zip(plans.iter()) {
        errors.extend(
            check_component_parity(root, component, &plan.version)?
                .into_iter()
                .map(|error| format!("{}: {error}", component.id)),
        );

        if !plan.changed {
            continue;
        }

        let candidate = Version::parse(&plan.version).with_context(|| {
            format!(
                "{} version is not valid semver: {}",
                component.id, plan.version
            )
        })?;
        if let Some(latest) = latest_version_from_plan(component, plan)? {
            if candidate <= latest {
                errors.push(format!(
                    "{} changed but version {} is not greater than latest {} tag version {}",
                    component.id, plan.version, component.tag_prefix, latest
                ));
            }
        }
        if tag_exists(root, &plan.candidate_tag)? {
            errors.push(format!(
                "{} changed but tag {} already exists",
                component.id, plan.candidate_tag
            ));
        }
    }

    print_plans(&plans, json)?;

    if !errors.is_empty() {
        for error in &errors {
            eprintln!("release version error: {error}");
        }
        bail!(
            "release version check failed ({} error(s)): {}",
            errors.len(),
            errors.join("; ")
        );
    }

    Ok(())
}

pub fn plan(
    root: &Path,
    base: Option<&str>,
    head: &str,
    mode: GateMode,
) -> Result<Vec<ComponentPlan>> {
    let manifest = load_manifest(root)?;
    build_plan(root, &manifest, base, head, mode)
}

pub fn print_plans(plans: &[ComponentPlan], json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(plans)?);
    } else {
        for plan in plans {
            println!(
                "{} changed={} version={} tag={} last_tag={} workflow={}",
                plan.id,
                plan.changed,
                plan.version,
                plan.candidate_tag,
                plan.last_tag.as_deref().unwrap_or("-"),
                plan.release_workflow
            );
        }
    }
    Ok(())
}

pub fn bump(root: &Path, component_id: &str, level: BumpLevel) -> Result<()> {
    let manifest = load_manifest(root)?;
    let component = manifest
        .components
        .iter()
        .find(|component| component.id == component_id)
        .with_context(|| format!("unknown release component {component_id}"))?;
    let current = read_version(root, &component.version_source)?;
    let current = Version::parse(&current)
        .with_context(|| format!("{} version is not valid semver: {current}", component.id))?;
    let next = match level {
        BumpLevel::Patch => Version::new(current.major, current.minor, current.patch + 1),
        BumpLevel::Minor => Version::new(current.major, current.minor + 1, 0),
        BumpLevel::Major => Version::new(current.major + 1, 0, 0),
    }
    .to_string();

    for file in &component.version_files {
        let path = root.join(&file.path);
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", file.path))?;
        let updated = match file.kind {
            VersionKind::CargoPackage => {
                replace_cargo_package_version(&content, file.package.as_deref(), &next)?
            }
            VersionKind::CargoLockPackage => {
                replace_cargo_lock_package_version(&content, file.package.as_deref(), &next)?
            }
            VersionKind::ChangelogHeading => ensure_changelog_heading(&content, &next),
            VersionKind::JsonVersion => {
                replace_json_version(&content, file.json_pointer.as_deref(), &next)?
            }
            VersionKind::OciIdentifierVersion => {
                replace_oci_identifier_version(&content, file.json_pointer.as_deref(), &next)?
            }
            VersionKind::JsonNoVersion => content.clone(),
        };
        if updated != content {
            std::fs::write(&path, updated)
                .with_context(|| format!("failed to write {}", file.path))?;
        }
    }

    Ok(())
}

pub fn check_version_sync(root: &Path) -> Result<()> {
    let manifest = load_manifest(root)?;
    for component in &manifest.components {
        let version = read_version(root, &component.version_source)?;
        Version::parse(&version)
            .with_context(|| format!("{} version is not valid semver: {version}", component.id))?;
        let errors = check_component_parity(root, component, &version)?;
        if !errors.is_empty() {
            for error in &errors {
                eprintln!("version sync error: {}: {error}", component.id);
            }
            bail!("version sync check failed ({} error(s))", errors.len());
        }
        println!(
            "OK: {} version-bearing files are in sync at {version}.",
            component.id
        );
    }
    Ok(())
}

fn load_manifest(root: &Path) -> Result<Manifest> {
    let content = std::fs::read_to_string(root.join("release/components.toml"))
        .context("failed to read release/components.toml")?;
    let manifest: Manifest =
        toml::from_str(&content).context("failed to parse release/components.toml")?;
    if manifest.schema_version != 1 {
        bail!(
            "unsupported release/components.toml schema_version {}",
            manifest.schema_version
        );
    }
    manifest::validate_manifest(&manifest)?;
    Ok(manifest)
}

fn build_plan(
    root: &Path,
    manifest: &Manifest,
    base: Option<&str>,
    head: &str,
    mode: GateMode,
) -> Result<Vec<ComponentPlan>> {
    manifest
        .components
        .iter()
        .map(|component| {
            let version = read_version(root, &component.version_source)?;
            Version::parse(&version).with_context(|| {
                format!("{} version is not valid semver: {version}", component.id)
            })?;
            let candidate_tag = format!("{}{}", component.tag_prefix, version);
            let last_tag = latest_tag(root, &component.tag_prefix)?;
            let changed = match mode {
                GateMode::Pr => {
                    let base = base.unwrap_or("origin/main");
                    let compare_ref = merge_base(root, base, head)?;
                    component_changed_since_ref(root, component, &compare_ref, head)?
                }
                GateMode::Main => match last_tag.as_deref() {
                    Some(tag) => component_changed_since_ref(root, component, tag, head)?,
                    None => true,
                },
            };
            Ok(ComponentPlan {
                id: component.id.clone(),
                name: component.name.clone(),
                changed,
                version,
                candidate_tag,
                last_tag,
                release_workflow: component.release_workflow.clone(),
                shipping_paths: component.shipping_paths.clone(),
            })
        })
        .collect()
}

fn read_version(root: &Path, file: &VersionFile) -> Result<String> {
    let path = root.join(&file.path);
    let content =
        std::fs::read_to_string(&path).with_context(|| format!("failed to read {}", file.path))?;
    match file.kind {
        VersionKind::CargoPackage => read_cargo_package_version(&content, file.package.as_deref()),
        VersionKind::CargoLockPackage => {
            read_cargo_lock_package_version(&content, file.package.as_deref())
        }
        VersionKind::JsonVersion => read_json_version(&content, file.json_pointer.as_deref()),
        VersionKind::OciIdentifierVersion => {
            read_oci_identifier_version(&content, file.json_pointer.as_deref())
        }
        VersionKind::ChangelogHeading | VersionKind::JsonNoVersion => {
            bail!("{:?} is not a canonical version source", file.kind)
        }
    }
    .with_context(|| format!("failed to read {:?} from {}", file.kind, file.path))
}

fn check_component_parity(
    root: &Path,
    component: &Component,
    expected: &str,
) -> Result<Vec<String>> {
    let mut errors = Vec::new();
    for file in &component.version_files {
        let content = match std::fs::read_to_string(root.join(&file.path)) {
            Ok(content) => content,
            Err(error) => {
                errors.push(format!("{}: failed to read: {error}", file.path));
                continue;
            }
        };
        let result = match file.kind {
            VersionKind::CargoPackage => check_version(
                read_cargo_package_version(&content, file.package.as_deref()),
                expected,
            ),
            VersionKind::CargoLockPackage => check_version(
                read_cargo_lock_package_version(&content, file.package.as_deref()),
                expected,
            ),
            VersionKind::ChangelogHeading => check_changelog_heading(&content, expected),
            VersionKind::JsonVersion => check_version(
                read_json_version(&content, file.json_pointer.as_deref()),
                expected,
            ),
            VersionKind::OciIdentifierVersion => check_version(
                read_oci_identifier_version(&content, file.json_pointer.as_deref()),
                expected,
            ),
            VersionKind::JsonNoVersion => check_json_no_version(&content),
        };
        if let Err(error) = result {
            errors.push(format!("{}: {error}", file.path));
        }
    }
    Ok(errors)
}

fn check_version(actual: Result<String>, expected: &str) -> Result<()> {
    let actual = actual?;
    if actual != expected {
        bail!("expected version {expected}, found {actual}");
    }
    Ok(())
}

fn read_cargo_package_version(content: &str, package: Option<&str>) -> Result<String> {
    let value: toml::Value = toml::from_str(content).context("invalid TOML")?;
    let table = value
        .get("package")
        .and_then(|value| value.as_table())
        .context("missing [package] table")?;
    if let Some(expected_name) = package {
        let name = table
            .get("name")
            .and_then(|value| value.as_str())
            .context("missing package.name")?;
        if name != expected_name {
            bail!("expected package {expected_name}, found {name}");
        }
    }
    table
        .get("version")
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
        .context("missing package.version")
}

fn read_cargo_lock_package_version(content: &str, package: Option<&str>) -> Result<String> {
    let package = package.context("cargo_lock_package requires package")?;
    let sections = cargo_lock_package_sections(content);
    sections
        .get(package)
        .map(ToOwned::to_owned)
        .with_context(|| format!("missing Cargo.lock package {package}"))
}

fn cargo_lock_package_sections(content: &str) -> BTreeMap<String, String> {
    let mut packages = BTreeMap::new();
    for section in content.split("[[package]]").skip(1) {
        let Some(name) = cargo_lock_field(section, "name") else {
            continue;
        };
        let Some(version) = cargo_lock_field(section, "version") else {
            continue;
        };
        packages.insert(name, version);
    }
    packages
}

fn cargo_lock_field(section: &str, key: &str) -> Option<String> {
    let prefix = format!("{key} = ");
    section.lines().find_map(|line| {
        line.trim()
            .strip_prefix(&prefix)
            .and_then(|value| value.trim().strip_prefix('"')?.strip_suffix('"'))
            .map(ToOwned::to_owned)
    })
}

fn read_json_version(content: &str, pointer: Option<&str>) -> Result<String> {
    let value = parse_json_value(content)?;
    let pointer = pointer.unwrap_or("/version");
    value
        .pointer(pointer)
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
        .with_context(|| format!("missing JSON string at {pointer}"))
}

fn read_oci_identifier_version(content: &str, pointer: Option<&str>) -> Result<String> {
    let identifier = read_json_version(content, pointer)?;
    identifier
        .rsplit_once(':')
        .map(|(_, version)| version.to_owned())
        .with_context(|| format!("OCI identifier {identifier:?} has no version tag suffix"))
}

fn check_changelog_heading(content: &str, expected: &str) -> Result<()> {
    let expected = format!("## [{expected}]");
    if !content.lines().any(|line| line.starts_with(&expected)) {
        bail!("missing '{expected}' heading");
    }
    Ok(())
}

fn check_json_no_version(content: &str) -> Result<()> {
    let value = parse_json_value(content)?;
    if contains_json_version_key(&value) {
        bail!("must not contain a version key");
    }
    Ok(())
}

fn contains_json_version_key(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(map) => {
            map.contains_key("version") || map.values().any(contains_json_version_key)
        }
        serde_json::Value::Array(values) => values.iter().any(contains_json_version_key),
        _ => false,
    }
}

fn replace_cargo_package_version(
    content: &str,
    package: Option<&str>,
    next: &str,
) -> Result<String> {
    read_cargo_package_version(content, package)?;
    replace_package_table_version(content, next)
}

fn replace_cargo_lock_package_version(
    content: &str,
    package: Option<&str>,
    next: &str,
) -> Result<String> {
    let package = package.context("cargo_lock_package requires package")?;
    let mut in_target = false;
    let mut replaced = false;
    let mut saw_name = false;
    let mut output = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "[[package]]" {
            in_target = false;
            saw_name = false;
        } else if let Some(name) = cargo_lock_field(line, "name") {
            saw_name = name == package;
            in_target = saw_name;
        }

        let mut next_line = line.to_owned();
        if in_target && saw_name && trimmed.starts_with("version = ") {
            let leading = &line[..line.len() - line.trim_start().len()];
            next_line = format!(r#"{leading}version = "{next}""#);
            replaced = true;
            in_target = false;
        }
        output.push(next_line);
    }
    if !replaced {
        bail!("missing Cargo.lock package {package} version");
    }
    Ok(preserve_trailing_newline(content, output.join("\n")))
}

fn replace_package_table_version(content: &str, next: &str) -> Result<String> {
    let mut in_package = false;
    let mut replaced = false;
    let mut output = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "[package]" {
            in_package = true;
        } else if in_package && trimmed.starts_with('[') {
            in_package = false;
        }

        let mut next_line = line.to_owned();
        if in_package && trimmed.starts_with("version = ") {
            let leading = &line[..line.len() - line.trim_start().len()];
            next_line = format!(r#"{leading}version = "{next}""#);
            replaced = true;
        }
        output.push(next_line);
    }
    if !replaced {
        bail!("missing Cargo package version");
    }
    Ok(preserve_trailing_newline(content, output.join("\n")))
}

fn replace_json_version(content: &str, pointer: Option<&str>, next: &str) -> Result<String> {
    let pointer = pointer.unwrap_or("/version");
    let mut value = parse_json_value(content)?;
    let target = value
        .pointer_mut(pointer)
        .with_context(|| format!("missing JSON version field at {pointer}"))?;
    if !target.is_string() {
        bail!("JSON version field at {pointer} is not a string");
    }
    *target = serde_json::Value::String(next.to_owned());
    write_json_preserving_prefix(content, &value)
}

fn replace_oci_identifier_version(
    content: &str,
    pointer: Option<&str>,
    next: &str,
) -> Result<String> {
    let pointer = pointer.context("oci_identifier_version requires json_pointer")?;
    let mut value = parse_json_value(content)?;
    let target = value
        .pointer_mut(pointer)
        .with_context(|| format!("missing JSON OCI identifier at {pointer}"))?;
    let identifier = target
        .as_str()
        .with_context(|| format!("JSON OCI identifier at {pointer} is not a string"))?;
    let Some((repo, _)) = identifier.rsplit_once(':') else {
        bail!("OCI identifier {identifier:?} has no version tag suffix");
    };
    *target = serde_json::Value::String(format!("{repo}:{next}"));
    write_json_preserving_prefix(content, &value)
}

fn parse_json_value(content: &str) -> Result<serde_json::Value> {
    let json = content
        .get(json_start(content)?..)
        .context("invalid UTF-8 boundary for JSON payload")?;
    serde_json::from_str(json).context("invalid JSON")
}

fn json_start(content: &str) -> Result<usize> {
    content.find('{').context("missing JSON object")
}

fn write_json_preserving_prefix(original: &str, value: &serde_json::Value) -> Result<String> {
    let start = json_start(original)?;
    let mut output = String::new();
    output.push_str(&original[..start]);
    output.push_str(&serde_json::to_string_pretty(value).context("failed to serialize JSON")?);
    output.push('\n');
    Ok(output)
}

fn ensure_changelog_heading(content: &str, next: &str) -> String {
    let heading = format!("## [{next}]");
    if content.lines().any(|line| line.starts_with(&heading)) {
        return content.to_owned();
    }
    let mut output = String::new();
    let mut inserted = false;
    for line in content.lines() {
        output.push_str(line);
        output.push('\n');
        if !inserted && line.trim() == "## [Unreleased]" {
            output.push('\n');
            output.push_str(&heading);
            output.push_str("\n\n");
            inserted = true;
        }
    }
    if inserted {
        output
    } else {
        format!("{content}\n\n{heading}\n")
    }
}

fn preserve_trailing_newline(original: &str, output: String) -> String {
    if original.ends_with('\n') && !output.ends_with('\n') {
        format!("{output}\n")
    } else {
        output
    }
}

fn latest_tag(root: &Path, prefix: &str) -> Result<Option<String>> {
    let output = git_output(root, &["tag", "-l", &format!("{prefix}*")])?;
    let mut candidates = Vec::new();
    for tag in output.lines().filter(|line| !line.trim().is_empty()) {
        let Some(version) = tag.strip_prefix(prefix) else {
            continue;
        };
        if let Ok(version) = Version::parse(version) {
            candidates.push((version, tag.to_owned()));
        }
    }
    candidates.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(candidates.pop().map(|(_, tag)| tag))
}

fn latest_version_from_plan(
    component: &Component,
    plan: &ComponentPlan,
) -> Result<Option<Version>> {
    plan.last_tag
        .as_deref()
        .map(|tag| {
            let version = tag
                .strip_prefix(&component.tag_prefix)
                .with_context(|| format!("{} latest tag has wrong prefix: {tag}", component.id))?;
            Version::parse(version).with_context(|| {
                format!(
                    "{} latest tag has invalid semver suffix: {tag}",
                    component.id
                )
            })
        })
        .transpose()
}

fn tag_exists(root: &Path, tag: &str) -> Result<bool> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "-q", "--verify"])
        .arg(format!("refs/tags/{tag}"))
        .output()
        .with_context(|| format!("failed to check tag {tag}"))?;
    Ok(output.status.success())
}

fn component_changed_since_ref(
    root: &Path,
    component: &Component,
    base: &str,
    head: &str,
) -> Result<bool> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["diff", "--name-only"])
        .arg(format!("{base}..{head}"))
        .arg("--")
        .args(&component.shipping_paths)
        .output()
        .with_context(|| format!("failed to diff {base}..{head}"))?;
    if !output.status.success() {
        bail!(
            "git diff failed for {base}..{head}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .any(|line| !line.trim().is_empty()))
}

fn merge_base(root: &Path, base: &str, head: &str) -> Result<String> {
    git_output(root, &["merge-base", base, head]).map(|output| output.trim().to_owned())
}

fn git_output(root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .with_context(|| format!("failed to run git {args:?}"))?;
    if !output.status.success() {
        bail!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(test)]
#[path = "release_versions_tests.rs"]
mod tests;
