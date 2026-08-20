use std::collections::BTreeMap;

use thiserror::Error;

use crate::security::redact::redact_stdio_args;

const PROTECTED_ENV_NAMES: &[&str] = &[
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "SHELL",
    "IFS",
    "PWD",
    "LD_PRELOAD",
    "LD_LIBRARY_PATH",
    "DYLD_INSERT_LIBRARIES",
    "DYLD_LIBRARY_PATH",
    "NODE_OPTIONS",
    "PYTHONPATH",
    "PYTHONHOME",
];

#[derive(Debug, Error, PartialEq, Eq)]
pub enum EnvPolicyError {
    #[error("environment variable name is invalid")]
    InvalidName,
    #[error("environment variable is protected")]
    ProtectedName,
    #[error("environment variable value contains a prohibited control character")]
    InvalidValue,
    #[error("argument tries to weaken spawn guard")]
    SpawnGuardOverride,
}

pub fn validate_env_name(name: &str) -> Result<(), EnvPolicyError> {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return Err(EnvPolicyError::InvalidName);
    };
    if !(first == '_' || first.is_ascii_uppercase()) {
        return Err(EnvPolicyError::InvalidName);
    }
    if !chars.all(|ch| ch == '_' || ch.is_ascii_uppercase() || ch.is_ascii_digit()) {
        return Err(EnvPolicyError::InvalidName);
    }
    if PROTECTED_ENV_NAMES.contains(&name) || name.starts_with("MCP_GATEWAY_") {
        return Err(EnvPolicyError::ProtectedName);
    }
    Ok(())
}

pub fn validate_env_value(value: &str) -> Result<(), EnvPolicyError> {
    if value.contains(['\n', '\r', '\0']) {
        return Err(EnvPolicyError::InvalidValue);
    }
    Ok(())
}

pub fn validate_spawn_env(env: &BTreeMap<String, String>) -> Result<(), EnvPolicyError> {
    for (key, value) in env {
        validate_env_name(key)?;
        validate_env_value(value)?;
    }
    Ok(())
}

pub fn reject_spawn_guard_overrides(args: &[String]) -> Result<(), EnvPolicyError> {
    let joined = args.join(" ");
    if joined.contains("disable_spawn_guard") || joined.contains("disable-spawn-guard") {
        return Err(EnvPolicyError::SpawnGuardOverride);
    }
    Ok(())
}

#[must_use]
pub fn redact_spawn_args_for_log(args: &[String]) -> Vec<String> {
    redact_stdio_args(args)
}

#[cfg(test)]
#[path = "env_tests.rs"]
mod tests;
