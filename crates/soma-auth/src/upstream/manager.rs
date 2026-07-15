//! Upstream OAuth lifecycle manager.
//!
//! `UpstreamOauthManager` orchestrates the full outbound `authorization_code` + PKCE
//! flow for one configured upstream MCP server.  It is per-upstream (constructed once
//! per `UpstreamConfig` that has an `oauth` block) and is `Clone` / `Send + Sync`.
//!
//! ## Subject
//!
//! All public methods take a `subject: &str` identifying the user initiating the
//! flow.  Credentials are stored and refreshed independently per `(upstream, subject)`.
//!
//! ## Two-phase authorization
//!
//! ```text
//! begin_authorization(subject)
//!   ↓  generates PKCE + CSRF, stores state in SQLite, returns redirect URL
//! browser → AS → callback
//!   ↓
//! complete_authorization_callback(subject, code, csrf)
//!   ↓  exchanges code, stores encrypted tokens in SQLite
//! build_auth_client(subject)
//!   ↓  loads stored credentials, proactively refreshes if stale
//! AuthClient<reqwest::Client>  → used by the consumer's connection pool for MCP calls
//! ```
//!
//! ## AS metadata caching
//!
//! Authorization server metadata is fetched once per upstream (not per-subject) and
//! cached to avoid an HTTP round-trip on every `build_auth_client` call.

mod client;

use std::sync::Arc;

use rmcp::transport::AuthorizationManager;
use rmcp::transport::auth::AuthorizationMetadata;
use rmcp_client as rmcp;
use serde::Deserialize;
use tokio::sync::RwLock;
use tracing::info;

use crate::sqlite::SqliteStore;
use crate::types::UpstreamOauthCredentialRow;
use crate::upstream::config::UpstreamConfig;
use crate::upstream::encryption::EncryptionKey;
use crate::upstream::refresh::{RefreshFailureCache, RefreshLocks};
use crate::upstream::store::{SqliteCredentialStore, SqliteStateStore};
use crate::upstream::types::{BeginAuthorization, OauthError};

const TOKEN_EXPIRY_WARNING_SECS: i64 = 300;
const PROACTIVE_REFRESH_WINDOW_SECS: i64 = 30;

/// Upstream OAuth manager for a single upstream MCP server.
///
/// Cheap to clone — all mutable state is behind `Arc`.
#[derive(Clone)]
pub struct UpstreamOauthManager {
    sqlite: SqliteStore,
    key: EncryptionKey,
    upstream: UpstreamConfig,
    redirect_uri: Arc<String>,
    locks: Arc<RefreshLocks>,
    /// Tracks recent refresh failures so a known-dead credential fails fast
    /// instead of hitting the authorization server on every request.
    refresh_failures: Arc<RefreshFailureCache>,
    /// Cached AS metadata (fetched once per upstream, shared across subjects).
    metadata_cache: Arc<RwLock<Option<AuthorizationMetadata>>>,
}

impl UpstreamOauthManager {
    /// Create a new manager for `upstream`.
    ///
    /// `redirect_uri` is the absolute URL of the OAuth callback endpoint that will
    /// receive the authorization code (e.g.
    /// `https://soma.example/v1/upstream-oauth/{name}/callback`).
    pub fn new(
        sqlite: SqliteStore,
        key: EncryptionKey,
        upstream: UpstreamConfig,
        redirect_uri: String,
    ) -> Self {
        Self {
            sqlite,
            key,
            upstream,
            redirect_uri: Arc::new(redirect_uri),
            locks: Arc::new(RefreshLocks::new()),
            refresh_failures: Arc::new(RefreshFailureCache::new()),
            metadata_cache: Arc::new(RwLock::new(None)),
        }
    }

    /// Return the `UpstreamConfig` this manager was constructed with.
    ///
    /// Used to persist transient (probe-created) managers back into the
    /// consumer's config when authorization completes for the first time.
    pub fn upstream_config(&self) -> &UpstreamConfig {
        &self.upstream
    }

    /// Return `true` if persisted credentials exist for `subject`.
    ///
    /// Does not check whether the credentials are still valid.
    #[allow(dead_code)]
    pub async fn has_credentials(&self, subject: &str) -> Result<bool, OauthError> {
        self.sqlite
            .find_upstream_oauth_credentials(&self.upstream.name, subject)
            .await
            .map(|opt| opt.is_some())
            .map_err(|e| OauthError::Internal(e.to_string()))
    }

    /// Begin the authorization flow.
    ///
    /// Discovers (or uses cached) AS metadata, registers or configures the OAuth
    /// client, generates a PKCE challenge, saves the pending state to SQLite, and
    /// returns the authorization URL to redirect the operator's browser to.
    ///
    /// Enforces S256 PKCE — returns `OauthError::UnsupportedMethod` if the AS does
    /// not advertise S256 in `code_challenge_methods_supported`.
    pub async fn begin_authorization(
        &self,
        subject: &str,
    ) -> Result<BeginAuthorization, OauthError> {
        let started = std::time::Instant::now();
        let oauth_cfg = self.oauth_config()?;
        let upstream_url = self.upstream_url()?;

        // rmcp's AuthorizationManager builds its own internal reqwest client.
        // See google.rs::GoogleProvider::new for why this call is needed
        // under "rustls-no-provider" -- idempotent, safe to ignore Err.
        drop(rustls::crypto::ring::default_provider().install_default());
        let mut manager = AuthorizationManager::new(upstream_url.as_str())
            .await
            .map_err(|e| {
                tracing::warn!(
                    upstream = %self.upstream.name,
                    subject,
                    kind = "internal_error",
                    error = %e,
                    "upstream oauth: failed to create authorization manager"
                );
                OauthError::Internal(format!("create auth manager: {e}"))
            })?;

        let cred_store = SqliteCredentialStore::new(
            self.sqlite.clone(),
            self.key.clone(),
            &self.upstream.name,
            subject,
        );
        let state_store = SqliteStateStore::new(self.sqlite.clone(), &self.upstream.name, subject);
        manager.set_credential_store(cred_store);
        manager.set_state_store(state_store);

        let metadata = self
            .get_or_discover_metadata(&mut manager)
            .await
            .map_err(|e| {
                tracing::warn!(
                    upstream = %self.upstream.name,
                    subject,
                    kind = e.kind(),
                    error = %e,
                    "upstream oauth: AS metadata discovery failed"
                );
                e
            })?;

        info!(
            upstream = %self.upstream.name,
            subject,
            issuer = metadata.issuer.as_deref().unwrap_or("<none>"),
            "upstream oauth: AS metadata ready"
        );

        self.verify_s256(&metadata.code_challenge_methods_supported)
            .inspect_err(|e| {
                tracing::warn!(
                    upstream = %self.upstream.name,
                    subject,
                    kind = e.kind(),
                    "upstream oauth: S256 PKCE verification failed"
                );
            })?;
        manager.set_metadata(metadata);

        let scopes: Vec<&str> = oauth_cfg
            .scopes
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(String::as_str)
            .collect();

        let client_cfg = self
            .resolve_client_config(
                &mut manager,
                subject,
                &scopes,
                DynamicClientRegistrationUse::BeginAuthorization,
            )
            .await
            .map_err(|e| {
                tracing::warn!(
                    upstream = %self.upstream.name,
                    subject,
                    kind = e.kind(),
                    error = %e,
                    "upstream oauth: client config resolution failed"
                );
                e
            })?;

        manager.configure_client(client_cfg).map_err(|e| {
            tracing::warn!(
                upstream = %self.upstream.name,
                subject,
                kind = "internal_error",
                error = %e,
                "upstream oauth: client configuration failed"
            );
            OauthError::Internal(format!("configure client: {e}"))
        })?;

        let authorization_url = manager.get_authorization_url(&scopes).await.map_err(|e| {
            tracing::warn!(
                upstream = %self.upstream.name,
                subject,
                kind = "internal_error",
                error = %e,
                "upstream oauth: authorization URL generation failed"
            );
            OauthError::Internal(format!("get authorization url: {e}"))
        })?;
        let authorization_url = google_offline_access_url(&authorization_url)?;

        let _csrf = extract_state_param(&authorization_url).ok_or_else(|| {
            tracing::warn!(
                upstream = %self.upstream.name,
                subject,
                kind = "internal_error",
                "upstream oauth: authorization URL missing state parameter"
            );
            OauthError::Internal("authorization url missing required state parameter".to_string())
        })?;

        info!(
            upstream = %self.upstream.name,
            subject,
            elapsed_ms = started.elapsed().as_millis(),
            "upstream oauth: authorization started"
        );

        Ok(BeginAuthorization { authorization_url })
    }

    /// Complete the authorization callback.
    ///
    /// Exchanges the authorization code for tokens and persists the encrypted
    /// credentials. Completion is reconstructed from persisted PKCE state rather
    /// than an in-memory pending map, so callbacks remain valid across restarts.
    pub async fn complete_authorization_callback(
        &self,
        subject: &str,
        code: &str,
        csrf_token: &str,
    ) -> Result<(), OauthError> {
        let started = std::time::Instant::now();

        let auth_manager = self
            .configured_authorization_manager(
                subject,
                DynamicClientRegistrationUse::CompleteAuthorization,
            )
            .await
            .map_err(|e| {
                tracing::warn!(
                    upstream = %self.upstream.name,
                    subject,
                    kind = e.kind(),
                    error = %e,
                    "upstream oauth: failed to build configured authorization manager for token exchange"
                );
                e
            })?;

        auth_manager
            .exchange_code_for_token(code, csrf_token)
            .await
            .map_err(|e| {
                let mapped = map_auth_error(e);
                tracing::warn!(
                    upstream = %self.upstream.name,
                    subject,
                    kind = mapped.kind(),
                    elapsed_ms = started.elapsed().as_millis(),
                    "upstream oauth: token exchange failed"
                );
                mapped
            })?;

        info!(
            upstream = %self.upstream.name,
            subject,
            elapsed_ms = started.elapsed().as_millis(),
            "upstream oauth: authorization completed, tokens stored"
        );

        // A fresh grant supersedes whatever was failing before -- don't make
        // the caller wait out the circuit-breaker cooldown after fixing it.
        self.refresh_failures.clear(&self.upstream.name, subject);

        Ok(())
    }

    /// Delete all stored credentials for `subject` and evict any cached state.
    pub async fn clear_credentials(&self, subject: &str) -> Result<(), OauthError> {
        self.refresh_failures.clear(&self.upstream.name, subject);
        self.sqlite
            .delete_upstream_oauth_credentials(&self.upstream.name, subject)
            .await
            .map_err(|e| {
                tracing::warn!(
                    upstream = %self.upstream.name,
                    subject,
                    kind = "internal_error",
                    error = %e,
                    "upstream oauth: failed to delete credentials from store"
                );
                OauthError::Internal(e.to_string())
            })?;

        self.sqlite
            .delete_dynamic_client_registration(&self.upstream.name, subject)
            .await
            .map_err(|e| {
                tracing::warn!(
                    upstream = %self.upstream.name,
                    subject,
                    kind = "internal_error",
                    error = %e,
                    "upstream oauth: failed to delete dynamic client registration"
                );
                OauthError::Internal(e.to_string())
            })?;

        info!(
            upstream = %self.upstream.name,
            subject,
            "upstream oauth: credentials and dynamic registration cleared"
        );

        Ok(())
    }

    pub async fn credential_row(
        &self,
        subject: &str,
    ) -> Result<Option<UpstreamOauthCredentialRow>, OauthError> {
        self.sqlite
            .find_upstream_oauth_credentials(&self.upstream.name, subject)
            .await
            .map_err(|e| OauthError::Internal(e.to_string()))
    }

    #[allow(dead_code)]
    pub async fn subject_for_state(&self, csrf_token: &str) -> Result<Option<String>, OauthError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| OauthError::Internal(format!("system clock error: {error}")))?
            .as_secs() as i64;
        self.sqlite
            .find_upstream_oauth_state_subject(&self.upstream.name, csrf_token, now)
            .await
            .map_err(|e| OauthError::Internal(e.to_string()))
    }

    /// Look up the stored dynamic `client_id` for `subject`, if any.
    ///
    /// Returns `None` when the upstream is not `Dynamic` or when no registration
    /// has been persisted yet. Used by `OauthClientCache` to include the
    /// per-subject `client_id` in the fingerprint so a re-registration is
    /// detected and the stale `AuthClient` is evicted.
    pub async fn stored_dynamic_client_id(
        &self,
        subject: &str,
    ) -> Result<Option<String>, OauthError> {
        self.sqlite
            .find_dynamic_client_registration(&self.upstream.name, subject)
            .await
            .map_err(|e| OauthError::Internal(e.to_string()))
    }

    // ---- private helpers ----

    async fn configured_authorization_manager(
        &self,
        subject: &str,
        dynamic_registration_use: DynamicClientRegistrationUse,
    ) -> Result<AuthorizationManager, OauthError> {
        let upstream_url = self.upstream_url()?;
        let oauth_cfg = self.oauth_config()?;

        // See begin_authorization above for why this call is needed under
        // "rustls-no-provider" -- idempotent, safe to ignore Err.
        drop(rustls::crypto::ring::default_provider().install_default());
        let mut manager = AuthorizationManager::new(upstream_url.as_str())
            .await
            .map_err(|e| OauthError::Internal(format!("create auth manager: {e}")))?;

        let cred_store = SqliteCredentialStore::new(
            self.sqlite.clone(),
            self.key.clone(),
            &self.upstream.name,
            subject,
        );
        let state_store = SqliteStateStore::new(self.sqlite.clone(), &self.upstream.name, subject);
        manager.set_credential_store(cred_store);
        manager.set_state_store(state_store);

        let metadata = self.get_or_discover_metadata(&mut manager).await?;
        self.verify_s256(&metadata.code_challenge_methods_supported)?;
        manager.set_metadata(metadata);

        let scopes: Vec<&str> = oauth_cfg
            .scopes
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(String::as_str)
            .collect();
        let client_cfg = self
            .resolve_client_config(&mut manager, subject, &scopes, dynamic_registration_use)
            .await?;
        manager
            .configure_client(client_cfg)
            .map_err(|e| OauthError::Internal(format!("configure client: {e}")))?;
        Ok(manager)
    }

    fn oauth_config(&self) -> Result<&crate::upstream::config::UpstreamOauthConfig, OauthError> {
        self.upstream
            .oauth
            .as_ref()
            .ok_or_else(|| OauthError::Internal("upstream has no oauth config".to_string()))
    }

    fn oauth_scope_label(&self) -> String {
        self.upstream
            .oauth
            .as_ref()
            .and_then(|cfg| cfg.scopes.as_ref())
            .filter(|scopes| !scopes.is_empty())
            .map(|scopes| scopes.join(" "))
            .unwrap_or_else(|| "<none>".to_string())
    }

    fn oauth_provider_label(&self) -> String {
        self.upstream.name.clone()
    }

    fn log_expiring_token(&self, subject: &str, state: &TokenRefreshState, elapsed_ms: u128) {
        if state.seconds_until_expiry <= TOKEN_EXPIRY_WARNING_SECS {
            tracing::warn!(
                upstream = %self.upstream.name,
                provider = %self.oauth_provider_label(),
                subject,
                scope = %self.oauth_scope_label(),
                seconds_until_expiry = state.seconds_until_expiry,
                refresh_token_present = state.refresh_token_present,
                elapsed_ms,
                "upstream oauth: access token nearing expiry"
            );
        }
    }

    fn log_refresh_attempt(&self, subject: &str, state: &TokenRefreshState, elapsed_ms: u128) {
        if !state.refresh_due() {
            return;
        }

        if state.refresh_token_present {
            tracing::info!(
                upstream = %self.upstream.name,
                provider = %self.oauth_provider_label(),
                subject,
                scope = %self.oauth_scope_label(),
                seconds_until_expiry = state.seconds_until_expiry,
                elapsed_ms,
                "upstream oauth: token refresh attempt"
            );
        } else {
            tracing::warn!(
                upstream = %self.upstream.name,
                provider = %self.oauth_provider_label(),
                subject,
                scope = %self.oauth_scope_label(),
                seconds_until_expiry = state.seconds_until_expiry,
                kind = "oauth_needs_reauth",
                elapsed_ms,
                fallback = "reauthorization_required",
                "upstream oauth: access token expired or near expiry without refresh token"
            );
        }
    }

    fn upstream_url(&self) -> Result<Arc<String>, OauthError> {
        let canonical = self
            .upstream
            .canonical_url()
            .ok_or_else(|| OauthError::Internal("upstream has no url".to_string()))?
            .map_err(|e| OauthError::Internal(format!("invalid upstream url: {e}")))?;
        Ok(Arc::new(canonical))
    }

    /// Fetch AS metadata, caching the result for subsequent calls.
    ///
    /// Enforces issuer binding per RFC 8414: `issuer` MUST be present and the
    /// `authorization_endpoint` + `token_endpoint` MUST share its origin. Rejects
    /// silent issuer drift between the first and subsequent discovery calls.
    ///
    /// Uses a single write-lock acquisition to avoid a TOCTOU race between a
    /// read-lock check and a subsequent write-lock cache update.
    async fn get_or_discover_metadata(
        &self,
        manager: &mut AuthorizationManager,
    ) -> Result<AuthorizationMetadata, OauthError> {
        let mut cache = self.metadata_cache.write().await;
        if let Some(meta) = cache.clone() {
            return Ok(meta);
        }

        let metadata = match manager.discover_metadata().await {
            Ok(metadata) => metadata,
            Err(error) => {
                match discover_metadata_via_protected_resource(self.upstream_url()?.as_str())
                    .await?
                {
                    Some(metadata) => metadata,
                    None => {
                        return Err(OauthError::Internal(format!("discover metadata: {error}")));
                    }
                }
            }
        };

        self.verify_issuer_binding(&metadata)?;

        *cache = Some(metadata.clone());
        Ok(metadata)
    }

    /// RFC 8414 §3 issuer binding: `issuer` must be present, and every
    /// non-jwks endpoint origin (scheme + host + port) must match the
    /// issuer origin. This is stricter than a host-only check: it rejects
    /// endpoints served over a different scheme (e.g. http vs https) or
    /// on a different port, which a host-only comparison would miss.
    fn verify_issuer_binding(&self, metadata: &AuthorizationMetadata) -> Result<(), OauthError> {
        let issuer_raw = metadata.issuer.as_deref().ok_or_else(|| {
            OauthError::IssuerMismatch(format!(
                "AS metadata for upstream '{}' is missing required `issuer` claim",
                self.upstream.name
            ))
        })?;
        // Normalize the issuer: strip trailing slashes for a canonical form.
        let issuer_normalized = issuer_raw.trim_end_matches('/');
        let issuer_origin = url_origin(issuer_normalized).ok_or_else(|| {
            OauthError::IssuerMismatch(format!("issuer `{issuer_raw}` is not a valid URL"))
        })?;
        for (label, endpoint) in [
            (
                "authorization_endpoint",
                Some(metadata.authorization_endpoint.as_str()),
            ),
            ("token_endpoint", Some(metadata.token_endpoint.as_str())),
            (
                "registration_endpoint",
                metadata.registration_endpoint.as_deref(),
            ),
        ] {
            let Some(endpoint) = endpoint else { continue };
            let Some(origin) = url_origin(endpoint) else {
                return Err(OauthError::IssuerMismatch(format!(
                    "{label} `{endpoint}` is not a valid URL"
                )));
            };
            if origin != issuer_origin
                && !is_known_split_endpoint_origin(issuer_origin.as_str(), origin.as_str())
            {
                return Err(OauthError::IssuerMismatch(format!(
                    "{label} origin `{origin}` does not match issuer origin `{issuer_origin}`"
                )));
            }
        }
        Ok(())
    }

    fn verify_s256(&self, methods: &Option<Vec<String>>) -> Result<(), OauthError> {
        match methods {
            Some(methods) if methods.iter().any(|m| m == "S256") => Ok(()),
            Some(methods) => Err(OauthError::UnsupportedMethod(format!(
                "AS does not advertise S256 PKCE; advertised methods: {methods:?}"
            ))),
            None => Err(OauthError::UnsupportedMethod(
                "AS did not advertise code_challenge_methods_supported; S256 is required"
                    .to_string(),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DynamicClientRegistrationUse {
    BeginAuthorization,
    CompleteAuthorization,
    StoredCredentials,
}

#[derive(Debug, Deserialize)]
struct ProtectedResourceMetadata {
    #[serde(default)]
    authorization_server: Option<String>,
    #[serde(default)]
    authorization_servers: Option<Vec<String>>,
}

async fn discover_metadata_via_protected_resource(
    upstream_url: &str,
) -> Result<Option<AuthorizationMetadata>, OauthError> {
    let upstream = url::Url::parse(upstream_url)
        .map_err(|error| OauthError::Internal(format!("invalid upstream url: {error}")))?;
    // See google.rs::GoogleProvider::new for why this call is needed
    // under "rustls-no-provider" -- idempotent, safe to ignore Err.
    drop(rustls::crypto::ring::default_provider().install_default());
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|error| OauthError::Internal(format!("build oauth metadata client: {error}")))?;

    for metadata_url in protected_resource_metadata_candidates(&upstream) {
        let response = match client.get(metadata_url.clone()).send().await {
            Ok(response) => response,
            Err(_) => continue,
        };
        if !response.status().is_success() {
            continue;
        }
        let Ok(resource_metadata) = response.json::<ProtectedResourceMetadata>().await else {
            continue;
        };

        let mut authorization_servers = Vec::new();
        if let Some(server) = resource_metadata.authorization_server {
            authorization_servers.push(server);
        }
        if let Some(servers) = resource_metadata.authorization_servers {
            authorization_servers.extend(servers);
        }

        for authorization_server in authorization_servers {
            let Ok(server_url) =
                resolve_authorization_server_url(&metadata_url, authorization_server.trim())
            else {
                continue;
            };
            for authorization_metadata_url in authorization_metadata_candidates(&server_url) {
                let response = match client.get(authorization_metadata_url).send().await {
                    Ok(response) => response,
                    Err(_) => continue,
                };
                if !response.status().is_success() {
                    continue;
                }
                if let Ok(metadata) = response.json::<AuthorizationMetadata>().await {
                    return Ok(Some(metadata));
                }
            }
        }
    }

    Ok(None)
}

fn protected_resource_metadata_candidates(upstream: &url::Url) -> Vec<url::Url> {
    let trimmed = upstream
        .path()
        .trim_start_matches('/')
        .trim_end_matches('/');
    let paths = if trimmed.is_empty() {
        vec!["/.well-known/oauth-protected-resource".to_string()]
    } else {
        vec![
            format!("/.well-known/oauth-protected-resource/{trimmed}"),
            format!("/{trimmed}/.well-known/oauth-protected-resource"),
            "/.well-known/oauth-protected-resource".to_string(),
        ]
    };

    paths
        .into_iter()
        .map(|path| {
            let mut candidate = upstream.clone();
            candidate.set_query(None);
            candidate.set_fragment(None);
            candidate.set_path(&path);
            candidate
        })
        .collect()
}

fn authorization_metadata_candidates(server: &url::Url) -> Vec<url::Url> {
    if server.path().contains("/.well-known/") {
        return vec![server.clone()];
    }

    [
        "/.well-known/oauth-authorization-server",
        "/.well-known/openid-configuration",
    ]
    .into_iter()
    .map(|path| {
        let mut candidate = server.clone();
        candidate.set_query(None);
        candidate.set_fragment(None);
        candidate.set_path(path);
        candidate
    })
    .collect()
}

fn resolve_authorization_server_url(
    metadata_url: &url::Url,
    authorization_server: &str,
) -> Result<url::Url, url::ParseError> {
    url::Url::parse(authorization_server).or_else(|_| metadata_url.join(authorization_server))
}

/// Return the normalized origin (scheme + "://" + lowercased host + optional explicit port)
/// of a URL, or `None` if the URL is invalid or has no host.
///
/// This is stricter than a host-only comparison: it rejects URLs that share a host
/// but differ in scheme or port (e.g. http vs https, or :80 vs :8080).
fn url_origin(s: &str) -> Option<String> {
    let u = url::Url::parse(s).ok()?;
    let host = u.host_str()?.to_ascii_lowercase();
    let scheme = u.scheme();
    match u.port() {
        Some(port) => Some(format!("{scheme}://{host}:{port}")),
        None => Some(format!("{scheme}://{host}")),
    }
}

fn is_known_split_endpoint_origin(issuer_origin: &str, endpoint_origin: &str) -> bool {
    issuer_origin == "https://accounts.google.com"
        && endpoint_origin == "https://oauth2.googleapis.com"
}

fn extract_state_param(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    parsed
        .query_pairs()
        .find(|(k, _)| k == "state")
        .map(|(_, v)| v.into_owned())
}

fn google_offline_access_url(url: &str) -> Result<String, OauthError> {
    let mut parsed = url::Url::parse(url).map_err(|error| {
        OauthError::Internal(format!("authorization url generated invalid URL: {error}"))
    })?;
    let is_google_authorize = parsed
        .host_str()
        .is_some_and(|host| host.eq_ignore_ascii_case("accounts.google.com"));
    if !is_google_authorize {
        return Ok(url.to_string());
    }

    let existing: std::collections::HashSet<String> = parsed
        .query_pairs()
        .map(|(key, _)| key.into_owned())
        .collect();
    {
        let mut query = parsed.query_pairs_mut();
        if !existing.contains("access_type") {
            query.append_pair("access_type", "offline");
        }
        if !existing.contains("prompt") {
            query.append_pair("prompt", "consent");
        }
        if !existing.contains("include_granted_scopes") {
            query.append_pair("include_granted_scopes", "true");
        }
    }
    Ok(parsed.into())
}

struct TokenRefreshState {
    seconds_until_expiry: i64,
    refresh_token_present: bool,
}

impl TokenRefreshState {
    fn from_row(row: &UpstreamOauthCredentialRow, now: i64) -> Option<Self> {
        if row.access_token_expires_at <= 0 {
            return None;
        }
        Some(Self {
            seconds_until_expiry: row.access_token_expires_at.saturating_sub(now),
            refresh_token_present: row.refresh_token_present,
        })
    }

    fn refresh_due(&self) -> bool {
        self.seconds_until_expiry <= PROACTIVE_REFRESH_WINDOW_SECS
    }
}

fn now_unix() -> Result<i64, OauthError> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| OauthError::Internal(format!("system clock error: {error}")))
        .map(|duration| duration.as_secs() as i64)
}

fn map_auth_error(e: rmcp::transport::AuthError) -> OauthError {
    match e {
        rmcp::transport::AuthError::AuthorizationRequired => {
            OauthError::NeedsReauth("authorization required".to_string())
        }
        rmcp::transport::AuthError::TokenExchangeFailed(msg) => OauthError::Internal(msg),
        rmcp::transport::AuthError::TokenRefreshFailed(msg) => {
            OauthError::NeedsReauth(format!("token refresh failed: {msg}"))
        }
        other => OauthError::Internal(other.to_string()),
    }
}

#[cfg(test)]
mod url_tests {
    use super::google_offline_access_url;

    #[test]
    fn google_authorization_url_requests_offline_consent() {
        let url = "https://accounts.google.com/o/oauth2/v2/auth?response_type=code&state=abc";
        let updated = google_offline_access_url(url).expect("url");
        let parsed = url::Url::parse(&updated).expect("updated url parses");
        let params: std::collections::HashMap<_, _> = parsed.query_pairs().collect();

        assert_eq!(
            params.get("access_type").map(|v| v.as_ref()),
            Some("offline")
        );
        assert_eq!(params.get("prompt").map(|v| v.as_ref()), Some("consent"));
        assert_eq!(
            params.get("include_granted_scopes").map(|v| v.as_ref()),
            Some("true")
        );
        assert_eq!(params.get("state").map(|v| v.as_ref()), Some("abc"));
    }

    #[test]
    fn non_google_authorization_url_is_unchanged() {
        let url = "https://auth.example.test/authorize?response_type=code&state=abc";
        let updated = google_offline_access_url(url).expect("url");
        assert_eq!(updated, url);
    }

    #[test]
    fn existing_google_authorization_params_are_preserved() {
        let url = "https://accounts.google.com/o/oauth2/v2/auth?access_type=online&prompt=select_account&include_granted_scopes=false";
        let updated = google_offline_access_url(url).expect("url");
        let parsed = url::Url::parse(&updated).expect("updated url parses");
        let params: std::collections::HashMap<_, _> = parsed.query_pairs().collect();

        assert_eq!(
            params.get("access_type").map(|v| v.as_ref()),
            Some("online")
        );
        assert_eq!(
            params.get("prompt").map(|v| v.as_ref()),
            Some("select_account")
        );
        assert_eq!(
            params.get("include_granted_scopes").map(|v| v.as_ref()),
            Some("false")
        );
    }
}
