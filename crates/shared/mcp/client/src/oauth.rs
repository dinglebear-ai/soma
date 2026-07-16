use std::collections::BTreeMap;
use std::sync::Arc;

use futures::future::BoxFuture;
use serde::Serialize;
use thiserror::Error;

use crate::config::UpstreamConfig;
use crate::upstream::http_body_cap::BodyCappedHttpClient;

pub type UpstreamOAuthHttpClient = rmcp::transport::AuthClient<BodyCappedHttpClient>;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BeginAuthorization {
    pub authorization_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpstreamOAuthCredentialStatus {
    pub access_token_expires_at: i64,
    pub refresh_token_present: bool,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("{kind}: {message}")]
pub struct UpstreamOAuthError {
    kind: String,
    message: String,
}

impl UpstreamOAuthError {
    #[must_use]
    pub fn new(kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
        }
    }

    #[must_use]
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new("internal_error", message)
    }

    #[must_use]
    pub fn kind(&self) -> &str {
        &self.kind
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

pub trait UpstreamOAuthProvider: Send + Sync {
    fn authenticated_http_client<'a>(
        &'a self,
        upstream: &'a UpstreamConfig,
        subject: &'a str,
        http_client: BodyCappedHttpClient,
    ) -> BoxFuture<'a, Result<UpstreamOAuthHttpClient, UpstreamOAuthError>>;

    fn evict_subject(&self, _upstream: &str, _subject: &str) {}

    fn evict_upstream(&self, _upstream: &str) {}
}

pub trait UpstreamOAuthManager: Send + Sync {
    fn begin_authorization<'a>(
        &'a self,
        subject: &'a str,
    ) -> BoxFuture<'a, Result<BeginAuthorization, UpstreamOAuthError>>;

    fn credential_status<'a>(
        &'a self,
        subject: &'a str,
    ) -> BoxFuture<'a, Result<Option<UpstreamOAuthCredentialStatus>, UpstreamOAuthError>>;

    fn clear_credentials<'a>(
        &'a self,
        subject: &'a str,
    ) -> BoxFuture<'a, Result<(), UpstreamOAuthError>>;

    fn access_token<'a>(
        &'a self,
        subject: &'a str,
    ) -> BoxFuture<'a, Result<String, UpstreamOAuthError>>;
}

#[derive(Clone)]
pub struct UpstreamOAuthRuntime {
    provider: Arc<dyn UpstreamOAuthProvider>,
    managers: Arc<BTreeMap<String, Arc<dyn UpstreamOAuthManager>>>,
}

impl UpstreamOAuthRuntime {
    #[must_use]
    pub fn new(
        provider: Arc<dyn UpstreamOAuthProvider>,
        managers: BTreeMap<String, Arc<dyn UpstreamOAuthManager>>,
    ) -> Self {
        Self {
            provider,
            managers: Arc::new(managers),
        }
    }

    #[must_use]
    pub fn provider(&self) -> Arc<dyn UpstreamOAuthProvider> {
        Arc::clone(&self.provider)
    }

    #[must_use]
    pub fn manager(&self, upstream: &str) -> Option<Arc<dyn UpstreamOAuthManager>> {
        self.managers.get(upstream).cloned()
    }

    pub fn evict_subject(&self, upstream: &str, subject: &str) {
        self.provider.evict_subject(upstream, subject);
    }

    pub fn evict_upstream(&self, upstream: &str) {
        self.provider.evict_upstream(upstream);
    }
}

#[cfg(test)]
#[path = "oauth_tests.rs"]
mod tests;
