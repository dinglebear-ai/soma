//! Gateway runtime seams.

pub mod catalog;
#[cfg(feature = "codemode")]
pub mod code_mode;
pub mod config_store;
pub mod dispatch;
pub mod manager;
#[cfg(feature = "oauth")]
pub mod oauth;
#[cfg(feature = "openapi")]
pub mod openapi;
#[cfg(feature = "palette")]
pub mod palette;
pub mod params;
pub mod projection;
#[cfg(feature = "protected-routes")]
pub mod protected_routes;
pub mod runtime;
pub mod view_models;
#[cfg(feature = "protected-routes")]
pub mod virtual_servers;

#[cfg(test)]
#[path = "gateway_tests.rs"]
mod tests;
