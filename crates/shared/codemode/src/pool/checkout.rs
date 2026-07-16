use std::sync::Arc;

use tokio::sync::{Mutex, Semaphore};

use crate::ToolError;

use super::config::PoolConfig;
use super::disposition::RunnerDisposition;
use super::runner_handle::{RunnerHandle, RunnerSpawn};

pub struct RunnerPool {
    config: PoolConfig,
    spawn: RunnerSpawn,
    overflow: Arc<Semaphore>,
    available: Mutex<Vec<RunnerHandle>>,
}

impl RunnerPool {
    pub fn new(config: PoolConfig, spawn: RunnerSpawn) -> Self {
        Self {
            overflow: Arc::new(Semaphore::new(
                config.size.saturating_add(config.max_overflow).max(1),
            )),
            config,
            spawn,
            available: Mutex::new(Vec::new()),
        }
    }

    pub async fn checkout(&self) -> Result<RunnerLease, ToolError> {
        let permit = self
            .overflow
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| ToolError::internal_message("runner pool semaphore closed"))?;
        let handle = if self.config.is_disabled() {
            RunnerHandle::spawn(&self.spawn)?
        } else {
            match self.available.lock().await.pop() {
                Some(handle) => handle,
                None => RunnerHandle::spawn(&self.spawn)?,
            }
        };
        Ok(RunnerLease {
            handle: Some(handle),
            _permit: permit,
        })
    }

    pub async fn release(&self, mut lease: RunnerLease, disposition: RunnerDisposition) {
        let Some(handle) = lease.handle.take() else {
            return;
        };
        if self.config.is_disabled() || !matches!(disposition, RunnerDisposition::Reuse) {
            return;
        }
        let mut available = self.available.lock().await;
        if available.len() < self.config.size {
            available.push(handle);
        }
    }

    pub fn config(&self) -> PoolConfig {
        self.config
    }

    pub fn spawn(&self) -> &RunnerSpawn {
        &self.spawn
    }
}

pub struct RunnerLease {
    pub handle: Option<RunnerHandle>,
    _permit: tokio::sync::OwnedSemaphorePermit,
}

impl RunnerLease {
    pub fn handle_mut(&mut self) -> Result<&mut RunnerHandle, ToolError> {
        self.handle
            .as_mut()
            .ok_or_else(|| ToolError::internal_message("runner lease has no handle"))
    }
}
