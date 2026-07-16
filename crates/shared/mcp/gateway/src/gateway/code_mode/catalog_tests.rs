use crate::upstream::{ToolDescriptor, TransportKind, UpstreamSnapshot};

use super::*;

#[test]
fn cold_catalog_renders_without_upstream_connects() {
    let mut snapshot = UpstreamSnapshot::empty("axon", TransportKind::InProcess);
    snapshot.tools.push(ToolDescriptor::new("search"));

    let catalog = CodeModeCatalog::from_snapshots(&[snapshot]);

    assert_eq!(catalog.names_only(), vec!["axon::search"]);
}

#[test]
fn schema_projection_is_capped() {
    let mut snapshot = UpstreamSnapshot::empty("axon", TransportKind::InProcess);
    let mut tool = ToolDescriptor::new("big");
    tool.input_schema = Some(serde_json::json!({"blob": "x".repeat(CODEMODE_SCHEMA_CAP_BYTES)}));
    snapshot.tools.push(tool);
    let catalog = CodeModeCatalog::from_snapshots(&[snapshot]);

    assert_eq!(
        catalog.schema_for("axon::big"),
        Err(CatalogError::SchemaTooLarge)
    );
}

#[test]
fn names_only_scales_without_schema_lookup() {
    let mut snapshot = UpstreamSnapshot::empty("bulk", TransportKind::InProcess);
    for index in 0..1000 {
        let mut tool = ToolDescriptor::new(format!("tool_{index}"));
        tool.input_schema = Some(serde_json::json!({"blob": "x".repeat(1024)}));
        snapshot.tools.push(tool);
    }
    let catalog = CodeModeCatalog::from_snapshots(&[snapshot]);

    assert_eq!(catalog.names_only().len(), 1000);
}
