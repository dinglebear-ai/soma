//! Upstream-OAuth stub implementations used by `http_tests.rs`. Split out to
//! stay under the PATTERNS.md module size hard limit.
use std::sync::Arc;

use futures::future::BoxFuture;
use tokio::sync::Mutex;

use mcp_client::{
    oauth::{
        BeginAuthorization, UpstreamOAuthCredentialStatus, UpstreamOAuthError,
        UpstreamOAuthHttpClient, UpstreamOAuthManager, UpstreamOAuthProvider,
    },
    upstream::http_body_cap::BodyCappedHttpClient,
};

#[cfg(feature = "oauth")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RecordedAuthorizationCallback {
    pub(super) subject: String,
    pub(super) code: String,
    pub(super) state: String,
    pub(super) issuer: Option<String>,
}

#[cfg(feature = "oauth")]
pub(super) struct RecordingOAuthManager {
    pub(super) callbacks: Arc<Mutex<Vec<RecordedAuthorizationCallback>>>,
}

#[cfg(feature = "oauth")]
impl UpstreamOAuthManager for RecordingOAuthManager {
    fn begin_authorization<'a>(
        &'a self,
        _subject: &'a str,
    ) -> BoxFuture<'a, Result<BeginAuthorization, UpstreamOAuthError>> {
        Box::pin(async { Err(UpstreamOAuthError::internal("unused by callback test")) })
    }

    fn complete_authorization<'a>(
        &'a self,
        subject: &'a str,
        code: &'a str,
        state: &'a str,
        issuer: Option<&'a str>,
    ) -> BoxFuture<'a, Result<(), UpstreamOAuthError>> {
        Box::pin(async move {
            self.callbacks
                .lock()
                .await
                .push(RecordedAuthorizationCallback {
                    subject: subject.to_owned(),
                    code: code.to_owned(),
                    state: state.to_owned(),
                    issuer: issuer.map(str::to_owned),
                });
            Ok(())
        })
    }

    fn credential_status<'a>(
        &'a self,
        _subject: &'a str,
    ) -> BoxFuture<'a, Result<Option<UpstreamOAuthCredentialStatus>, UpstreamOAuthError>> {
        Box::pin(async { Ok(None) })
    }

    fn clear_credentials<'a>(
        &'a self,
        _subject: &'a str,
    ) -> BoxFuture<'a, Result<(), UpstreamOAuthError>> {
        Box::pin(async { Ok(()) })
    }

    fn access_token<'a>(
        &'a self,
        _subject: &'a str,
    ) -> BoxFuture<'a, Result<String, UpstreamOAuthError>> {
        Box::pin(async { Err(UpstreamOAuthError::internal("unused by callback test")) })
    }
}

#[cfg(feature = "oauth")]
pub(super) struct FakeOAuthProvider;

#[cfg(feature = "oauth")]
impl UpstreamOAuthProvider for FakeOAuthProvider {
    fn authenticated_http_client<'a>(
        &'a self,
        _upstream: &'a mcp_client::config::UpstreamConfig,
        _subject: &'a str,
        _http_client: BodyCappedHttpClient,
    ) -> BoxFuture<'a, Result<UpstreamOAuthHttpClient, UpstreamOAuthError>> {
        Box::pin(async {
            Err(UpstreamOAuthError::internal(
                "unused by protected proxy test",
            ))
        })
    }
}

#[cfg(feature = "oauth")]
pub(super) struct FakeOAuthManager;

#[cfg(feature = "oauth")]
impl UpstreamOAuthManager for FakeOAuthManager {
    fn begin_authorization<'a>(
        &'a self,
        _subject: &'a str,
    ) -> BoxFuture<'a, Result<BeginAuthorization, UpstreamOAuthError>> {
        Box::pin(async {
            Err(UpstreamOAuthError::internal(
                "unused by protected proxy test",
            ))
        })
    }

    fn complete_authorization<'a>(
        &'a self,
        _subject: &'a str,
        _code: &'a str,
        _state: &'a str,
        _issuer: Option<&'a str>,
    ) -> BoxFuture<'a, Result<(), UpstreamOAuthError>> {
        Box::pin(async { Ok(()) })
    }

    fn credential_status<'a>(
        &'a self,
        _subject: &'a str,
    ) -> BoxFuture<'a, Result<Option<UpstreamOAuthCredentialStatus>, UpstreamOAuthError>> {
        Box::pin(async {
            Ok(Some(UpstreamOAuthCredentialStatus {
                access_token_expires_at: 4_102_444_800,
                refresh_token_present: true,
            }))
        })
    }

    fn clear_credentials<'a>(
        &'a self,
        _subject: &'a str,
    ) -> BoxFuture<'a, Result<(), UpstreamOAuthError>> {
        Box::pin(async { Ok(()) })
    }

    fn access_token<'a>(
        &'a self,
        _subject: &'a str,
    ) -> BoxFuture<'a, Result<String, UpstreamOAuthError>> {
        Box::pin(async { Ok("oauth-token".to_owned()) })
    }
}
