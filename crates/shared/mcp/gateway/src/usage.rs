use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageEvent {
    pub action: String,
    pub upstream: Option<String>,
    pub success: bool,
    pub bytes: usize,
}

pub trait UsageSink: Send + Sync {
    fn record(&self, event: UsageEvent);
}

#[derive(Debug, Default)]
pub struct NoopUsageSink;

impl UsageSink for NoopUsageSink {
    fn record(&self, _event: UsageEvent) {}
}

#[derive(Debug, Default)]
pub struct MemoryUsageSink {
    events: Mutex<Vec<UsageEvent>>,
}

impl MemoryUsageSink {
    #[must_use]
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    #[must_use]
    pub fn events(&self) -> Vec<UsageEvent> {
        self.events.lock().expect("usage sink poisoned").clone()
    }
}

impl UsageSink for MemoryUsageSink {
    fn record(&self, event: UsageEvent) {
        self.events.lock().expect("usage sink poisoned").push(event);
    }
}

#[cfg(test)]
#[path = "usage_tests.rs"]
mod tests;
