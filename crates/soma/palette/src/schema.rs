//! Schema lookup for a single Palette launcher action.

use soma_application::CatalogSnapshot;
use soma_provider_core::ProviderSurface;

use crate::dto::LauncherSchemaResponse;

/// Find `id`'s tool across every catalog in `snapshot` and return its
/// schemas, provided it's still exposed on the Palette surface. `None` means
/// "not found or no longer palette-exposed" — callers map that to 404.
#[must_use]
pub fn find_schema(snapshot: &CatalogSnapshot, id: &str) -> Option<LauncherSchemaResponse> {
    snapshot
        .catalogs
        .iter()
        .flat_map(|catalog| catalog.tools.iter())
        .find(|tool| tool.name == id && tool.exposed_on(ProviderSurface::Palette))
        .map(|tool| LauncherSchemaResponse {
            id: tool.name.clone(),
            input_schema: tool.input_schema.clone(),
            output_schema: tool.output_schema.clone(),
        })
}

#[cfg(test)]
#[path = "schema_tests.rs"]
mod tests;
