use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub(crate) struct Graph {
    pub(crate) packages: BTreeMap<String, Package>,
    pub(crate) edges: Vec<Edge>,
    pub(crate) edges_by_from: BTreeMap<String, Vec<usize>>,
}

impl Graph {
    pub(crate) fn from_metadata(root: &Path, metadata: &Value) -> Result<Self> {
        let mut packages = BTreeMap::new();
        let mut path_to_id = BTreeMap::new();
        let package_values = metadata
            .get("packages")
            .and_then(Value::as_array)
            .context("metadata has packages")?;
        let workspace_members = workspace_members(metadata)?;

        for value in package_values.iter().filter(|p| {
            p.get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| workspace_members.contains(id))
        }) {
            let id = text(value, "id")?.to_owned();
            let name = text(value, "name")?.to_owned();
            let rel_path = package_rel_path(root, Path::new(text(value, "manifest_path")?))?;
            let layer = Layer::from_path(&rel_path)
                .with_context(|| format!("{name} has unsupported architecture path {rel_path}"))?;
            let metadata_layer = metadata_layer(value)?;
            path_to_id.insert(rel_path.clone(), id.clone());
            packages.insert(
                id.clone(),
                Package {
                    id,
                    name,
                    rel_path,
                    layer,
                    metadata_layer,
                },
            );
        }

        let mut graph = Self {
            packages,
            edges: Vec::new(),
            edges_by_from: BTreeMap::new(),
        };
        graph.collect_edges(root, package_values, &workspace_members, &path_to_id)?;
        Ok(graph)
    }

    pub(crate) fn package(&self, id: &str) -> &Package {
        self.packages.get(id).expect("edge references package")
    }

    pub(crate) fn direct_dependencies_except(
        &self,
        id: &str,
        exceptions: &[crate::architecture::ArchitectureException],
    ) -> Vec<&Package> {
        self.edges_by_from
            .get(id)
            .into_iter()
            .flat_map(|edges| edges.iter())
            .map(|edge| &self.edges[*edge])
            .filter(|edge| {
                !exceptions
                    .iter()
                    .any(|exception| exception.matches(self, edge))
            })
            .filter_map(|edge| self.packages.get(&edge.to))
            .collect()
    }

    pub(crate) fn edge_label(&self, edge: &Edge) -> String {
        let from = self.package(&edge.from);
        let to = self.package(&edge.to);
        let optional = if edge.optional { " optional" } else { "" };
        format!("{} --{} {}--> {}", from.name, optional, edge.kind, to.name)
    }

    fn collect_edges(
        &mut self,
        root: &Path,
        package_values: &[Value],
        workspace_members: &BTreeSet<String>,
        path_to_id: &BTreeMap<String, String>,
    ) -> Result<()> {
        for value in package_values.iter().filter(|p| {
            p.get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| workspace_members.contains(id))
        }) {
            let from = text(value, "id")?;
            for dependency in value
                .get("dependencies")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let kind = dependency_kind(dependency);
                if kind != "normal" {
                    continue;
                }
                let Some(path) = dependency.get("path").and_then(Value::as_str) else {
                    continue;
                };
                let Ok(rel_path) = rel_slash(root, Path::new(path)) else {
                    continue;
                };
                let Some(to) = path_to_id.get(&rel_path).cloned() else {
                    continue;
                };
                self.push_edge(Edge {
                    from: from.to_owned(),
                    to,
                    kind: kind.to_owned(),
                    optional: dependency
                        .get("optional")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                });
            }
        }
        Ok(())
    }

    fn push_edge(&mut self, edge: Edge) {
        let index = self.edges.len();
        self.edges_by_from
            .entry(edge.from.clone())
            .or_default()
            .push(index);
        self.edges.push(edge);
    }
}

pub(crate) struct Package {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) rel_path: String,
    pub(crate) layer: Layer,
    pub(crate) metadata_layer: Option<Layer>,
}

pub(crate) struct Edge {
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) kind: String,
    pub(crate) optional: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Layer {
    App,
    Shared,
    Vendor,
    ProductDomain,
    ProductApplication,
    ProductIntegration,
    ProductRuntime,
    ProductSurface,
    ProductSupport,
    Legacy,
}

impl Layer {
    fn from_path(path: &str) -> Option<Self> {
        match path {
            "apps/soma" => Some(Self::App),
            "crates/soma/domain" => Some(Self::ProductDomain),
            "crates/soma/application" => Some(Self::ProductApplication),
            "crates/soma/integrations" => Some(Self::ProductIntegration),
            "crates/soma/runtime" => Some(Self::ProductRuntime),
            "crates/soma/api"
            | "crates/soma/cli"
            | "crates/soma/mcp"
            | "crates/soma/web"
            | "crates/soma/palette" => Some(Self::ProductSurface),
            "crates/soma/test-support" => Some(Self::ProductSupport),
            "crates/soma/contracts" | "crates/soma/service" | "xtask" => Some(Self::Legacy),
            path if path.starts_with("crates/shared/") => Some(Self::Shared),
            path if path.starts_with("crates/integrations/") => Some(Self::Vendor),
            path if path.starts_with("crates/soma/") => Some(Self::Legacy),
            _ => None,
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "app" => Some(Self::App),
            "shared" => Some(Self::Shared),
            "vendor" => Some(Self::Vendor),
            "product-domain" => Some(Self::ProductDomain),
            "product-application" => Some(Self::ProductApplication),
            "product-integration" => Some(Self::ProductIntegration),
            "product-runtime" => Some(Self::ProductRuntime),
            "product-surface" => Some(Self::ProductSurface),
            "product-support" => Some(Self::ProductSupport),
            "legacy" => Some(Self::Legacy),
            _ => None,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::App => "app",
            Self::Shared => "shared",
            Self::Vendor => "vendor",
            Self::ProductDomain => "product-domain",
            Self::ProductApplication => "product-application",
            Self::ProductIntegration => "product-integration",
            Self::ProductRuntime => "product-runtime",
            Self::ProductSurface => "product-surface",
            Self::ProductSupport => "product-support",
            Self::Legacy => "legacy",
        }
    }
}

fn metadata_layer(package: &Value) -> Result<Option<Layer>> {
    let Some(value) = package
        .pointer("/metadata/soma-architecture/layer")
        .and_then(Value::as_str)
    else {
        return Ok(None);
    };
    Layer::parse(value)
        .with_context(|| format!("unknown architecture layer {value:?}"))
        .map(Some)
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .with_context(|| format!("package is missing string field {key:?}"))
}

fn workspace_members(metadata: &Value) -> Result<BTreeSet<String>> {
    metadata
        .get("workspace_members")
        .and_then(Value::as_array)
        .context("metadata has workspace_members")?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .context("workspace member id is a string")
        })
        .collect()
}

fn dependency_kind(dependency: &Value) -> &str {
    dependency
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("normal")
}

fn package_rel_path(root: &Path, manifest_path: &Path) -> Result<String> {
    let package_root = manifest_path
        .parent()
        .context("manifest path has no parent directory")?;
    rel_slash(root, package_root)
}

fn rel_slash(root: &Path, path: &Path) -> Result<String> {
    let rel = path
        .strip_prefix(root)
        .with_context(|| format!("{} is outside {}", path.display(), root.display()))?;
    Ok(rel
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/"))
}

#[cfg(test)]
#[path = "architecture_graph_tests.rs"]
mod tests;
