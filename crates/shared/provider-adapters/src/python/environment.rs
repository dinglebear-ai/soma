//! Non-executing PEP 723 discovery and deterministic Python environment plans.
//!
//! This module is intentionally side-effect free. Parsing never imports provider
//! code, and planning never creates a virtual environment or contacts a package
//! index. A later cache manager consumes the immutable plan.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

const PEP_723_START: &str = "# /// script";
const PEP_723_END: &str = "# ///";
const MAX_METADATA_BYTES: usize = 64 * 1024;
const ENVIRONMENT_PLAN_VERSION: u32 = 1;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PythonRuntimeFingerprint {
    pub implementation: String,
    pub version: String,
    pub platform: String,
}

impl PythonRuntimeFingerprint {
    pub fn new(
        implementation: impl Into<String>,
        version: impl Into<String>,
        platform: impl Into<String>,
    ) -> Result<Self, PythonEnvironmentError> {
        Ok(Self {
            implementation: required_component("implementation", implementation.into())?,
            version: required_component("version", version.into())?,
            platform: required_component("platform", platform.into())?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonEnvironmentPlan {
    pub key: String,
    pub directory: PathBuf,
    pub dependency_count: usize,
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
    let fingerprint = EnvironmentFingerprint {
        plan_version: ENVIRONMENT_PLAN_VERSION,
        metadata: &metadata,
        runtime,
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
        dependency_count: metadata.dependencies.len(),
    })
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
    sdk_wheel_sha256: &'a str,
    uv_version: &'a str,
}

fn normalize_metadata(block: &str) -> Result<Pep723Metadata, PythonEnvironmentError> {
    let raw: RawPep723Metadata = toml::from_str(block)
        .map_err(|error| PythonEnvironmentError::InvalidToml(error.to_string()))?;
    let requires_python = raw
        .requires_python
        .map(|value| normalize_nonempty("requires-python", value))
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

#[cfg(test)]
#[path = "environment_tests.rs"]
mod tests;
