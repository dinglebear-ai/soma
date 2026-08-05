//! `AuthClient` construction, forced refresh, and OAuth client-config
//! resolution for [`super::UpstreamOauthManager`]. Split out of
//! `manager.rs` to keep that module under the repo's file-size contract —
//! this is a second `impl` block for the same type, not a separate
//! abstraction; being a child module of `manager`, it sees `manager`'s
//! private fields and helper functions the same as if this were still one
//! file.

use rmcp::transport::auth::OAuthClientConfig;
use rmcp::transport::streamable_http_client::StreamableHttpClient;
use rmcp::transport::{AuthClient, AuthorizationManager};
use rmcp_client as rmcp;

use crate::upstream::config::UpstreamOauthRegistration;
use crate::upstream::types::OauthError;

use super::{DynamicClientRegistrationUse, TokenRefreshState, UpstreamOauthManager, now_unix};

fn should_use_client_metadata_document(
    registration: &UpstreamOauthRegistration,
    preference: Option<bool>,
    server_supports_cimd: bool,
) -> bool {
    server_supports_cimd
        && (matches!(registration, UpstreamOauthRegistration::Auto) || preference == Some(true))
}

fn dynamic_registration_bootstrap_config(redirect_uri: &str) -> OAuthClientConfig {
    OAuthClientConfig::new("oauth-registration-placeholder", redirect_uri)
        .with_application_type("web")
}

fn generated_client_metadata_document_url(
    redirect_uri: &str,
    upstream_name: &str,
) -> Result<Option<String>, OauthError> {
    let mut client_id = url::Url::parse(redirect_uri).map_err(|error| {
        OauthError::Internal(format!("invalid upstream OAuth redirect URI: {error}"))
    })?;
    if client_id.scheme() != "https" {
        return Ok(None);
    }
    client_id.set_query(None);
    client_id.set_fragment(None);
    {
        let mut segments = client_id.path_segments_mut().map_err(|_| {
            OauthError::Internal(
                "upstream OAuth redirect URI cannot host a client metadata document".to_string(),
            )
        })?;
        segments
            .clear()
            .push("auth")
            .push("upstream")
            .push("client-metadata")
            .push(upstream_name);
    }
    Ok(Some(client_id.to_string()))
}

impl UpstreamOauthManager {
    /// Return an `AuthClient` ready for use, proactively refreshing if near expiry.
    ///
    /// Creates a fresh `AuthorizationManager` backed by stored credentials.  Uses
    /// cached AS metadata to avoid an extra HTTP round-trip.
    ///
    /// Returns `OauthError::NeedsReauth` when no credentials are stored or the
    /// refresh token has been revoked.
    pub async fn build_auth_client(
        &self,
        subject: &str,
    ) -> Result<AuthClient<reqwest::Client>, OauthError> {
        let started = std::time::Instant::now();
        let lock = self.locks.acquire(&self.upstream.name, subject);
        let _guard = lock.lock().await;

        let mut manager = self
            .configured_authorization_manager(
                subject,
                DynamicClientRegistrationUse::StoredCredentials,
            )
            .await
            .inspect_err(|e| {
                tracing::warn!(
                    upstream = %self.upstream.name,
                    provider = %self.oauth_provider_label(),
                    subject,
                    scope = %self.oauth_scope_label(),
                    kind = e.kind(),
                    elapsed_ms = started.elapsed().as_millis(),
                    fallback = "reauthorization_required",
                    "upstream oauth: failed to build auth client manager"
                );
            })?;
        let initialized = manager.initialize_from_store().await.map_err(|e| {
            tracing::warn!(
                upstream = %self.upstream.name,
                provider = %self.oauth_provider_label(),
                subject,
                scope = %self.oauth_scope_label(),
                kind = "internal_error",
                elapsed_ms = started.elapsed().as_millis(),
                fallback = "reauthorization_required",
                "upstream oauth: failed to initialize auth client from credential store"
            );
            OauthError::Internal(format!("initialize from store: {e}"))
        })?;

        if !initialized {
            tracing::warn!(
                upstream = %self.upstream.name,
                provider = %self.oauth_provider_label(),
                subject,
                scope = %self.oauth_scope_label(),
                kind = "oauth_needs_reauth",
                elapsed_ms = started.elapsed().as_millis(),
                fallback = "reauthorization_required",
                "upstream oauth: no stored credentials for auth client"
            );
            return Err(OauthError::NeedsReauth(format!(
                "no stored credentials for upstream '{}' subject '{subject}'",
                self.upstream.name
            )));
        }

        let credential_row = self.credential_row(subject).await?;
        let refresh_state = credential_row
            .as_ref()
            .and_then(|row| TokenRefreshState::from_row(row, now_unix().ok()?));
        let refresh_due = refresh_state
            .as_ref()
            .is_some_and(TokenRefreshState::refresh_due);
        if let Some(state) = refresh_state.as_ref() {
            self.log_expiring_token(subject, state, started.elapsed().as_millis());
            self.log_refresh_attempt(subject, state, started.elapsed().as_millis());
        }

        if refresh_due
            && self
                .refresh_failures
                .recently_failed(&self.upstream.name, subject)
        {
            tracing::warn!(
                upstream = %self.upstream.name,
                provider = %self.oauth_provider_label(),
                subject,
                scope = %self.oauth_scope_label(),
                kind = "oauth_needs_reauth",
                elapsed_ms = started.elapsed().as_millis(),
                fallback = "reauthorization_required",
                "upstream oauth: token refresh skipped, recently failed"
            );
            return Err(OauthError::NeedsReauth(format!(
                "upstream '{}' subject '{subject}' refresh failed recently; skipping retry until cooldown elapses",
                self.upstream.name
            )));
        }

        manager.get_access_token().await.map_err(|e| {
            let mapped = super::map_auth_error(e);
            if refresh_due {
                self.refresh_failures
                    .record_failure(&self.upstream.name, subject);
                tracing::warn!(
                    upstream = %self.upstream.name,
                    provider = %self.oauth_provider_label(),
                    subject,
                    scope = %self.oauth_scope_label(),
                    kind = mapped.kind(),
                    elapsed_ms = started.elapsed().as_millis(),
                    fallback = "reauthorization_required",
                    "upstream oauth: token refresh failed"
                );
            }
            mapped
        })?;

        self.refresh_failures.clear(&self.upstream.name, subject);
        if refresh_due {
            tracing::info!(
                upstream = %self.upstream.name,
                provider = %self.oauth_provider_label(),
                subject,
                scope = %self.oauth_scope_label(),
                elapsed_ms = started.elapsed().as_millis(),
                fallback = "none",
                "upstream oauth: token refresh succeeded"
            );
        }

        // See google.rs::GoogleProvider::new for why this call is needed
        // under "rustls-no-provider" -- idempotent, safe to ignore Err.
        drop(rustls::crypto::ring::default_provider().install_default());
        Ok(AuthClient::new(reqwest::Client::new(), manager))
    }

    /// Build an `AuthClient<C>` wrapping the supplied HTTP client.
    ///
    /// Identical to `build_auth_client` except the caller provides the HTTP
    /// transport, enabling `BodyCappedHttpClient` or any other
    /// `StreamableHttpClient` to be used on the OAuth path.  The resulting
    /// client is NOT cached — callers that need caching must do so themselves.
    pub async fn build_auth_client_with<C>(
        &self,
        subject: &str,
        http_client: C,
    ) -> Result<AuthClient<C>, OauthError>
    where
        C: StreamableHttpClient,
    {
        let started = std::time::Instant::now();
        let lock = self.locks.acquire(&self.upstream.name, subject);
        let _guard = lock.lock().await;

        let mut manager = self
            .configured_authorization_manager(
                subject,
                DynamicClientRegistrationUse::StoredCredentials,
            )
            .await
            .inspect_err(|e| {
                tracing::warn!(
                    upstream = %self.upstream.name,
                    provider = %self.oauth_provider_label(),
                    subject,
                    scope = %self.oauth_scope_label(),
                    kind = e.kind(),
                    elapsed_ms = started.elapsed().as_millis(),
                    fallback = "reauthorization_required",
                    "upstream oauth: failed to build auth client manager (with_client)"
                );
            })?;
        let initialized = manager.initialize_from_store().await.map_err(|e| {
            tracing::warn!(
                upstream = %self.upstream.name,
                provider = %self.oauth_provider_label(),
                subject,
                scope = %self.oauth_scope_label(),
                kind = "internal_error",
                elapsed_ms = started.elapsed().as_millis(),
                fallback = "reauthorization_required",
                "upstream oauth: failed to initialize auth client from credential store (with_client)"
            );
            OauthError::Internal(format!("initialize from store: {e}"))
        })?;

        if !initialized {
            tracing::warn!(
                upstream = %self.upstream.name,
                provider = %self.oauth_provider_label(),
                subject,
                scope = %self.oauth_scope_label(),
                kind = "oauth_needs_reauth",
                elapsed_ms = started.elapsed().as_millis(),
                fallback = "reauthorization_required",
                "upstream oauth: no stored credentials for auth client (with_client)"
            );
            return Err(OauthError::NeedsReauth(format!(
                "no stored credentials for upstream '{}' subject '{subject}'",
                self.upstream.name
            )));
        }

        let credential_row = self.credential_row(subject).await?;
        let refresh_state = credential_row
            .as_ref()
            .and_then(|row| TokenRefreshState::from_row(row, now_unix().ok()?));
        let refresh_due = refresh_state
            .as_ref()
            .is_some_and(TokenRefreshState::refresh_due);
        if let Some(state) = refresh_state.as_ref() {
            self.log_expiring_token(subject, state, started.elapsed().as_millis());
            self.log_refresh_attempt(subject, state, started.elapsed().as_millis());
        }

        if refresh_due
            && self
                .refresh_failures
                .recently_failed(&self.upstream.name, subject)
        {
            tracing::warn!(
                upstream = %self.upstream.name,
                provider = %self.oauth_provider_label(),
                subject,
                scope = %self.oauth_scope_label(),
                kind = "oauth_needs_reauth",
                elapsed_ms = started.elapsed().as_millis(),
                fallback = "reauthorization_required",
                "upstream oauth: token refresh skipped, recently failed (with_client)"
            );
            return Err(OauthError::NeedsReauth(format!(
                "upstream '{}' subject '{subject}' refresh failed recently; skipping retry until cooldown elapses",
                self.upstream.name
            )));
        }

        manager.get_access_token().await.map_err(|e| {
            let mapped = super::map_auth_error(e);
            if refresh_due {
                self.refresh_failures
                    .record_failure(&self.upstream.name, subject);
                tracing::warn!(
                    upstream = %self.upstream.name,
                    provider = %self.oauth_provider_label(),
                    subject,
                    scope = %self.oauth_scope_label(),
                    kind = mapped.kind(),
                    elapsed_ms = started.elapsed().as_millis(),
                    fallback = "reauthorization_required",
                    "upstream oauth: token refresh failed (with_client)"
                );
            }
            mapped
        })?;

        self.refresh_failures.clear(&self.upstream.name, subject);
        if refresh_due {
            tracing::info!(
                upstream = %self.upstream.name,
                provider = %self.oauth_provider_label(),
                subject,
                scope = %self.oauth_scope_label(),
                elapsed_ms = started.elapsed().as_millis(),
                fallback = "none",
                "upstream oauth: token refresh succeeded (with_client)"
            );
        }

        Ok(AuthClient::new(http_client, manager))
    }

    /// Force a refresh for stored credentials.
    ///
    /// `AuthorizationManager::get_access_token()` only refreshes inside rmcp's
    /// short refresh buffer. Status checks need an explicit refresh so UI state
    /// cannot report a stale credential row as connected.
    pub async fn refresh_auth_client(&self, subject: &str) -> Result<(), OauthError> {
        let started = std::time::Instant::now();
        let lock = self.locks.acquire(&self.upstream.name, subject);
        let _guard = lock.lock().await;

        let mut manager = self
            .configured_authorization_manager(
                subject,
                DynamicClientRegistrationUse::StoredCredentials,
            )
            .await
            .inspect_err(|e| {
                tracing::warn!(
                    upstream = %self.upstream.name,
                    provider = %self.oauth_provider_label(),
                    subject,
                    scope = %self.oauth_scope_label(),
                    kind = e.kind(),
                    elapsed_ms = started.elapsed().as_millis(),
                    fallback = "reauthorization_required",
                    "upstream oauth: failed to build refresh manager"
                );
            })?;
        let initialized = manager.initialize_from_store().await.map_err(|e| {
            tracing::warn!(
                upstream = %self.upstream.name,
                provider = %self.oauth_provider_label(),
                subject,
                scope = %self.oauth_scope_label(),
                kind = "internal_error",
                elapsed_ms = started.elapsed().as_millis(),
                fallback = "reauthorization_required",
                "upstream oauth: failed to initialize refresh manager from credential store"
            );
            OauthError::Internal(format!("initialize from store: {e}"))
        })?;

        if !initialized {
            return Err(OauthError::NeedsReauth(format!(
                "no stored credentials for upstream '{}' subject '{subject}'",
                self.upstream.name
            )));
        }

        // rmcp's `initialize_from_store()` reconfigures the OAuth client via
        // `configure_client_id()`, which hardcodes `client_secret: None` and so
        // discards the secret that `configured_authorization_manager` just set.
        // Confidential upstreams (e.g. Google, which requires `client_secret` on
        // the refresh_token grant) then fail refresh with "client_secret is
        // missing". Re-apply the resolved client config — including the secret —
        // now that stored credentials and metadata are loaded. `refresh_token()`
        // reads the refresh token from the credential store, not the client
        // config, so re-configuring the client does not disturb it. For public
        // clients `resolve_client_config` yields no secret, so this is a no-op.
        let scopes_owned = self.oauth_config()?.scopes.clone().unwrap_or_default();
        let scopes: Vec<&str> = scopes_owned.iter().map(String::as_str).collect();
        let issuer = self
            .metadata_cache
            .read()
            .await
            .as_ref()
            .and_then(|metadata| metadata.issuer.as_deref())
            .map(|issuer| issuer.trim_end_matches('/').to_string())
            .ok_or_else(|| {
                OauthError::IssuerMismatch("authorization server issuer is unavailable".to_string())
            })?;
        let client_cfg = self
            .resolve_client_config(
                &mut manager,
                subject,
                &scopes,
                &issuer,
                DynamicClientRegistrationUse::StoredCredentials,
            )
            .await?;
        manager.configure_client(client_cfg).map_err(|e| {
            OauthError::Internal(format!(
                "re-configure client with credentials after store init: {e}"
            ))
        })?;

        manager
            .refresh_token()
            .await
            .map_err(super::map_auth_error)?;
        tracing::info!(
            upstream = %self.upstream.name,
            provider = %self.oauth_provider_label(),
            subject,
            scope = %self.oauth_scope_label(),
            elapsed_ms = started.elapsed().as_millis(),
            "upstream oauth: status refresh succeeded"
        );
        Ok(())
    }

    pub(super) async fn resolve_client_config(
        &self,
        manager: &mut AuthorizationManager,
        subject: &str,
        scopes: &[&str],
        issuer: &str,
        dynamic_registration_use: DynamicClientRegistrationUse,
    ) -> Result<OAuthClientConfig, OauthError> {
        let oauth_cfg = self.oauth_config()?;
        match &oauth_cfg.registration {
            UpstreamOauthRegistration::Preregistered {
                client_id,
                client_secret_env,
            } => {
                let secret = match client_secret_env.as_deref() {
                    None => None,
                    Some(var) => {
                        let val = std::env::var(var).unwrap_or_default();
                        if val.is_empty() {
                            return Err(OauthError::Internal(format!(
                                "client_secret_env '{var}' is configured but env var '{var}' is not set or is empty"
                            )));
                        }
                        Some(val)
                    }
                };

                let mut cfg = OAuthClientConfig::new(client_id.clone(), self.redirect_uri.as_str());
                if let Some(s) = secret {
                    cfg = cfg.with_client_secret(s);
                }
                cfg = cfg.with_scopes(scopes.iter().map(|s| s.to_string()).collect());
                Ok(cfg)
            }
            UpstreamOauthRegistration::Auto | UpstreamOauthRegistration::Dynamic => {
                let (supports_cimd, supports_dcr) = {
                    let metadata = self.metadata_cache.read().await;
                    let metadata = metadata.as_ref();
                    (
                        metadata
                            .and_then(|metadata| {
                                metadata
                                    .additional_fields
                                    .get("client_id_metadata_document_supported")
                            })
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false),
                        metadata
                            .and_then(|metadata| metadata.registration_endpoint.as_deref())
                            .is_some(),
                    )
                };
                if should_use_client_metadata_document(
                    &oauth_cfg.registration,
                    oauth_cfg.prefer_client_metadata_document,
                    supports_cimd,
                ) && let Some(client_id) = generated_client_metadata_document_url(
                    self.redirect_uri.as_str(),
                    &self.upstream.name,
                )? {
                    return Ok(
                        OAuthClientConfig::new(client_id, self.redirect_uri.as_str())
                            .with_scopes(scopes.iter().map(|scope| scope.to_string()).collect())
                            .with_application_type("web"),
                    );
                }

                // Dynamic registration (RFC 7591) has two different lifetimes:
                //   1. Stored credentials are durable and remain authoritative after
                //      a successful token exchange for normal MCP calls.
                //   2. The dynamic registration row is only pending state between
                //      begin_authorization and callback. It survives process restarts,
                //      but must not be reused to start a new flow because upstream AS
                //      state can be reset independently, leaving a stale client_id behind.

                match dynamic_registration_use {
                    DynamicClientRegistrationUse::StoredCredentials => {
                        if let Some(row) = self
                            .sqlite
                            .find_upstream_oauth_credentials(&self.upstream.name, subject)
                            .await
                            .map_err(|e| OauthError::Internal(e.to_string()))?
                        {
                            if row.issuer.is_empty() || row.issuer != issuer {
                                self.sqlite
                                    .delete_upstream_oauth_credentials(&self.upstream.name, subject)
                                    .await
                                    .map_err(|e| OauthError::Internal(e.to_string()))?;
                                self.sqlite
                                    .delete_dynamic_client_registration(
                                        &self.upstream.name,
                                        subject,
                                    )
                                    .await
                                    .map_err(|e| OauthError::Internal(e.to_string()))?;
                                return Err(OauthError::NeedsReauth(format!(
                                    "authorization server changed for upstream '{}'",
                                    self.upstream.name
                                )));
                            }
                            let mut cfg =
                                OAuthClientConfig::new(row.client_id, self.redirect_uri.as_str());
                            cfg = cfg.with_scopes(scopes.iter().map(|s| s.to_string()).collect());
                            return Ok(cfg);
                        }

                        return Err(OauthError::NeedsReauth(format!(
                            "no stored credentials for upstream '{}' subject '{subject}'",
                            self.upstream.name
                        )));
                    }
                    DynamicClientRegistrationUse::CompleteAuthorization => {
                        // Callback/token exchange path: use the client_id created
                        // by the begin_authorization call. This keeps callbacks
                        // valid across process restarts and lets an explicit
                        // reauth flow replace stale stored credentials.
                        if let Some(registration) = self
                            .sqlite
                            .find_dynamic_client_registration(&self.upstream.name, subject)
                            .await
                            .map_err(|e| OauthError::Internal(e.to_string()))?
                        {
                            if registration.issuer.is_empty() || registration.issuer != issuer {
                                self.sqlite
                                    .delete_dynamic_client_registration(
                                        &self.upstream.name,
                                        subject,
                                    )
                                    .await
                                    .map_err(|e| OauthError::Internal(e.to_string()))?;
                                return Err(OauthError::NeedsReauth(format!(
                                    "authorization server changed for upstream '{}'",
                                    self.upstream.name
                                )));
                            }
                            let mut cfg = OAuthClientConfig::new(
                                registration.client_id,
                                self.redirect_uri.as_str(),
                            );
                            cfg = cfg.with_scopes(scopes.iter().map(|s| s.to_string()).collect());
                            return Ok(cfg);
                        }

                        return Err(OauthError::NeedsReauth(format!(
                            "no dynamic client registration for upstream '{}' subject '{subject}'",
                            self.upstream.name
                        )));
                    }
                    DynamicClientRegistrationUse::BeginAuthorization => {}
                }

                if !supports_dcr {
                    return Err(OauthError::UnsupportedMethod(
                        "authorization server supports neither a usable Client ID Metadata Document nor Dynamic Client Registration"
                            .to_string(),
                    ));
                }

                // Beginning a new flow: register with the AS every time there are
                // no stored credentials. This self-heals when the upstream AS loses
                // its dynamic-client DB while this process still has an old pending row.
                manager
                    .configure_client(dynamic_registration_bootstrap_config(
                        self.redirect_uri.as_str(),
                    ))
                    .map_err(|e| {
                        OauthError::Internal(format!(
                            "configure dynamic registration application_type: {e}"
                        ))
                    })?;
                let cfg = manager
                    .register_client(
                        self.client_name.as_str(),
                        self.redirect_uri.as_str(),
                        scopes,
                    )
                    .await
                    .map_err(|e| OauthError::Internal(format!("dynamic registration: {e}")))?;

                self.sqlite
                    .save_dynamic_client_registration(
                        &self.upstream.name,
                        subject,
                        &cfg.client_id,
                        issuer,
                    )
                    .await
                    .map_err(|e| OauthError::Internal(e.to_string()))?;

                // Read back the persisted value to use the DB-canonical client_id.
                let canonical_registration = self
                    .sqlite
                    .find_dynamic_client_registration(&self.upstream.name, subject)
                    .await
                    .map_err(|e| OauthError::Internal(e.to_string()))?
                    .ok_or_else(|| {
                        OauthError::Internal(
                            "dynamic registration saved but read-back returned nothing".to_string(),
                        )
                    })?;

                let mut canonical_cfg = OAuthClientConfig::new(
                    canonical_registration.client_id,
                    self.redirect_uri.as_str(),
                );
                canonical_cfg =
                    canonical_cfg.with_scopes(scopes.iter().map(|s| s.to_string()).collect());
                Ok(canonical_cfg)
            }
            UpstreamOauthRegistration::ClientMetadataDocument { url } => {
                // Client ID Metadata Document (CIMD): the metadata-document URL
                // *is* the client identifier. No registration_endpoint call is
                // issued — the AS fetches the document itself when it first sees
                // the client_id. We construct the OAuth client locally.
                let parsed = url::Url::parse(url).map_err(|e| {
                    OauthError::Internal(format!("invalid client_metadata_document url: {e}"))
                })?;
                if parsed.scheme() != "https" {
                    return Err(OauthError::Internal(format!(
                        "client_metadata_document url must use https, got `{}`",
                        parsed.scheme()
                    )));
                }
                let cfg = OAuthClientConfig::new(url.clone(), self.redirect_uri.as_str())
                    .with_scopes(scopes.iter().map(|s| s.to_string()).collect())
                    .with_application_type("web");
                Ok(cfg)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_prefers_cimd_only_when_the_server_advertises_it() {
        assert!(should_use_client_metadata_document(
            &UpstreamOauthRegistration::Auto,
            None,
            true,
        ));
        assert!(!should_use_client_metadata_document(
            &UpstreamOauthRegistration::Auto,
            None,
            false,
        ));
    }

    #[test]
    fn explicit_dynamic_remains_dcr_unless_the_compatibility_override_is_enabled() {
        assert!(!should_use_client_metadata_document(
            &UpstreamOauthRegistration::Dynamic,
            None,
            true,
        ));
        assert!(should_use_client_metadata_document(
            &UpstreamOauthRegistration::Dynamic,
            Some(true),
            true,
        ));
    }

    #[test]
    fn dynamic_registration_identifies_soma_as_a_web_client() {
        let config =
            dynamic_registration_bootstrap_config("https://soma.example/auth/upstream/callback");
        assert_eq!(config.application_type.as_deref(), Some("web"));
    }

    #[test]
    fn generated_cimd_url_uses_the_public_callback_origin() {
        let url = generated_client_metadata_document_url(
            "https://soma.example/auth/upstream/callback?ignored=yes",
            "github",
        )
        .expect("metadata URL")
        .expect("https callback should host CIMD");
        assert_eq!(
            url,
            "https://soma.example/auth/upstream/client-metadata/github"
        );
    }

    #[test]
    fn generated_cimd_url_rejects_plain_http_so_auto_can_fall_back_to_dcr() {
        let url = generated_client_metadata_document_url(
            "http://127.0.0.1:40060/auth/upstream/callback",
            "github",
        )
        .expect("valid callback URL");
        assert!(url.is_none());
    }
}
