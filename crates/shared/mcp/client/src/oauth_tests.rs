use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use futures::future::BoxFuture;

use crate::config::UpstreamConfig;
use crate::upstream::http_body_cap::BodyCappedHttpClient;

use super::*;

#[derive(Default)]
struct FakeProvider {
    evicted: Mutex<Vec<(String, String)>>,
}

impl UpstreamOAuthProvider for FakeProvider {
    fn authenticated_http_client<'a>(
        &'a self,
        _upstream: &'a UpstreamConfig,
        _subject: &'a str,
        _http_client: BodyCappedHttpClient,
    ) -> BoxFuture<'a, Result<UpstreamOAuthHttpClient, UpstreamOAuthError>> {
        Box::pin(async { Err(UpstreamOAuthError::internal("not implemented in fake")) })
    }

    fn evict_subject(&self, upstream: &str, subject: &str) {
        self.evicted
            .lock()
            .expect("fake provider lock poisoned")
            .push((upstream.to_owned(), subject.to_owned()));
    }
}

struct FakeManager;

impl UpstreamOAuthManager for FakeManager {
    fn begin_authorization<'a>(
        &'a self,
        subject: &'a str,
    ) -> BoxFuture<'a, Result<BeginAuthorization, UpstreamOAuthError>> {
        Box::pin(async move {
            Ok(BeginAuthorization {
                authorization_url: format!("https://auth.example/authorize?subject={subject}"),
            })
        })
    }

    fn credential_status<'a>(
        &'a self,
        _subject: &'a str,
    ) -> BoxFuture<'a, Result<Option<UpstreamOAuthCredentialStatus>, UpstreamOAuthError>> {
        Box::pin(async {
            Ok(Some(UpstreamOAuthCredentialStatus {
                access_token_expires_at: 123,
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
        Box::pin(async { Ok("token".to_owned()) })
    }
}

#[tokio::test]
async fn oauth_runtime_uses_generic_provider_and_manager_traits() {
    let provider = Arc::new(FakeProvider::default());
    let mut managers: BTreeMap<String, Arc<dyn UpstreamOAuthManager>> = BTreeMap::new();
    managers.insert("drive".to_owned(), Arc::new(FakeManager));
    let runtime = UpstreamOAuthRuntime::new(provider.clone(), managers);

    let manager = runtime.manager("drive").expect("manager registered");
    let begin = manager.begin_authorization("alice").await.unwrap();
    let status = manager.credential_status("alice").await.unwrap().unwrap();

    runtime.evict_subject("drive", "alice");

    assert!(begin.authorization_url.contains("subject=alice"));
    assert_eq!(status.access_token_expires_at, 123);
    assert_eq!(
        provider
            .evicted
            .lock()
            .expect("fake provider lock poisoned")[0],
        ("drive".to_owned(), "alice".to_owned())
    );
}
