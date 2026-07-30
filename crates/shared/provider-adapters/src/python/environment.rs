//! Non-executing PEP 723 discovery and deterministic Python environment plans.
//!
//! This module is intentionally side-effect free. Parsing never imports provider
//! code, and planning never creates a virtual environment or contacts a package
//! index. A later cache manager consumes the immutable plan.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    str::FromStr,
};

use pep440_rs::{Version, VersionSpecifiers};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

const PEP_723_START: &str = "# /// script";
const PEP_723_END: &str = "# ///";
const MAX_METADATA_BYTES: usize = 64 * 1024;
/// Cache-policy schema version used by immutable Python environment plans.
pub const ENVIRONMENT_PLAN_VERSION: u32 = 2;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Pep723Metadata {
    pub requires_python: Option<String>,
    pub dependencies: Vec<String>,
    pub uv: Option<toml::Value>,
}

#[derive(Debug, Deserialize)]
struct RawPep723Metadata {
    #[serde(rename = "requires-python")]
    requires_python: Option<String>,
    #[serde(default)]
    dependencies: Vec<String>,
    #[serde(default)]
    tool: BTreeMap<String, toml::Value>,
}

/// Immutable identity of the selected Python runtime and wheel target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PythonRuntimeFingerprint {
    pub implementation: String,
    pub version: String,
    pub platform: String,
    pub wheel_platform_tag: String,
}

impl PythonRuntimeFingerprint {
    pub fn new(
        implementation: impl Into<String>,
        version: impl Into<String>,
        platform: impl Into<String>,
        wheel_platform_tag: impl Into<String>,
    ) -> Result<Self, PythonEnvironmentError> {
        let implementation =
            normalize_runtime_component("implementation", implementation.into(), |byte| {
                byte.is_ascii_alphanumeric() || byte == b'_'
            })?;
        let version = required_component("version", version.into())?;
        let version = Version::from_str(&version).map_err(|error| {
            PythonEnvironmentError::InvalidRuntimeVersion {
                version,
                message: error.to_string(),
            }
        })?;
        let platform = normalize_runtime_component("platform", platform.into(), |byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.')
        })?;
        let wheel_platform_tag =
            normalize_runtime_component("wheel_platform_tag", wheel_platform_tag.into(), |byte| {
                byte.is_ascii_alphanumeric() || byte == b'_'
            })?;

        Ok(Self {
            implementation: normalize_implementation(&implementation),
            version: version.to_string(),
            platform,
            wheel_platform_tag,
        })
    }
}

/// One expanded compatibility tag selected from an SDK wheel filename.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PythonWheelTag {
    pub python: String,
    pub abi: String,
    pub platform: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonEnvironmentPlan {
    pub key: String,
    pub directory: PathBuf,
    pub plan_version: u32,
    pub dependency_count: usize,
    pub runtime: PythonRuntimeFingerprint,
    pub sdk_wheel_tag: PythonWheelTag,
    pub sdk_wheel_sha256: String,
    pub uv_version: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PythonEnvironmentError {
    #[error("PEP 723 script metadata exceeds {MAX_METADATA_BYTES} bytes")]
    MetadataTooLarge,
    #[error("multiple PEP 723 script metadata blocks are not allowed")]
    MultipleScriptBlocks,
    #[error("PEP 723 script metadata block is not terminated")]
    UnterminatedScriptBlock,
    #[error("PEP 723 line {line} must remain a Python comment")]
    NonCommentLine { line: usize },
    #[error("invalid PEP 723 script metadata: {0}")]
    InvalidToml(String),
    #[error("invalid PEP 723 {field}: {message}")]
    InvalidMetadata {
        field: &'static str,
        message: String,
    },
    #[error("Python environment fingerprint {field} must not be empty")]
    EmptyFingerprint { field: &'static str },
    #[error("Python environment fingerprint {field} contains unsupported characters")]
    InvalidFingerprintComponent { field: &'static str },
    #[error("invalid Python runtime version `{version}`: {message}")]
    InvalidRuntimeVersion { version: String, message: String },
    #[error("invalid PEP 723 requires-python `{specifier}`: {message}")]
    InvalidRequiresPython { specifier: String, message: String },
    #[error("Python {version} does not satisfy PEP 723 requires-python `{requires_python}`")]
    IncompatiblePython {
        version: String,
        requires_python: String,
    },
    #[error("invalid SDK wheel filename `{filename}`: {message}")]
    InvalidSdkWheelFilename { filename: String, message: String },
    #[error(
        "SDK wheel `{filename}` is incompatible with {implementation} {version} and platform tag `{wheel_platform_tag}`"
    )]
    IncompatibleSdkWheel {
        filename: String,
        implementation: String,
        version: String,
        wheel_platform_tag: String,
    },
    #[error("SDK wheel SHA-256 must contain exactly 64 hexadecimal characters")]
    InvalidSdkDigest,
}

/// Parse the PEP 723 script block without importing or evaluating the source.
pub fn parse_pep723_metadata(
    source: &str,
) -> Result<Option<Pep723Metadata>, PythonEnvironmentError> {
    let mut body = None;
    let mut lines = source.lines().enumerate();

    while let Some((index, line)) = lines.next() {
        if line != PEP_723_START {
            continue;
        }
        if body.is_some() {
            return Err(PythonEnvironmentError::MultipleScriptBlocks);
        }

        let mut block = String::new();
        let mut terminated = false;
        for (body_index, body_line) in lines.by_ref() {
            if body_line == PEP_723_END {
                terminated = true;
                break;
            }
            if body_line == PEP_723_START {
                return Err(PythonEnvironmentError::MultipleScriptBlocks);
            }
            let comment =
                body_line
                    .strip_prefix('#')
                    .ok_or(PythonEnvironmentError::NonCommentLine {
                        line: body_index + 1,
                    })?;
            let content = comment.strip_prefix(' ').unwrap_or(comment);
            if block.len().saturating_add(content.len()).saturating_add(1) > MAX_METADATA_BYTES {
                return Err(PythonEnvironmentError::MetadataTooLarge);
            }
            block.push_str(content);
            block.push('\n');
        }
        if !terminated {
            return Err(PythonEnvironmentError::UnterminatedScriptBlock);
        }
        body = Some((index + 1, block));
    }

    body.map(|(_, block)| normalize_metadata(&block))
        .transpose()
}

/// Produce an immutable cache plan. This function performs no filesystem I/O.
pub fn plan_python_environment(
    cache_root: &Path,
    metadata: Option<&Pep723Metadata>,
    runtime: &PythonRuntimeFingerprint,
    sdk_wheel: &Path,
    sdk_wheel_sha256: &str,
    uv_version: &str,
) -> Result<PythonEnvironmentPlan, PythonEnvironmentError> {
    let sdk_wheel_sha256 = sdk_wheel_sha256.trim().to_ascii_lowercase();
    if sdk_wheel_sha256.len() != 64
        || !sdk_wheel_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(PythonEnvironmentError::InvalidSdkDigest);
    }
    let uv_version = required_component("uv_version", uv_version.to_owned())?;
    let metadata = metadata.cloned().unwrap_or_default();
    validate_requires_python(&metadata, runtime)?;
    let sdk_wheel_tag = select_compatible_sdk_wheel_tag(sdk_wheel, runtime)?;
    let fingerprint = EnvironmentFingerprint {
        plan_version: ENVIRONMENT_PLAN_VERSION,
        metadata: &metadata,
        runtime,
        sdk_wheel_tag: &sdk_wheel_tag,
        sdk_wheel_sha256: &sdk_wheel_sha256,
        uv_version: &uv_version,
    };
    let canonical = serde_json::to_vec(&fingerprint)
        .expect("environment fingerprint contains only serializable values");
    let key = sha256_hex(&canonical);
    let directory = cache_root
        .join("python")
        .join(format!("v{ENVIRONMENT_PLAN_VERSION}"))
        .join(&key);

    Ok(PythonEnvironmentPlan {
        key,
        directory,
        plan_version: ENVIRONMENT_PLAN_VERSION,
        dependency_count: metadata.dependencies.len(),
        runtime: runtime.clone(),
        sdk_wheel_tag,
        sdk_wheel_sha256,
        uv_version,
    })
}

fn validate_requires_python(
    metadata: &Pep723Metadata,
    runtime: &PythonRuntimeFingerprint,
) -> Result<(), PythonEnvironmentError> {
    let Some(requires_python) = &metadata.requires_python else {
        return Ok(());
    };
    let specifiers = VersionSpecifiers::from_str(requires_python).map_err(|error| {
        PythonEnvironmentError::InvalidRequiresPython {
            specifier: requires_python.clone(),
            message: error.to_string(),
        }
    })?;
    let version = Version::from_str(&runtime.version).map_err(|error| {
        PythonEnvironmentError::InvalidRuntimeVersion {
            version: runtime.version.clone(),
            message: error.to_string(),
        }
    })?;
    if specifiers.contains(&version) {
        Ok(())
    } else {
        Err(PythonEnvironmentError::IncompatiblePython {
            version: runtime.version.clone(),
            requires_python: requires_python.clone(),
        })
    }
}

fn select_compatible_sdk_wheel_tag(
    sdk_wheel: &Path,
    runtime: &PythonRuntimeFingerprint,
) -> Result<PythonWheelTag, PythonEnvironmentError> {
    let filename = sdk_wheel
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| PythonEnvironmentError::InvalidSdkWheelFilename {
            filename: sdk_wheel.display().to_string(),
            message: "wheel filename must be valid UTF-8".to_owned(),
        })?;
    let WheelTagFields {
        python: python_tags,
        abi: abi_tags,
        platform: platform_tags,
    } = parse_wheel_filename_tags(filename)?;
    let runtime_version = Version::from_str(&runtime.version).map_err(|error| {
        PythonEnvironmentError::InvalidRuntimeVersion {
            version: runtime.version.clone(),
            message: error.to_string(),
        }
    })?;

    for python in python_tags {
        for abi in &abi_tags {
            for platform in &platform_tags {
                if platform == &runtime.wheel_platform_tag
                    && sdk_python_abi_compatible(&python, abi, runtime, &runtime_version)
                {
                    return Ok(PythonWheelTag {
                        python,
                        abi: abi.clone(),
                        platform: platform.clone(),
                    });
                }
            }
        }
    }

    Err(PythonEnvironmentError::IncompatibleSdkWheel {
        filename: filename.to_owned(),
        implementation: runtime.implementation.clone(),
        version: runtime.version.clone(),
        wheel_platform_tag: runtime.wheel_platform_tag.clone(),
    })
}

struct WheelTagFields {
    python: Vec<String>,
    abi: Vec<String>,
    platform: Vec<String>,
}

fn parse_wheel_filename_tags(filename: &str) -> Result<WheelTagFields, PythonEnvironmentError> {
    let stem = filename.strip_suffix(".whl").ok_or_else(|| {
        PythonEnvironmentError::InvalidSdkWheelFilename {
            filename: filename.to_owned(),
            message: "wheel filename must end in .whl".to_owned(),
        }
    })?;
    let mut parts = stem.rsplitn(4, '-');
    let platform = parts.next();
    let abi = parts.next();
    let python = parts.next();
    let distribution_and_version = parts.next();
    if distribution_and_version.is_none_or(|prefix| !prefix.contains('-')) {
        return Err(PythonEnvironmentError::InvalidSdkWheelFilename {
            filename: filename.to_owned(),
            message: "expected distribution-version-python-abi-platform tags".to_owned(),
        });
    }

    Ok(WheelTagFields {
        python: split_wheel_tag_field(filename, "python", python.unwrap_or_default())?,
        abi: split_wheel_tag_field(filename, "ABI", abi.unwrap_or_default())?,
        platform: split_wheel_tag_field(filename, "platform", platform.unwrap_or_default())?,
    })
}

fn split_wheel_tag_field(
    filename: &str,
    field: &'static str,
    value: &str,
) -> Result<Vec<String>, PythonEnvironmentError> {
    let tags = value
        .split('.')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .collect::<Vec<_>>();
    if tags.is_empty()
        || tags.iter().any(|tag| {
            !tag.bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        })
    {
        return Err(PythonEnvironmentError::InvalidSdkWheelFilename {
            filename: filename.to_owned(),
            message: format!("invalid {field} tag field"),
        });
    }
    Ok(tags.into_iter().map(str::to_owned).collect())
}

fn sdk_python_abi_compatible(
    python_tag: &str,
    abi_tag: &str,
    runtime: &PythonRuntimeFingerprint,
    runtime_version: &Version,
) -> bool {
    if runtime.implementation != "cpython" || abi_tag != "abi3" {
        return false;
    }
    let Some((required_major, required_minor)) = compact_python_tag(python_tag, "cp") else {
        return false;
    };
    let release = runtime_version.release();
    let runtime_major = release.first().copied().unwrap_or_default();
    let runtime_minor = release.get(1).copied().unwrap_or_default();
    runtime_major == required_major && runtime_minor >= required_minor
}

fn compact_python_tag(tag: &str, prefix: &str) -> Option<(u64, u64)> {
    let digits = tag.strip_prefix(prefix)?;
    if digits.len() < 2 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let (major, minor) = digits.split_at(1);
    Some((major.parse().ok()?, minor.parse().ok()?))
}

fn sha256_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

#[derive(Serialize)]
struct EnvironmentFingerprint<'a> {
    plan_version: u32,
    metadata: &'a Pep723Metadata,
    runtime: &'a PythonRuntimeFingerprint,
    sdk_wheel_tag: &'a PythonWheelTag,
    sdk_wheel_sha256: &'a str,
    uv_version: &'a str,
}

fn normalize_metadata(block: &str) -> Result<Pep723Metadata, PythonEnvironmentError> {
    let raw: RawPep723Metadata = toml::from_str(block)
        .map_err(|error| PythonEnvironmentError::InvalidToml(error.to_string()))?;
    let requires_python = raw
        .requires_python
        .map(normalize_requires_python)
        .transpose()?;
    let mut dependencies = raw
        .dependencies
        .into_iter()
        .map(|value| normalize_nonempty("dependency", value))
        .collect::<Result<Vec<_>, _>>()?;
    dependencies.sort();
    dependencies.dedup();

    let uv = raw.tool.get("uv").cloned();
    if uv.as_ref().is_some_and(|value| !value.is_table()) {
        return Err(PythonEnvironmentError::InvalidMetadata {
            field: "tool.uv",
            message: "must be a table".to_owned(),
        });
    }

    Ok(Pep723Metadata {
        requires_python,
        dependencies,
        uv,
    })
}

fn normalize_requires_python(value: String) -> Result<String, PythonEnvironmentError> {
    let value = normalize_nonempty("requires-python", value)?;
    VersionSpecifiers::from_str(&value)
        .map(|specifiers| specifiers.to_string())
        .map_err(|error| PythonEnvironmentError::InvalidRequiresPython {
            specifier: value,
            message: error.to_string(),
        })
}

fn normalize_nonempty(
    field: &'static str,
    value: String,
) -> Result<String, PythonEnvironmentError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(PythonEnvironmentError::InvalidMetadata {
            field,
            message: "must not be empty".to_owned(),
        });
    }
    Ok(value.to_owned())
}

fn required_component(
    field: &'static str,
    value: String,
) -> Result<String, PythonEnvironmentError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(PythonEnvironmentError::EmptyFingerprint { field });
    }
    Ok(value.to_owned())
}

fn normalize_runtime_component(
    field: &'static str,
    value: String,
    allowed: impl Fn(u8) -> bool,
) -> Result<String, PythonEnvironmentError> {
    let value = required_component(field, value)?.to_ascii_lowercase();
    if !value.bytes().all(allowed) {
        return Err(PythonEnvironmentError::InvalidFingerprintComponent { field });
    }
    Ok(value)
}

fn normalize_implementation(value: &str) -> String {
    match value {
        "cp" | "cpython" => "cpython".to_owned(),
        "pp" | "pypy" => "pypy".to_owned(),
        other => other.to_owned(),
    }
}

#[cfg(test)]
#[path = "environment_tests.rs"]
mod tests;
