use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;

use futures::StreamExt;
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{Mutex, Notify};
use tokio_util::codec::{FramedRead, LinesCodec};

use crate::runner::limits::MAX_STDIO_LINE_BYTES;
use crate::ToolError;

use super::job_guard::JobGuard;

pub type RunnerLines = FramedRead<tokio::process::ChildStdout, LinesCodec>;

#[derive(Debug, Clone)]
pub struct RunnerSpawn {
    pub program: PathBuf,
    pub args: Vec<String>,
}

impl RunnerSpawn {
    pub fn current_exe() -> Result<Self, ToolError> {
        Ok(Self {
            program: crate::runner_exe::resolve_runner_exe()?,
            args: Vec::new(),
        })
    }
}

#[derive(Clone, Debug)]
pub struct StderrBuffer {
    lines: Arc<Mutex<Vec<String>>>,
    notify: Arc<Notify>,
}

impl StderrBuffer {
    fn new() -> Self {
        Self {
            lines: Arc::new(Mutex::new(Vec::new())),
            notify: Arc::new(Notify::new()),
        }
    }

    pub async fn mark(&self) -> usize {
        self.lines.lock().await.len()
    }

    pub async fn take_since_and_clear(&self, start_index: usize) -> Vec<String> {
        let mut guard = self.lines.lock().await;
        let captured = guard
            .get(start_index..)
            .map(<[String]>::to_vec)
            .unwrap_or_default();
        guard.clear();
        captured
    }

    pub async fn clear(&self) {
        self.lines.lock().await.clear();
    }

    pub async fn flush_settle(&self) {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(50);
        let mut last_len = self.lines.lock().await.len();
        while let Ok(()) = tokio::time::timeout_at(deadline, self.notify.notified()).await {
            let len = self.lines.lock().await.len();
            if len == last_len {
                break;
            }
            last_len = len;
        }
    }
}

#[derive(Debug)]
pub struct RunnerHandle {
    pub child: Child,
    pub child_pid: Option<u32>,
    pub stdin: ChildStdin,
    pub lines: RunnerLines,
    pub stderr: StderrBuffer,
    pub success_count: u64,
    _job_guard: JobGuard,
    drain_task: tokio::task::JoinHandle<()>,
    _temp_dir: tempfile::TempDir,
}

impl RunnerHandle {
    pub fn spawn(spawn: &RunnerSpawn) -> Result<Self, ToolError> {
        let temp_dir = tempfile::TempDir::new().map_err(|err| {
            ToolError::internal_message(format!("failed to create runner temp dir: {err}"))
        })?;
        let mut command = Command::new(&spawn.program);
        command
            .args(&spawn.args)
            .current_dir(temp_dir.path())
            .env_clear()
            .kill_on_drop(true)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        command.process_group(0);

        let mut child = command.spawn().map_err(|err| {
            ToolError::internal_message(format!(
                "failed to spawn Code Mode runner from `{}`: {err}",
                spawn.program.display()
            ))
        })?;
        let child_pid = child.id();
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| ToolError::internal_message("runner stdin unavailable"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ToolError::internal_message("runner stdout unavailable"))?;
        let stderr_pipe = child
            .stderr
            .take()
            .ok_or_else(|| ToolError::internal_message("runner stderr unavailable"))?;
        let stderr = StderrBuffer::new();
        let drain_task = spawn_stderr_drain(stderr_pipe, stderr.clone());
        Ok(Self {
            child,
            child_pid,
            stdin,
            lines: FramedRead::new(
                stdout,
                LinesCodec::new_with_max_length(MAX_STDIO_LINE_BYTES),
            ),
            stderr,
            success_count: 0,
            _job_guard: JobGuard::new(child_pid),
            drain_task,
            _temp_dir: temp_dir,
        })
    }

    #[cfg(test)]
    pub fn spawn_stub_command(program: &str, args: &[&str]) -> Result<Self, ToolError> {
        let spawn = RunnerSpawn {
            program: PathBuf::from(program),
            args: args.iter().map(|value| value.to_string()).collect(),
        };
        Self::spawn(&spawn)
    }
}

impl Drop for RunnerHandle {
    fn drop(&mut self) {
        self.drain_task.abort();
        #[cfg(unix)]
        if let Some(pid) = self.child_pid {
            use nix::sys::signal::Signal;
            use nix::unistd::Pid;
            let _ = nix::sys::signal::killpg(Pid::from_raw(pid as i32), Signal::SIGKILL);
        }
    }
}

fn spawn_stderr_drain(
    stderr: tokio::process::ChildStderr,
    buffer: StderrBuffer,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        const CAP_ENTRIES: usize = 100_000;
        const CAP_BYTES: usize = 8 * 1024 * 1024;
        let mut lines = FramedRead::new(
            stderr,
            LinesCodec::new_with_max_length(MAX_STDIO_LINE_BYTES),
        );
        let mut total_bytes = 0usize;
        while let Some(line) = lines.next().await {
            let line = match line {
                Ok(line) => line,
                Err(_) => "[soma] runner stderr truncated".to_string(),
            };
            total_bytes = total_bytes.saturating_add(line.len() + 1);
            let mut buf = buffer.lines.lock().await;
            if buf.len() < CAP_ENTRIES && total_bytes <= CAP_BYTES {
                buf.push(line);
            } else if buf
                .last()
                .is_none_or(|last| last != "[soma] runner stderr truncated")
            {
                buf.push("[soma] runner stderr truncated".to_string());
            }
            drop(buf);
            buffer.notify.notify_waiters();
        }
    })
}
