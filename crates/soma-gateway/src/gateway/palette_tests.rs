use std::time::{Duration, Instant};

use crate::gateway::code_mode::catalog::CodeModeCatalog;
use crate::upstream::{ToolDescriptor, TransportKind, UpstreamSnapshot};

use super::*;

fn code_mode_catalog() -> CodeModeCatalog {
    let mut snapshot = UpstreamSnapshot::empty("axon", TransportKind::InProcess);
    snapshot.tools.push(ToolDescriptor::new("search"));
    CodeModeCatalog::from_snapshots(&[snapshot])
}

#[test]
fn palette_catalog_cache_avoids_warm_reprobes() {
    let mut cache = PaletteCache::new(Duration::from_secs(60));
    let catalog = code_mode_catalog();
    let now = Instant::now();

    cache.catalog_from_code_mode(&catalog, now);
    cache.catalog_from_code_mode(&catalog, now + Duration::from_secs(1));

    assert_eq!(cache.reprobe_count(), 1);
}

#[test]
fn palette_schema_resolves_only_requested_schema_and_caps_size() {
    let mut cache = PaletteCache::new(Duration::from_secs(60));
    cache.set_schema("axon::small", serde_json::json!({"type": "object"}));
    cache.set_schema(
        "axon::big",
        serde_json::json!({"blob": "x".repeat(PALETTE_SCHEMA_CAP_BYTES)}),
    );

    assert_eq!(
        cache.schema("axon::small").unwrap(),
        serde_json::json!({"type": "object"})
    );
    assert_eq!(cache.schema("axon::big"), Err(PaletteError::SchemaTooLarge));
}
