//! Deprecated compatibility facade — see `soma_domain::errors`.

pub use soma_domain::errors::*;

#[cfg(test)]
#[path = "errors_tests.rs"]
mod tests;
