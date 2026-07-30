use super::{PythonSupervisorError, Worker};

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

#[derive(Debug, Default)]
pub(super) struct JobGuard;

impl JobGuard {
    pub(super) fn new(_pid: Option<u32>) -> Result<Self, PythonSupervisorError> {
        Ok(Self)
    }
}
