// Render per-item feature-requirement badges when rustdoc runs on nightly with
// `--cfg docsrs` (docs.rs posture; locally via `cargo xtask doc --docsrs-cfg`).
// Inert under the stable CI doc gate: stable rustdoc never sets `docsrs`.
#![cfg_attr(docsrs, feature(doc_auto_cfg))]
pub mod server;

#[cfg(feature = "protected-routes")]
pub mod protected_routes;
#[cfg(feature = "protected-routes")]
mod protected_routes_proxy;
#[cfg(all(test, feature = "protected-routes"))]
mod test_support;

pub use server::{
    resolve_auth_policy_kind, AppState, AuthPolicy, AuthPolicyKind, ResponsePageStore, SomaRuntime,
};
