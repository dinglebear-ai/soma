use std::collections::BTreeSet;
use std::path::Path;

use thiserror::Error;

const DANGEROUS_DOCKER_FLAGS: &[&str] = &[
    "--privileged",
    "--cap-add",
    "--volume",
    "-v",
    "--device",
    "--network",
    "--pid",
    "--ipc",
];
const DANGEROUS_NODE_FLAGS: &[&str] = &[
    "--inspect",
    "--require",
    "-r",
    "--experimental",
    "--allow",
    "-e",
    "--eval",
    "-p",
    "--print",
];
const DANGEROUS_BUN_FLAGS: &[&str] = &[
    "--inspect",
    "--inspect-wait",
    "--inspect-brk",
    "--preload",
    "--require",
    "--import",
    "-r",
    "-e",
    "--eval",
    "-p",
    "--print",
];
const DANGEROUS_PYTHON_FLAGS: &[&str] = &["-c", "--command", "-"];
const DANGEROUS_DENO_FLAGS: &[&str] = &["eval", "--allow-all", "-A"];

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SpawnGuardError {
    #[error("stdio command must be a bare executable name")]
    PathCommandDenied,
    #[error("stdio command is not allowlisted")]
    CommandDenied,
    #[error("stdio command must not be empty")]
    EmptyCommand,
    #[error("stdio argument contains a prohibited control character")]
    InvalidArgument,
    #[error("stdio argument is not allowed for runtime {runtime}")]
    DangerousArgument { runtime: String },
}

#[derive(Debug, Clone)]
pub struct SpawnGuard {
    allowed: BTreeSet<String>,
}

impl Default for SpawnGuard {
    fn default() -> Self {
        Self::new([
            "npx", "uvx", "docker", "node", "bun", "python", "python3", "deno", "pipx", "dnx",
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

    pub fn validate_args(&self, command: &str, args: &[String]) -> Result<(), SpawnGuardError> {
        for arg in args {
            if arg.contains(['\n', '\r', '\0']) {
                return Err(SpawnGuardError::InvalidArgument);
            }
            if runtime_argument_denied(command, arg) {
                return Err(SpawnGuardError::DangerousArgument {
                    runtime: command.to_owned(),
                });
            }
        }
        Ok(())
    }
}

fn runtime_argument_denied(runtime: &str, arg: &str) -> bool {
    let flag = arg.split('=').next().unwrap_or(arg);
    match runtime {
        "docker" => DANGEROUS_DOCKER_FLAGS.contains(&flag) || arg.starts_with("-v"),
        "node" => DANGEROUS_NODE_FLAGS
            .iter()
            .any(|denied| node_flag_matches(denied, flag, true)),
        "npx" => DANGEROUS_NODE_FLAGS
            .iter()
            .any(|denied| node_flag_matches(denied, flag, false)),
        "bun" => DANGEROUS_BUN_FLAGS
            .iter()
            .any(|denied| bun_flag_matches(denied, flag)),
        "python" | "python3" => DANGEROUS_PYTHON_FLAGS.contains(&flag) || arg.starts_with("-c"),
        "deno" => DANGEROUS_DENO_FLAGS.contains(&flag),
        _ => false,
    }
}

fn node_flag_matches(denied: &str, flag: &str, concatenated_short: bool) -> bool {
    flag == denied
        || concatenated_short && matches!(denied, "-e" | "-p" | "-r") && flag.starts_with(denied)
        || matches!(denied, "--allow" | "--experimental" | "--inspect")
            && flag.starts_with(&format!("{denied}-"))
}

fn bun_flag_matches(denied: &str, flag: &str) -> bool {
    flag == denied || matches!(denied, "-e" | "-p" | "-r") && flag.starts_with(denied)
}

#[cfg(test)]
#[path = "guard_tests.rs"]
mod tests;
