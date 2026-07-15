use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use serde_json::Value;
use thiserror::Error;

use crate::gateway::code_mode::catalog::CodeModeCatalog;

pub const PALETTE_SCHEMA_CAP_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteCatalog {
    pub tools: Vec<String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PaletteError {
    #[error("schema exceeds palette cap")]
    SchemaTooLarge,
    #[error("schema not found")]
    NotFound,
}

#[derive(Debug, Clone)]
pub struct PaletteCache {
    freshness: Duration,
    cached_at: Option<Instant>,
    catalog: PaletteCatalog,
    reprobe_count: usize,
    schemas: BTreeMap<String, Value>,
}

impl PaletteCache {
    #[must_use]
    pub fn new(freshness: Duration) -> Self {
        Self {
            freshness,
            cached_at: None,
            catalog: PaletteCatalog { tools: Vec::new() },
            reprobe_count: 0,
            schemas: BTreeMap::new(),
        }
    }

    pub fn catalog_from_code_mode(
        &mut self,
        catalog: &CodeModeCatalog,
        now: Instant,
    ) -> PaletteCatalog {
        if self
            .cached_at
            .is_some_and(|cached_at| now.duration_since(cached_at) < self.freshness)
        {
            return self.catalog.clone();
        }
        self.reprobe_count += 1;
        self.catalog = PaletteCatalog {
            tools: catalog.names_only(),
        };
        self.cached_at = Some(now);
        self.catalog.clone()
    }

    pub fn set_schema(&mut self, id: impl Into<String>, schema: Value) {
        self.schemas.insert(id.into(), schema);
    }

    pub fn schema(&self, id: &str) -> Result<Value, PaletteError> {
        let schema = self.schemas.get(id).ok_or(PaletteError::NotFound)?;
        let bytes = serde_json::to_vec(schema).map_or(usize::MAX, |bytes| bytes.len());
        if bytes > PALETTE_SCHEMA_CAP_BYTES {
            return Err(PaletteError::SchemaTooLarge);
        }
        Ok(schema.clone())
    }

    #[must_use]
    pub fn reprobe_count(&self) -> usize {
        self.reprobe_count
    }
}

#[cfg(test)]
#[path = "palette_tests.rs"]
mod tests;
