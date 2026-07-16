use std::process::Command;

use serde::{Deserialize, Serialize};

include!(concat!(env!("OUT_DIR"), "/compat_generated.rs"));

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SurfaceSummary {
    pub client_requests: usize,
    pub server_requests: usize,
    pub server_notifications: usize,
    pub client_notifications: usize,
}

impl SurfaceSummary {
    pub const fn current() -> Self {
        Self {
            client_requests: CLIENT_REQUEST_METHOD_COUNT,
            server_requests: SERVER_REQUEST_METHOD_COUNT,
            server_notifications: SERVER_NOTIFICATION_METHOD_COUNT,
            client_notifications: CLIENT_NOTIFICATION_METHOD_COUNT,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CompatibilityReport {
    pub schema_codex_version: String,
    pub installed_codex_version: Option<String>,
    pub surface: SurfaceSummary,
}

impl CompatibilityReport {
    pub fn current() -> Self {
        Self::from_installed_version(installed_codex_version())
    }

    pub fn from_installed_version(installed_codex_version: Option<String>) -> Self {
        Self {
            schema_codex_version: CODEX_SCHEMA_VERSION.trim().to_owned(),
            installed_codex_version,
            surface: SurfaceSummary::current(),
        }
    }

    pub fn schema_matches_installed(&self) -> bool {
        self.installed_codex_version
            .as_deref()
            .map(str::trim)
            .is_some_and(|installed| installed == self.schema_codex_version)
    }
}

fn installed_codex_version() -> Option<String> {
    let output = Command::new("codex").arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let version = String::from_utf8(output.stdout).ok()?;
    let version = version.trim();
    (!version.is_empty()).then(|| version.to_owned())
}
