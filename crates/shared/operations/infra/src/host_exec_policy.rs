use std::path::{Path, PathBuf};

#[cfg(any(feature = "process-driver", test))]
use crate::HostExecRequest;
use crate::{FileReadPolicy, InfraError, InfraResult};

const MAX_EXEC_ROOTS: usize = 32;

/// Explicit read roots used by the typed host command launcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostExecPolicy {
    files: FileReadPolicy,
}

impl HostExecPolicy {
    /// Creates a host command policy with one to thirty-two absolute roots.
    pub fn new<I, P>(roots: I) -> InfraResult<Self>
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        let roots = roots.into_iter().map(Into::into).collect::<Vec<_>>();
        if roots.is_empty() || roots.len() > MAX_EXEC_ROOTS {
            return Err(invalid(format!(
                "host execution requires 1-{MAX_EXEC_ROOTS} read roots"
            )));
        }
        Ok(Self {
            files: FileReadPolicy::new(roots)?,
        })
    }

    /// Returns roots in deterministic order.
    pub fn roots(&self) -> impl Iterator<Item = &Path> {
        self.files.roots()
    }

    #[cfg(any(feature = "process-driver", test))]
    pub(crate) fn launcher_plan(&self, request: &HostExecRequest) -> InfraResult<LauncherPlan> {
        let path_indices =
            crate::host_exec_argv::filesystem_operand_indices(request.command(), request.args())?;
        for index in &path_indices {
            let path = validate_operand_path(&request.args()[*index])?;
            self.files.resolve(path)?;
        }
        if let Some(path) = request.working_dir() {
            self.files.resolve(path)?;
        }
        let roots = self
            .files
            .roots()
            .map(|root| root.to_string_lossy().into_owned())
            .collect();
        Ok(LauncherPlan {
            path_indices,
            roots,
            working_dir: request
                .working_dir()
                .map(|path| path.to_string_lossy().into_owned()),
        })
    }
}

#[cfg(any(feature = "process-driver", test))]
pub(crate) struct LauncherPlan {
    pub(crate) path_indices: Vec<usize>,
    pub(crate) roots: Vec<String>,
    pub(crate) working_dir: Option<String>,
}

#[cfg(any(feature = "process-driver", test))]
fn validate_operand_path(value: &str) -> InfraResult<&Path> {
    let path = Path::new(value);
    if !path.is_absolute()
        || path.components().any(|part| {
            matches!(
                part,
                std::path::Component::ParentDir | std::path::Component::CurDir
            )
        })
        || value.chars().any(char::is_control)
    {
        Err(invalid(format!(
            "filesystem command operands must be absolute and normalized: {value:?}"
        )))
    } else {
        Ok(path)
    }
}

fn invalid(message: impl Into<String>) -> InfraError {
    InfraError::InvalidRequest {
        domain: "host-exec",
        message: message.into(),
    }
}

#[cfg(test)]
#[path = "host_exec_policy_tests.rs"]
mod tests;
