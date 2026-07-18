//! Thin bridge from a drop-in provider tool onto `soma-codemode`'s sandboxed
//! JS snippet runner, satisfying plan section 3.9's
//! `provider-adapters::codemode delegates to soma-codemode` bridge.
//!
//! There is exactly one Code Mode execution engine in the workspace
//! (`soma_codemode::execute::execute_inline`, which spawns the bounded
//! `soma-codemode-runner` subprocess); this adapter calls it directly rather
//! than re-implementing any part of the runner, sandbox, or result-shaping
//! pipeline.
//!
//! `soma_provider_core::ProviderKind` has no `CodeMode` variant yet, so this
//! adapter is not reachable through [`crate::manifest_file::build_provider`]'s
//! drop-in-manifest kind dispatch — it is real, compiles, and is
//! unit-tested, but wiring a `ProviderKind::CodeMode` drop-in manifest shape
//! is a schema change appropriately scoped to a follow-up.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::json;
use soma_codemode::{execute::execute_inline, CodeModeConfig, UiLink};
use soma_provider_core::{Provider, ProviderCall, ProviderCatalog, ProviderError, ProviderOutput};

/// Runs one fixed JS snippet on every call, ignoring `call.params` beyond
/// exposing them to the snippet as `soma.input` is out of scope for this
/// thin bridge — the snippet itself decides what, if anything, to do with
/// arguments passed through the surrounding provider manifest.
#[derive(Clone)]
pub struct CodeModeSnippetProvider {
    catalog: ProviderCatalog,
    code: String,
    config: CodeModeConfig,
}

impl CodeModeSnippetProvider {
    pub fn new(catalog: ProviderCatalog, code: impl Into<String>, config: CodeModeConfig) -> Self {
        Self {
            catalog,
            code: code.into(),
            config,
        }
    }

    pub fn arc(
        catalog: ProviderCatalog,
        code: impl Into<String>,
        config: CodeModeConfig,
    ) -> Arc<Self> {
        Arc::new(Self::new(catalog, code, config))
    }
}

#[async_trait]
impl Provider for CodeModeSnippetProvider {
    fn catalog(&self) -> ProviderCatalog {
        self.catalog.clone()
    }

    async fn call(&self, call: ProviderCall) -> Result<ProviderOutput, ProviderError> {
        let ui_capture: Arc<Mutex<Option<UiLink>>> = Arc::new(Mutex::new(None));
        let outcome = execute_inline(&self.code, self.config.clone(), ui_capture)
            .await
            .map_err(|error| {
                ProviderError::execution(&self.catalog.provider.name, call.action.clone(), error)
                    .with_provider_kind("codemode")
            })?;
        Ok(ProviderOutput::json(json!({
            "result": outcome.display_response.result,
            "logs": outcome.display_response.logs,
        })))
    }
}

#[cfg(test)]
#[path = "codemode_tests.rs"]
mod tests;
