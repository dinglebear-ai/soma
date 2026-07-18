use super::*;
use serde_json::json;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from("/repo")
}

fn pkg(name: &str, rel: &str, layer: &str, deps: Vec<Value>) -> Value {
    json!({
        "id": name,
        "name": name,
        "source": null,
        "manifest_path": format!("/repo/{rel}/Cargo.toml"),
        "metadata": {
            "soma-architecture": {
                "layer": layer
            }
        },
        "dependencies": deps
    })
}

fn dep(name: &str, rel: &str) -> Value {
    dep_with(name, rel, "normal", false)
}

fn dep_with(name: &str, rel: &str, kind: &str, optional: bool) -> Value {
    json!({
        "name": name,
        "path": format!("/repo/{rel}"),
        "kind": if kind == "normal" { Value::Null } else { Value::String(kind.to_owned()) },
        "optional": optional
    })
}

fn graph(packages: Vec<Value>) -> Graph {
    let workspace_members: Vec<Value> = packages
        .iter()
        .map(|package| package["id"].clone())
        .collect();
    let metadata = json!({ "packages": packages, "workspace_members": workspace_members });
    Graph::from_metadata(&root(), &metadata).expect("synthetic metadata graph")
}

fn failures(packages: Vec<Value>) -> Vec<String> {
    check_graph(&graph(packages), &[])
}

#[test]
fn valid_shared_only_graph_passes() {
    let failures = failures(vec![
        pkg("soma-auth", "crates/shared/auth", "shared", vec![]),
        pkg(
            "soma-gateway",
            "crates/shared/mcp/gateway",
            "shared",
            vec![dep("soma-auth", "crates/shared/auth")],
        ),
        pkg("soma-api", "crates/soma/api", "product-surface", vec![]),
        pkg(
            "soma",
            "apps/soma",
            "app",
            vec![dep("soma-api", "crates/soma/api")],
        ),
    ]);

    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn shared_optional_dependency_on_product_fails() {
    let failures = failures(vec![
        pkg(
            "soma-gateway",
            "crates/shared/mcp/gateway",
            "shared",
            vec![dep_with(
                "soma-contracts",
                "crates/soma/contracts",
                "normal",
                true,
            )],
        ),
        pkg("soma-contracts", "crates/soma/contracts", "legacy", vec![]),
    ]);

    let report = failures.join("\n");
    assert!(report.contains("shared package soma-gateway"));
    assert!(report.contains("optional normal"));
}

#[test]
fn soma_mcp_cannot_depend_directly_on_legacy_or_gateway_engines() {
    for (name, path, layer) in [
        ("soma-runtime", "crates/soma/runtime", "product-runtime"),
        ("soma-gateway", "crates/shared/mcp/gateway", "shared"),
    ] {
        let failures = failures(vec![
            pkg(
                "soma-mcp",
                "crates/soma/mcp",
                "product-surface",
                vec![dep(name, path)],
            ),
            pkg(name, path, layer, vec![]),
        ]);

        assert!(
            failures
                .join("\n")
                .contains("soma-mcp must depend on SomaApplication ports"),
            "expected direct edge to {name} to fail: {failures:#?}"
        );
    }
}

#[test]
fn dev_and_build_dependencies_do_not_create_production_edges() {
    let failures = failures(vec![
        pkg(
            "soma-gateway",
            "crates/shared/mcp/gateway",
            "shared",
            vec![
                dep_with("soma-contracts", "crates/soma/contracts", "dev", false),
                dep_with("soma-service", "crates/soma/service", "build", false),
            ],
        ),
        pkg("soma-contracts", "crates/soma/contracts", "legacy", vec![]),
        pkg("soma-service", "crates/soma/service", "legacy", vec![]),
    ]);

    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn shared_chain_fails_at_first_non_shared_boundary_edge() {
    let failures = failures(vec![
        pkg(
            "soma-gateway",
            "crates/shared/mcp/gateway",
            "shared",
            vec![dep("soma-codemode", "crates/shared/codemode")],
        ),
        pkg(
            "soma-codemode",
            "crates/shared/codemode",
            "shared",
            vec![dep("soma-service", "crates/soma/service")],
        ),
        pkg("soma-service", "crates/soma/service", "legacy", vec![]),
    ]);

    let report = failures.join("\n");
    assert!(report.contains("shared package soma-codemode"));
    assert!(report.contains("depends on non-shared package soma-service"));
}

#[test]
fn metadata_layer_must_match_physical_path() {
    let failures = failures(vec![pkg(
        "soma-auth",
        "crates/shared/auth",
        "product-surface",
        vec![],
    )]);

    assert!(failures
        .join("\n")
        .contains("declares architecture layer \"product-surface\""));
}

#[test]
fn surfaces_must_not_depend_on_one_another() {
    let failures = failures(vec![
        pkg(
            "soma-api",
            "crates/soma/api",
            "product-surface",
            vec![dep("soma-mcp", "crates/soma/mcp")],
        ),
        pkg("soma-mcp", "crates/soma/mcp", "product-surface", vec![]),
    ]);

    assert!(failures
        .join("\n")
        .contains("product-surface packages must not depend on one another"));
}

#[test]
fn all_product_surfaces_are_isolated_by_layer_not_name_list() {
    let failures = failures(vec![
        pkg(
            "renamed-web",
            "crates/soma/web",
            "product-surface",
            vec![dep("soma-api", "crates/soma/api")],
        ),
        pkg("soma-api", "crates/soma/api", "product-surface", vec![]),
    ]);

    assert!(failures
        .join("\n")
        .contains("product-surface packages must not depend on one another"));
}

#[test]
fn product_integration_cannot_depend_on_runtime_or_surface_crates() {
    for (name, path, layer) in [
        ("soma-runtime", "crates/soma/runtime", "product-runtime"),
        ("soma-mcp", "crates/soma/mcp", "product-surface"),
    ] {
        let failures = failures(vec![
            pkg(
                "soma-integrations",
                "crates/soma/integrations",
                "product-integration",
                vec![dep(name, path)],
            ),
            pkg(name, path, layer, vec![]),
        ]);

        assert!(
            failures.join("\n").contains(
                "product-integration packages must not depend on product-runtime or product-surface crates"
            ),
            "expected direct edge to {name} to fail: {failures:#?}"
        );
    }
}

#[test]
fn product_domain_rules_follow_layer_even_when_package_name_changes() {
    let failures = failures(vec![
        pkg(
            "renamed-domain",
            "crates/soma/domain",
            "product-domain",
            vec![dep("soma-api", "crates/soma/api")],
        ),
        pkg("soma-api", "crates/soma/api", "product-surface", vec![]),
    ]);

    assert!(failures
        .join("\n")
        .contains("product-domain packages must not depend outward"));
}

#[test]
fn product_application_cannot_depend_on_legacy_or_integration_without_exception() {
    let failures = failures(vec![
        pkg(
            "renamed-application",
            "crates/soma/application",
            "product-application",
            vec![
                dep("soma-service", "crates/soma/service"),
                dep("soma-integrations", "crates/soma/integrations"),
            ],
        ),
        pkg("soma-service", "crates/soma/service", "legacy", vec![]),
        pkg(
            "soma-integrations",
            "crates/soma/integrations",
            "product-integration",
            vec![],
        ),
    ]);

    let report = failures.join("\n");
    assert!(report.contains("product-application packages must not depend"));
    assert!(report.contains("soma-service"));
    assert!(report.contains("soma-integrations"));
}

#[test]
fn path_anchored_exceptions_cannot_be_spoofed_by_package_name() {
    let graph = graph(vec![
        pkg(
            "soma-application",
            "crates/shared/pretender",
            "shared",
            vec![dep("soma-service", "crates/soma/service")],
        ),
        pkg("soma-service", "crates/soma/service", "legacy", vec![]),
    ]);
    let exception = ArchitectureException {
        from_path: "crates/soma/application",
        to_path: "crates/soma/service",
        owner: "architecture-refactor",
        reason: "temporary strangler edge",
        removal_pr: "PR 12",
        expiration_milestone: "before stable boundary",
    };

    assert!(!exception.matches(&graph, &graph.edges[0]));
    assert!(check_direct_edges(&graph, &[])
        .join("\n")
        .contains("shared package soma-application"));
}

#[test]
fn temporary_exceptions_must_match_one_current_edge() {
    let graph = graph(vec![
        pkg(
            "renamed-application",
            "crates/soma/application",
            "product-application",
            vec![dep("soma-service", "crates/soma/service")],
        ),
        pkg("soma-service", "crates/soma/service", "legacy", vec![]),
    ]);
    let live_exception = ArchitectureException {
        from_path: "crates/soma/application",
        to_path: "crates/soma/service",
        owner: "architecture-refactor",
        reason: "temporary strangler edge",
        removal_pr: "PR 12",
        expiration_milestone: "before stable boundary",
    };
    let stale_exception = ArchitectureException {
        from_path: "crates/soma/application",
        to_path: "crates/soma/contracts",
        owner: "architecture-refactor",
        reason: "temporary strangler edge",
        removal_pr: "PR 13",
        expiration_milestone: "before stable boundary",
    };

    assert!(check_exception_integrity(&graph, &[live_exception]).is_empty());
    assert!(check_exception_integrity(&graph, &[stale_exception])
        .join("\n")
        .contains("does not match a current normal workspace edge"));
}

#[test]
fn only_app_or_integration_may_bridge_application_ports_to_engines() {
    let failures = failures(vec![
        pkg(
            "soma-api",
            "crates/soma/api",
            "product-surface",
            vec![
                dep("soma-application", "crates/soma/application"),
                dep("soma-gateway", "crates/shared/mcp/gateway"),
            ],
        ),
        pkg(
            "soma-application",
            "crates/soma/application",
            "product-application",
            vec![],
        ),
        pkg(
            "soma-gateway",
            "crates/shared/mcp/gateway",
            "shared",
            vec![],
        ),
    ]);

    assert!(failures
        .join("\n")
        .contains("depends on both product application ports and concrete shared engines"));
}

#[test]
fn mixed_application_engine_check_honors_only_the_named_exception_edge() {
    let graph = graph(vec![
        pkg(
            "soma-api",
            "crates/soma/api",
            "product-surface",
            vec![
                dep("soma-application", "crates/soma/application"),
                dep("soma-gateway", "crates/shared/mcp/gateway"),
                dep("soma-openapi", "crates/shared/openapi"),
            ],
        ),
        pkg(
            "soma-application",
            "crates/soma/application",
            "product-application",
            vec![],
        ),
        pkg(
            "soma-gateway",
            "crates/shared/mcp/gateway",
            "shared",
            vec![],
        ),
        pkg("soma-openapi", "crates/shared/openapi", "shared", vec![]),
    ]);
    let gateway_exception = ArchitectureException {
        from_path: "crates/soma/api",
        to_path: "crates/shared/mcp/gateway",
        owner: "architecture-refactor",
        reason: "temporary surface bridge",
        removal_pr: "PR 6",
        expiration_milestone: "REST migration",
    };

    let report = check_mixed_application_and_engine_edges(&graph, &[gateway_exception]).join("\n");

    assert!(
        report.contains("depends on both product application ports and concrete shared engines")
    );
}

#[test]
fn mixed_application_engine_check_allows_a_named_temporary_bridge() {
    let graph = graph(vec![
        pkg(
            "soma-api",
            "crates/soma/api",
            "product-surface",
            vec![
                dep("soma-application", "crates/soma/application"),
                dep("soma-gateway", "crates/shared/mcp/gateway"),
            ],
        ),
        pkg(
            "soma-application",
            "crates/soma/application",
            "product-application",
            vec![],
        ),
        pkg(
            "soma-gateway",
            "crates/shared/mcp/gateway",
            "shared",
            vec![],
        ),
    ]);
    let gateway_exception = ArchitectureException {
        from_path: "crates/soma/api",
        to_path: "crates/shared/mcp/gateway",
        owner: "architecture-refactor",
        reason: "temporary surface bridge",
        removal_pr: "PR 6",
        expiration_milestone: "REST migration",
    };

    assert!(check_mixed_application_and_engine_edges(&graph, &[gateway_exception]).is_empty());
}

#[test]
fn vendor_package_depending_on_non_vendor_package_fails() {
    let failures = failures(vec![
        pkg(
            "unifi",
            "crates/integrations/unifi",
            "vendor",
            vec![dep("soma-observability", "crates/shared/observability")],
        ),
        pkg(
            "soma-observability",
            "crates/shared/observability",
            "shared",
            vec![],
        ),
    ]);

    let report = failures.join("\n");
    assert!(report.contains("vendor package unifi"));
    assert!(report.contains("depends on non-vendor package soma-observability"));
}

#[test]
fn vendor_packages_may_depend_on_one_another() {
    let failures = failures(vec![
        pkg(
            "sonarr",
            "crates/integrations/sonarr",
            "vendor",
            vec![dep("arr-common", "crates/integrations/arr-common")],
        ),
        pkg(
            "arr-common",
            "crates/integrations/arr-common",
            "vendor",
            vec![],
        ),
    ]);

    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn internal_dependency_cycles_fail() {
    let failures = failures(vec![
        pkg(
            "soma-api",
            "crates/soma/api",
            "product-surface",
            vec![dep("soma-runtime", "crates/soma/runtime")],
        ),
        pkg(
            "soma-runtime",
            "crates/soma/runtime",
            "product-runtime",
            vec![dep("soma-api", "crates/soma/api")],
        ),
    ]);

    assert!(failures
        .join("\n")
        .contains("internal dependency cycle detected"));
}
