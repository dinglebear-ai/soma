#![forbid(unsafe_code)]

pub mod config;
pub mod convert;
pub mod dispatch;
pub mod error;
pub mod http;
pub mod registry;
pub mod ssrf;

#[cfg(test)]
mod config_tests;
#[cfg(test)]
mod convert_tests;
#[cfg(test)]
mod dispatch_tests;
#[cfg(test)]
mod error_tests;
#[cfg(test)]
mod http_tests;
#[cfg(test)]
mod lib_tests;
#[cfg(test)]
mod registry_tests;
#[cfg(test)]
mod ssrf_tests;

pub const CRATE_NAME: &str = "soma-openapi";

pub use config::{OpenApiConfig, OpenApiCredential, OpenApiSpecConfig, SpecSource};
pub use dispatch::dispatch_openapi_call;
pub use error::{OpenApiError, SsrfError};
pub use registry::OpenApiRegistry;
