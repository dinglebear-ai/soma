use std::{collections::BTreeMap, sync::Arc};

use anyhow::{Context, Result};
use futures::future::BoxFuture;
use mcp_client::{
    config::{GatewayUpstreamOauthMode, GatewayUpstreamOauthRegistration, UpstreamConfig},
    oauth::{
        BeginAuthorization, UpstreamOAuthCredentialStatus, UpstreamOAuthError,
        UpstreamOAuthHttpClient, UpstreamOAuthManager, UpstreamOAuthProvider, UpstreamOAuthRuntime,
    },
    upstream::http_body_cap::BodyCappedHttpClient,
};

#[derive(Clone)]
struct SomaOAuthProvider {
    cache: soma_auth::upstream::cache::OauthClientCache,
}

impl UpstreamOAuthProvider for SomaOAuthProvider {
    fn authenticated_http_client<'a>(
        &'a self,
        upstream: &'a UpstreamConfig,
        subject: &'a str,
        http_client: BodyCappedHttpClient,
    ) -> BoxFuture<'a, Result<UpstreamOAuthHttpClient, UpstreamOAuthError>> {
        Box::pin(async move {
            let upstream = to_auth_upstream(upstream)?;
            self.cache
                .get_or_build_capped(&upstream, subject, http_client)
                .await
                .map_err(map_oauth_error)
        })
    }

    fn evict_subject(&self, upstream: &str, subject: &str) {
        self.cache.evict_subject(upstream, subject);
    }

    fn evict_upstream(&self, upstream: &str) {
        self.cache.evict_upstream(upstream);
    }
}

#[derive(Clone)]
struct SomaOAuthManager {
    manager: soma_auth::upstream::manager::UpstreamOauthManager,
    cache: soma_auth::upstream::cache::OauthClientCache,
}

impl UpstreamOAuthManager for SomaOAuthManager {
    fn begin_authorization<'a>(
        &'a self,
        subject: &'a str,
    ) -> BoxFuture<'a, Result<BeginAuthorization, UpstreamOAuthError>> {
        Box::pin(async move {
            self.manager
                .begin_authorization(subject)
                .await
                .map(|result| BeginAuthorization {
                    authorization_url: result.authorization_url,
                })
                .map_err(map_oauth_error)
        })
    }

    fn credential_status<'a>(
        &'a self,
        subject: &'a str,
    ) -> BoxFuture<'a, Result<Option<UpstreamOAuthCredentialStatus>, UpstreamOAuthError>> {
        Box::pin(async move {
            self.manager
                .credential_row(subject)
                .await
                .map(|row| {
                    row.map(|row| UpstreamOAuthCredentialStatus {
                        access_token_expires_at: row.access_token_expires_at,
                        refresh_token_present: row.refresh_token_present,
                    })
                })
                .map_err(map_oauth_error)
        })
    }

    fn clear_credentials<'a>(
        &'a self,
        subject: &'a str,
    ) -> BoxFuture<'a, Result<(), UpstreamOAuthError>> {
        Box::pin(async move {
            self.manager
                .clear_credentials(subject)
                .await
                .map_err(map_oauth_error)?;
            self.cache
                .evict_subject(&self.manager.upstream_config().name, subject);
            Ok(())
        })
    }

    fn access_token<'a>(
        &'a self,
        subject: &'a str,
    ) -> BoxFuture<'a, Result<String, UpstreamOAuthError>> {
        Box::pin(async move {
            let client = self
                .cache
                .get_or_build(self.manager.upstream_config(), subject)
                .await
                .map_err(map_oauth_error)?;
            client
                .get_access_token()
                .await
                .map_err(|error| UpstreamOAuthError::internal(error.to_string()))
        })
    }
}

pub async fn build_runtime(
    upstreams: &[UpstreamConfig],
    auth_config: &soma_auth::config::AuthConfig,
    encryption_key: Option<&str>,
) -> Result<Option<UpstreamOAuthRuntime>> {
    let auth_upstreams = upstreams
        .iter()
        .filter(|upstream| upstream.oauth.is_some())
        .map(to_auth_upstream)
        .collect::<Result<Vec<_>, _>>()?;
    let Some(runtime) = soma_auth::upstream::runtime::build_upstream_oauth_runtime(
        &auth_upstreams,
        auth_config,
        encryption_key,
    )
    .await
    .context("build Soma upstream OAuth runtime")?
    else {
        return Ok(None);
    };

    let provider = Arc::new(SomaOAuthProvider {
        cache: runtime.cache.clone(),
    });
    let managers = runtime
        .managers
        .iter()
        .map(|entry| {
            (
                entry.key().clone(),
                Arc::new(SomaOAuthManager {
                    manager: entry.value().clone(),
                    cache: runtime.cache.clone(),
                }) as Arc<dyn UpstreamOAuthManager>,
            )
        })
        .collect::<BTreeMap<_, _>>();
    Ok(Some(UpstreamOAuthRuntime::new(provider, managers)))
}

fn to_auth_upstream(
    upstream: &UpstreamConfig,
) -> Result<soma_auth::upstream::config::UpstreamConfig, UpstreamOAuthError> {
    let oauth = upstream
        .oauth
        .as_ref()
        .ok_or_else(|| UpstreamOAuthError::internal("upstream OAuth config is missing"))?;
    let url = upstream
        .url
        .clone()
        .ok_or_else(|| UpstreamOAuthError::internal("upstream OAuth URL is missing"))?;
    Ok(soma_auth::upstream::config::UpstreamConfig {
        name: upstream.name.clone(),
        url: Some(url),
        oauth: Some(soma_auth::upstream::config::UpstreamOauthConfig {
            mode: match oauth.mode {
                GatewayUpstreamOauthMode::AuthorizationCodePkce => {
                    soma_auth::upstream::config::UpstreamOauthMode::AuthorizationCodePkce
                }
            },
            registration: match &oauth.registration {
                GatewayUpstreamOauthRegistration::ClientMetadataDocument { url } => {
                    soma_auth::upstream::config::UpstreamOauthRegistration::ClientMetadataDocument {
                        url: url.clone(),
                    }
                }
                GatewayUpstreamOauthRegistration::Preregistered {
                    client_id,
                    client_secret_env,
                } => soma_auth::upstream::config::UpstreamOauthRegistration::Preregistered {
                    client_id: client_id.clone(),
                    client_secret_env: client_secret_env.clone(),
                },
                GatewayUpstreamOauthRegistration::Dynamic => {
                    soma_auth::upstream::config::UpstreamOauthRegistration::Dynamic
                }
            },
            scopes: oauth.scopes.clone(),
            prefer_client_metadata_document: oauth.prefer_client_metadata_document,
        }),
    })
}

fn map_oauth_error(error: soma_auth::upstream::types::OauthError) -> UpstreamOAuthError {
    UpstreamOAuthError::new(error.kind(), error.to_string())
}

#[cfg(test)]
#[path = "gateway_auth_tests.rs"]
mod tests;
