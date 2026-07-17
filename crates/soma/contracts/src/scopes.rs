//! Deprecated compatibility facade — see `soma_domain::scopes`.

pub use soma_domain::scopes::{has_admin_scope, ADMIN_SCOPE};

#[cfg(test)]
#[path = "scopes_tests.rs"]
mod tests;
