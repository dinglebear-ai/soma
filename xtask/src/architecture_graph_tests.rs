use super::*;
use serde_json::{json, Value};

#[test]
fn layer_paths_match_architecture_taxonomy() {
    assert_eq!(Layer::from_path("apps/soma"), Some(Layer::App));
    assert_eq!(
        Layer::from_path("crates/shared/mcp/gateway"),
        Some(Layer::Shared)
    );
    assert_eq!(
        Layer::from_path("crates/soma/runtime"),
        Some(Layer::ProductRuntime)
    );
    assert_eq!(
        Layer::from_path("crates/soma/api"),
        Some(Layer::ProductSurface)
    );
    assert_eq!(
        Layer::from_path("crates/integrations/unifi"),
        Some(Layer::Vendor)
    );
    assert_eq!(Layer::from_path("xtask"), Some(Layer::Legacy));
    assert_eq!(Layer::from_path("apps/web"), None);
}

#[test]
fn graph_ignores_local_path_dependencies_outside_workspace_members() {
    let root = std::path::Path::new("/repo");
    let package = pkg(
        "soma-api",
        "crates/soma/api",
        "product-surface",
        vec![json!({
            "name": "local-helper",
            "path": "/tmp/local-helper",
            "kind": Value::Null,
            "optional": false
        })],
    );
    let external = json!({
        "id": "local-helper",
        "name": "local-helper",
        "source": null,
        "manifest_path": "/tmp/local-helper/Cargo.toml",
        "metadata": {},
        "dependencies": []
    });
    let metadata = json!({
        "packages": [package, external],
        "workspace_members": ["soma-api"]
    });

    let graph = Graph::from_metadata(root, &metadata).expect("graph");

    assert_eq!(graph.packages.len(), 1);
    assert!(graph.edges.is_empty());
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
