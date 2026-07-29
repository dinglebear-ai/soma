use anyhow::{Context, Result, bail};

use super::{parse_json_value, read_json_version, write_json_preserving_prefix};

pub(super) fn read_oci_identifier_version(content: &str, pointer: Option<&str>) -> Result<String> {
    let identifier = read_json_version(content, pointer)?;
    identifier
        .rsplit_once(':')
        .map(|(_, version)| version.to_owned())
        .with_context(|| format!("OCI identifier {identifier:?} has no version tag suffix"))
}

pub(super) fn read_npm_identifier_version(content: &str, pointer: Option<&str>) -> Result<String> {
    let identifier = read_json_version(content, pointer)?;
    identifier
        .rsplit_once('@')
        .and_then(|(package, version)| (!package.is_empty()).then(|| version.to_owned()))
        .with_context(|| format!("npm identifier {identifier:?} has no version suffix"))
}

pub(super) fn replace_oci_identifier_version(
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

pub(super) fn replace_npm_identifier_version(
    content: &str,
    pointer: Option<&str>,
    next: &str,
) -> Result<String> {
    let pointer = pointer.context("npm_identifier_version requires json_pointer")?;
    let mut value = parse_json_value(content)?;
    let target = value
        .pointer_mut(pointer)
        .with_context(|| format!("missing JSON npm identifier at {pointer}"))?;
    let identifier = target
        .as_str()
        .with_context(|| format!("JSON npm identifier at {pointer} is not a string"))?;
    let Some((package, _)) = identifier.rsplit_once('@') else {
        bail!("npm identifier {identifier:?} has no version suffix");
    };
    if package.is_empty() {
        bail!("npm identifier {identifier:?} has no package name");
    }
    *target = serde_json::Value::String(format!("{package}@{next}"));
    write_json_preserving_prefix(content, &value)
}
