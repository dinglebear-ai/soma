# soma-auth

Reusable OAuth 2.1, OpenID Connect, JWT, Axum authorization, SQLite token storage, and upstream MCP OAuth primitives.

The crate name identifies its home project. Runtime behavior is product-neutral: applications provide their own identity through `AuthProfile`.

## Features

| Feature | Purpose |
| --- | --- |
| default | Core configuration, JWT, provider, storage, and token primitives |
| `http-axum` | Axum middleware and OAuth authorization-server routes |
| `upstream-oauth-rmcp` | Outbound Authorization Code + PKCE for protected MCP servers |

Minimum supported Rust version: **1.97.1**.

## Install

```toml
[dependencies]
soma-auth = { version = "0.5", features = ["http-axum"] }
```

For outbound MCP OAuth:

```toml
[dependencies]
soma-auth = { version = "0.5", features = ["upstream-oauth-rmcp"] }
```

## Product profile

The generic profile defaults to:

- Environment prefix: `APP`
- Cookie: `auth_session`
- Scopes: `app:read`, `app:admin`
- Data directory: `.auth`
- Upstream registration name: `app`
- Upstream callback path: `/auth/upstream/callback`

Production applications should declare these values explicitly at their composition root:

```rust
use soma_auth::config::{AuthConfigBuilder, AuthProfile};

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

let config = AuthConfigBuilder::from_profile(profile).build()?;
# Ok::<(), soma_auth::error::AuthError>(())
```

## Typed configuration and environment overlays

`AuthConfigBuilder::build` validates typed configuration without reading process environment variables. `EnvAuthConfigLoader` and `build_from_sources` optionally overlay env-style key/value pairs. The loader accepts any iterator, so values may come from process env, TOML, CLI flags, Vault, or another secret provider.

```rust
use soma_auth::config::{AuthConfigBuilder, AuthProfile};

let config = AuthConfigBuilder::from_profile(AuthProfile::default())
    .session_cookie_name("my_service_session")
    .build_from_sources(std::env::vars())?;
# Ok::<(), soma_auth::error::AuthError>(())
```

## HTTP authorization server

Enable `http-axum` to mount authorization, token, registration, revocation, metadata, login, and provider callback routes. Build an `AuthState`, then merge `soma_auth::routes::router(state)` into the application router.

Supported identity providers include Google, GitHub, and generic OIDC through Authelia-compatible discovery. OAuth mode validates HTTPS issuer URLs, callback collisions, redirect allowlists, provider credentials, scopes, and bootstrap-admin policy before runtime startup.

## Outbound MCP OAuth

Enable `upstream-oauth-rmcp` for Authorization Code + PKCE against protected MCP servers. The consumer supplies both the dynamic-registration client name and callback path through `AuthProfile`; neither is hardcoded by the crate.

Refresh tokens and credentials are stored in SQLite. Configure an encryption key before enabling outbound OAuth. The runtime refuses insecure public URLs and invalid callback paths.

## Storage and security

- SQLite is bundled through `rusqlite` for portable deployment.
- JWT signing uses Ed25519.
- Provider and upstream refresh tokens support ChaCha20-Poly1305 encryption at rest.
- PKCE S256 is required for upstream authorization.
- Secrets are redacted from `Debug` output where represented by secret wrapper types.
- Dynamic client registration is disabled by default.
- Static bearer access during OAuth remains an explicit consumer policy.

## Examples

```bash
cargo run -p soma-auth --example bearer
cargo run -p soma-auth --example typed_oauth
cargo run -p soma-auth --example upstream_mcp_oauth --features upstream-oauth-rmcp
```

## Versioning

The crate follows semantic versioning. Product-profile and public configuration changes are treated as public API changes. Published releases are tagged `soma-auth-vX.Y.Z` from the Soma repository.

## License

MIT
