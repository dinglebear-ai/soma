use std::sync::Arc;

use tokio::sync::Semaphore;

use crate::ToolError;

use super::config::PoolConfig;
use super::runner_handle::{RunnerHandle, RunnerSpawn};

pub struct RunnerPool {
    config: PoolConfig,
    spawn: RunnerSpawn,
    overflow: Arc<Semaphore>,
}

impl RunnerPool {
    pub fn new(config: PoolConfig, spawn: RunnerSpawn) -> Self {
        Self {
            overflow: Arc::new(Semaphore::new(config.max_overflow.max(1))),
            config,
            spawn,
        }
    }

    pub async fn checkout(&self) -> Result<RunnerLease, ToolError> {
        let permit = self
            .overflow
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| ToolError::internal_message("runner pool semaphore closed"))?;
        Ok(RunnerLease {
            handle: None,
            _permit: permit,
        })
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
