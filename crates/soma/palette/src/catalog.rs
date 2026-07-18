//! Product mapping from provider `ToolSpec` / Palette overlays into Palette
//! launcher actions, plus in-memory search over the mapped catalog.

use soma_application::CatalogSnapshot;
use soma_provider_core::{ProviderSurface, ToolSpec};

use crate::dto::{LauncherCatalogEntry, LauncherCatalogResponse};

/// Map every palette-exposed tool across every catalog in `snapshot` into a
/// [`LauncherCatalogEntry`]. A tool is included when it exposes itself on the
/// Palette surface, per `ToolSpec::exposed_on` (the `palette` overlay
/// defaults to enabled when absent).
#[must_use]
pub fn palette_entries(snapshot: &CatalogSnapshot) -> Vec<LauncherCatalogEntry> {
    snapshot
        .catalogs
        .iter()
        .flat_map(|catalog| {
            catalog
                .tools
                .iter()
                .filter(|tool| tool.exposed_on(ProviderSurface::Palette))
                .map(|tool| tool_to_entry(&catalog.provider.name, tool))
        })
        .collect()
}

/// Build the full catalog response for `GET /v1/palette/catalog`.
#[must_use]
pub fn catalog_response(snapshot: &CatalogSnapshot) -> LauncherCatalogResponse {
    LauncherCatalogResponse {
        schema_version: 1,
        fingerprint: snapshot.fingerprint.clone(),
        entries: palette_entries(snapshot),
    }
}

fn tool_to_entry(provider: &str, tool: &ToolSpec) -> LauncherCatalogEntry {
    let overlay = tool.palette.as_ref();
    LauncherCatalogEntry {
        id: tool.name.clone(),
        provider: provider.to_owned(),
        title: tool.title.clone().unwrap_or_else(|| tool.name.clone()),
        description: tool.description.clone(),
        category: overlay.and_then(|overlay| overlay.category.clone()),
        icon: overlay.and_then(|overlay| overlay.icon.clone()),
        tone: overlay.and_then(|overlay| overlay.tone.clone()),
        arg_mode: overlay.and_then(|overlay| overlay.arg_mode.clone()),
        result_view: overlay.and_then(|overlay| overlay.result_view.clone()),
        destructive: tool.destructive,
        requires_admin: tool.requires_admin,
    }
}

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;
