#![forbid(unsafe_code)]

//! Standalone Synapse product runtime over Soma's canonical operation catalog, fleet, and infrastructure engines.
//!
//! CLI, REST, HTTP MCP, and stdio MCP adapters all delegate to the same 59-operation runtime.

mod activity;
mod cli;
mod composition;
mod config;
mod fleet;
mod http;
mod mcp;
mod openapi;
mod runtime;

pub use activity::{ActivityEvent, ActivityLog};
pub use config::{EndpointConfig, HostConfig, ServerConfig, SynapseConfig};
pub use runtime::{ExecuteOptions, StandaloneError, StandaloneRuntime};

pub async fn run<I, T>(args: I) -> anyhow::Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    cli::run(args).await
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
