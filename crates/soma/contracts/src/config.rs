//! Deprecated compatibility facade — see `soma_config`.

pub use soma_config::*;

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
