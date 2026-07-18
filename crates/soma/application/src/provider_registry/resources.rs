//! MCP resource support: dynamic resource template types, `RegistrySnapshot`
//! resource matching, `ProviderRegistry::read_resource`, and the
//! `build_snapshot()` bookkeeping that registers each provider's static and
//! dynamic resources. Split out of `provider_registry.rs` to stay under the
//! module size hard limit — see `docs/contracts/drop-in-provider-layout.md`
//! for the contract this implements.

use std::collections::{BTreeMap, HashMap};

use soma_domain::{actions::scopes_satisfy, provider_validation::ProviderValidationError};
use soma_provider_core::ProviderResource;

use crate::{
    provider_errors::ProviderError,
    providers::resource_uri::{PathSegment, ResourcePath},
};

use super::{Provider, ProviderAuthMode, ProviderPrincipal, ProviderRegistry, RegistrySnapshot};

/// A dynamic resource template a provider can serve, beyond the exact
/// `uri_template` strings already listed in `catalog().resources` (which
/// cover only static, parameter-free resources). Populated by file-based
/// dynamic resource readers under `providers/resources/` — see
/// `providers::resource_files`.
#[derive(Debug, Clone)]
pub struct DynamicResourceTemplate {
    pub(crate) path: ResourcePath,
    pub name: String,
    pub description: String,
    pub mime_type: Option<String>,
    /// Scope required to invoke this template, enforced the same way
    /// `ProviderResource::scope` is enforced for static resources — see
    /// `ProviderRegistry::read_resource`. `None` means no scope beyond
    /// baseline `Mounted` authentication is required.
    pub scope: Option<String>,
}

impl DynamicResourceTemplate {
    /// The RFC 6570-flavored URI template string (e.g.
    /// `soma://resources/service/{name}`) advertised via MCP
    /// `resources/templates/list`. `ResourcePath` itself stays private to
    /// this crate — this is the only piece other crates need.
    pub fn uri_template(&self) -> String {
        self.path.uri_string()
    }

    /// Builds a template from raw path segments using the same bracket
    /// syntax as `providers/resources/*.ts` filenames (`[name]` for a
    /// parameter, `[...name]` for a catch-all — see
    /// `docs/contracts/drop-in-provider-layout.md`). The public constructor
    /// for any `Provider` (file-based or not) that wants to advertise a
    /// dynamic resource template, since `ResourcePath` itself is private.
    /// `scope` defaults to `None`; set the returned value's `scope` field
    /// directly to require one.
    pub fn from_path_segments(
        segments: &[&str],
        name: impl Into<String>,
        description: impl Into<String>,
        mime_type: Option<String>,
    ) -> Result<Self, String> {
        let path = crate::providers::resource_uri::parse_resource_path(segments)
            .map_err(|error| error.0)?;
        Ok(Self {
            path,
            name: name.into(),
            description: description.into(),
            mime_type,
            scope: None,
        })
    }
}

/// Content resolved for a matched resource, ready to become an MCP
/// `ResourceContents::text`/`::blob`.
#[derive(Debug)]
pub enum ResourceReadOutput {
    Text {
        text: String,
        mime_type: Option<String>,
    },
    Blob {
        blob_base64: String,
        mime_type: Option<String>,
    },
}

/// Per-snapshot resource indexes, built once in `build_snapshot()` and
/// consulted by `RegistrySnapshot::match_resource`.
pub(super) struct ResourceIndex {
    pub(super) exact: HashMap<String, (String, ProviderResource)>,
    pub(super) dynamic: Vec<(String, DynamicResourceTemplate)>,
}

impl ResourceIndex {
    pub(super) fn new() -> Self {
        Self {
            exact: HashMap::new(),
            dynamic: Vec::new(),
        }
    }

    /// Registers `catalog`'s static resources (duplicate-name checking is
    /// the caller's job via `insert_primitive`; this only tracks URI
    /// collisions, including against dynamic templates — a static exact
    /// resource and a zero-param dynamic template rendering to the same URI
    /// are just as ambiguous as two dynamic templates of the same shape,
    /// since the exact-match tier would silently and permanently shadow the
    /// dynamic one) and `provider`'s dynamic resource templates (ambiguity
    /// checked pairwise against everything already registered, in both
    /// tiers).
    ///
    /// A provider whose kind can't serve resource reads
    /// (`!provider.supports_resource_reads()`) has its declared resources
    /// skipped here entirely — not indexed for `resources/list` or
    /// `resources/read`, and not snapshot-validation errors either, since
    /// `RegistrySnapshot::inspection_report` reads `catalog().resources`
    /// directly (not through this index) and legitimately uses the field
    /// for documentation/reporting even when nothing can serve it live.
    /// This only prevents the previously-broken outcome: a resource that
    /// lists successfully but always fails to read.
    pub(super) fn register(
        &mut self,
        provider: &dyn Provider,
        provider_name: &str,
        resources: &[ProviderResource],
    ) -> Result<(), ProviderValidationError> {
        if !provider.supports_resource_reads() {
            return Ok(());
        }

        for template in provider.dynamic_resource_templates() {
            for (owner, other) in &self.dynamic {
                if template.path.is_ambiguous_with(&other.path) {
                    return Err(ProviderValidationError::new(
                        "ambiguous_resource_template",
                        format!(
                            "resource template `{}` (provider `{provider_name}`) is ambiguous with `{}` (provider `{owner}`)",
                            template.path.uri_string(),
                            other.path.uri_string(),
                        ),
                    ));
                }
            }
            for (uri, (owner, _)) in &self.exact {
                if let Some(exact_path) = literal_resource_path(uri) {
                    if template.path.is_ambiguous_with(&exact_path) {
                        return Err(ProviderValidationError::new(
                            "ambiguous_resource_template",
                            format!(
                                "resource template `{}` (provider `{provider_name}`) is ambiguous with exact resource `{uri}` (provider `{owner}`)",
                                template.path.uri_string(),
                            ),
                        ));
                    }
                }
            }
            self.dynamic.push((provider_name.to_owned(), template));
        }

        for resource in resources {
            if !resource.mcp.as_ref().map(|mcp| mcp.enabled).unwrap_or(true) {
                // `mcp: { enabled: false }` opts a resource out of the MCP
                // surface (matching how tools/prompts honor the same
                // overlay) — never index it for live `resources/list` or
                // `resources/read`. Reporting/remote-catalog code still
                // preserves the raw field via `catalog().resources`.
                continue;
            }
            if let Some(exact_path) = literal_resource_path(&resource.uri_template) {
                for (owner, other) in &self.dynamic {
                    if exact_path.is_ambiguous_with(&other.path) {
                        return Err(ProviderValidationError::new(
                            "ambiguous_resource_template",
                            format!(
                                "resource `{}` (provider `{provider_name}`) is ambiguous with dynamic template `{}` (provider `{owner}`)",
                                resource.uri_template,
                                other.path.uri_string(),
                            ),
                        ));
                    }
                }
            }
            if let Some((owner, _)) = self.exact.insert(
                resource.uri_template.clone(),
                (provider_name.to_owned(), resource.clone()),
            ) {
                return Err(ProviderValidationError::new(
                    "duplicate_resource_uri",
                    format!(
                        "duplicate resource URI `{}` (already claimed by provider `{owner}`)",
                        resource.uri_template
                    ),
                ));
            }
        }
        Ok(())
    }
}

/// Parses an exact resource's `uri_template` string into an all-literal
/// `ResourcePath`, for reuse with `ResourcePath::is_ambiguous_with` when
/// comparing against dynamic templates. Returns `None` for a URI that
/// doesn't use the resource scheme at all (never ambiguous with a resource
/// template either way).
fn literal_resource_path(uri: &str) -> Option<ResourcePath> {
    let segments = crate::providers::resource_uri::request_segments(uri)?;
    Some(ResourcePath {
        segments: segments
            .into_iter()
            .map(|segment| PathSegment::Literal(segment.to_owned()))
            .collect(),
    })
}

impl RegistrySnapshot {
    /// Every provider's dynamic resource templates, for `resources/templates/list`.
    pub fn dynamic_resource_templates(&self) -> &[(String, DynamicResourceTemplate)] {
        &self.dynamic_resources
    }

    /// The live, MCP-visible, readable exact resources — i.e. exactly the
    /// set `resources/read` can actually resolve. Already excludes
    /// resources from providers that can't serve reads
    /// (`!supports_resource_reads()`) and resources explicitly disabled via
    /// `mcp: { enabled: false }`, since both are filtered out before
    /// insertion in `ResourceIndex::register`. Use this for
    /// `resources/list` instead of walking raw `catalogs` directly, or the
    /// list can advertise resources that always fail to read.
    pub fn exact_resources(&self) -> impl Iterator<Item = &ProviderResource> {
        self.exact_resources.values().map(|(_, resource)| resource)
    }

    /// Resolves a request resource URI against exact resources first, then
    /// dynamic templates in precedence order (exact-dynamic before
    /// parameterized before catch-all — enforced by trying shorter/no-param
    /// matches first via `is_dynamic()`), returning the owning provider
    /// name, captured params, and the required scope (if any), regardless
    /// of which tier matched.
    pub fn match_resource(
        &self,
        uri: &str,
    ) -> Option<(&str, BTreeMap<String, String>, Option<&str>)> {
        if let Some((provider, resource)) = self.exact_resources.get(uri) {
            return Some((
                provider.as_str(),
                BTreeMap::new(),
                resource.scope.as_deref(),
            ));
        }
        let segments: Vec<&str> = crate::providers::resource_uri::request_segments(uri)?;
        // Exact-dynamic (zero-param) templates before parameterized before
        // catch-all: `is_dynamic()` is false only for exact matches (no
        // Param/CatchAll segments), so sorting by that flag first gives the
        // right precedence without a separate tier enum.
        let mut candidates: Vec<&(String, DynamicResourceTemplate)> =
            self.dynamic_resources.iter().collect();
        candidates.sort_by_key(|(_, template)| {
            (
                template.path.is_dynamic(),
                matches!(
                    template.path.segments.last(),
                    Some(PathSegment::CatchAll(_))
                ),
            )
        });
        for (provider, template) in candidates {
            if let Some(params) = template.path.match_segments(&segments) {
                return Some((provider.as_str(), params, template.scope.as_deref()));
            }
        }
        None
    }
}

impl ProviderRegistry {
    /// Matches `uri` against the active snapshot's exact resources and
    /// dynamic resource templates (in that precedence order), enforces
    /// `resource.scope` under `ProviderAuthMode::Mounted` the same way
    /// `dispatch()` enforces `tool.scope`, then delegates to the owning
    /// provider's `read_resource`.
    ///
    /// The match and the provider clone are fetched from the same read
    /// lock acquisition, mirroring `dispatch()`'s pattern for tools — a
    /// concurrent `refresh_file_providers()` between two separate lock
    /// acquisitions could otherwise return params/scope from one snapshot
    /// but a provider instance from a newer one (e.g. a hot-swapped
    /// `resources/foo.md` -> `resources/foo.ts` letting a request matched
    /// against the old unscoped static resource invoke the new
    /// `soma:write`-scoped dynamic reader without ever being checked
    /// against its scope).
    pub async fn read_resource(
        &self,
        uri: &str,
        principal: &ProviderPrincipal,
        auth_mode: ProviderAuthMode,
    ) -> Result<ResourceReadOutput, ProviderError> {
        let (provider_name, params, resource_scope, provider) = {
            let state = self
                .state
                .read()
                .expect("provider registry lock should not be poisoned");
            let Some((provider_name, params, scope)) = state.snapshot.match_resource(uri) else {
                return Err(ProviderError::validation(
                    "registry",
                    uri,
                    "unknown_resource",
                    format!("unknown resource `{uri}`"),
                ));
            };
            let provider_name = provider_name.to_owned();
            let provider = state.providers.get(&provider_name).cloned();
            (
                provider_name,
                params,
                scope.map(ToOwned::to_owned),
                provider,
            )
        };

        if matches!(auth_mode, ProviderAuthMode::Mounted) {
            if let Some(scope) = resource_scope.as_deref() {
                if !scopes_satisfy(&principal.scopes, scope) {
                    return Err(ProviderError::new(
                        "insufficient_scope",
                        provider_name.clone(),
                        None,
                        format!("resource `{uri}` requires scope `{scope}`"),
                        "Authenticate with a token that includes the required scope.",
                    ));
                }
            }
        }

        let Some(provider) = provider else {
            return Err(ProviderError::new(
                "provider_not_loaded",
                provider_name,
                None,
                "provider is not loaded in the active registry",
                "Reload providers and retry.",
            ));
        };
        provider.read_resource(uri, &params).await
    }
}

#[cfg(test)]
#[path = "resources_tests.rs"]
mod tests;
