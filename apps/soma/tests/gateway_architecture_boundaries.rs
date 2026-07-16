//! Gateway dependency boundaries preserved from the self-contained gateway landing.
//!
//! CLAUDE.md says the MCP and CLI shims (`tools.rs`, `cli/lib.rs`) hold zero
//! business logic and reach the service layer (`SomaService` /
//! `dispatch_action`), never the transport client (`SomaClient`) or raw
//! HTTP. These tests read the shim source and fail if that boundary is crossed,
//! so the rule is enforced by CI instead of by reviewer vigilance.
//!
//! The checks are deliberately textual and conservative: they target import and
//! call-site forms (`use … SomaClient`, `SomaClient::`, `reqwest`) so a
//! mention inside a doc comment or help string is not a false positive.

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const GATEWAY_FORBIDDEN_DIRECT: &[&str] = &[
    "soma",
    "soma-api",
    "soma-cli",
    "soma-contracts",
    "soma-mcp",
    "soma-runtime",
    "soma-service",
];

const GATEWAY_OPTIONAL_LEAF_SOMA_DEPS: &[&str] = &[
    "soma-auth",
    "soma-codemode",
    "soma-mcp-client",
    "soma-mcp-proxy",
    "soma-openapi",
];

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is `crates/soma`; the workspace root is two up.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root is two levels above the crate manifest")
        .to_path_buf()
}

fn read_shim(relative: &str) -> String {
    let path = workspace_root().join(relative);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
}

fn imports_symbol(src: &str, symbol: &str) -> bool {
    src.lines()
        .map(str::trim_start)
        .filter(|line| line.starts_with("use "))
        .any(|line| line.contains(symbol))
}

#[test]
fn mcp_tools_shim_does_not_touch_the_transport_client() {
    let src = read_shim("crates/soma/mcp/src/tools.rs");
    assert!(
        !imports_symbol(&src, "SomaClient"),
        "tools.rs must dispatch through SomaService, never import SomaClient (thin-shim rule)"
    );
    assert!(
        !src.contains("SomaClient::"),
        "tools.rs must not construct or call SomaClient directly; go through the service layer"
    );
    assert!(
        !src.contains("reqwest"),
        "tools.rs must not perform transport/HTTP work; that belongs in soma.rs (the client)"
    );
}

#[test]
fn cli_shim_does_not_perform_transport_work() {
    let src = read_shim("crates/soma/cli/src/lib.rs");
    // The CLI may construct the client (composition root) but must not do HTTP.
    assert!(
        !src.contains("reqwest"),
        "cli/lib.rs must not perform transport/HTTP work; it wires the service and dispatches only"
    );
    for forbidden in [
        "fn provider_validation_summary",
        "fn provider_inspection",
        "fn provider_runtime_security",
    ] {
        assert!(
            !src.contains(forbidden),
            "cli/lib.rs must not own provider report domain logic ({forbidden}); use soma-service"
        );
    }
}

#[test]
fn mcp_tools_shim_reaches_the_shared_service_seam() {
    let src = read_shim("crates/soma/mcp/src/tools.rs");
    assert!(
        src.contains("dispatch_action") || imports_symbol(&src, "SomaService"),
        "tools.rs should reach the service layer via dispatch_action / SomaService"
    );
}

#[test]
fn soma_gateway_has_no_forbidden_direct_dependencies() {
    let metadata = cargo_metadata();
    let package = package_by_name(&metadata, "soma-gateway");
    let direct_dependencies = package["dependencies"]
        .as_array()
        .expect("dependencies must be an array");

    for dependency in direct_dependencies {
        let name = dependency["name"].as_str().expect("dependency name");
        assert!(
            !name.starts_with("labby-"),
            "soma-gateway must not depend on Labby crate {name}"
        );
        assert!(
            !GATEWAY_FORBIDDEN_DIRECT.contains(&name),
            "soma-gateway must not depend on forbidden Soma crate {name}"
        );
        if name.starts_with("soma-") {
            assert!(
                GATEWAY_OPTIONAL_LEAF_SOMA_DEPS.contains(&name),
                "unexpected internal Soma dependency {name}"
            );
        }
    }
}

#[test]
fn soma_gateway_resolved_graph_has_no_labby_or_product_crates() {
    let metadata = cargo_metadata();
    let packages = package_names_by_id(&metadata);
    let gateway_id = package_id_by_name(&metadata, "soma-gateway");
    let closure = resolved_dependency_closure(&metadata, &gateway_id);

    for package_id in closure {
        let name = packages
            .get(&package_id)
            .unwrap_or_else(|| panic!("missing package id {package_id}"));
        assert!(
            !name.starts_with("labby-"),
            "soma-gateway resolved graph includes Labby crate {name}"
        );
        assert!(
            !GATEWAY_FORBIDDEN_DIRECT.contains(&name.as_str()),
            "soma-gateway resolved graph includes forbidden crate {name}"
        );
    }
}

fn cargo_metadata() -> Value {
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--all-features"])
        .current_dir(workspace_root())
        .output()
        .expect("cargo metadata should run");
    assert!(
        output.status.success(),
        "cargo metadata failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("cargo metadata must be json")
}

fn package_by_name<'a>(metadata: &'a Value, name: &str) -> &'a Value {
    metadata["packages"]
        .as_array()
        .expect("packages must be an array")
        .iter()
        .find(|package| package["name"] == name)
        .unwrap_or_else(|| panic!("package {name} not found"))
}

fn package_id_by_name(metadata: &Value, name: &str) -> String {
    package_by_name(metadata, name)["id"]
        .as_str()
        .expect("package id")
        .to_owned()
}

fn package_names_by_id(metadata: &Value) -> BTreeMap<String, String> {
    metadata["packages"]
        .as_array()
        .expect("packages must be an array")
        .iter()
        .map(|package| {
            (
                package["id"].as_str().expect("package id").to_owned(),
                package["name"].as_str().expect("package name").to_owned(),
            )
        })
        .collect()
}

fn resolved_dependency_closure(metadata: &Value, root_id: &str) -> BTreeSet<String> {
    let nodes = metadata["resolve"]["nodes"]
        .as_array()
        .expect("resolve nodes must be an array");
    let deps_by_id: BTreeMap<String, Vec<String>> = nodes
        .iter()
        .map(|node| {
            let id = node["id"].as_str().expect("node id").to_owned();
            let deps = node["deps"]
                .as_array()
                .expect("node deps must be an array")
                .iter()
                .map(|dep| dep["pkg"].as_str().expect("dep package id").to_owned())
                .collect();
            (id, deps)
        })
        .collect();

    let mut seen = BTreeSet::new();
    let mut stack = deps_by_id
        .get(root_id)
        .unwrap_or_else(|| panic!("resolve node for {root_id} not found"))
        .clone();
    while let Some(package_id) = stack.pop() {
        if !seen.insert(package_id.clone()) {
            continue;
        }
        if let Some(deps) = deps_by_id.get(&package_id) {
            stack.extend(deps.iter().cloned());
        }
    }
    seen
}
