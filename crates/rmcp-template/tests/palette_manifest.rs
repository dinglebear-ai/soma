use std::{fs, path::Path};

#[test]
fn palette_manifest_is_bounded_and_disables_builtin_capabilities() {
    let manifest = workspace_root().join("docs/generated/palette-manifest.json");
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(manifest).expect("palette manifest should exist"))
            .expect("palette manifest JSON");

    assert_eq!(value["schema_version"], 1);
    assert!(value["provider_fingerprint"]
        .as_str()
        .expect("fingerprint string")
        .starts_with("sha256:"));
    assert_eq!(value["builtins"]["file_explorer"], false);
    assert_eq!(value["builtins"]["github"], false);
    assert_eq!(value["builtins"]["browser"], false);
    assert_eq!(value["builtins"]["terminal"], false);
    assert!(value["commands"].as_array().expect("commands").len() <= 64);
    assert!(value["limits"]["max_inline_schema_bytes"].as_u64().unwrap() <= 16384);
}

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
}
