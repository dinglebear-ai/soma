use std::collections::BTreeMap;

use thiserror::Error;

use super::guard::{SpawnGuard, SpawnGuardError};
use crate::security::env::{self, EnvPolicyError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StdioProcessSpec {
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum StdioSpecError {
    #[error(transparent)]
    Guard(#[from] SpawnGuardError),
    #[error(transparent)]
    Env(#[from] EnvPolicyError),
}

impl StdioProcessSpec {
    pub fn validate(&self, guard: &SpawnGuard) -> Result<(), StdioSpecError> {
        guard.validate_command(&self.command)?;
        env::validate_spawn_env(&self.env)?;
        env::reject_spawn_guard_overrides(&self.args)?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "stdio_tests.rs"]
mod tests;
