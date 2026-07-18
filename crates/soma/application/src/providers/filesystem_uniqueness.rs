//! Directory-wide uniqueness checks for non-executing provider inspection.
//!
//! Complements the per-file checks in `filesystem.rs`
//! (`validate_provider_manifest` + schema compilation): a manifest can pass
//! every per-file check individually and still collide with another
//! provider — or with the built-in `static-rust` provider — once loaded
//! together, which the live registry
//! (`provider_registry::{provider_map, build_snapshot}`) rejects.

use std::collections::HashMap;

use soma_provider_core::ProviderCatalog;

use crate::{
    provider_registry::DynamicResourceTemplate,
    providers::resource_uri::{PathSegment, ResourcePath},
};

use super::{ProviderFileInspection, ProviderFileInspectionStatus};

/// Mirrors the cross-provider uniqueness checks
/// `provider_registry::{provider_map, build_snapshot}` run over the whole
/// *loaded* provider set: duplicate provider name, action/tool name, REST
/// route, CLI command/alias, and MCP primitive name. Two files can each pass
/// per-file validation individually and still collide once loaded together,
/// which the live registry (and `soma providers validate`) rejects — a
/// non-executing `lint` has to catch the same thing or it gives false
/// confidence. Only files still `Loaded` after the per-file pass participate;
/// `Disabled`/`Invalid`/`Skipped` files are excluded, matching `load()`,
/// which never registers a disabled provider in the first place.
///
/// The namespace is seeded with the built-in `static-rust` catalog first —
/// `dynamic_provider_registry_from_dir()` always loads it alongside drop-in
/// files (see `crate::dynamic_provider_registry_from_dir`), so a drop-in file
/// reusing a built-in provider name or action (e.g. `status`) collides at
/// real registry construction even though no *other drop-in file* is
/// involved. Lint has to reserve those names too.
///
/// On the first file to reuse an already-claimed name, that file (not the
/// original owner) is marked `Invalid`, mirroring the live registry's
/// insert-into-a-map-fails-on-second-entry semantics.
///
/// `dynamic_resource_templates` is index-aligned with `files`/`loaded_catalogs`
/// and holds `Some(template)` only for dynamic `.ts` resource readers — the
/// only `Provider` kind whose `dynamic_resource_templates()` isn't already
/// captured in `catalog().resources` (dynamic templates are derived from the
/// filename, not declared data), so the ambiguity check
/// `provider_registry::ResourceIndex::register` runs against them at real
/// registry construction time has no equivalent in the catalog-only checks
/// above without this.
pub(super) fn apply_directory_wide_checks(
    files: &mut [ProviderFileInspection],
    loaded_catalogs: &[Option<ProviderCatalog>],
    dynamic_resource_templates: &[Option<DynamicResourceTemplate>],
) {
    let mut namespace = DirectoryNamespace::default();
    namespace.register(
        &crate::providers::static_rust::StaticRustProvider::catalog_static(),
        BUILTIN_PROVIDER_LABEL,
    );

    for index in 0..files.len() {
        let catalog = &loaded_catalogs[index];
        let template = &dynamic_resource_templates[index];
        if catalog.is_none() && template.is_none() {
            continue;
        }

        if let Some(catalog) = catalog {
            if let Some(message) = namespace.find_conflict(catalog) {
                files[index].status = ProviderFileInspectionStatus::Invalid;
                files[index].actions = Vec::new();
                files[index].error = Some(message);
                continue;
            }
        }
        if let Some(template) = template {
            if let Some(message) = namespace.find_template_conflict(template) {
                files[index].status = ProviderFileInspectionStatus::Invalid;
                files[index].actions = Vec::new();
                files[index].error = Some(message);
                continue;
            }
        }

        if let Some(catalog) = catalog {
            namespace.register(catalog, &files[index].file_name);
        }
        if let Some(template) = template {
            namespace.register_template(template, &files[index].file_name);
        }
    }
}

const BUILTIN_PROVIDER_LABEL: &str = "the built-in `static-rust` provider";

/// Literal paths wired directly in `apps/soma/src/routes.rs`, registered
/// on the same router as — and ahead of — the `/v1/{*path}` dynamic-provider
/// fallback. Axum resolves a request by *path* first; once the path matches
/// one of these, a method that path's own handler doesn't support gets a 405
/// from that route directly, not a fallthrough to the dynamic dispatcher. So
/// **any** method on one of these paths is shadowed, not just the specific
/// method Soma itself registers for it — a provider declaring `GET
/// /v1/greet` is exactly as unreachable as one declaring `POST /v1/greet`
/// despite Soma's own `/v1/greet` being a POST. `/v1/greet`, `/v1/echo`,
/// `/v1/status`, and `/v1/help` also have an `ACTION_SPECS` entry and so are
/// *additionally* reserved via the built-in `static-rust` catalog seed
/// above (for their specific action/method/path) — this list is what
/// reserves the path itself, method-independent, for all six.
const RESERVED_INFRASTRUCTURE_PATHS: &[&str] = &[
    "/v1/capabilities",
    "/v1/providers",
    "/v1/greet",
    "/v1/echo",
    "/v1/status",
    "/v1/help",
];

const INFRASTRUCTURE_ROUTE_LABEL: &str = "Soma's built-in HTTP infrastructure routes";

/// Tracks provider/action/REST-route/CLI-command/MCP-primitive names already
/// claimed in this directory (plus the built-in catalog), each mapped to a
/// human-readable label identifying the owner — a file name, or
/// [`BUILTIN_PROVIDER_LABEL`].
#[derive(Default)]
struct DirectoryNamespace {
    provider_names: HashMap<String, String>,
    action_names: HashMap<String, String>,
    rest_routes: HashMap<(String, String), String>,
    cli_commands: HashMap<String, String>,
    primitives: HashMap<String, String>,
    /// `catalog().resources[].uri_template`, owner — used to cross-check
    /// dynamic resource templates against exact/static resources, mirroring
    /// `provider_registry::ResourceIndex::register`'s cross-tier ambiguity
    /// check (a static exact resource and a zero-param dynamic template
    /// rendering to the same URI are just as ambiguous as two same-shape
    /// dynamic templates).
    exact_resource_uris: HashMap<String, String>,
    /// Every dynamic `.ts` resource reader's template registered so far,
    /// checked pairwise against each newly-discovered one.
    dynamic_templates: Vec<(ResourcePath, String)>,
}

impl DirectoryNamespace {
    /// Returns an error message for the first collision found, checking in
    /// the same order `provider_registry::{provider_map, build_snapshot}`
    /// does. Does not mutate — call `register` separately once the caller
    /// has decided this catalog is conflict-free.
    fn find_conflict(&self, catalog: &ProviderCatalog) -> Option<String> {
        if let Some(other) = self.provider_names.get(&catalog.provider.name) {
            return Some(conflict_message("provider", &catalog.provider.name, other));
        }
        for tool in &catalog.tools {
            if let Some(other) = self.action_names.get(&tool.name) {
                return Some(conflict_message("action", &tool.name, other));
            }
            if let Some(rest) = &tool.rest {
                if rest.enabled {
                    let key = rest_route_key(tool.name.as_str(), rest);
                    let label = format!("{} {}", key.0, key.1);
                    if is_shadowed_by_generic_tools_route(&key.1) {
                        return Some(conflict_message(
                            "REST route",
                            &label,
                            "Soma's built-in `/v1/tools/{action}` dispatch route",
                        ));
                    }
                    if RESERVED_INFRASTRUCTURE_PATHS.contains(&key.1.as_str()) {
                        return Some(conflict_message(
                            "REST route",
                            &label,
                            INFRASTRUCTURE_ROUTE_LABEL,
                        ));
                    }
                    if let Some(other) = self.rest_routes.get(&key) {
                        return Some(conflict_message("REST route", &label, other));
                    }
                }
            }
            if let Some(cli) = &tool.cli {
                if cli.enabled {
                    let command = cli_command(tool.name.as_str(), cli);
                    if let Some(other) = self.cli_commands.get(&command) {
                        return Some(conflict_message("CLI command", &command, other));
                    }
                    for alias in &cli.aliases {
                        if let Some(other) = self.cli_commands.get(alias) {
                            return Some(conflict_message("CLI alias", alias, other));
                        }
                    }
                }
            }
        }
        for name in primitive_names(catalog) {
            if let Some(other) = self.primitives.get(&name) {
                return Some(conflict_message("MCP primitive", &name, other));
            }
        }
        None
    }

    /// The dynamic-template counterpart to `find_conflict`: checks a
    /// candidate template against every dynamic template already
    /// registered (same overlap logic `ResourcePath::is_ambiguous_with`
    /// uses at real registry construction time) and against every exact
    /// resource URI already registered from a catalog (the cross-tier
    /// case).
    fn find_template_conflict(&self, template: &DynamicResourceTemplate) -> Option<String> {
        for (path, owner) in &self.dynamic_templates {
            if template.path.is_ambiguous_with(path) {
                return Some(conflict_message(
                    "resource template",
                    &template.uri_template(),
                    owner,
                ));
            }
        }
        for (uri, owner) in &self.exact_resource_uris {
            if let Some(exact_path) = literal_resource_path(uri) {
                if template.path.is_ambiguous_with(&exact_path) {
                    return Some(conflict_message(
                        "resource template",
                        &template.uri_template(),
                        owner,
                    ));
                }
            }
        }
        None
    }

    fn register_template(&mut self, template: &DynamicResourceTemplate, owner: &str) {
        self.dynamic_templates
            .push((template.path.clone(), owner.to_owned()));
    }

    fn register(&mut self, catalog: &ProviderCatalog, owner: &str) {
        self.provider_names
            .insert(catalog.provider.name.clone(), owner.to_owned());
        for resource in &catalog.resources {
            self.exact_resource_uris
                .insert(resource.uri_template.clone(), owner.to_owned());
        }
        for tool in &catalog.tools {
            self.action_names
                .insert(tool.name.clone(), owner.to_owned());
            if let Some(rest) = &tool.rest {
                if rest.enabled {
                    self.rest_routes
                        .insert(rest_route_key(tool.name.as_str(), rest), owner.to_owned());
                }
            }
            if let Some(cli) = &tool.cli {
                if cli.enabled {
                    self.cli_commands
                        .insert(cli_command(tool.name.as_str(), cli), owner.to_owned());
                    for alias in &cli.aliases {
                        self.cli_commands.insert(alias.clone(), owner.to_owned());
                    }
                }
            }
        }
        for name in primitive_names(catalog) {
            self.primitives.insert(name, owner.to_owned());
        }
    }
}

/// `/v1/tools/{action}` (`apps/soma/src/routes.rs`) is a wildcard route
/// matching exactly one path segment after `/v1/tools/` — a provider
/// declaring a literal path shaped like that is shadowed by it regardless of
/// what's registered where, so this isn't an exact-match reservation like
/// the others; it's checked as a pattern.
fn is_shadowed_by_generic_tools_route(path: &str) -> bool {
    path.strip_prefix("/v1/tools/")
        .is_some_and(|rest| !rest.is_empty() && !rest.contains('/'))
}

fn conflict_message(kind: &str, name: &str, owner: &str) -> String {
    format!("duplicate {kind} `{name}` (already claimed by {owner})")
}

fn rest_route_key(tool_name: &str, rest: &soma_provider_core::RestOverlay) -> (String, String) {
    let method = rest.method.clone().unwrap_or_else(|| "POST".to_owned());
    let path = rest
        .path
        .clone()
        .unwrap_or_else(|| format!("/v1/{tool_name}"));
    (method, path)
}

fn cli_command(tool_name: &str, cli: &soma_provider_core::CliOverlay) -> String {
    cli.command.clone().unwrap_or_else(|| tool_name.to_owned())
}

/// Parses an exact resource's `uri_template` string into an all-literal
/// `ResourcePath`, for reuse with `ResourcePath::is_ambiguous_with` when
/// comparing against a dynamic template — mirrors
/// `provider_registry::resources::literal_resource_path`, duplicated here
/// since that one is private to a different module tree. Returns `None` for
/// a URI that doesn't use the resource scheme at all.
fn literal_resource_path(uri: &str) -> Option<ResourcePath> {
    let segments = crate::providers::resource_uri::request_segments(uri)?;
    Some(ResourcePath {
        segments: segments
            .into_iter()
            .map(|segment| PathSegment::Literal(segment.to_owned()))
            .collect(),
    })
}

fn primitive_names(catalog: &ProviderCatalog) -> Vec<String> {
    catalog
        .prompts
        .iter()
        .map(|prompt| prompt.name.clone())
        .chain(
            catalog
                .resources
                .iter()
                .map(|resource| resource.name.clone()),
        )
        .chain(catalog.tasks.iter().map(|task| task.name.clone()))
        .chain(
            catalog
                .elicitation
                .iter()
                .map(|elicitation| elicitation.name.clone()),
        )
        .collect()
}

#[cfg(test)]
#[path = "filesystem_uniqueness_tests.rs"]
mod tests;
