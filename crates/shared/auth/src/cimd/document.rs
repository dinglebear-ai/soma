//! Fetch, validate, and cache OAuth Client ID Metadata Documents (CIMD).
//!
//! Split into independently testable layers:
//! 1. [`ssrf::validate_url_shape`] — static URL checks, no network (tested
//!    in isolation in `cimd::ssrf`).
//! 2. [`resolve_and_validate_address`] — real DNS resolution, bounded by a
//!    timeout, rejecting the whole resolved-address set if any address is
//!    private. Tested with literal loopback/private hostnames — no real
//!    network access needed to prove a *rejection*; a real "successful
//!    public resolution" is not unit-tested here (see the plan's Global
//!    Constraints for why: no network access in CI).
//! 3. [`fetch_via_pinned_address`] / [`fetch_document_at`] — given an
//!    ALREADY resolved+validated address, builds a pinned/no-proxy/
//!    no-redirect client and does the GET + peer-recheck + streaming-cap +
//!    parse + validate. Tested against a local `wiremock` server by
//!    pointing the pin directly at its real bound address — this
//!    deliberately bypasses DNS resolution (same as production code does
//!    once step 2 has already resolved+validated an address), so it needs
//!    no network and no HTTPS certificate.
//!
//! [`fetch_and_validate_client_metadata`] composes all three for the real
//! production path, with per-key single-flight coordination and a short
//! negative-result cooldown for cached failures — the mechanism mirrors
//! `crate::upstream::cache::OauthClientCache`'s `build_locks` pattern, but
//! unlike that cache's `(upstream_name, subject)` key (bounded by operator
//! config and authenticated sessions, not attacker-controlled), `client_id`
//! here is an anonymous, attacker-controlled URL — see [`DocumentCache`]'s
//! own doc for how that difference is handled.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use serde::Deserialize;
use tokio::sync::Mutex;
use tracing::warn;

use crate::cimd::ssrf;

/// Maximum response body size accepted from a CIMD fetch, enforced via a
/// running counter WHILE STREAMING (never buffer-then-check — a hostile
/// server can otherwise force unbounded memory use regardless of this
/// constant).
const MAX_DOCUMENT_BYTES: usize = 64 * 1024;

/// Fetch timeout for CIMD document requests, applied to the HTTP client
/// AFTER DNS resolution has already completed (see `DNS_TIMEOUT` for the
/// separate bound on resolution itself). Matches this crate's existing
/// precedent for a request an interactive caller is actively waiting on —
/// `google.rs::GOOGLE_JWKS_FETCH_TIMEOUT` also uses 5s, so that a slow
/// upstream response can't stall the request past what a caller on
/// `/authorize` will tolerate.
const FETCH_TIMEOUT: Duration = Duration::from_secs(5);

/// Timeout for the DNS resolution step, bounded separately from
/// `FETCH_TIMEOUT` because `tokio::net::lookup_host` has no timeout of its
/// own — it delegates to the OS resolver, whose worst-case latency is
/// governed by `/etc/resolv.conf`/systemd-resolved settings, not by
/// anything in this code.
const DNS_TIMEOUT: Duration = Duration::from_secs(3);

/// Cache TTL for a successfully fetched and validated document.
const CACHE_TTL: Duration = Duration::from_secs(300);

/// Cache TTL for a *failed* fetch/validation attempt. Short — long enough
/// to blunt a burst of retries against a hostile or broken `client_id`
/// without permanently poisoning a transiently-unreachable legitimate one.
const NEGATIVE_CACHE_TTL: Duration = Duration::from_secs(60);

/// Hard cap on distinct cached URLs. `client_id` cardinality is
/// attacker-controlled (any public HTTPS server counts), not
/// traffic-volume-controlled, so this cannot be sized by "realistic
/// legitimate usage" — it exists specifically to bound the memory an
/// adversary can force this map to hold.
const MAX_CACHE_ENTRIES: usize = 10_000;

#[derive(Debug, Clone, Deserialize)]
pub struct ClientMetadataDocument {
    pub client_id: String,
    pub client_name: String,
    #[serde(default)]
    pub redirect_uris: Vec<String>,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum CimdError {
    #[error(transparent)]
    Ssrf(#[from] ssrf::SsrfError),
    /// A genuine DNS lookup failure (NXDOMAIN, resolver timeout, network
    /// unreachable) — an operational problem, NOT a security event. Kept
    /// distinct from [`Self::DnsBlocked`] so logs/callers can tell a
    /// mistyped hostname apart from an actual SSRF attempt.
    #[error("dns resolution failed for `{0}`: {1}")]
    DnsResolutionFailed(String, String),
    /// DNS resolution succeeded but at least one resolved address was
    /// private/loopback/link-local/etc — the whole result is rejected
    /// rather than falling back to a public address in the same set,
    /// since a hostname resolving to a mix of public and private
    /// addresses is itself a signal worth treating as untrusted.
    #[error(
        "`{0}` resolved to at least one private/loopback/link-local address; blocked to prevent SSRF"
    )]
    DnsBlocked(String),
    #[error("fetch failed: {0}")]
    Fetch(String),
    /// The actual TCP peer the response came from did not match the
    /// address this fetch was pinned to. This is the post-connect
    /// TOCTOU/proxy-interception backstop — see `fetch_document_at`.
    #[error(
        "peer address {actual} did not match the validated address {expected}; possible proxy interception or DNS-rebinding attempt"
    )]
    PeerMismatch {
        expected: SocketAddr,
        actual: SocketAddr,
    },
    #[error("invalid client metadata document: {0}")]
    InvalidDocument(String),
    #[error(
        "client metadata document client_id `{document_client_id}` does not match the requested URL `{requested_url}`"
    )]
    ClientIdMismatch {
        document_client_id: String,
        requested_url: String,
    },
}

impl CimdError {
    /// Stable kind string for structured logging. Deliberately NOT surfaced
    /// verbatim (via `Display`/`to_string()`) to the anonymous `/authorize`
    /// caller — see `authorize::resolve_client_redirect_uris`, which logs
    /// the full error server-side via this `kind()` plus `Display` but
    /// returns only a generic message in the HTTP response.
    /// A detailed message returned to an unauthenticated caller lets them
    /// distinguish "resolves internally" from "doesn't exist" from
    /// "resolves publicly but unreachable," which is a network-topology
    /// mapping oracle.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Ssrf(e) => e.kind(),
            Self::DnsResolutionFailed(..) => "dns_resolution_failed",
            Self::DnsBlocked(_) => "ssrf_blocked",
            Self::Fetch(_) => "cimd_fetch_failed",
            Self::PeerMismatch { .. } => "ssrf_blocked",
            Self::InvalidDocument(_) => "invalid_client_metadata",
            Self::ClientIdMismatch { .. } => "invalid_client_metadata",
        }
    }
}

/// Cheap detection heuristic: a CIMD `client_id` is an `https://` URL.
/// soma-auth's own DCR-issued `client_id`s are opaque base64url tokens
/// (`random_token(18)` in `authorize::register_client`) and can never start
/// with `https://`.
#[must_use]
pub fn is_cimd_client_id(client_id: &str) -> bool {
    client_id.starts_with("https://")
}

/// Resolve `host:port` via DNS (bounded by [`DNS_TIMEOUT`]) and return the
/// first resolved address, rejecting the *entire* result set if *any*
/// resolved address is private/loopback/etc — a hostname resolving to a
/// mix of public and private addresses is treated as untrusted outright
/// rather than cherry-picking a public one, since DNS load-balancing could
/// non-deterministically prefer the private one on a subsequent lookup
/// even though this specific call pins one address.
///
/// # Errors
/// Returns [`CimdError::DnsResolutionFailed`] on timeout/lookup failure or
/// an empty result set, and [`CimdError::DnsBlocked`] if any resolved
/// address is private.
pub async fn resolve_and_validate_address(host: &str, port: u16) -> Result<SocketAddr, CimdError> {
    let lookup = tokio::time::timeout(DNS_TIMEOUT, tokio::net::lookup_host((host, port)))
        .await
        .map_err(|_| {
            CimdError::DnsResolutionFailed(
                host.to_string(),
                format!("timed out after {DNS_TIMEOUT:?}"),
            )
        })?
        .map_err(|e| CimdError::DnsResolutionFailed(host.to_string(), e.to_string()))?;
    let addrs: Vec<SocketAddr> = lookup.collect();
    if addrs.is_empty() {
        return Err(CimdError::DnsResolutionFailed(
            host.to_string(),
            "resolved to no addresses".to_string(),
        ));
    }
    if addrs
        .iter()
        .any(|addr| ssrf::check_ip_not_private(addr.ip(), host).is_err())
    {
        return Err(CimdError::DnsBlocked(host.to_string()));
    }
    Ok(addrs[0])
}

/// Given an already resolved+validated `addr`, build a pinned, no-proxy,
/// no-redirect `reqwest::Client` and run the guarded fetch. This is the
/// test seam: tests call it directly with a local `wiremock` server's real
/// bound address, entirely bypassing DNS resolution — exactly what
/// production code does once [`resolve_and_validate_address`] (or the
/// IP-literal branch in [`fetch_and_validate_client_metadata`]) has already
/// produced a validated `addr`.
///
/// The pin host is derived from `url` itself (not taken as a separate
/// parameter) so it can never diverge from the host `.resolve()` needs to
/// intercept — a caller-supplied `host` that didn't match `url`'s real host
/// would silently defeat the pin with no compiler or runtime signal.
///
/// # Errors
/// Propagates [`CimdError`] from URL parsing, client construction, or
/// [`fetch_document_at`].
pub(crate) async fn fetch_via_pinned_address(
    url: &str,
    addr: SocketAddr,
) -> Result<ClientMetadataDocument, CimdError> {
    let parsed =
        url::Url::parse(url).map_err(|e| CimdError::Fetch(format!("parse `{url}`: {e}")))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| CimdError::Fetch(format!("no host in `{url}`")))?;
    let client = reqwest::Client::builder()
        .resolve(host, addr)
        // Without this, an ambient HTTPS_PROXY/ALL_PROXY env var makes
        // reqwest connect to a proxy that resolves `host` ITSELF, silently
        // discarding the `.resolve()` pin above and reopening the exact
        // DNS-rebinding window this whole module exists to close.
        .no_proxy()
        // A redirect would fetch a URL other than `url`, which
        // `fetch_document_at`'s exact-match check couldn't validate
        // against `client_id` — treat any 3xx as a hard failure instead
        // of following it.
        .redirect(reqwest::redirect::Policy::none())
        .timeout(FETCH_TIMEOUT)
        .build()
        .map_err(|e| CimdError::Fetch(format!("build pinned client for `{url}`: {e}")))?;
    fetch_document_at(&client, url, addr).await
}

/// Fetch and validate a CIMD document at `url` using an already
/// address-pinned `client`. Does NOT perform DNS resolution or SSRF
/// filtering itself — that is [`resolve_and_validate_address`]'s job. Does,
/// however, re-validate the actual TCP peer the response came from against
/// `pinned_addr` — this closes the gap a bare `.resolve()` pin leaves open
/// if a proxy intercepted the connection despite `.no_proxy()`, or if the
/// pin's `host` key ever diverges from the authority host reqwest derives
/// when re-parsing `url` internally.
///
/// # Errors
/// Returns [`CimdError::Fetch`] on transport/HTTP failure or a non-success
/// status, [`CimdError::PeerMismatch`] if the connected peer doesn't match
/// `pinned_addr`, [`CimdError::InvalidDocument`] on an oversized body,
/// malformed JSON, or missing/empty required fields, and
/// [`CimdError::ClientIdMismatch`] when the document's `client_id` does not
/// equal `url` exactly.
pub(crate) async fn fetch_document_at(
    client: &reqwest::Client,
    url: &str,
    pinned_addr: SocketAddr,
) -> Result<ClientMetadataDocument, CimdError> {
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|e| CimdError::Fetch(format!("GET `{url}`: {e}")))?;

    // No `check_ip_not_private` call on `peer` here: `pinned_addr` is
    // guaranteed non-private by the caller before it ever reaches this
    // function (either `resolve_and_validate_address`'s DNS-resolved
    // result, or an IP-literal host that already passed
    // `ssrf::validate_url_shape`'s own `check_ip_not_private` call). Once
    // `peer == pinned_addr` holds, re-running the private-range check on
    // `peer` would be redundant by construction — and would incorrectly
    // reject every test that pins directly at a local `wiremock` server,
    // which is the deliberate test seam this function's callers rely on.
    //
    // A missing `remote_addr()` fails CLOSED, not open: this peer-recheck is
    // the load-bearing TOCTOU/DNS-rebinding backstop, so "peer unknowable"
    // must never be treated as "peer trusted."
    match response.remote_addr() {
        Some(peer) if peer == pinned_addr => {}
        Some(peer) => {
            return Err(CimdError::PeerMismatch {
                expected: pinned_addr,
                actual: peer,
            });
        }
        None => {
            return Err(CimdError::Fetch(format!(
                "no remote peer address available for `{url}`; refusing to trust an unverified connection"
            )));
        }
    }

    if !response.status().is_success() {
        return Err(CimdError::Fetch(format!(
            "GET `{url}` returned HTTP {}",
            response.status()
        )));
    }

    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| CimdError::Fetch(format!("read body from `{url}`: {e}")))?
    {
        buf.extend_from_slice(&chunk);
        if buf.len() > MAX_DOCUMENT_BYTES {
            return Err(CimdError::InvalidDocument(format!(
                "document at `{url}` exceeds the {MAX_DOCUMENT_BYTES}-byte limit"
            )));
        }
    }

    let document: ClientMetadataDocument = serde_json::from_slice(&buf).map_err(|e| {
        CimdError::InvalidDocument(format!("document at `{url}` is not valid JSON: {e}"))
    })?;
    if document.client_id.is_empty() || document.client_name.is_empty() {
        return Err(CimdError::InvalidDocument(format!(
            "document at `{url}` is missing required client_id or client_name"
        )));
    }
    if document.redirect_uris.is_empty() {
        return Err(CimdError::InvalidDocument(format!(
            "document at `{url}` declares no redirect_uris"
        )));
    }
    if document.client_id != url {
        return Err(CimdError::ClientIdMismatch {
            document_client_id: document.client_id,
            requested_url: url.to_string(),
        });
    }
    Ok(document)
}

struct CacheEntry {
    /// The original `CimdError` is cached directly (it's already `Clone`) so
    /// a cache hit reports the same `kind()` a cache miss would have — e.g. a
    /// negatively-cached SSRF block must keep logging `kind="ssrf_blocked"`,
    /// not a generic fetch-failure tag, for the entire `NEGATIVE_CACHE_TTL`
    /// window an attacker's repeat requests spend being served from here.
    result: Result<ClientMetadataDocument, CimdError>,
    fetched_at: Instant,
    ttl: Duration,
}

/// Single-flight, TTL-and-negative-cached store for fetched CIMD documents,
/// keyed by the requested URL. Mirrors
/// `crate::upstream::cache::OauthClientCache`'s `build_locks` pattern:
/// concurrent callers for the same never-cached (or just-expired) URL
/// serialize on a per-key lock so only one of them actually performs the
/// DNS resolution + fetch; the rest wait for and reuse that result.
///
/// Unlike `OauthClientCache` (keyed by `(upstream_name, subject)`, a
/// cardinality bounded by operator config and authenticated sessions),
/// `build_locks` here is keyed by an anonymous, attacker-controlled `url`
/// string — so it uses the same `MAX_CACHE_ENTRIES` threshold `entries`
/// does, sweeping out locks nobody currently holds (`Arc::strong_count ==
/// 1` means only this map references it) whenever a new key is requested
/// at capacity. This is a softer guarantee than `entries`' hard cap: if
/// every held lock is genuinely busy (an in-flight fetch), a new key still
/// gets inserted, so sustained full-concurrency load across
/// `MAX_CACHE_ENTRIES`+ distinct URLs can transiently exceed the
/// threshold — self-correcting as those fetches complete and free their
/// locks, unlike the original unbounded-forever growth this replaced.
pub struct DocumentCache {
    entries: DashMap<String, CacheEntry>,
    build_locks: DashMap<String, Arc<Mutex<()>>>,
}

impl DocumentCache {
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: DashMap::new(),
            build_locks: DashMap::new(),
        }
    }

    fn get_fresh(&self, url: &str) -> Option<Result<ClientMetadataDocument, CimdError>> {
        let entry = self.entries.get(url)?;
        if entry.fetched_at.elapsed() >= entry.ttl {
            return None;
        }
        Some(entry.result.clone())
    }

    /// Acquire (creating if absent) the per-URL single-flight lock, sweeping
    /// out idle locks first if the map is at capacity. A lock is idle iff
    /// this map is the only owner (`strong_count == 1`) — an in-flight
    /// fetch holds a second clone for the duration of its lookup, so a swept
    /// lock can never be one another task is actively waiting on.
    fn lock_for(&self, url: String) -> Arc<Mutex<()>> {
        if self.build_locks.len() >= MAX_CACHE_ENTRIES {
            self.build_locks
                .retain(|_, lock| Arc::strong_count(lock) > 1);
        }
        self.build_locks
            .entry(url)
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    fn insert(
        &self,
        url: String,
        result: &Result<ClientMetadataDocument, CimdError>,
        ttl: Duration,
    ) {
        if self.entries.len() >= MAX_CACHE_ENTRIES {
            self.entries.retain(|_, e| e.fetched_at.elapsed() < e.ttl);
            if self.entries.len() >= MAX_CACHE_ENTRIES {
                // Every entry is still fresh — there's nothing expired to
                // evict. Skip caching this result rather than growing past
                // the cap: the fetch itself already succeeded/failed
                // correctly, this only forgoes memoizing it.
                warn!(
                    cache_size = self.entries.len(),
                    "CIMD document cache at capacity with no expired entries to evict; not caching this result"
                );
                return;
            }
        }
        self.entries.insert(
            url,
            CacheEntry {
                result: result.clone(),
                fetched_at: Instant::now(),
                ttl,
            },
        );
    }
}

impl Default for DocumentCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Production entry point: single-flight-locked cache lookup (including a
/// short negative-result cooldown for cached failures), else SSRF-validate
/// the URL shape, resolve+validate DNS (for domain hosts) or use the
/// already-validated IP literal directly, fetch via
/// [`fetch_via_pinned_address`], and cache the result either way.
///
/// # Errors
/// Propagates [`CimdError`] from any of the composed validation/fetch
/// steps.
pub async fn fetch_and_validate_client_metadata(
    cache: &DocumentCache,
    url: &str,
) -> Result<ClientMetadataDocument, CimdError> {
    if let Some(cached) = cache.get_fresh(url) {
        return cached;
    }

    // Validate shape BEFORE creating a build_locks entry: a malformed or
    // non-https client_id is rejected for free, without ever occupying a
    // permanent slot in the lock map — closing off the cheapest version of
    // the unbounded-growth attack (garbage URLs that never even reach the
    // network layer).
    let parsed = ssrf::validate_url_shape(url)?;

    let lock = cache.lock_for(url.to_string());
    let _guard = lock.lock().await;

    // Re-check after acquiring the lock: another caller may have finished
    // fetching (successfully or not) while we were waiting.
    if let Some(cached) = cache.get_fresh(url) {
        return cached;
    }

    let result: Result<ClientMetadataDocument, CimdError> = async {
        let host = parsed
            .host_str()
            .ok_or_else(|| CimdError::DnsResolutionFailed(url.to_string(), "no host".to_string()))?
            .to_string();
        let port = parsed.port_or_known_default().unwrap_or(443);

        let addr = match parsed.host() {
            Some(url::Host::Domain(_)) => resolve_and_validate_address(&host, port).await?,
            // IP-literal hosts already passed check_ip_not_private inside
            // validate_url_shape; no DNS step needed.
            Some(url::Host::Ipv4(ip)) => SocketAddr::new(IpAddr::V4(ip), port),
            Some(url::Host::Ipv6(ip)) => SocketAddr::new(IpAddr::V6(ip), port),
            None => unreachable!("validate_url_shape guarantees a host"),
        };

        fetch_via_pinned_address(url, addr).await
    }
    .await;

    let ttl = if result.is_ok() {
        CACHE_TTL
    } else {
        NEGATIVE_CACHE_TTL
    };
    cache.insert(url.to_string(), &result, ttl);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn is_cimd_client_id_detects_https_urls_only() {
        assert!(is_cimd_client_id(
            "https://app.example.com/oauth/client-metadata.json"
        ));
        assert!(!is_cimd_client_id("abcDEF123opaque-token"));
        assert!(!is_cimd_client_id("http://app.example.com/client.json"));
    }

    #[tokio::test]
    async fn resolve_and_validate_address_rejects_loopback_host() {
        let err = resolve_and_validate_address("localhost", 443)
            .await
            .unwrap_err();
        assert_eq!(err.kind(), "ssrf_blocked");
    }

    #[tokio::test]
    async fn resolve_and_validate_address_reports_dns_failure_distinctly_from_ssrf_block() {
        // A hostname under a reserved-for-documentation TLD that will not
        // resolve is a genuine lookup failure, not an SSRF block -- the
        // `kind()` must distinguish the two so operators aren't misled
        // into thinking a typo is an attack.
        let err = resolve_and_validate_address("definitely-does-not-exist.invalid", 443)
            .await
            .unwrap_err();
        assert_eq!(err.kind(), "dns_resolution_failed");
    }

    #[tokio::test]
    async fn fetch_via_pinned_address_succeeds_for_matching_client_id() {
        let server = MockServer::start().await;
        let addr = server.address();
        let url = format!("{}/client.json", server.uri());
        Mock::given(method("GET"))
            .and(path("/client.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "client_id": url,
                "client_name": "Example MCP Client",
                "redirect_uris": ["http://127.0.0.1:3000/callback"],
            })))
            .mount(&server)
            .await;

        let document = fetch_via_pinned_address(&url, *addr)
            .await
            .expect("fetch ok");
        assert_eq!(document.client_id, url);
        assert_eq!(document.client_name, "Example MCP Client");
        assert_eq!(
            document.redirect_uris,
            vec!["http://127.0.0.1:3000/callback"]
        );
    }

    #[tokio::test]
    async fn fetch_document_at_rejects_peer_mismatch() {
        // Simulates what would happen if the pin's target address ever
        // diverged from the actual connected peer (proxy interception,
        // resolve()-key mismatch): even though the client genuinely
        // connects to the real mock server, passing a WRONG `pinned_addr`
        // must be rejected rather than silently trusted.
        let server = MockServer::start().await;
        let real_addr = *server.address();
        let url = format!("{}/client.json", server.uri());
        Mock::given(method("GET"))
            .and(path("/client.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "client_id": url,
                "client_name": "Example",
                "redirect_uris": ["http://127.0.0.1:3000/callback"],
            })))
            .mount(&server)
            .await;

        let wrong_addr = SocketAddr::new(real_addr.ip(), real_addr.port().wrapping_add(1).max(1));
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let err = fetch_document_at(&client, &url, wrong_addr)
            .await
            .unwrap_err();
        assert!(matches!(err, CimdError::PeerMismatch { .. }));
        assert_eq!(err.kind(), "ssrf_blocked");
    }

    #[tokio::test]
    async fn fetch_via_pinned_address_rejects_client_id_mismatch() {
        let server = MockServer::start().await;
        let addr = *server.address();
        let url = format!("{}/client.json", server.uri());
        Mock::given(method("GET"))
            .and(path("/client.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "client_id": "https://attacker.example/spoofed.json",
                "client_name": "Spoofed Client",
                "redirect_uris": ["http://127.0.0.1:9999/callback"],
            })))
            .mount(&server)
            .await;

        let err = fetch_via_pinned_address(&url, addr).await.unwrap_err();
        assert!(matches!(err, CimdError::ClientIdMismatch { .. }));
        assert_eq!(err.kind(), "invalid_client_metadata");
    }

    #[tokio::test]
    async fn fetch_via_pinned_address_rejects_missing_required_fields() {
        let server = MockServer::start().await;
        let addr = *server.address();
        let url = format!("{}/client.json", server.uri());
        Mock::given(method("GET"))
            .and(path("/client.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "client_id": url,
                "redirect_uris": ["http://127.0.0.1:3000/callback"],
            })))
            .mount(&server)
            .await;

        let err = fetch_via_pinned_address(&url, addr).await.unwrap_err();
        assert!(matches!(err, CimdError::InvalidDocument(_)));
        assert_eq!(err.kind(), "invalid_client_metadata");
    }

    #[tokio::test]
    async fn fetch_via_pinned_address_rejects_malformed_json() {
        let server = MockServer::start().await;
        let addr = *server.address();
        let url = format!("{}/client.json", server.uri());
        Mock::given(method("GET"))
            .and(path("/client.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{not valid json"))
            .mount(&server)
            .await;

        let err = fetch_via_pinned_address(&url, addr).await.unwrap_err();
        assert!(matches!(err, CimdError::InvalidDocument(_)));
        assert_eq!(err.kind(), "invalid_client_metadata");
    }

    #[tokio::test]
    async fn fetch_via_pinned_address_rejects_empty_redirect_uris() {
        let server = MockServer::start().await;
        let addr = *server.address();
        let url = format!("{}/client.json", server.uri());
        Mock::given(method("GET"))
            .and(path("/client.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "client_id": url,
                "client_name": "Example",
                "redirect_uris": [],
            })))
            .mount(&server)
            .await;

        let err = fetch_via_pinned_address(&url, addr).await.unwrap_err();
        assert!(matches!(err, CimdError::InvalidDocument(_)));
        assert_eq!(err.kind(), "invalid_client_metadata");
    }

    #[tokio::test]
    async fn fetch_via_pinned_address_rejects_non_success_status() {
        let server = MockServer::start().await;
        let addr = *server.address();
        let url = format!("{}/missing.json", server.uri());
        Mock::given(method("GET"))
            .and(path("/missing.json"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let err = fetch_via_pinned_address(&url, addr).await.unwrap_err();
        assert!(matches!(err, CimdError::Fetch(_)));
        assert_eq!(err.kind(), "cimd_fetch_failed");
    }

    #[tokio::test]
    async fn fetch_via_pinned_address_does_not_follow_redirects() {
        let server = MockServer::start().await;
        let addr = *server.address();
        let url = format!("{}/redirecting.json", server.uri());
        Mock::given(method("GET"))
            .and(path("/redirecting.json"))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("Location", "https://attacker.example/elsewhere.json"),
            )
            .mount(&server)
            .await;

        let err = fetch_via_pinned_address(&url, addr).await.unwrap_err();
        assert!(matches!(err, CimdError::Fetch(_)));
        assert_eq!(err.kind(), "cimd_fetch_failed");
    }

    #[tokio::test]
    async fn fetch_via_pinned_address_rejects_oversized_body_without_hanging() {
        let server = MockServer::start().await;
        let addr = *server.address();
        let url = format!("{}/big.json", server.uri());
        let oversized = "x".repeat(MAX_DOCUMENT_BYTES + 1024);
        Mock::given(method("GET"))
            .and(path("/big.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string(oversized))
            .mount(&server)
            .await;

        let err = fetch_via_pinned_address(&url, addr).await.unwrap_err();
        assert!(matches!(err, CimdError::InvalidDocument(_)));
        assert_eq!(err.kind(), "invalid_client_metadata");
    }

    #[test]
    fn cache_returns_none_when_expired() {
        let cache = DocumentCache::new();
        let doc = ClientMetadataDocument {
            client_id: "https://app.example.com/client.json".to_string(),
            client_name: "Example".to_string(),
            redirect_uris: vec!["http://127.0.0.1:3000/callback".to_string()],
        };
        cache.insert(
            "https://app.example.com/client.json".to_string(),
            &Ok(doc),
            Duration::from_millis(1),
        );
        std::thread::sleep(Duration::from_millis(20));
        assert!(
            cache
                .get_fresh("https://app.example.com/client.json")
                .is_none()
        );
    }

    #[test]
    fn cache_returns_document_when_fresh() {
        let cache = DocumentCache::new();
        let doc = ClientMetadataDocument {
            client_id: "https://app.example.com/client.json".to_string(),
            client_name: "Example".to_string(),
            redirect_uris: vec!["http://127.0.0.1:3000/callback".to_string()],
        };
        cache.insert(
            "https://app.example.com/client.json".to_string(),
            &Ok(doc),
            CACHE_TTL,
        );
        assert!(
            cache
                .get_fresh("https://app.example.com/client.json")
                .is_some()
        );
    }

    #[test]
    fn cache_caches_negative_results_too() {
        let cache = DocumentCache::new();
        let err = CimdError::DnsBlocked("app.example.com".to_string());
        cache.insert(
            "https://app.example.com/client.json".to_string(),
            &Err(err),
            NEGATIVE_CACHE_TTL,
        );
        let cached = cache
            .get_fresh("https://app.example.com/client.json")
            .expect("negative result should be cached");
        // The specific variant (and therefore `kind()`) must survive the
        // cache round-trip -- a security-relevant classification like
        // "ssrf_blocked" must not be downgraded to a generic failure on a
        // cache hit (see CacheEntry's doc comment for why).
        assert!(matches!(cached, Err(CimdError::DnsBlocked(_))));
        assert_eq!(cached.unwrap_err().kind(), "ssrf_blocked");
    }

    #[test]
    fn entries_insert_skips_caching_once_at_capacity_with_nothing_expired() {
        let cache = DocumentCache::new();
        for i in 0..MAX_CACHE_ENTRIES {
            let url = format!("https://app.example.com/{i}.json");
            let doc = ClientMetadataDocument {
                client_id: url.clone(),
                client_name: "Example".to_string(),
                redirect_uris: vec!["http://127.0.0.1:3000/callback".to_string()],
            };
            cache.insert(url, &Ok(doc), CACHE_TTL);
        }
        assert_eq!(cache.entries.len(), MAX_CACHE_ENTRIES);

        let overflow_url = "https://app.example.com/overflow.json".to_string();
        let doc = ClientMetadataDocument {
            client_id: overflow_url.clone(),
            client_name: "Example".to_string(),
            redirect_uris: vec!["http://127.0.0.1:3000/callback".to_string()],
        };
        cache.insert(overflow_url.clone(), &Ok(doc), CACHE_TTL);

        // At capacity with nothing expired (every entry shares CACHE_TTL and
        // was just inserted): the overflow insert must be skipped rather
        // than growing the map past MAX_CACHE_ENTRIES.
        assert_eq!(cache.entries.len(), MAX_CACHE_ENTRIES);
        assert!(cache.get_fresh(&overflow_url).is_none());
    }

    #[test]
    fn lock_for_sweeps_idle_locks_through_the_real_entry_point_when_at_capacity() {
        let cache = DocumentCache::new();
        for i in 0..MAX_CACHE_ENTRIES {
            cache.build_locks.insert(
                format!("https://idle.example/{i}.json"),
                Arc::new(Mutex::new(())),
            );
        }
        assert_eq!(cache.build_locks.len(), MAX_CACHE_ENTRIES);

        let _ = cache.lock_for("https://new.example/client.json".to_string());

        // lock_for's own `len() >= MAX_CACHE_ENTRIES` branch (not just the
        // retain predicate in isolation) must have triggered the sweep: all
        // idle locks are gone, leaving only the newly-created one.
        assert_eq!(cache.build_locks.len(), 1);
        assert!(
            cache
                .build_locks
                .get("https://new.example/client.json")
                .is_some()
        );
    }

    #[tokio::test]
    async fn build_locks_coalesces_two_concurrent_misses_for_the_same_url_to_one_fetch() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        // Exercises the exact single-flight primitives production code uses
        // (`lock_for`, `get_fresh`, `insert`), with a fake "fetch" in place
        // of the real network call -- proving two genuinely-concurrent
        // cache MISSES for the same URL collapse to one fetch, which the
        // deleted network-level test never actually demonstrated (it
        // pre-seeded the cache, so both racers took the cache-hit path and
        // the lock was never contended). A real network-level version of
        // this test would need a public DNS-resolvable host, which this
        // module's other tests also avoid (see the module doc above) since
        // CI has no network access.
        async fn simulate_fetch(
            cache: &DocumentCache,
            url: &str,
            fetch_count: &AtomicUsize,
        ) -> ClientMetadataDocument {
            if let Some(Ok(doc)) = cache.get_fresh(url) {
                return doc;
            }
            let lock = cache.lock_for(url.to_string());
            let _guard = lock.lock().await;
            if let Some(Ok(doc)) = cache.get_fresh(url) {
                return doc;
            }
            fetch_count.fetch_add(1, Ordering::SeqCst);
            // Simulate fetch latency so both callers are genuinely
            // in-flight together rather than serializing by accident.
            tokio::time::sleep(Duration::from_millis(30)).await;
            let doc = ClientMetadataDocument {
                client_id: url.to_string(),
                client_name: "Example".to_string(),
                redirect_uris: vec!["http://127.0.0.1:3000/callback".to_string()],
            };
            cache.insert(url.to_string(), &Ok(doc.clone()), CACHE_TTL);
            doc
        }

        let cache = DocumentCache::new();
        let url = "https://app.example.com/client.json";
        let fetch_count = AtomicUsize::new(0);
        let (a, b) = tokio::join!(
            simulate_fetch(&cache, url, &fetch_count),
            simulate_fetch(&cache, url, &fetch_count),
        );
        assert_eq!(a.client_id, url);
        assert_eq!(b.client_id, url);
        assert_eq!(
            fetch_count.load(Ordering::SeqCst),
            1,
            "two concurrent misses for the same URL must coalesce to exactly one fetch"
        );
    }

    #[test]
    fn build_locks_retain_keeps_only_locks_with_an_active_holder() {
        // `lock_for`'s capacity sweep relies on this exact predicate: a
        // lock still referenced by an in-flight fetch (an extra Arc clone
        // beyond the map's own) must survive eviction, while one nobody is
        // using (strong_count == 1, held only by the map) must not.
        let cache = DocumentCache::new();
        let idle = Arc::new(Mutex::new(()));
        let busy = Arc::new(Mutex::new(()));
        let _busy_holder = busy.clone(); // simulates an in-flight fetch's lock clone
        cache.build_locks.insert("idle.example".to_string(), idle);
        cache.build_locks.insert("busy.example".to_string(), busy);
        cache
            .build_locks
            .retain(|_, lock| Arc::strong_count(lock) > 1);
        assert!(cache.build_locks.get("idle.example").is_none());
        assert!(cache.build_locks.get("busy.example").is_some());
    }
}
