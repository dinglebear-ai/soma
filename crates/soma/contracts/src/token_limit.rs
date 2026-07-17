//! Deprecated compatibility facade — see `soma_domain::token_limit`.

pub use soma_domain::token_limit::*;

#[cfg(test)]
#[path = "token_limit_tests.rs"]
mod tests;
