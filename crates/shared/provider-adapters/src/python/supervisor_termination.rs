use super::{PythonSupervisorError, Worker};

pub(super) struct ProcessTreeStartupGuard {
    pid: Option<u32>,
    armed: bool,
}

impl ProcessTreeStartupGuard {
    pub(super) fn new(pid: Option<u32>) -> Self {
        Self { pid, armed: true }
    }

    pub(super) fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for ProcessTreeStartupGuard {
    fn drop(&mut self) {
        if self.armed {
            terminate_process_tree(self.pid);
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        // `Child::kill_on_drop` only covers the direct child. The worker owns
        // the whole process tree, including descendants created by provider
        // code, so rollback and failed unpublished candidates must signal it.
        terminate_process_tree(self.child_pid);
        self.stderr_task.abort();
    }
}

pub(super) async fn terminate_worker(worker: Option<Worker>) {
    let Some(mut worker) = worker else {
        return;
    };
    terminate_process_tree(worker.child_pid);
    if let Err(error) = worker.child.kill().await
        && error.kind() != std::io::ErrorKind::InvalidInput
    {
        tracing::warn!(pid = ?worker.child_pid, %error, "failed to kill Python worker child");
    }
    if let Err(error) = worker.child.wait().await {
        tracing::warn!(pid = ?worker.child_pid, %error, "failed to reap Python worker child");
    }
    worker.stderr_task.abort();
}

#[cfg(unix)]
pub(super) fn terminate_process_tree(pid: Option<u32>) -> bool {
    use nix::{sys::signal::Signal, unistd::Pid};
    let Some(pid) = pid else {
        return true;
    };
    match nix::sys::signal::killpg(Pid::from_raw(pid as i32), Signal::SIGKILL) {
        Ok(()) | Err(nix::errno::Errno::ESRCH) => true,
        Err(error) => {
            tracing::warn!(pid, %error, "failed to terminate Python worker process group");
            false
        }
    }
}

#[cfg(windows)]
pub(super) fn terminate_process_tree(pid: Option<u32>) -> bool {
    let Some(pid) = pid else {
        return true;
    };
    match std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .status()
    {
        Ok(status) if status.success() || status.code() == Some(128) => true,
        Ok(status) => {
            tracing::warn!(
                pid,
                ?status,
                "taskkill failed to terminate Python worker tree"
            );
            false
        }
        Err(error) => {
            tracing::warn!(pid, %error, "failed to launch taskkill for Python worker tree");
            false
        }
    }
}

#[cfg(not(any(unix, windows)))]
pub(super) fn terminate_process_tree(_pid: Option<u32>) -> bool {
    tracing::warn!("process-tree termination is unsupported on this platform");
    false
}

#[cfg(not(windows))]
#[derive(Debug, Default)]
pub(super) struct JobGuard;

#[cfg(not(windows))]
impl JobGuard {
    pub(super) fn new(_child: &tokio::process::Child) -> Result<Self, PythonSupervisorError> {
        Ok(Self)
    }
}

#[cfg(windows)]
#[derive(Debug)]
pub(super) struct JobGuard {
    _job: win32job::Job,
}

#[cfg(windows)]
impl JobGuard {
    pub(super) fn new(child: &tokio::process::Child) -> Result<Self, PythonSupervisorError> {
        let process = child.raw_handle().ok_or_else(|| {
            PythonSupervisorError::new(
                "python_worker_start_failed",
                "Python worker process handle is unavailable",
            )
        })?;
        let mut info = win32job::ExtendedLimitInfo::new();
        info.limit_kill_on_job_close();
        let job = win32job::Job::create_with_limit_info(&info).map_err(|_| {
            PythonSupervisorError::new(
                "python_worker_start_failed",
                "Windows Job Object creation or configuration failed",
            )
        })?;
        job.assign_process(process as isize).map_err(|_| {
            PythonSupervisorError::new(
                "python_worker_start_failed",
                "Python worker could not be assigned to its Windows Job Object",
            )
        })?;
        Ok(Self { _job: job })
    }
}
