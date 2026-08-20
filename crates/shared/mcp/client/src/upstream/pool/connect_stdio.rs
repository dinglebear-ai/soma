use thiserror::Error;

use crate::config::UpstreamConfig;
use crate::process::guard::{SpawnGuard, SpawnGuardError};
use crate::process::stdio::{StdioProcessSpec, StdioSpecError};
use crate::security::env::{self, EnvPolicyError};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConnectStdioError {
    #[error("upstream is not configured for stdio")]
    NotStdio,
    #[error(transparent)]
    Spec(#[from] StdioSpecError),
    #[error(transparent)]
    Guard(#[from] SpawnGuardError),
    #[error(transparent)]
    BearerEnv(#[from] EnvPolicyError),
}

pub fn plan_stdio_connection(
    config: &UpstreamConfig,
    guard: &SpawnGuard,
) -> Result<StdioProcessSpec, ConnectStdioError> {
    let command = config.command.clone().ok_or(ConnectStdioError::NotStdio)?;
    if let Some(env_name) = config.bearer_token_env.as_deref() {
        env::validate_env_name(env_name)?;
    }
    let spec = StdioProcessSpec {
        command,
        args: config.args.clone(),
        env: config.env.clone(),
    };
    spec.validate(guard)?;
    Ok(spec)
}

#[cfg(test)]
#[path = "connect_stdio_tests.rs"]
mod tests;
