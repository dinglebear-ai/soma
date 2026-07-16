use std::collections::BTreeSet;
use std::path::Path;

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SpawnGuardError {
    #[error("stdio command must be a bare executable name")]
    PathCommandDenied,
    #[error("stdio command is not allowlisted")]
    CommandDenied,
    #[error("stdio command must not be empty")]
    EmptyCommand,
}

#[derive(Debug, Clone)]
pub struct SpawnGuard {
    allowed: BTreeSet<String>,
}

impl Default for SpawnGuard {
    fn default() -> Self {
        Self::new([
            "npx", "uvx", "docker", "node", "python", "python3", "deno", "pipx", "dnx",
        ])
    }
}

impl SpawnGuard {
    pub fn new(commands: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            allowed: commands.into_iter().map(Into::into).collect(),
        }
    }

    pub fn with_extra(mut self, commands: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.allowed.extend(commands.into_iter().map(Into::into));
        self
    }

    pub fn validate_command(&self, command: &str) -> Result<(), SpawnGuardError> {
        if command.trim().is_empty() {
            return Err(SpawnGuardError::EmptyCommand);
        }
        if Path::new(command).components().count() > 1 || command.contains(['/', '\\']) {
            return Err(SpawnGuardError::PathCommandDenied);
        }
        if !self.allowed.contains(command) {
            return Err(SpawnGuardError::CommandDenied);
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "guard_tests.rs"]
mod tests;
