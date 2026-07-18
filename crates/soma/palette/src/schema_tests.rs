use serde_json::json;
use soma_application::CatalogSnapshot;
use soma_provider_core::{PaletteOverlay, ProviderId, ProviderManifest, ToolSpec};

use super::find_schema;

fn snapshot_with(tool: ToolSpec) -> CatalogSnapshot {
    let mut manifest =
        ProviderManifest::new(ProviderId::new("demo").expect("valid id"), "demo", "0.1.0");
    manifest.tools = vec![tool];
    CatalogSnapshot {
        id: "snap".to_string(),
        fingerprint: "sha256:x".to_string(),
        catalogs: vec![manifest],
    }
}

#[test]
fn finds_schema_by_id() {
    let mut tool = ToolSpec::new(
        "greet",
        "Greet",
        json!({"type": "object", "properties": {}}),
    );
    tool.output_schema = Some(json!({"type": "string"}));
    let snap = snapshot_with(tool);

    let schema = find_schema(&snap, "greet").expect("schema found");
    assert_eq!(schema.id, "greet");
    assert_eq!(
        schema.input_schema,
        json!({"type": "object", "properties": {}})
    );
    assert_eq!(schema.output_schema, Some(json!({"type": "string"})));
}

#[test]
fn returns_none_for_unknown_id() {
    let tool = ToolSpec::new("greet", "Greet", json!({"type": "object"}));
    let snap = snapshot_with(tool);
    assert!(find_schema(&snap, "does-not-exist").is_none());
}

#[test]
fn returns_none_for_palette_disabled_tool() {
    let mut tool = ToolSpec::new("hidden", "Hidden", json!({"type": "object"}));
    tool.palette = Some(PaletteOverlay {
        enabled: false,
        category: None,
        icon: None,
        tone: None,
        arg_mode: None,
        result_view: None,
        aurora_blocks: vec![],
    });
    let snap = snapshot_with(tool);
    assert!(find_schema(&snap, "hidden").is_none());
}
