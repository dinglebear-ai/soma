use serde_json::Value;
use std::fs;
#[cfg(unix)]
use std::{os::unix::fs::PermissionsExt, path::Path};

fn read(path: &str) -> String {
    fs::read_to_string(repo_path(path)).unwrap_or_else(|err| panic!("failed to read {path}: {err}"))
}

fn json(path: &str) -> Value {
    serde_json::from_str(&read(path)).unwrap_or_else(|err| panic!("failed to parse {path}: {err}"))
}

fn repo_path(path: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

#[test]
#[cfg(unix)]
fn portable_scripts_are_executable_and_documented() {
    let docs = read("scripts/README.md");
    for path in [
        "scripts/check-dependency-updates.sh",
        "scripts/check-file-size.sh",
        "scripts/asciicheck.py",
        "scripts/check-blob-size.py",
        "scripts/check-runtime-current.sh",
        "scripts/validate-plugin-layout.sh",
        "scripts/test-mcp-auth.sh",
        "scripts/pre-release-check.sh",
        "scripts/test-soma-features.sh",
        "scripts/check-schema-docs.py",
        "scripts/check-coupled-files.sh",
    ] {
        let metadata = fs::metadata(repo_path(path)).unwrap_or_else(|err| panic!("{path}: {err}"));
        assert!(
            metadata.permissions().mode() & 0o111 != 0,
            "{path} should be executable"
        );
        let basename = Path::new(path).file_name().unwrap().to_string_lossy();
        assert!(
            docs.contains(basename.as_ref()),
            "scripts/README.md should document {basename}"
        );
    }
}

#[test]
fn justfile_exposes_ported_automation_recipes() {
    let justfile = read("Justfile");
    for recipe in [
        "install-tools:",
        "bootstrap:",
        "install-hooks:",
        "uninstall-hooks:",
        "deps-check:",
        "blob-size-check:",
        "coupled-files-check:",
        "ascii-check:",
        "ascii-fix:",
        "file-size-check:",
        "schema-docs:",
        "schema-docs-check:",
        "contract-audit:",
        "soma-features:",
        "soma-check:",
        "test-cov:",
        "watch:",
        "runtime-current:",
        "auth-smoke:",
        "pre-release:",
        "up:",
        "down:",
        "release:",
    ] {
        assert!(justfile.contains(recipe), "Justfile missing {recipe}");
    }
}

#[test]
fn contract_audit_is_exposed_in_automation_and_docs() {
    for path in [
        "xtask/src/main.rs",
        "Justfile",
        "README.md",
        "docs/TESTING.md",
        "docs/PATTERNS.md",
    ] {
        let content = read(path);
        assert!(
            content.contains("contract-audit"),
            "{path} should mention contract-audit"
        );
    }

    let testing = read("docs/TESTING.md");
    for tier in ["static-spec", "contract-real", "production-real"] {
        assert!(
            testing.contains(tier),
            "docs/TESTING.md should describe {tier} evidence"
        );
    }
}

#[test]
fn plugin_manifests_do_not_have_version_fields() {
    for path in [
        "plugins/soma/.claude-plugin/plugin.json",
        "plugins/soma/.codex-plugin/plugin.json",
        "plugins/soma/gemini-extension.json",
    ] {
        let manifest = json(path);
        assert!(
            !manifest.as_object().unwrap().contains_key("version"),
            "{path} must not contain a version field"
        );
    }
}

#[test]
fn schema_contract_doc_tracks_known_actions() {
    let doc = read("docs/MCP_SCHEMA.md");
    // ACTION_SPECS moved from crates/soma/contracts/src/actions.rs to
    // crates/soma/domain/src/actions.rs (plan section 6.2; PR 13).
    let actions = read("crates/soma/domain/src/actions.rs");
    let schemas = read("crates/soma/mcp/src/schemas.rs");
    for action in ["greet", "echo", "status", "elicit_name", "help"] {
        assert!(actions.contains(action), "actions.rs missing {action}");
        assert!(
            doc.contains(&format!("`{action}`")),
            "schema doc missing {action}"
        );
    }
    assert!(
        schemas.contains("tool_definitions_for_catalogs")
            && schemas.contains("fn action_names(catalogs: &[ProviderCatalog])"),
        "schemas.rs should derive action enum from provider catalog metadata"
    );
}
