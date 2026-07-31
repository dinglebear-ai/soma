use serde_json::Value;

use crate::ProviderSurface;

/// Authenticated, traceable context carried into an isolated provider runtime.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProviderInvocationContext {
    pub request_id: String,
    pub actor_id: Option<String>,
    pub actor_scopes: Vec<String>,
    pub traceparent: Option<String>,
    pub tracestate: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProviderCall {
    pub provider: String,
    pub action: String,
    pub params: Value,
    pub surface: ProviderSurface,
    pub snapshot_id: String,
    pub context: ProviderInvocationContext,
}

impl ProviderCall {
    pub fn new(action: impl Into<String>, arguments: Value) -> Self {
        Self {
            provider: String::new(),
            action: action.into(),
            params: arguments,
            surface: ProviderSurface::Internal,
            snapshot_id: String::new(),
            context: ProviderInvocationContext::default(),
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

    #[must_use]
    pub fn with_context(mut self, context: ProviderInvocationContext) -> Self {
        self.context = context;
        self
    }
}
