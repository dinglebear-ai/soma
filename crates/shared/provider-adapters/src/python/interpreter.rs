use std::path::PathBuf;

use super::materializer;

/// Selects the Python interpreter. `Ambient` preserves historical command
/// overrides, while `Prepared` is authoritative so a managed immutable
/// environment cannot be bypassed by process or provider configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum PythonInterpreter {
    #[default]
    Ambient,
    Prepared(PathBuf),
}

impl PythonInterpreter {
    pub fn prepared(environment: &materializer::PreparedPythonEnvironment) -> Self {
        Self::Prepared(environment.python.clone())
    }

    pub(crate) fn command(&self) -> String {
        match self {
            Self::Ambient => default_python_command().to_owned(),
            Self::Prepared(path) => path.to_string_lossy().into_owned(),
        }
    }
}

pub(super) fn select_python_command(
    manifest_command: Option<&str>,
    environment_command: Option<String>,
    interpreter: &PythonInterpreter,
) -> String {
    match interpreter {
        PythonInterpreter::Prepared(_) => interpreter.command(),
        PythonInterpreter::Ambient => manifest_command
            .map(str::to_owned)
            .or(environment_command)
            .unwrap_or_else(|| interpreter.command()),
    }
}

#[cfg(windows)]
pub(crate) fn default_python_command() -> &'static str {
    "python"
}

#[cfg(not(windows))]
pub(crate) fn default_python_command() -> &'static str {
    "python3"
}
