//! Architecture boundary tests — make the thin-shim rule executable.
//!
//! CLAUDE.md says the MCP and CLI shims (`tools.rs`, `cli/lib.rs`) hold zero
//! business logic and reach the application facade (`SomaApplication`), never
//! the transport client (`SomaClient`), legacy service/registry, or raw HTTP.
//! These tests read the shim source and fail if that boundary is crossed,
//! so the rule is enforced by CI instead of by reviewer vigilance.
//!
//! The checks are deliberately textual and conservative: they target import and
//! call-site forms (`use … SomaClient`, `SomaClient::`, `reqwest`) so a
//! mention inside a doc comment or help string is not a false positive.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is `apps/soma`; the workspace root is two up.
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

fn read_workspace_file(relative: &str) -> String {
    let path = workspace_root().join(relative);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
}

fn cargo(args: &[&str]) -> String {
    let output = Command::new("cargo")
        .args(args)
        .current_dir(workspace_root())
        .output()
        .unwrap_or_else(|e| panic!("failed to run cargo {}: {e}", args.join(" ")));
    assert!(
        output.status.success(),
        "cargo {} failed\nstdout:\n{}\nstderr:\n{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("cargo output should be utf-8")
}

fn tree_mentions_package(tree: &str, package: &str) -> bool {
    tree.lines().any(|line| {
        let without_path = line.split(" (").next().unwrap_or(line);
        without_path.split_whitespace().any(|word| word == package)
    })
}

fn collect_rs_files(root: &str) -> Vec<PathBuf> {
    fn walk(path: PathBuf, files: &mut Vec<PathBuf>) {
        let entries = fs::read_dir(&path)
            .unwrap_or_else(|e| panic!("failed to read directory {}: {e}", path.display()));
        for entry in entries {
            let entry = entry.unwrap_or_else(|e| panic!("failed to read directory entry: {e}"));
            let path = entry.path();
            if path.is_dir() {
                walk(path, files);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }

    let mut files = Vec::new();
    let root = workspace_root().join(root);
    assert!(
        root.exists(),
        "expected crate source root {}",
        root.display()
    );
    walk(root, &mut files);
    files
}

fn rel(path: &std::path::Path) -> String {
    path.strip_prefix(workspace_root())
        .unwrap_or(path)
        .display()
        .to_string()
}

fn rel_slash(path: &Path) -> String {
    rel(path).replace('\\', "/")
}

fn cargo_metadata() -> serde_json::Value {
    let stdout = cargo(&["metadata", "--format-version", "1", "--no-deps"]);
    serde_json::from_str(&stdout).expect("cargo metadata should be valid JSON")
}

fn package_root(package: &serde_json::Value) -> PathBuf {
    PathBuf::from(
        package
            .get("manifest_path")
            .and_then(serde_json::Value::as_str)
            .expect("package has manifest_path"),
    )
    .parent()
    .expect("manifest path has a parent")
    .to_path_buf()
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
        "tools.rs must dispatch through SomaApplication, never import SomaClient (thin-shim rule)"
    );
    assert!(
        !src.contains("SomaClient::"),
        "tools.rs must not construct or call SomaClient directly; go through the service layer"
    );
    assert!(
        !src.contains("reqwest"),
        "tools.rs must not perform transport/HTTP work; that belongs in soma-client (the transport crate)"
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
            "cli/lib.rs must not own provider report domain logic ({forbidden}); use soma-application"
        );
    }
}

#[test]
fn mcp_tools_shim_reaches_the_application_facade() {
    let src = read_shim("crates/soma/mcp/src/tools.rs");
    assert!(
        src.contains("use soma_application::")
            && src.contains("SomaApplication")
            && src.contains(".execute_action("),
        "tools.rs should reach business operations through SomaApplication::execute_action"
    );
    for forbidden in [
        "SomaService",
        "ProviderRegistry",
        "ProviderCall",
        ".dispatch(",
    ] {
        assert!(
            !src.contains(forbidden),
            "tools.rs must not reach the legacy business engine directly ({forbidden})"
        );
    }
}

#[test]
fn runtime_state_exposes_the_application_facade_not_legacy_engines() {
    let metadata = cargo_metadata();
    let runtime = metadata
        .get("packages")
        .and_then(serde_json::Value::as_array)
        .expect("cargo metadata has packages")
        .iter()
        .find(|package| {
            package.get("name").and_then(serde_json::Value::as_str) == Some("soma-runtime")
        })
        .expect("workspace contains soma-runtime");
    let dependencies = runtime
        .get("dependencies")
        .and_then(serde_json::Value::as_array)
        .expect("soma-runtime has dependencies");
    let dependency_names = dependencies
        .iter()
        .filter_map(|dependency| dependency.get("name").and_then(serde_json::Value::as_str))
        .collect::<Vec<_>>();

    assert!(
        dependency_names.contains(&"soma-application"),
        "soma-runtime must own the initialized SomaApplication handle"
    );

    let runtime_sources = collect_rs_files("crates/soma/runtime/src");
    for source in &runtime_sources {
        // Test-only files are exempt: `test_support.rs` (dev-dependency only,
        // `#![cfg(test)]`, excluded from `cargo xtask check-architecture`'s
        // layer graph — see its own module doc) constructs a real
        // `SomaService`/`ProviderRegistry` stub to build a realistic
        // `SomaApplication` for `protected_routes`/`protected_routes_proxy`
        // axum-harness tests. This check's intent is soma-runtime's
        // *production* surface never exposing raw legacy engines in place of
        // the `SomaApplication` facade — not that the crate's own test
        // fixtures can't construct one to exercise real behavior.
        let name = source.file_name().and_then(|name| name.to_str());
        if name == Some("test_support.rs") || name.is_some_and(|name| name.ends_with("_tests.rs")) {
            continue;
        }
        let contents = fs::read_to_string(source)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", source.display()));
        for forbidden in ["SomaService", "ProviderRegistry", "ProviderCall"] {
            assert!(
                !contents.contains(forbidden),
                "{} must not expose or import legacy runtime engine {forbidden}",
                rel(source)
            );
        }
    }

    let routes = read_workspace_file("apps/soma/src/http.rs");
    assert!(
        routes.contains("state.application_handle()") && !routes.contains("application_for_state"),
        "HTTP adapters must receive the stored application facade instead of rebuilding it"
    );

    let composition = read_workspace_file("apps/soma/src/bootstrap.rs");
    assert_eq!(
        composition.matches("SomaRuntime::new(").count(),
        1,
        "the composition root must construct the application and runtime together"
    );
    assert!(
        !runtime_sources.iter().any(|source| {
            fs::read_to_string(source).is_ok_and(|contents| contents.contains("pub fn gateway("))
        }),
        "runtime state must expose narrow gateway operations, not the raw gateway engine"
    );
}

#[test]
fn shared_crates_do_not_depend_on_soma_product_crates() {
    let metadata = cargo_metadata();
    let packages = metadata
        .get("packages")
        .and_then(serde_json::Value::as_array)
        .expect("cargo metadata has packages");
    let mut failures = Vec::new();

    for package in packages {
        let package_root = package_root(package);
        let package_rel = rel_slash(&package_root);
        if !package_rel.starts_with("crates/shared/") {
            continue;
        }

        let package_name = package
            .get("name")
            .and_then(serde_json::Value::as_str)
            .expect("package has name");
        let dependencies = package
            .get("dependencies")
            .and_then(serde_json::Value::as_array)
            .expect("package has dependencies");

        for dependency in dependencies {
            let Some(path) = dependency.get("path").and_then(serde_json::Value::as_str) else {
                continue;
            };
            let dependency_rel = rel_slash(Path::new(path));
            if dependency_rel == "apps/soma" || dependency_rel.starts_with("crates/soma/") {
                let dependency_name = dependency
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("<unknown>");
                failures.push(format!(
                    "{package_name} ({package_rel}) must stay reusable but depends on {dependency_name} at {dependency_rel}"
                ));
            }
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn self_update_crate_has_no_workspace_dependencies() {
    let metadata = cargo_metadata();
    let packages = metadata["packages"]
        .as_array()
        .expect("cargo metadata packages should be an array");
    let package = packages
        .iter()
        .find(|package| package["name"] == "soma-self-update")
        .expect("soma-self-update should be a workspace package");

    for dependency in package["dependencies"]
        .as_array()
        .expect("package dependencies should be an array")
    {
        assert!(
            dependency["path"].is_null(),
            "soma-self-update dependency {} must come from crates.io, not path {}",
            dependency["name"],
            dependency["path"]
        );
    }

    let tree = cargo(&["tree", "-p", "soma-self-update", "--edges", "normal"]);
    for workspace_package in packages {
        if workspace_package["name"] == "soma-self-update" {
            continue;
        }
        let root = package_root(workspace_package);
        if root.starts_with(workspace_root()) {
            let name = workspace_package["name"]
                .as_str()
                .expect("workspace package should have a name");
            assert!(
                !tree_mentions_package(&tree, name),
                "soma-self-update normal dependency tree must not include workspace package {name}\n{tree}"
            );
        }
    }
}

#[test]
fn codemode_openapi_crates_have_no_forbidden_internal_dependencies() {
    let root_manifest = read_workspace_file("Cargo.toml");
    for member in ["crates/shared/openapi", "crates/shared/codemode"] {
        assert!(
            root_manifest.contains(&format!("\"{member}\"")),
            "workspace Cargo.toml must include {member}"
        );
    }

    let openapi_manifest = read_workspace_file("crates/shared/openapi/Cargo.toml");
    let codemode_manifest = read_workspace_file("crates/shared/codemode/Cargo.toml");

    for (name, manifest) in [
        ("soma-openapi", openapi_manifest.as_str()),
        ("soma-codemode", codemode_manifest.as_str()),
    ] {
        for forbidden in [
            "labby-",
            "labby_",
            "labby-runtime",
            "labby-primitives",
            "labby-winjob",
            "rmcp-openapi",
            "soma-api",
            "soma-auth",
            "soma-cli",
            "soma-mcp",
            "soma-observability",
            "soma-plugin-support",
            "soma-runtime",
            "soma-test-support",
            "soma-web",
        ] {
            if name == "soma-codemode" && forbidden == "soma-openapi" {
                continue;
            }
            assert!(
                !manifest.contains(forbidden),
                "{name} manifest must not depend on or mention forbidden internal crate {forbidden}"
            );
        }
    }

    let openapi_tree = cargo(&["tree", "-p", "soma-openapi"]);
    assert!(
        !tree_mentions_package(&openapi_tree, "labby-runtime")
            && !tree_mentions_package(&openapi_tree, "labby-primitives")
            && !tree_mentions_package(&openapi_tree, "labby-winjob")
            && !tree_mentions_package(&openapi_tree, "soma-codemode"),
        "soma-openapi cargo tree must stay independent:\n{openapi_tree}"
    );
}

#[test]
fn codemode_openapi_feature_graph_is_explicit() {
    let manifest = read_workspace_file("crates/shared/codemode/Cargo.toml");
    assert!(
        manifest.contains("default = []"),
        "soma-codemode must have empty default features"
    );
    assert!(
        manifest.contains("openapi = [\"dep:reqwest\", \"dep:soma-openapi\"]"),
        "soma-codemode openapi feature must use explicit dep: edges"
    );
    assert!(
        manifest.contains("reqwest = ")
            && manifest.contains("optional = true")
            && manifest.contains(
                "soma-openapi = { workspace = true, optional = true, default-features = true }"
            ),
        "soma-codemode reqwest and soma-openapi dependencies must be optional"
    );

    let no_feature_tree = cargo(&["tree", "-p", "soma-codemode", "--no-default-features"]);
    for forbidden in [
        "soma-openapi",
        "reqwest",
        "soma-api",
        "soma-auth",
        "soma-cli",
        "soma-mcp",
        "soma-runtime",
        "soma-web",
    ] {
        assert!(
            !tree_mentions_package(&no_feature_tree, forbidden),
            "no-feature soma-codemode tree must not contain {forbidden}:\n{no_feature_tree}"
        );
    }
    for forbidden in ["labby-runtime", "labby-primitives", "labby-winjob"] {
        assert!(
            !tree_mentions_package(&no_feature_tree, forbidden),
            "no-feature soma-codemode tree must not contain {forbidden}:\n{no_feature_tree}"
        );
    }

    let openapi_tree = cargo(&["tree", "-p", "soma-codemode", "--features", "openapi"]);
    assert!(
        tree_mentions_package(&openapi_tree, "soma-openapi"),
        "openapi feature tree must include soma-openapi:\n{openapi_tree}"
    );
}

#[test]
fn codemode_openapi_sources_have_sibling_tests_and_size_caps() {
    let mut failures = Vec::new();
    for root in ["crates/shared/openapi", "crates/shared/codemode"] {
        let root_path = workspace_root().join(root);
        let mod_rs = root_path.join("src").join("mod.rs");
        assert!(!mod_rs.exists(), "{} must not exist", mod_rs.display());

        for file in collect_rs_files(root) {
            let contents = fs::read_to_string(&file)
                .unwrap_or_else(|e| panic!("failed to read {}: {e}", file.display()));
            let line_count = contents.lines().count();
            if line_count > 500 {
                failures.push(format!("{} has {line_count} physical LOC", rel(&file)));
            }

            let name = file
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            let is_integration_test = file
                .strip_prefix(workspace_root().join(root))
                .ok()
                .and_then(|path| path.components().next())
                .and_then(|component| component.as_os_str().to_str())
                == Some("tests");
            if !is_integration_test
                && name != "lib.rs"
                && name != "main.rs"
                && !name.ends_with("_tests.rs")
            {
                let stem = name.strip_suffix(".rs").expect("rust file has .rs suffix");
                let sibling = file.with_file_name(format!("{stem}_tests.rs"));
                if !sibling.exists() {
                    failures.push(format!(
                        "{} is missing sibling {}",
                        rel(&file),
                        sibling.file_name().unwrap().to_string_lossy()
                    ));
                }
            }

            if contents.contains("mod tests {") || contents.contains("mod tests\n{") {
                failures.push(format!("{} contains inline mod tests", rel(&file)));
            }
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn codemode_openapi_sources_do_not_reintroduce_labby_runtime_names() {
    let mut failures = Vec::new();
    for root in ["crates/shared/openapi", "crates/shared/codemode"] {
        for file in collect_rs_files(root) {
            let name = file
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            let contents = fs::read_to_string(&file)
                .unwrap_or_else(|e| panic!("failed to read {}: {e}", file.display()));
            for forbidden in [
                "labby-runtime",
                "labby_primitives",
                "labby_runtime",
                "labby-winjob",
                "LABBY_",
                "~/.labby",
                ".labby/",
                "labby.service",
            ] {
                if contents.contains(forbidden) {
                    failures.push(format!(
                        "{} contains stale Lab runtime name {forbidden}",
                        rel(&file)
                    ));
                }
            }

            if !name.ends_with("_tests.rs") {
                for forbidden in ["labby-", "labby_"] {
                    if contents.contains(forbidden) {
                        failures.push(format!(
                            "{} contains forbidden Lab crate spelling {forbidden}",
                            rel(&file)
                        ));
                    }
                }
            }
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn application_ports_are_available_to_all_composition_profiles() {
    let source = fs::read_to_string(workspace_root().join("apps/soma/src/lib.rs"))
        .expect("read Soma facade source");
    let lines = source.lines().map(str::trim).collect::<Vec<_>>();

    assert!(
        source.contains("feature = \"mcp-stdio\"") && source.contains("feature = \"mcp-http\"")
    );
    assert!(lines.contains(&"mod bootstrap;"));
}

/// PR 18's acceptance criterion is that `apps/soma` "contains no business
/// rules" (plan section 3.1). A prior review found `protected_routes.rs`
/// (bearer-token auth, scope authorization) and `protected_routes_proxy.rs`
/// (the inbound-to-upstream reverse-proxy engine) still living directly in
/// `apps/soma/src` — real business/security logic in the composition root.
/// Both were moved to `crates/soma/integrations` (their permanent home; see
/// that crate's module doc comment). This test fails CI if that logic (or
/// something that looks like it) is ever reintroduced into `apps/soma/src`,
/// instead of relying on reviewer vigilance to catch it a second time.
#[test]
fn apps_soma_does_not_reintroduce_protected_route_business_logic() {
    let forbidden = [
        // Bearer-token validation / OAuth-scope authorization decisions —
        // protected_routes.rs's `authenticate_protected_route_request`.
        "validate_access_token_with_issuer",
        "fn authenticate_protected_route_request",
        // Gateway-subset dispatch workflow — protected_routes.rs's
        // `dispatch_gateway_subset`.
        "fn dispatch_gateway_subset",
        // Inbound-to-upstream reverse-proxy engine —
        // protected_routes_proxy.rs's `proxy_protected_mcp_route` and its
        // backend-target resolver.
        "fn proxy_protected_mcp_route",
        "fn protected_route_upstream_target",
    ];

    for path in collect_rs_files("apps/soma/src") {
        let src = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
        for pattern in forbidden {
            assert!(
                !src.contains(pattern),
                "{} contains `{pattern}` — protected-route business logic belongs in \
                 crates/soma/integrations (soma-integrations), not apps/soma \
                 (composition root; plan section 3.1 \"Does not own\")",
                rel(&path)
            );
        }
    }
}

#[test]
fn bare_mcp_profile_compiles_without_client_or_observability_features() {
    cargo(&[
        "check",
        "-p",
        "soma",
        "--no-default-features",
        "--features",
        "mcp",
    ]);
    let tree = cargo(&[
        "tree",
        "-p",
        "soma",
        "--no-default-features",
        "--features",
        "mcp",
        "-e",
        "normal",
        "-f",
        "{p} {f}",
    ]);
    let service = tree
        .lines()
        .find(|line| line.contains("soma-application v"))
        .expect("bare MCP graph contains soma-application");
    assert!(
        !service.contains("client"),
        "unexpected client feature: {service}"
    );
    assert!(
        !service.contains("observability"),
        "unexpected observability feature: {service}"
    );
    assert!(
        !tree_mentions_package(&tree, "soma-observability"),
        "bare MCP production graph must not include soma-observability"
    );
}
