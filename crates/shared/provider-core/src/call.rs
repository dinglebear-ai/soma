use serde_json::Value;

use crate::ProviderSurface;

#[derive(Debug, Clone)]
pub struct ProviderCall {
    pub provider: String,
    pub action: String,
    pub params: Value,
    pub surface: ProviderSurface,
    pub snapshot_id: String,
}

impl ProviderCall {
    pub fn new(action: impl Into<String>, arguments: Value) -> Self {
        Self {
            provider: String::new(),
            action: action.into(),
            params: arguments,
            surface: ProviderSurface::Internal,
            snapshot_id: String::new(),
        }
    }

    #[must_use]
    pub fn with_surface(mut self, surface: ProviderSurface) -> Self {
        self.surface = surface;
        self
    }

    pub fn tool(&self) -> &str {
        &self.action
    }

    pub fn arguments(&self) -> &Value {
        &self.params
    }
}
