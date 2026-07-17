//! Deprecated compatibility facade — see `soma_domain::provider_validation`.

pub use soma_domain::provider_validation::*;

#[cfg(test)]
#[path = "provider_validation_tests.rs"]
mod tests;
