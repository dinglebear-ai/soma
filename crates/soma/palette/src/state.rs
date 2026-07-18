use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use soma_application::{ExecutionContext, SomaApplication};
use soma_domain::{AuthorizationMode, Principal, RequestId, ScopeSet, Surface};

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;

/// Axum `State` for `/v1/palette/*` routes: the shared product application
/// plus the mounted authorization mode.
#[derive(Clone)]
pub struct PaletteState {
    application: Arc<SomaApplication>,
    authorization_mode: AuthorizationMode,
}

impl PaletteState {
    #[must_use]
    pub fn new(application: Arc<SomaApplication>, authorization_mode: AuthorizationMode) -> Self {
        Self {
            application,
            authorization_mode,
        }
    }

    #[must_use]
    pub fn application(&self) -> &SomaApplication {
        self.application.as_ref()
    }

    /// Build an [`ExecutionContext`] for a Palette request, tagged with
    /// `Surface::Palette` so downstream provider dispatch applies Palette
    /// surface policy (see `ToolSpec::exposed_on`).
    #[must_use]
    pub fn execution_context(&self, subject: Option<&str>, scopes: &[String]) -> ExecutionContext {
        ExecutionContext {
            principal: subject
                .map(|subject| Principal::new(subject, ScopeSet::new(scopes.iter().cloned()))),
            authorization_mode: self.authorization_mode,
            surface: Surface::Palette,
            trace: None,
            destructive_confirmation: Default::default(),
            response_limit: None,
            request_id: next_request_id(),
        }
    }
}

fn next_request_id() -> RequestId {
    static REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);
    let sequence = REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    RequestId::new(format!("palette-{}-{sequence}", std::process::id()))
        .expect("generated palette request ids are valid")
}
