//! Generic manifest -> concrete-provider dispatch, factored out of
//! soma-service's drop-in provider directory loader
//! (`crates/soma/service/src/providers/filesystem.rs`'s `provider_for_catalog`
//! function). Given an already-parsed, already-validated `ProviderManifest`
//! and the file it came from, builds the matching adapter for its declared
//! `ProviderKind`.
//!
//! The directory scan, fingerprinting, and Soma-specific manifest policy
//! (reserved CLI command names, `SOMA_`/`LAB_` env-prefix denial) stay in
//! soma-service — see the PR10 deviation notes for why that orchestration
//! did not move here.

use std::path::PathBuf;

use soma_provider_core::{Provider, ProviderCatalog, ProviderKind};

/// Builds the concrete adapter for `catalog`'s declared kind.
///
/// `env_prefix` is the product's env-namespace (e.g. `"SOMA"`), forwarded to
/// the ai-sdk and python adapters' `EnvRequirement` resolution.
///
/// Returns `None` when the crate was not built with the feature that owns
/// `catalog`'s kind. Every `ProviderKind` has a matching adapter here,
/// feature-gated per plan section 3.9 ("Do not over-split") so a consumer
/// only pays for the runtimes it actually drop-in-loads.
#[allow(unused_variables)]
pub fn build_provider(
    path: PathBuf,
    catalog: ProviderCatalog,
    env_prefix: &str,
) -> Option<std::sync::Arc<dyn Provider>> {
    match catalog.provider.kind {
        #[cfg(feature = "openapi")]
        ProviderKind::Openapi => Some(crate::openapi::OpenApiProvider::arc(catalog)),
        #[cfg(not(feature = "openapi"))]
        ProviderKind::Openapi => None,

        #[cfg(feature = "gateway")]
        ProviderKind::Mcp => Some(crate::gateway::UpstreamMcpProvider::arc(catalog)),
        #[cfg(not(feature = "gateway"))]
        ProviderKind::Mcp => None,

        #[cfg(feature = "ai-sdk")]
        ProviderKind::AiSdk => Some(crate::ai_sdk::AiSdkProvider::arc(path, catalog, env_prefix)),
        #[cfg(not(feature = "ai-sdk"))]
        ProviderKind::AiSdk => None,

        #[cfg(feature = "wasm")]
        ProviderKind::Wasm => Some(crate::wasm::WasmProvider::arc(path, catalog)),
        #[cfg(not(feature = "wasm"))]
        ProviderKind::Wasm => None,

        #[cfg(feature = "python")]
        ProviderKind::Python | ProviderKind::Langchain | ProviderKind::Llamaindex => Some(
            crate::python::PythonProvider::arc(path, catalog, env_prefix),
        ),
        #[cfg(not(feature = "python"))]
        ProviderKind::Python | ProviderKind::Langchain | ProviderKind::Llamaindex => None,

        #[cfg(feature = "static-echo")]
        ProviderKind::StaticRust => {
            Some(crate::static_rust::StaticEchoProvider::arc(path, catalog))
        }
        #[cfg(not(feature = "static-echo"))]
        ProviderKind::StaticRust => None,
    }
}

#[cfg(test)]
#[path = "manifest_file_tests.rs"]
mod tests;
