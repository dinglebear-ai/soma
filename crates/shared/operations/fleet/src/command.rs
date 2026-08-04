use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use soma_ops::Timestamp;

use crate::{RequestError, request::validate_absolute_path};

const MAX_PROGRAM_CHARS: usize = 4096;
const MAX_ARGUMENT_CHARS: usize = 4096;
// Allows the 256 canonical command arguments plus a bounded typed-launcher prelude.
const MAX_ARGUMENTS: usize = 320;
const MAX_OUTPUT_BYTES: usize = 16 * 1024 * 1024;

/// Bounded exec-style command request with no shell interpretation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandRequest {
    program: String,
    args: Vec<String>,
    working_dir: Option<PathBuf>,
    deadline: Timestamp,
    max_stdout_bytes: usize,
    max_stderr_bytes: usize,
}

impl CommandRequest {
    /// Creates and validates an exec-style request.
    pub fn new<I, S>(
        program: impl Into<String>,
        args: I,
        deadline: Timestamp,
    ) -> Result<Self, RequestError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let program = program.into();
        validate_program(&program)?;
        let args = args.into_iter().map(Into::into).collect::<Vec<_>>();
        if args.len() > MAX_ARGUMENTS {
            return Err(RequestError::TooManyArguments {
                count: args.len(),
                max: MAX_ARGUMENTS,
            });
        }
        for (index, argument) in args.iter().enumerate() {
            if !valid_text(argument, MAX_ARGUMENT_CHARS) {
                return Err(RequestError::InvalidArgument { index });
            }
        }
        Ok(Self {
            program,
            args,
            working_dir: None,
            deadline,
            max_stdout_bytes: 256 * 1024,
            max_stderr_bytes: 256 * 1024,
        })
    }

    /// Sets an absolute normalized working directory.
    pub fn with_working_dir(
        mut self,
        working_dir: impl Into<PathBuf>,
    ) -> Result<Self, RequestError> {
        self.working_dir = Some(validate_absolute_path(working_dir.into())?);
        Ok(self)
    }

    /// Sets bounded stdout and stderr budgets.
    pub fn with_output_limits(
        mut self,
        stdout_bytes: usize,
        stderr_bytes: usize,
    ) -> Result<Self, RequestError> {
        validate_output_limit("stdout", stdout_bytes)?;
        validate_output_limit("stderr", stderr_bytes)?;
        self.max_stdout_bytes = stdout_bytes;
        self.max_stderr_bytes = stderr_bytes;
        Ok(self)
    }

    /// Rejects a request whose deadline has already elapsed.
    pub fn validate_at(&self, now: Timestamp) -> Result<(), RequestError> {
        if self.deadline <= now {
            Err(RequestError::DeadlineElapsed)
        } else {
            Ok(())
        }
    }

    /// Returns the executable path or name.
    #[must_use]
    pub fn program(&self) -> &str {
        &self.program
    }

    /// Returns positional arguments without shell interpolation.
    #[must_use]
    pub fn args(&self) -> &[String] {
        &self.args
    }

    /// Returns the optional absolute working directory.
    #[must_use]
    pub fn working_dir(&self) -> Option<&Path> {
        self.working_dir.as_deref()
    }

    /// Returns the request deadline.
    #[must_use]
    pub const fn deadline(&self) -> Timestamp {
        self.deadline
    }

    /// Returns the maximum captured stdout bytes.
    #[must_use]
    pub const fn max_stdout_bytes(&self) -> usize {
        self.max_stdout_bytes
    }

    /// Returns the maximum captured stderr bytes.
    #[must_use]
    pub const fn max_stderr_bytes(&self) -> usize {
        self.max_stderr_bytes
    }
}

/// Bounded command execution output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandOutput {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    exit_code: Option<i32>,
    truncated: bool,
}

impl CommandOutput {
    /// Creates a command output record.
    #[must_use]
    pub fn new(stdout: Vec<u8>, stderr: Vec<u8>, exit_code: Option<i32>, truncated: bool) -> Self {
        Self {
            stdout,
            stderr,
            exit_code,
            truncated,
        }
    }

    /// Returns captured stdout bytes.
    #[must_use]
    pub fn stdout(&self) -> &[u8] {
        &self.stdout
    }

    /// Returns captured stderr bytes.
    #[must_use]
    pub fn stderr(&self) -> &[u8] {
        &self.stderr
    }

    /// Returns the process exit code when available.
    #[must_use]
    pub const fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }

    /// Returns whether either output stream was truncated.
    #[must_use]
    pub const fn truncated(&self) -> bool {
        self.truncated
    }
}

fn validate_program(program: &str) -> Result<(), RequestError> {
    if valid_text(program, MAX_PROGRAM_CHARS) {
        Ok(())
    } else {
        Err(RequestError::InvalidProgram)
    }
}

fn valid_text(value: &str, max_chars: usize) -> bool {
    let count = value.chars().count();
    count > 0 && count <= max_chars && !value.chars().any(char::is_control)
}

fn validate_output_limit(stream: &'static str, bytes: usize) -> Result<(), RequestError> {
    if bytes == 0 || bytes > MAX_OUTPUT_BYTES {
        Err(RequestError::InvalidOutputLimit { stream, bytes })
    } else {
        Ok(())
    }
}
#[cfg(test)]
#[path = "command_tests.rs"]
mod tests;
