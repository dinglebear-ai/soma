use std::{fs, path::Path};

#[test]
fn plugin_manifests_remain_versionless_when_present() {
    let root = workspace_root();
    for path in [
        ".claude-plugin/plugin.json",
        ".codex-plugin/plugin.json",
        "gemini-extension.json",
    ] {
        let path = root.join(path);
        if !path.exists() {
            continue;
        }
        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).expect("manifest should read"))
                .expect("manifest JSON");
        assert!(
            value.get("version").is_none(),
            "{} must stay versionless",
            path.display()
        );
    }
}

#[test]
fn generated_docs_do_not_elevate_provider_descriptions_to_instructions() {
    let root = workspace_root();
    let manifest = root.join("docs/generated/palette-manifest.json");
    let text = fs::read_to_string(manifest).expect("palette manifest should exist");
    assert!(!text.contains("ignore previous instructions"));
    assert!(!text.contains("developer message"));
}

#[test]
fn generated_marketplaces_point_at_soma_plugin() {
    let root = workspace_root();
    let codex: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join(".agents/plugins/marketplace.json")).expect("codex marketplace"),
    )
    .expect("codex marketplace JSON");
    assert_eq!(codex["plugins"][0]["source"]["path"], "./plugins/soma");
    assert!(codex["plugins"][0].get("version").is_none());

    let claude: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join(".claude-plugin/marketplace.json")).expect("claude marketplace"),
    )
    .expect("claude marketplace JSON");
    assert_eq!(claude["plugins"][0]["source"], "./plugins/soma");
    assert!(claude["plugins"][0].get("version").is_none());
}

#[test]
fn public_docs_and_smokes_do_not_reference_removed_example_binary() {
    let root = workspace_root();
    let checked_files = [
        "docs/QUICKSTART.md",
        "docs/PATTERNS.md",
        "docs/PLUGINS.md",
        "docs/adr/0001-stdio-first-plugin-adapter.md",
        "apps/soma/tests/mcporter/test-mcp.sh",
    ];

    for relative in checked_files {
        let text = fs::read_to_string(root.join(relative)).expect("checked file should read");
        assert!(
            !text.contains("target/debug/example"),
            "{relative} still references the removed example binary"
        );
        assert!(
            !text.contains("expected \\\"example\\\""),
            "{relative} still expects the removed example MCP tool"
        );
        assert!(
            !text.contains("\"command\": \"example\""),
            "{relative} still configures the removed example command"
        );
    }
}

#[test]
fn installer_targets_real_soma_release() {
    let root = workspace_root();
    let text = fs::read_to_string(root.join("install.sh")).expect("installer should read");

    assert!(!text.contains("your-org/soma-mcp"));
    assert!(text.contains("REPO=\"jmagar/soma\""));
    assert!(text.contains("BINARY_NAME=\"soma\""));
    assert!(text.contains("soma serve"));
    assert!(text.contains("localhost:40060/health"));
    let legacy_split_server_command = format!("{}-server serve", "soma");
    assert!(!text.contains(&legacy_split_server_command));
    assert!(!text.contains("localhost:3000/health"));
}

#[test]
fn generated_distribution_plugin_points_at_all_artifacts() {
    let root = workspace_root();
    let plugin: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("docs/generated/plugin.json")).expect("plugin"))
            .expect("plugin JSON");

    assert!(plugin.get("version").is_none());
    assert_eq!(plugin["plugin_root"], "plugins/soma");
    assert_eq!(
        plugin["codex"]["plugin_json"],
        "plugins/soma/.codex-plugin/plugin.json"
    );
    assert_eq!(
        plugin["claude"]["plugin_json"],
        "plugins/soma/.claude-plugin/plugin.json"
    );
    assert_eq!(plugin["skills"], "plugins/soma/skills");
    assert_eq!(plugin["node_package"], "packages/soma-rmcp/package.json");
    assert_eq!(plugin["docs"], "docs/generated/provider-surfaces.md");
    assert!(plugin["provider_fingerprint"]
        .as_str()
        .unwrap_or_default()
        .starts_with("sha256:"));
}

#[test]
fn node_package_exposes_npx_launcher() {
    let root = workspace_root();
    let package: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join("packages/soma-rmcp/package.json")).expect("package json"),
    )
    .expect("package JSON");
    assert_eq!(package["bin"]["soma"], "bin/soma-rmcp.js");

    let launcher = root.join("packages/soma-rmcp/bin/soma-rmcp.js");
    let mode = fs::metadata(&launcher)
        .expect("launcher metadata")
        .permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_ne!(mode.mode() & 0o111, 0, "launcher should be executable");
    }
}

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
}
