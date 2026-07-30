//! Configuration for production-managed Python provider environments.

use serde::{Deserialize, Serialize};

/// Product configuration for immutable PEP 723 Python environments.
///
/// The lifecycle is disabled by default. When enabled, every identity-bearing
/// field is required so cache keys never depend on ambient interpreter state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct PythonEnvironmentConfig {
    pub enabled: bool,
    pub cache_root: String,
    pub uv_program: String,
    pub uv_version: String,
    pub python_executable: String,
    pub runtime_implementation: String,
    pub runtime_version: String,
    pub runtime_platform: String,
    pub wheel_platform_tag: String,
    pub sdk_wheel: String,
    pub sdk_wheel_sha256: String,
    pub offline: bool,
    pub update: bool,
    pub policy_version: u32,
}

impl Default for PythonEnvironmentConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            cache_root: String::new(),
            uv_program: String::new(),
            uv_version: String::new(),
            python_executable: String::new(),
            runtime_implementation: String::new(),
            runtime_version: String::new(),
            runtime_platform: String::new(),
            wheel_platform_tag: String::new(),
            sdk_wheel: String::new(),
            sdk_wheel_sha256: String::new(),
            offline: false,
            update: false,
            policy_version: 2,
        }
    }
}

impl PythonEnvironmentConfig {
    pub(crate) fn validate(&self) -> anyhow::Result<()> {
        if !self.enabled {
            return Ok(());
        }
        for (name, value) in [
            ("SOMA_PYTHON_ENVIRONMENT_CACHE_ROOT", &self.cache_root),
            ("SOMA_PYTHON_ENVIRONMENT_UV_PROGRAM", &self.uv_program),
            ("SOMA_PYTHON_ENVIRONMENT_UV_VERSION", &self.uv_version),
            (
                "SOMA_PYTHON_ENVIRONMENT_PYTHON_EXECUTABLE",
                &self.python_executable,
            ),
            (
                "SOMA_PYTHON_ENVIRONMENT_RUNTIME_IMPLEMENTATION",
                &self.runtime_implementation,
            ),
            (
                "SOMA_PYTHON_ENVIRONMENT_RUNTIME_VERSION",
                &self.runtime_version,
            ),
            (
                "SOMA_PYTHON_ENVIRONMENT_RUNTIME_PLATFORM",
                &self.runtime_platform,
            ),
            (
                "SOMA_PYTHON_ENVIRONMENT_WHEEL_PLATFORM_TAG",
                &self.wheel_platform_tag,
            ),
            ("SOMA_PYTHON_ENVIRONMENT_SDK_WHEEL", &self.sdk_wheel),
            (
                "SOMA_PYTHON_ENVIRONMENT_SDK_WHEEL_SHA256",
                &self.sdk_wheel_sha256,
            ),
        ] {
            if value.trim().is_empty() {
                anyhow::bail!("{name} is required when Python environments are enabled");
            }
        }
        let digest = self.sdk_wheel_sha256.trim();
        if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            anyhow::bail!(
                "SOMA_PYTHON_ENVIRONMENT_SDK_WHEEL_SHA256 must be exactly 64 hexadecimal characters"
            );
        }
        if self.policy_version == 0 {
            anyhow::bail!("SOMA_PYTHON_ENVIRONMENT_POLICY_VERSION must be greater than zero");
        }
        if self.offline && self.update {
            anyhow::bail!(
                "SOMA_PYTHON_ENVIRONMENT_UPDATE cannot be enabled with SOMA_PYTHON_ENVIRONMENT_OFFLINE"
            );
        }
        for (name, value) in [
            ("SOMA_PYTHON_ENVIRONMENT_CACHE_ROOT", &self.cache_root),
            ("SOMA_PYTHON_ENVIRONMENT_UV_PROGRAM", &self.uv_program),
            (
                "SOMA_PYTHON_ENVIRONMENT_PYTHON_EXECUTABLE",
                &self.python_executable,
            ),
            ("SOMA_PYTHON_ENVIRONMENT_SDK_WHEEL", &self.sdk_wheel),
        ] {
            if !std::path::Path::new(value).is_absolute() {
                anyhow::bail!("{name} must be an absolute path");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::PythonEnvironmentConfig;

    fn enabled() -> PythonEnvironmentConfig {
        PythonEnvironmentConfig {
            enabled: true,
            cache_root: "/var/cache/soma".to_owned(),
            uv_program: "/usr/local/bin/uv".to_owned(),
            uv_version: "0.11.31".to_owned(),
            python_executable: "/usr/bin/python3".to_owned(),
            runtime_implementation: "cpython".to_owned(),
            runtime_version: "3.12.4".to_owned(),
            runtime_platform: "linux-x86_64".to_owned(),
            wheel_platform_tag: "manylinux_2_17_x86_64".to_owned(),
            sdk_wheel: "/opt/soma/soma_provider-0.2.0-cp38-abi3-manylinux_2_17_x86_64.whl"
                .to_owned(),
            sdk_wheel_sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_owned(),
            ..PythonEnvironmentConfig::default()
        }
    }

    #[test]
    fn update_and_offline_are_mutually_exclusive() {
        let mut config = enabled();
        config.offline = true;
        config.update = true;
        assert!(
            config
                .validate()
                .unwrap_err()
                .to_string()
                .contains("cannot be enabled")
        );
    }

    #[test]
    fn policy_version_must_be_positive() {
        let mut config = enabled();
        config.policy_version = 0;
        assert!(
            config
                .validate()
                .unwrap_err()
                .to_string()
                .contains("POLICY_VERSION")
        );
    }
}
