//! Reusable OAuth 2.1, OIDC, JWT, HTTP authorization, token storage, and upstream MCP authentication primitives.
//!
//! Applications provide product identity through [`config::AuthProfile`], build typed
//! configuration with [`config::AuthConfigBuilder`], and may optionally overlay
//! environment-style values through [`config::EnvAuthConfigLoader`]. No Soma or
//! Labby identity is assumed by the runtime defaults.

// Render per-item feature-requirement badges when rustdoc runs on nightly with
// `--cfg docsrs` (docs.rs posture; locally via `cargo xtask doc --docsrs-cfg`).
// Inert under the stable CI doc gate: stable rustdoc never sets `docsrs`.
#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#![allow(deprecated)]

pub(crate) mod aead;
pub mod at_rest;
#[cfg(feature = "http-axum")]
pub mod auth_context;
pub mod authelia;
#[cfg(feature = "http-axum")]
pub mod authorize;
#[cfg(feature = "http-axum")]
pub mod cimd;
pub mod config;
pub mod error;
pub mod github;
pub mod google;
pub mod jwt;
#[cfg(feature = "http-axum")]
pub mod metadata;
#[cfg(feature = "http-axum")]
pub mod middleware;
pub mod oauth_provider;
pub(crate) mod oidc;
pub(crate) mod provider_http;
#[cfg(feature = "http-axum")]
pub mod redirect_uri;
#[cfg(feature = "http-axum")]
pub mod registration;
#[cfg(feature = "http-axum")]
pub mod revoke;
#[cfg(feature = "http-axum")]
pub mod routes;
#[cfg(feature = "http-axum")]
pub mod session;
pub mod sqlite;
pub mod state;
#[cfg(feature = "http-axum")]
pub mod token;
// OAuth 2.1 client authentication (client_secret_basic / private_key_jwt) and
// the client-credentials + jwt-bearer machine grants. Wired into the token
// endpoint by `token::prepare_client_credentials`, `token::authenticate_client`,
// and `token::machine_client_grant`.
#[cfg(feature = "http-axum")]
mod token_client_auth;
pub mod types;
#[cfg(feature = "upstream-oauth-rmcp")]
pub mod upstream;
pub mod util;

#[cfg(feature = "http-axum")]
pub use auth_context::{AuthContext, auth_context, www_authenticate_value};
#[cfg(feature = "http-axum")]
pub use middleware::{ActorKeyDeriver, AuthLayer, AuthService, parse_bearer_token, tokens_equal};

#[cfg(test)]
pub mod test_support;
