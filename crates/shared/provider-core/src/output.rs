use serde_json::Value;

use crate::ProviderProgressEvent;

#[derive(Debug, Clone, PartialEq)]
pub struct ProviderOutput {
    pub value: Value,
    pub progress: Vec<ProviderProgressEvent>,
}

impl ProviderOutput {
    pub fn value(value: Value) -> Self {
        Self {
            value,
            progress: Vec::new(),
        }
    }

    pub fn json(value: Value) -> Self {
        Self::value(value)
    }

    pub fn into_value(self) -> Value {
        self.value
    }

    #[must_use]
    pub fn with_progress(mut self, progress: Vec<ProviderProgressEvent>) -> Self {
        self.progress = progress;
        self
    }
}
