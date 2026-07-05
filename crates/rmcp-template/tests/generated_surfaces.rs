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

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
}
