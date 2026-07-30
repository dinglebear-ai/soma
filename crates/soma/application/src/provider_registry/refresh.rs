use std::{collections::BTreeSet, path::Path};

use crate::providers::filesystem::{
    FileProviderLoadError, FileProviderSource, PythonProviderEnvironmentSelections,
};

use super::{ProviderSurface, RegistrySnapshot, provider_tool_surface_enabled};

pub(super) async fn fingerprint_file_source(
    source: FileProviderSource,
    environments: PythonProviderEnvironmentSelections,
) -> Result<String, FileProviderLoadError> {
    let root = source.root().to_path_buf();
    tokio::task::spawn_blocking(move || source.fingerprint_with_python_environments(&environments))
        .await
        .map_err(|error| FileProviderLoadError {
            path: root,
            message: format!("provider fingerprint task failed: {error}"),
        })?
}

pub(super) struct ProviderRefreshEvent {
    fingerprint: String,
    added_actions: Vec<String>,
    removed_actions: Vec<String>,
    mcp_actions: Vec<String>,
    cli_actions: Vec<String>,
    rest_routes: Vec<String>,
}

impl ProviderRefreshEvent {
    pub(super) fn new(previous: &RegistrySnapshot, next: &RegistrySnapshot) -> Self {
        let previous_actions = previous
            .action_names()
            .into_iter()
            .map(str::to_owned)
            .collect::<BTreeSet<_>>();
        let next_actions = next
            .action_names()
            .into_iter()
            .map(str::to_owned)
            .collect::<BTreeSet<_>>();
        Self {
            fingerprint: next.fingerprint.clone(),
            added_actions: next_actions
                .difference(&previous_actions)
                .cloned()
                .collect(),
            removed_actions: previous_actions
                .difference(&next_actions)
                .cloned()
                .collect(),
            mcp_actions: surface_actions(next, ProviderSurface::Mcp),
            cli_actions: surface_actions(next, ProviderSurface::Cli),
            rest_routes: rest_routes(next),
        }
    }

    pub(super) fn log(&self, provider_dir: &Path) {
        tracing::info!(
            provider_dir = %provider_dir.display(),
            fingerprint = %self.fingerprint,
            added_actions = ?self.added_actions,
            removed_actions = ?self.removed_actions,
            mcp_actions = ?self.mcp_actions,
            cli_actions = ?self.cli_actions,
            rest_routes = ?self.rest_routes,
            "file providers refreshed; MCP, CLI, and API surfaces recomputed"
        );
    }
}

fn surface_actions(snapshot: &RegistrySnapshot, surface: ProviderSurface) -> Vec<String> {
    let mut actions = snapshot
        .catalogs
        .iter()
        .flat_map(|catalog| catalog.tools.iter())
        .filter(|tool| provider_tool_surface_enabled(tool, surface))
        .map(|tool| tool.name.clone())
        .collect::<Vec<_>>();
    actions.sort();
    actions
}

fn rest_routes(snapshot: &RegistrySnapshot) -> Vec<String> {
    let mut routes = snapshot
        .rest_routes()
        .map(|(method, path, _)| format!("{method} {path}"))
        .collect::<Vec<_>>();
    routes.sort();
    routes
}

#[cfg(test)]
#[path = "refresh_tests.rs"]
mod tests;
