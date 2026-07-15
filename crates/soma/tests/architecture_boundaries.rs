//! Architecture boundary tests — make the thin-shim rule executable.
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

use std::fs;
use std::path::PathBuf;
use std::process::Command;

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

fn imports_symbol(src: &str, symbol: &str) -> bool {
    src.lines()
        .map(str::trim_start)
        .filter(|line| line.starts_with("use "))
        .any(|line| line.contains(symbol))
}

#[test]
fn mcp_tools_shim_does_not_touch_the_transport_client() {
    let src = read_shim("crates/soma-mcp/src/tools.rs");
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
    let src = read_shim("crates/soma-cli/src/lib.rs");
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
    let src = read_shim("crates/soma-mcp/src/tools.rs");
    assert!(
        src.contains("dispatch_action") || imports_symbol(&src, "SomaService"),
        "tools.rs should reach the service layer via dispatch_action / SomaService"
    );
}

#[test]
fn codemode_openapi_crates_have_no_forbidden_internal_dependencies() {
    let root_manifest = read_workspace_file("Cargo.toml");
    for member in ["crates/soma-openapi", "crates/soma-codemode"] {
        assert!(
            root_manifest.contains(&format!("\"{member}\"")),
            "workspace Cargo.toml must include {member}"
        );
    }

    let openapi_manifest = read_workspace_file("crates/soma-openapi/Cargo.toml");
    let codemode_manifest = read_workspace_file("crates/soma-codemode/Cargo.toml");

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
            "soma-contracts",
            "soma-mcp",
            "soma-observability",
            "soma-plugin-support",
            "soma-runtime",
            "soma-service",
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
    let manifest = read_workspace_file("crates/soma-codemode/Cargo.toml");
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
            && manifest.contains("soma-openapi = { path = \"../soma-openapi\", optional = true }"),
        "soma-codemode reqwest and soma-openapi dependencies must be optional"
    );

    let no_feature_tree = cargo(&["tree", "-p", "soma-codemode", "--no-default-features"]);
    for forbidden in [
        "soma-openapi",
        "reqwest",
        "soma-api",
        "soma-auth",
        "soma-cli",
        "soma-contracts",
        "soma-mcp",
        "soma-runtime",
        "soma-service",
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
    for root in ["crates/soma-openapi", "crates/soma-codemode"] {
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
    for root in ["crates/soma-openapi", "crates/soma-codemode"] {
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
