use std::sync::Arc;

use crate::config::AuthConfig;
use crate::sqlite::SqliteStore;
use crate::upstream::cache::OauthClientCache;
use crate::upstream::config::UpstreamConfig;
use crate::upstream::encryption::{EncryptionKey, load_key};
use crate::upstream::manager::UpstreamOauthManager;
use anyhow::{Context, Result};

pub struct UpstreamOauthRuntime {
    pub managers: Arc<dashmap::DashMap<String, UpstreamOauthManager>>,
    pub cache: OauthClientCache,
    pub sqlite: SqliteStore,
    pub key: EncryptionKey,
    pub redirect_uri: String,
    pub client_name: String,
}

/// Build the upstream OAuth runtime for the upstreams that declare an `oauth`
/// block.
///
/// Takes the upstream slice directly rather than a whole product config type
/// so this runtime stays decoupled from the consumer's config type: `soma-auth`
/// reads only the upstream list, never anything else.
pub async fn build_upstream_oauth_runtime(
    upstreams: &[UpstreamConfig],
    auth_config: &AuthConfig,
    encryption_key_raw: Option<&str>,
) -> Result<Option<UpstreamOauthRuntime>> {
    let Some(public_url) = auth_config.public_url.as_ref() else {
        tracing::info!(
            subsystem = "gateway_client",
            phase = "oauth.runtime.disabled",
            "upstream oauth runtime disabled because no public url is configured"
        );
        return Ok(None);
    };
    let Some(encryption_key_raw) =
        encryption_key_raw.and_then(|value| (!value.trim().is_empty()).then_some(value))
    else {
        tracing::info!(
            subsystem = "gateway_client",
            phase = "oauth.runtime.disabled",
            "upstream oauth runtime disabled because no encryption key is configured"
        );
        return Ok(None);
    };
    anyhow::ensure!(
        public_url.scheme() == "https",
        "public_url must be absolute https:// when upstream oauth is configured"
    );
    let key = load_key(encryption_key_raw)
        .map_err(|error| anyhow::anyhow!("invalid upstream OAuth encryption key: {error}"))?;
    let sqlite = SqliteStore::open(auth_config.sqlite_path.clone())
        .await
        .context("open sqlite store for upstream oauth")?;
    let redirect_uri =
        build_upstream_oauth_callback_uri(public_url, &auth_config.upstream_callback_path)?;

    Ok(Some(build_upstream_oauth_runtime_from_parts(
        upstreams,
        sqlite,
        key,
        redirect_uri,
        auth_config.upstream_client_name.clone(),
    )))
}

/// Assemble the runtime from pre-loaded parts.
///
/// `upstreams` is the upstream slice (decoupled from any product config); only the
/// entries with an `oauth` block get a manager.
pub fn build_upstream_oauth_runtime_from_parts(
    upstreams: &[UpstreamConfig],
    sqlite: SqliteStore,
    key: EncryptionKey,
    redirect_uri: String,
    client_name: String,
) -> UpstreamOauthRuntime {
    let managers = Arc::new(dashmap::DashMap::new());
    for upstream in upstreams.iter().filter(|upstream| upstream.oauth.is_some()) {
        managers.insert(
            upstream.name.clone(),
            UpstreamOauthManager::new(
                sqlite.clone(),
                key.clone(),
                upstream.clone(),
                redirect_uri.clone(),
                client_name.clone(),
            ),
        );
    }
    let cache = OauthClientCache::new(Arc::clone(&managers));
    tracing::info!(
        subsystem = "gateway_client",
        phase = "oauth.runtime.ready",
        oauth_upstream_count = managers.len(),
        "upstream oauth runtime initialized"
    );
    UpstreamOauthRuntime {
        managers,
        cache,
        sqlite,
        key,
        redirect_uri,
        client_name,
    }
}

pub fn build_upstream_oauth_callback_uri(
    public_url: &url::Url,
    callback_path: &str,
) -> Result<String> {
    anyhow::ensure!(
        callback_path.starts_with('/'),
        "upstream OAuth callback path must start with `/`"
    );
    let mut redirect_uri = public_url.clone();
    let base_path = redirect_uri.path().trim_end_matches('/');
    let callback_path = callback_path.trim_start_matches('/');
    let next_path = if base_path.is_empty() {
        format!("/{callback_path}")
    } else {
        format!("{base_path}/{callback_path}")
    };
    redirect_uri.set_path(&next_path);
    redirect_uri.set_query(None);
    redirect_uri.set_fragment(None);
    Ok(redirect_uri.to_string())
}

#[cfg(test)]
mod tests {
    use super::build_upstream_oauth_callback_uri;

    #[test]
    fn callback_uri_uses_the_consumer_supplied_path() {
        let public_url = url::Url::parse("https://axon.example.com/base").unwrap();
        let callback =
            build_upstream_oauth_callback_uri(&public_url, "/oauth/upstream/complete").unwrap();

        assert_eq!(
            callback,
            "https://axon.example.com/base/oauth/upstream/complete"
        );
    }

    #[test]
    fn callback_uri_rejects_relative_paths() {
        let public_url = url::Url::parse("https://axon.example.com").unwrap();
        let error = build_upstream_oauth_callback_uri(&public_url, "oauth/callback").unwrap_err();

        assert!(error.to_string().contains("must start with `/`"));
    }
}
