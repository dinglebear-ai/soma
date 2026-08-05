use soma_auth::config::{AuthConfigBuilder, AuthProfile};
use soma_auth::upstream::config::{
    UpstreamConfig, UpstreamOauthConfig, UpstreamOauthMode, UpstreamOauthRegistration,
};

fn main() -> Result<(), soma_auth::error::AuthError> {
    let auth = AuthConfigBuilder::from_profile(AuthProfile {
        upstream_client_name: "axon".into(),
        upstream_callback_path: "/oauth/upstream/callback".into(),
        ..AuthProfile::default()
    })
    .build()?;

    let upstream = UpstreamConfig {
        name: "protected-tools".into(),
        url: Some("https://tools.example.com/mcp".into()),
        oauth: Some(UpstreamOauthConfig {
            mode: UpstreamOauthMode::AuthorizationCodePkce,
            registration: UpstreamOauthRegistration::Auto,
            scopes: Some(vec!["tools:read".into()]),
            prefer_client_metadata_document: None,
        }),
    };

    println!(
        "client={} callback={} upstream={}",
        auth.upstream_client_name, auth.upstream_callback_path, upstream.name
    );
    Ok(())
}
