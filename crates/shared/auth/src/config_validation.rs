//! Security-sensitive validation for externally visible auth configuration.

use url::Url;

use crate::error::AuthError;

use super::{AuthConfig, AuthMode};

pub(super) fn validate_security_sensitive_config(config: &AuthConfig) -> Result<(), AuthError> {
    validate_env_prefix(&config.env_prefix)?;
    validate_route_path("resource_path", &config.resource_path)?;
    validate_route_path("login_path", &config.login_path)?;
    validate_route_path("google.callback_path", &config.google.callback_path)?;
    validate_route_path("authelia.callback_path", &config.authelia.callback_path)?;
    validate_route_path("github.callback_path", &config.github.callback_path)?;
    validate_route_path("upstream_callback_path", &config.upstream_callback_path)?;
    validate_cookie_name(&config.session_cookie_name)?;

    if config.upstream_client_name.trim().is_empty() {
        return Err(AuthError::Config(
            "upstream_client_name must not be empty".to_string(),
        ));
    }
    if matches!(config.mode, AuthMode::OAuth) {
        let public_url = config.public_url.as_ref().ok_or_else(|| {
            AuthError::Config(format!(
                "{}_PUBLIC_URL is required when {}_AUTH_MODE=oauth",
                config.env_prefix, config.env_prefix
            ))
        })?;
        validate_oauth_public_url(public_url)?;
    }
    Ok(())
}

fn validate_env_prefix(prefix: &str) -> Result<(), AuthError> {
    if prefix.is_empty()
        || !prefix
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(AuthError::Config(
            "env_prefix must contain only ASCII letters, digits, or underscore".to_string(),
        ));
    }
    Ok(())
}

fn validate_route_path(name: &str, path: &str) -> Result<(), AuthError> {
    if !path.starts_with('/')
        || path.starts_with("//")
        || path.contains('?')
        || path.contains('#')
        || path.bytes().any(|byte| byte.is_ascii_control())
    {
        return Err(AuthError::Config(format!(
            "{name} must be an absolute path without authority, query, fragment, or control characters, got `{path}`"
        )));
    }
    Ok(())
}

fn validate_cookie_name(name: &str) -> Result<(), AuthError> {
    fn is_tchar(byte: u8) -> bool {
        byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                33 | 35 | 36 | 37 | 38 | 39 | 42 | 43 | 45 | 46 | 94 | 95 | 96 | 124 | 126
            )
    }

    if name.is_empty() || !name.bytes().all(is_tchar) {
        return Err(AuthError::Config(
            "session_cookie_name must be a valid HTTP cookie token".to_string(),
        ));
    }
    Ok(())
}

fn validate_oauth_public_url(url: &Url) -> Result<(), AuthError> {
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(AuthError::Config(
            "public_url for OAuth must be an https URL with no credentials, query, or fragment"
                .to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{validate_cookie_name, validate_oauth_public_url, validate_route_path};

    #[test]
    fn rejects_public_urls_that_break_issuer_or_endpoint_construction() {
        for value in [
            "http://auth.example.com",
            "https://user:pass@auth.example.com",
            "https://auth.example.com?tenant=a",
            "https://auth.example.com#fragment",
        ] {
            let url = url::Url::parse(value).unwrap();
            assert!(validate_oauth_public_url(&url).is_err(), "{value}");
        }
        validate_oauth_public_url(&url::Url::parse("https://auth.example.com").unwrap()).unwrap();
        validate_oauth_public_url(&url::Url::parse("https://auth.example.com/base").unwrap())
            .unwrap();
    }

    #[test]
    fn validates_cookie_tokens_and_route_paths() {
        validate_cookie_name("__Host-auth_session").unwrap();
        assert!(validate_cookie_name("auth session").is_err());
        validate_route_path("callback", "/auth/callback").unwrap();
        for path in [
            "auth/callback",
            "//evil.example/callback",
            "/cb?x=1",
            "/cb#x",
        ] {
            assert!(validate_route_path("callback", path).is_err(), "{path}");
        }
    }
}
