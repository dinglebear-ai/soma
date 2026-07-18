use serde_json::json;
use soma_application::CatalogSnapshot;
use soma_provider_core::{PaletteOverlay, ProviderId, ProviderManifest, ToolSpec};

use super::{catalog_response, palette_entries};

fn manifest_with_tools(name: &str, tools: Vec<ToolSpec>) -> ProviderManifest {
    let mut manifest = ProviderManifest::new(
        ProviderId::new(name).expect("valid provider id"),
        name,
        "0.1.0",
    );
    manifest.tools = tools;
    manifest
}

fn snapshot(catalogs: Vec<ProviderManifest>) -> CatalogSnapshot {
    CatalogSnapshot {
        id: "snap-1".to_string(),
        fingerprint: "sha256:test".to_string(),
        catalogs,
    }
}

#[test]
fn maps_palette_enabled_tool_with_overlay_fields() {
    let mut tool = ToolSpec::new("send_alert", "Send an alert", json!({"type": "object"}));
    tool.title = Some("Send Alert".to_string());
    tool.destructive = true;
    tool.palette = Some(PaletteOverlay {
        enabled: true,
        category: Some("notify".to_string()),
        icon: Some("bell".to_string()),
        tone: Some("warning".to_string()),
        arg_mode: Some("form".to_string()),
        result_view: Some("card".to_string()),
        aurora_blocks: vec![],
    });

    let snap = snapshot(vec![manifest_with_tools("gotify", vec![tool])]);
    let entries = palette_entries(&snap);

    assert_eq!(entries.len(), 1);
    let entry = &entries[0];
    assert_eq!(entry.id, "send_alert");
    assert_eq!(entry.provider, "gotify");
    assert_eq!(entry.title, "Send Alert");
    assert_eq!(entry.category.as_deref(), Some("notify"));
    assert_eq!(entry.icon.as_deref(), Some("bell"));
    assert!(entry.destructive);
}

#[test]
fn falls_back_to_tool_name_when_no_title() {
    let tool = ToolSpec::new("ping", "Ping", json!({"type": "object"}));
    let snap = snapshot(vec![manifest_with_tools("net", vec![tool])]);
    let entries = palette_entries(&snap);
    assert_eq!(entries[0].title, "ping");
}

#[test]
fn excludes_tools_with_palette_overlay_disabled() {
    let mut tool = ToolSpec::new("internal_only", "internal", json!({"type": "object"}));
    tool.palette = Some(PaletteOverlay {
        enabled: false,
        category: None,
        icon: None,
        tone: None,
        arg_mode: None,
        result_view: None,
        aurora_blocks: vec![],
    });
    let snap = snapshot(vec![manifest_with_tools("internal", vec![tool])]);
    assert!(palette_entries(&snap).is_empty());
}

#[test]
fn includes_tool_with_no_palette_overlay_by_default() {
    let tool = ToolSpec::new("default_on", "on by default", json!({"type": "object"}));
    let snap = snapshot(vec![manifest_with_tools("default", vec![tool])]);
    assert_eq!(palette_entries(&snap).len(), 1);
}

#[test]
fn catalog_response_carries_fingerprint_and_schema_version() {
    let snap = snapshot(vec![]);
    let response = catalog_response(&snap);
    assert_eq!(response.schema_version, 1);
    assert_eq!(response.fingerprint, "sha256:test");
    assert!(response.entries.is_empty());
}

#[test]
fn flat_maps_tools_across_multiple_catalogs() {
    let alpha = ToolSpec::new("alpha_tool", "Alpha", json!({"type": "object"}));
    let beta = ToolSpec::new("beta_tool", "Beta", json!({"type": "object"}));
    let snap = snapshot(vec![
        manifest_with_tools("alpha", vec![alpha]),
        manifest_with_tools("beta", vec![beta]),
    ]);

    let entries = palette_entries(&snap);

    assert_eq!(entries.len(), 2);
    assert!(entries
        .iter()
        .any(|entry| entry.id == "alpha_tool" && entry.provider == "alpha"));
    assert!(entries
        .iter()
        .any(|entry| entry.id == "beta_tool" && entry.provider == "beta"));
}
