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
