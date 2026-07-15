pub mod catalog;
pub mod discovery;
pub mod local;
pub mod names;
#[cfg(feature = "openapi")]
pub mod openapi;

#[cfg(test)]
mod catalog_tests;
#[cfg(test)]
mod discovery_tests;
#[cfg(test)]
mod local_tests;
#[cfg(test)]
mod names_tests;
#[cfg(all(test, feature = "openapi"))]
mod openapi_tests;

pub use catalog::generate_js_proxy_from_catalog;
pub use discovery::generate_discovery_js;
pub use local::generate_local_provider_js;
pub use names::{namespace_segment, tool_name_to_snake};
#[cfg(feature = "openapi")]
pub use openapi::generate_openapi_provider_js;
