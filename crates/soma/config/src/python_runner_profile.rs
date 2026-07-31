use serde::{Deserialize, Serialize};

/// Python provider execution mode.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PythonRunnerMode {
    /// Spawn one bounded interpreter for each catalog or invocation request.
    #[default]
    OneShot,
    /// Reuse one supervised interpreter per active Python provider.
    Persistent,
}

/// Ambient-authority posture for persistent Python workers.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PythonExecutionProfile {
    Disabled,
    #[default]
    Trusted,
    Brokered,
}

#[cfg(test)]
#[path = "python_runner_profile_tests.rs"]
mod tests;
