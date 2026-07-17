//! Deprecated compatibility facade — see `soma_config::env_registry`.

pub use soma_config::env_registry::*;

#[cfg(test)]
#[path = "env_registry_tests.rs"]
mod tests;
