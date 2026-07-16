use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

pub(crate) use crate::architecture_graph::{Edge, Graph, Layer, Package};

const APPLICATION_PORT_PATHS: &[&str] = &["crates/soma/application", "crates/soma/service"];
const CONCRETE_SHARED_ENGINE_PATHS: &[&str] = &[
    "crates/shared/codemode",
    "crates/shared/mcp/gateway",
    "crates/shared/openapi",
];

const TEMPORARY_EXCEPTIONS: &[ArchitectureException] = &[
    ArchitectureException {
        from_path: "crates/soma/api",
        to_path: "crates/shared/mcp/gateway",
        owner: "architecture-refactor",
        reason: "REST still composes the legacy service and shared gateway during migration",
        removal_pr: "PR 6",
        expiration_milestone: "REST migration to SomaApplication",
    },
    ArchitectureException {
        from_path: "crates/soma/mcp",
        to_path: "crates/shared/mcp/gateway",
        owner: "architecture-refactor",
        reason: "MCP still composes the legacy service and shared gateway during migration",
        removal_pr: "PR 7",
        expiration_milestone: "MCP migration to SomaApplication",
    },
    ArchitectureException {
        from_path: "crates/soma/runtime",
        to_path: "crates/shared/mcp/gateway",
        owner: "architecture-refactor",
        reason: "runtime still composes the legacy service and shared gateway during migration",
        removal_pr: "PR 8",
        expiration_milestone: "runtime migration to SomaApplication",
    },
];

#[derive(Debug)]
pub(crate) struct ArchitectureException {
    from_path: &'static str,
    to_path: &'static str,
    owner: &'static str,
    reason: &'static str,
    removal_pr: &'static str,
    expiration_milestone: &'static str,
}

pub fn check(root: &Path) -> Result<()> {
    let root = root
        .canonicalize()
        .with_context(|| format!("failed to canonicalize {}", root.display()))?;
    let metadata = cargo_metadata(&root)?;
    let graph = Graph::from_metadata(&root, &metadata)?;
    let failures = check_graph(&graph, TEMPORARY_EXCEPTIONS);

    if failures.is_empty() {
        println!(
            "Architecture check passed ({} workspace packages, {} internal edges).",
            graph.packages.len(),
            graph.edges.len()
        );
        return Ok(());
    }

    eprintln!("Architecture check failed:");
    for failure in &failures {
        eprintln!("\n{failure}");
    }
    bail!("architecture boundary check failed")
}

fn cargo_metadata(root: impl AsRef<Path>) -> Result<Value> {
    let output = Command::new("cargo")
        .args([
            "metadata",
            "--locked",
            "--all-features",
            "--no-deps",
            "--format-version",
            "1",
        ])
        .current_dir(root.as_ref())
        .output()
        .context("failed to run cargo metadata")?;
    if !output.status.success() {
        bail!(
            "cargo metadata failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    serde_json::from_slice(&output.stdout).context("cargo metadata emitted invalid JSON")
}

fn check_graph(graph: &Graph, exceptions: &[ArchitectureException]) -> Vec<String> {
    let mut failures = Vec::new();
    failures.extend(check_metadata_layers(graph));
    failures.extend(check_exception_integrity(graph, exceptions));
    failures.extend(check_direct_edges(graph, exceptions));
    failures.extend(check_internal_cycles(graph));
    failures
}

fn check_metadata_layers(graph: &Graph) -> Vec<String> {
    graph
        .packages
        .values()
        .filter_map(|package| match package.metadata_layer {
            None => Some(format!(
                "{} ({}) is missing [package.metadata.soma-architecture] layer = {:?}",
                package.name,
                package.rel_path,
                package.layer.as_str()
            )),
            Some(layer) if layer != package.layer => Some(format!(
                "{} ({}) declares architecture layer {:?}, but its path requires {:?}",
                package.name,
                package.rel_path,
                layer.as_str(),
                package.layer.as_str()
            )),
            _ => None,
        })
        .collect()
}

fn check_direct_edges(graph: &Graph, exceptions: &[ArchitectureException]) -> Vec<String> {
    let mut failures = Vec::new();
    for edge in graph.edges.iter().filter(|edge| edge.from != edge.to) {
        if is_exception(graph, edge, exceptions) {
            continue;
        }
        let from = graph.package(&edge.from);
        let to = graph.package(&edge.to);
        if from.layer == Layer::Shared && to.layer != Layer::Shared {
            failures.push(format!(
                "shared package {} ({}) depends on non-shared package {} ({})\n  edge: {}",
                from.name,
                from.rel_path,
                to.name,
                to.rel_path,
                graph.edge_label(edge)
            ));
        }
        failures.extend(check_layer_edge(graph, edge, from, to));
    }
    failures.extend(check_mixed_application_and_engine_edges(graph, exceptions));
    failures
}

fn check_layer_edge(graph: &Graph, edge: &Edge, from: &Package, to: &Package) -> Vec<String> {
    let mut failures = Vec::new();
    if from.layer == Layer::ProductDomain
        && !matches!(to.layer, Layer::Shared | Layer::ProductDomain)
    {
        failures.push(format!(
            "product-domain packages must not depend outward to {}\n  edge: {}",
            to.layer.as_str(),
            graph.edge_label(edge)
        ));
    }

    if from.layer == Layer::ProductApplication
        && (matches!(
            to.layer,
            Layer::App
                | Layer::Legacy
                | Layer::ProductIntegration
                | Layer::ProductRuntime
                | Layer::ProductSurface
                | Layer::ProductSupport
        ) || is_concrete_shared_engine(to))
    {
        failures.push(format!(
            "product-application packages must not depend on app/legacy/integration/runtime/surface/support/concrete engines without a live exception\n  edge: {}",
            graph.edge_label(edge)
        ));
    }

    if from.layer == Layer::ProductSurface && to.layer == Layer::ProductSurface {
        failures.push(format!(
            "product-surface packages must not depend on one another\n  edge: {}",
            graph.edge_label(edge)
        ));
    }
    failures
}

fn check_mixed_application_and_engine_edges(
    graph: &Graph,
    exceptions: &[ArchitectureException],
) -> Vec<String> {
    graph
        .packages
        .values()
        .filter_map(|package| {
            let deps = graph.direct_dependencies_except(&package.id, exceptions);
            let has_application_port = deps.iter().any(|package| is_application_port(package));
            let has_concrete_engine = deps.iter().any(|package| is_concrete_shared_engine(package));
            (has_application_port
                && has_concrete_engine
                && !matches!(package.layer, Layer::App | Layer::ProductIntegration))
            .then(|| {
                format!(
                    "{} ({}) depends on both product application ports and concrete shared engines; move that bridge to apps/soma or crates/soma/integrations",
                    package.name, package.rel_path
                )
            })
        })
        .collect()
}

fn check_exception_integrity(graph: &Graph, exceptions: &[ArchitectureException]) -> Vec<String> {
    exceptions
        .iter()
        .filter_map(|exception| exception_integrity_failure(graph, exception))
        .collect()
}

fn exception_integrity_failure(graph: &Graph, exception: &ArchitectureException) -> Option<String> {
    if exception.owner.is_empty()
        || exception.reason.is_empty()
        || exception.removal_pr.is_empty()
        || exception.expiration_milestone.is_empty()
    {
        return Some(format!(
            "architecture exception {} -> {} is missing owner, reason, removal PR, or expiration milestone",
            exception.from_path, exception.to_path
        ));
    }

    let matches = graph
        .edges
        .iter()
        .filter(|edge| exception.matches(graph, edge))
        .count();
    match matches {
        1 => None,
        0 => Some(format!(
            "architecture exception {} -> {} does not match a current normal workspace edge; remove stale exceptions or add the edge with its owning PR",
            exception.from_path, exception.to_path
        )),
        _ => Some(format!(
            "architecture exception {} -> {} matches {matches} edges; exceptions must identify one current edge",
            exception.from_path, exception.to_path
        )),
    }
}

fn check_internal_cycles(graph: &Graph) -> Vec<String> {
    find_cycle(graph)
        .map(|cycle| {
            vec![format!(
                "internal dependency cycle detected: {}",
                cycle.join(" -> ")
            )]
        })
        .unwrap_or_default()
}

fn find_cycle(graph: &Graph) -> Option<Vec<String>> {
    let mut states = BTreeMap::new();
    let mut stack = Vec::new();
    for id in graph.packages.keys() {
        if states.get(id).copied().unwrap_or(0) == 0 {
            if let Some(cycle) = visit_cycle(graph, id, &mut states, &mut stack) {
                return Some(cycle);
            }
        }
    }
    None
}

fn visit_cycle(
    graph: &Graph,
    id: &str,
    states: &mut BTreeMap<String, u8>,
    stack: &mut Vec<String>,
) -> Option<Vec<String>> {
    states.insert(id.to_owned(), 1);
    stack.push(id.to_owned());
    for edge_index in graph.edges_by_from.get(id).into_iter().flatten() {
        let edge = &graph.edges[*edge_index];
        if edge.from == edge.to {
            continue;
        }
        match states.get(&edge.to).copied().unwrap_or(0) {
            0 => {
                if let Some(cycle) = visit_cycle(graph, &edge.to, states, stack) {
                    return Some(cycle);
                }
            }
            1 => return Some(cycle_names(graph, stack, &edge.to)),
            _ => {}
        }
    }
    stack.pop();
    states.insert(id.to_owned(), 2);
    None
}

fn cycle_names(graph: &Graph, stack: &[String], repeated: &str) -> Vec<String> {
    let start = stack.iter().position(|node| node == repeated).unwrap_or(0);
    let mut cycle: Vec<String> = stack[start..]
        .iter()
        .map(|node| graph.package(node).name.clone())
        .collect();
    cycle.push(graph.package(repeated).name.clone());
    cycle
}

fn is_exception(graph: &Graph, edge: &Edge, exceptions: &[ArchitectureException]) -> bool {
    exceptions
        .iter()
        .any(|exception| exception.matches(graph, edge))
}

impl ArchitectureException {
    pub(crate) fn matches(&self, graph: &Graph, edge: &Edge) -> bool {
        let from = &graph.package(&edge.from).rel_path;
        let to = &graph.package(&edge.to).rel_path;
        self.from_path == from && self.to_path == to && edge.kind == "normal"
    }
}

fn is_application_port(package: &Package) -> bool {
    package.layer == Layer::ProductApplication
        || APPLICATION_PORT_PATHS.contains(&package.rel_path.as_str())
}

fn is_concrete_shared_engine(package: &Package) -> bool {
    CONCRETE_SHARED_ENGINE_PATHS.contains(&package.rel_path.as_str())
}

#[cfg(test)]
#[path = "architecture_tests.rs"]
mod tests;
