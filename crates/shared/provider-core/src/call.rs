use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
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
    /// Host-owned channel used to return capability progress to the surface
    /// that initiated this invocation.
    pub progress: ProviderProgressReporter,
}

/// One bounded progress notification emitted by an isolated provider.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ProviderProgressEvent {
    pub current: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Cloneable, invocation-scoped progress collector.
///
/// The reporter deliberately has no global registry: a provider can only
/// write to the collector that the authenticated host call supplied.
#[derive(Debug, Clone, Default)]
pub struct ProviderProgressReporter(Arc<Mutex<Vec<ProviderProgressEvent>>>);

impl ProviderProgressReporter {
    const MAX_EVENTS: usize = 256;
    const MAX_MESSAGE_CHARS: usize = 1_024;

    pub fn report(&self, current: u64, total: Option<u64>, message: Option<&str>) {
        let mut events = self
            .0
            .lock()
            .expect("provider progress lock should not be poisoned");
        if events.len() == Self::MAX_EVENTS {
            events.remove(0);
        }
        events.push(ProviderProgressEvent {
            current,
            total,
            message: message.map(|value| value.chars().take(Self::MAX_MESSAGE_CHARS).collect()),
        });
    }

    #[must_use]
    pub fn events(&self) -> Vec<ProviderProgressEvent> {
        self.0
            .lock()
            .expect("provider progress lock should not be poisoned")
            .clone()
    }
}

impl PartialEq for ProviderProgressReporter {
    fn eq(&self, other: &Self) -> bool {
        self.events() == other.events()
    }
}

impl Eq for ProviderProgressReporter {}

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
