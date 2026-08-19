//! Bounded remote JWKS cache for CIMD private_key_jwt clients.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use jsonwebtoken::jwk::JwkSet;
use tokio::sync::Mutex;

use crate::error::AuthError;

use super::{document, ssrf};

const MAX_JWKS_CACHE_ENTRIES: usize = 256;
const JWKS_CACHE_TTL: Duration = Duration::from_secs(300);
const JWKS_NEGATIVE_TTL: Duration = Duration::from_secs(30);

#[derive(Clone)]
struct CachedJwks {
    jwks: JwkSet,
    fetched_at: Instant,
}

/// Bounded, single-flight cache for public key sets referenced by CIMD clients.
///
/// Positive entries are keyed by URL. Negative entries include the required
/// key id so one just-rotated key does not suppress a different key already
/// present in the same remote document.
pub(crate) struct JwksCache {
    entries: DashMap<String, CachedJwks>,
    negative: DashMap<String, Instant>,
    build_locks: DashMap<String, Arc<Mutex<()>>>,
}

impl JwksCache {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            entries: DashMap::new(),
            negative: DashMap::new(),
            build_locks: DashMap::new(),
        }
    }

    pub(crate) async fn fetch_for_kid(
        &self,
        url: &str,
        required_kid: &str,
    ) -> Result<JwkSet, AuthError> {
        let parsed = ssrf::validate_url_shape(url).map_err(|_| unavailable())?;
        if let Some(jwks) = self.cached_for_kid(url, required_kid) {
            return Ok(jwks);
        }
        let negative_key = negative_key(url, required_kid);
        if self.negative_fresh(&negative_key) {
            return Err(unavailable());
        }

        let lock = self.lock_for(url.to_owned());
        let _guard = lock.lock().await;
        if let Some(jwks) = self.cached_for_kid(url, required_kid) {
            return Ok(jwks);
        }
        if self.negative_fresh(&negative_key) {
            return Err(unavailable());
        }

        let host = parsed.host_str().ok_or_else(unavailable)?.to_owned();
        let port = parsed.port_or_known_default().unwrap_or(443);
        let addr = match parsed.host() {
            Some(url::Host::Domain(_)) => document::resolve_and_validate_address(&host, port)
                .await
                .map_err(|_| unavailable())?,
            Some(url::Host::Ipv4(ip)) => SocketAddr::new(IpAddr::V4(ip), port),
            Some(url::Host::Ipv6(ip)) => SocketAddr::new(IpAddr::V6(ip), port),
            None => return Err(unavailable()),
        };
        self.fetch_at(url, addr, required_kid).await
    }

    /// Network/parse/cache half of remote JWKS loading after URL shape and DNS
    /// validation have produced the exact address to pin. Kept separate so
    /// tests can exercise the real transport path against localhost without
    /// weakening production SSRF preflight.
    async fn fetch_at(
        &self,
        url: &str,
        addr: SocketAddr,
        required_kid: &str,
    ) -> Result<JwkSet, AuthError> {
        let negative_key = negative_key(url, required_kid);
        let client = document::build_pinned_client(url, addr).map_err(|_| unavailable())?;
        let body = document::fetch_body_at(&client, url, addr)
            .await
            .map_err(|_| unavailable())?;
        let jwks: JwkSet = serde_json::from_slice(&body).map_err(|_| unavailable())?;

        if jwks.find(required_kid).is_none() {
            self.record_negative(negative_key);
            tracing::warn!(
                event = "oauth.cimd_jwks.refresh",
                outcome = "required_key_absent",
                "CIMD remote JWKS refresh completed without the required key"
            );
            return Err(unavailable());
        }

        self.negative.remove(&negative_key);
        self.insert(url.to_owned(), jwks.clone());
        tracing::info!(
            event = "oauth.cimd_jwks.refresh",
            outcome = "required_key_found",
            "CIMD remote JWKS refresh completed"
        );
        Ok(jwks)
    }

    fn cached_for_kid(&self, url: &str, required_kid: &str) -> Option<JwkSet> {
        let entry = self.entries.get(url)?;
        if entry.fetched_at.elapsed() >= JWKS_CACHE_TTL || entry.jwks.find(required_kid).is_none() {
            return None;
        }
        Some(entry.jwks.clone())
    }

    fn negative_fresh(&self, key: &str) -> bool {
        self.negative
            .get(key)
            .is_some_and(|entry| entry.elapsed() < JWKS_NEGATIVE_TTL)
    }

    fn lock_for(&self, url: String) -> Arc<Mutex<()>> {
        if self.build_locks.len() >= MAX_JWKS_CACHE_ENTRIES {
            self.build_locks
                .retain(|_, lock| Arc::strong_count(lock) > 1);
        }
        self.build_locks
            .entry(url)
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    #[cfg(test)]
    pub(crate) fn seed_for_test(&self, url: impl Into<String>, jwks: JwkSet) {
        self.insert(url.into(), jwks);
    }

    fn insert(&self, url: String, jwks: JwkSet) {
        if self.entries.len() >= MAX_JWKS_CACHE_ENTRIES {
            self.entries
                .retain(|_, entry| entry.fetched_at.elapsed() < JWKS_CACHE_TTL);
            if self.entries.len() >= MAX_JWKS_CACHE_ENTRIES
                && let Some(oldest) = self
                    .entries
                    .iter()
                    .max_by_key(|entry| entry.value().fetched_at.elapsed())
                    .map(|entry| entry.key().clone())
            {
                self.entries.remove(&oldest);
            }
        }
        self.entries.insert(
            url,
            CachedJwks {
                jwks,
                fetched_at: Instant::now(),
            },
        );
    }

    fn record_negative(&self, key: String) {
        if self.negative.len() >= MAX_JWKS_CACHE_ENTRIES {
            self.negative
                .retain(|_, instant| instant.elapsed() < JWKS_NEGATIVE_TTL);
            if self.negative.len() >= MAX_JWKS_CACHE_ENTRIES
                && let Some(oldest) = self
                    .negative
                    .iter()
                    .max_by_key(|entry| entry.value().elapsed())
                    .map(|entry| entry.key().clone())
            {
                self.negative.remove(&oldest);
            }
        }
        self.negative.insert(key, Instant::now());
    }
}

impl Default for JwksCache {
    fn default() -> Self {
        Self::new()
    }
}

fn negative_key(url: &str, kid: &str) -> String {
    format!("{url}{kid}")
}

fn unavailable() -> AuthError {
    AuthError::AuthFailed("client JWKS is unavailable".to_string())
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    #[test]
    fn negative_cache_key_separates_key_ids() {
        assert_ne!(
            negative_key("https://client.example/jwks", "a"),
            negative_key("https://client.example/jwks", "b")
        );
    }

    #[tokio::test]
    async fn pinned_fetch_parses_and_caches_the_required_key() {
        let server = MockServer::start().await;
        let addr = *server.address();
        let url = format!("{}/jwks", server.uri());
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(crate::authorize::tests::test_jwks()),
            )
            .expect(1)
            .mount(&server)
            .await;

        let cache = JwksCache::new();
        let jwks = cache
            .fetch_at(&url, addr, "test-kid")
            .await
            .expect("remote JWKS contains required test key");
        assert!(jwks.find("test-kid").is_some());
        assert!(cache.cached_for_kid(&url, "test-kid").is_some());
        server.verify().await;
    }

    #[tokio::test]
    async fn pinned_fetch_negative_caches_a_missing_required_key() {
        let server = MockServer::start().await;
        let addr = *server.address();
        let url = format!("{}/jwks", server.uri());
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(crate::authorize::tests::test_jwks()),
            )
            .mount(&server)
            .await;

        let cache = JwksCache::new();
        let error = cache
            .fetch_at(&url, addr, "rotated-kid-not-yet-published")
            .await
            .expect_err("missing required kid must fail closed");
        assert!(matches!(error, AuthError::AuthFailed(_)));
        assert!(cache.negative_fresh(&negative_key(&url, "rotated-kid-not-yet-published")));
    }

    #[test]
    fn cache_is_bounded_when_many_entries_are_inserted() {
        let cache = JwksCache::new();
        let jwks = JwkSet { keys: Vec::new() };
        for index in 0..(MAX_JWKS_CACHE_ENTRIES + 8) {
            cache.insert(format!("https://client.example/{index}"), jwks.clone());
        }
        assert!(cache.entries.len() <= MAX_JWKS_CACHE_ENTRIES);
    }
}
