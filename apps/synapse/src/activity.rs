use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use soma_ops::Timestamp;

const DEFAULT_ACTIVITY_CAPACITY: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityEvent {
    pub timestamp: Timestamp,
    pub surface: String,
    pub operation: String,
    pub success: bool,
    pub elapsed_ms: u64,
    pub message: Option<String>,
}

#[derive(Clone)]
pub struct ActivityLog {
    events: Arc<Mutex<VecDeque<ActivityEvent>>>,
    capacity: usize,
}

impl Default for ActivityLog {
    fn default() -> Self {
        Self::new(DEFAULT_ACTIVITY_CAPACITY)
    }
}

impl ActivityLog {
    pub fn new(capacity: usize) -> Self {
        Self {
            events: Arc::new(Mutex::new(VecDeque::new())),
            capacity: capacity.clamp(1, 10_000),
        }
    }

    pub fn record(
        &self,
        surface: impl Into<String>,
        operation: impl Into<String>,
        success: bool,
        elapsed: Duration,
        message: Option<String>,
    ) {
        let mut events = self
            .events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while events.len() >= self.capacity {
            events.pop_front();
        }
        events.push_back(ActivityEvent {
            timestamp: Timestamp::now(),
            surface: bounded(surface.into(), 64),
            operation: bounded(operation.into(), 256),
            success,
            elapsed_ms: u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX),
            message: message.map(|message| bounded(message, 512)),
        });
    }

    pub fn snapshot(&self) -> Vec<ActivityEvent> {
        self.events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .cloned()
            .collect()
    }

    pub fn len(&self) -> usize {
        self.events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

fn bounded(mut value: String, max: usize) -> String {
    value.retain(|character| !character.is_control());
    if value.chars().count() > max {
        value = value.chars().take(max).collect();
    }
    value
}

#[cfg(test)]
#[path = "activity_tests.rs"]
mod tests;
