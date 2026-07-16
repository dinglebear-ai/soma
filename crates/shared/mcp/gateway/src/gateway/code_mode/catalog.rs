use std::collections::BTreeMap;

use serde_json::Value;
use thiserror::Error;

use crate::upstream::UpstreamSnapshot;

pub const CODEMODE_SCHEMA_CAP_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct CodeModeCatalogEntry {
    pub id: String,
    pub namespace: String,
    pub tool: String,
    pub description: Option<String>,
    pub input_schema: Option<Value>,
    pub ui_link: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CodeModeCatalog {
    entries: BTreeMap<String, CodeModeCatalogEntry>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CatalogError {
    #[error("schema exceeds Code Mode cap")]
    SchemaTooLarge,
    #[error("schema not found")]
    NotFound,
}

impl CodeModeCatalog {
    #[must_use]
    pub fn from_snapshots(snapshots: &[UpstreamSnapshot]) -> Self {
        let mut entries = BTreeMap::new();
        for snapshot in snapshots {
            for tool in &snapshot.tools {
                let descriptor = soma_codemode::ToolDescriptor::tool(
                    &snapshot.name,
                    &tool.name,
                    tool.description.as_deref().unwrap_or_default(),
                    tool.input_schema.clone(),
                    tool.output_schema.clone(),
                );
                let id = descriptor.id;
                entries.insert(
                    id.clone(),
                    CodeModeCatalogEntry {
                        id,
                        namespace: snapshot.name.clone(),
                        tool: tool.name.clone(),
                        description: tool.description.clone(),
                        input_schema: descriptor.schema,
                        ui_link: None,
                    },
                );
            }
        }
        Self { entries }
    }

    #[must_use]
    pub fn names_only(&self) -> Vec<String> {
        self.entries.keys().cloned().collect()
    }

    pub fn schema_for(&self, id: &str) -> Result<Option<Value>, CatalogError> {
        let entry = self.entries.get(id).ok_or(CatalogError::NotFound)?;
        let Some(schema) = &entry.input_schema else {
            return Ok(None);
        };
        let bytes = serde_json::to_vec(schema).map_or(usize::MAX, |bytes| bytes.len());
        if bytes > CODEMODE_SCHEMA_CAP_BYTES {
            return Err(CatalogError::SchemaTooLarge);
        }
        Ok(Some(schema.clone()))
    }
}

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;
