//! Gateway-local security helpers.

pub mod env;
pub mod redact;
pub mod ssrf;

pub use ssrf::OutboundPolicy;

#[cfg(test)]
#[path = "security_tests.rs"]
mod tests;
