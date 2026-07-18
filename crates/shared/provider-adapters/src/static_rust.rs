//! The generic, declarative `static-rust` provider kind: a drop-in manifest
//! with no external process, sidecar, or upstream call, whose tools either
//! echo a canned `meta.result` value or, absent one, echo their own call
//! shape back for inspection/testing. Ported from the private `FileProvider`
//! struct in Soma's filesystem loader (originally `soma-service`, now
//! `crates/soma/application`), which is the actual product-neutral "static
//! Rust provider abstraction" referenced by the architecture plan — Soma's
//! own concrete built-in-actions provider (also historically named
//! `static_rust.rs`) is a distinct, product-specific instance that dispatches
//! into `SomaService` and stays in `crates/soma/application`.

use std::{path::PathBuf, sync::Arc};

use async_trait::async_trait;
use serde_json::json;
use soma_provider_core::{Provider, ProviderCall, ProviderCatalog, ProviderError, ProviderOutput};

#[derive(Clone)]
pub struct StaticEchoProvider {
    path: PathBuf,
    catalog: ProviderCatalog,
}

impl StaticEchoProvider {
    pub fn new(path: PathBuf, catalog: ProviderCatalog) -> Self {
        Self { path, catalog }
    }

    pub fn arc(path: PathBuf, catalog: ProviderCatalog) -> Arc<Self> {
        Arc::new(Self::new(path, catalog))
    }
}

#[async_trait]
impl Provider for StaticEchoProvider {
    fn catalog(&self) -> ProviderCatalog {
        self.catalog.clone()
    }

    async fn call(&self, call: ProviderCall) -> Result<ProviderOutput, ProviderError> {
        let tool = self
            .catalog
            .tools
            .iter()
            .find(|tool| tool.name == call.action)
            .ok_or_else(|| {
                ProviderError::validation(
                    &self.catalog.provider.name,
                    &call.action,
                    "unknown_file_provider_action",
                    format!(
                        "provider file `{}` does not expose this action",
                        self.path.display()
                    ),
                )
            })?;

        if let Some(result) = tool.meta.get("result").cloned() {
            return Ok(ProviderOutput::json(result));
        }

        Ok(ProviderOutput::json(json!({
            "kind": "file_provider_result",
            "schema_version": 1,
            "provider": self.catalog.provider.name,
            "provider_kind": self.catalog.provider.kind.as_str(),
            "action": call.action,
            "params": call.params,
            "source": self.path.display().to_string(),
        })))
    }
}

#[cfg(test)]
#[path = "static_rust_tests.rs"]
mod tests;
