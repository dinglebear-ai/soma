use soma_auth::config::{AuthConfigBuilder, AuthMode, AuthProfile, GoogleConfig};
use url::Url;

fn main() -> Result<(), soma_auth::error::AuthError> {
    let profile = AuthProfile {
        env_prefix: "AXON".into(),
        default_data_dir: "/var/lib/axon/auth".into(),
        session_cookie_name: "axon_session".into(),
        scopes_supported: vec!["axon:read".into(), "axon:admin".into()],
        resource_path: "/mcp".into(),
        default_scope: "axon:read".into(),
        static_token_scopes: vec!["axon:read".into(), "axon:admin".into()],
        login_path: "/auth/login".into(),
        enable_dynamic_registration: true,
        disable_static_token_with_oauth: true,
        upstream_client_name: "axon".into(),
        upstream_callback_path: "/oauth/upstream/callback".into(),
    };

    let config = AuthConfigBuilder::from_profile(profile)
        .mode(AuthMode::OAuth)
        .public_url(Url::parse("https://axon.example.com").expect("valid public URL"))
        .admin_email("admin@example.com")
        .google(GoogleConfig {
            client_id: "client-id".into(),
            client_secret: "client-secret".into(),
            ..GoogleConfig::default()
        })
        .build()?;

    println!("issuer={}", config.public_url.expect("configured"));
    Ok(())
}
