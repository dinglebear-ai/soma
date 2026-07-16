use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use soma_application::{ExecutionContext, SomaApplication};
use soma_domain::{AuthorizationMode, Principal, RequestId, ScopeSet, Surface};

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;

#[derive(Clone)]
pub struct ApiState {
    application: Arc<SomaApplication>,
    authorization_mode: AuthorizationMode,
    server_name: Arc<str>,
}

impl ApiState {
    pub fn new(
        application: Arc<SomaApplication>,
        authorization_mode: AuthorizationMode,
        server_name: impl Into<Arc<str>>,
    ) -> Self {
        Self {
            application,
            authorization_mode,
            server_name: server_name.into(),
        }
    }

    pub fn application(&self) -> &SomaApplication {
        self.application.as_ref()
    }

    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    pub fn execution_context(&self, subject: Option<&str>, scopes: &[String]) -> ExecutionContext {
        ExecutionContext {
            principal: subject
                .map(|subject| Principal::new(subject, ScopeSet::new(scopes.iter().cloned()))),
            authorization_mode: self.authorization_mode,
            surface: Surface::Rest,
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
    RequestId::new(format!("rest-{}-{sequence}", std::process::id()))
        .expect("generated REST request ids are valid")
}
