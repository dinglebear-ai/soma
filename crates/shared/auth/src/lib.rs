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
pub mod routes;
#[cfg(feature = "http-axum")]
pub mod session;
pub mod sqlite;
pub mod state;
#[cfg(feature = "http-axum")]
pub mod token;
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
