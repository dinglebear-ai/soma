use std::{collections::BTreeSet, path::Path};

use serde::Deserialize;
use soma_ops::OperationName;

#[derive(Debug, Deserialize)]
struct Fixture {
    operation_count: usize,
    operations: Vec<OperationFixture>,
}

#[derive(Debug, Deserialize)]
struct OperationFixture {
    legacy_name: String,
    canonical_name: String,
}

#[test]
fn all_pinned_synapse_operations_map_to_valid_unique_neutral_names() {
    let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../docs/unify/03-contracts/examples/synapse-operations.json");
    let fixture: Fixture = serde_json::from_str(
        &std::fs::read_to_string(&fixture_path).expect("read pinned Synapse fixture"),
    )
    .expect("parse pinned Synapse fixture");

    assert_eq!(fixture.operation_count, 59);
    assert_eq!(fixture.operations.len(), 59);

    let mut legacy = BTreeSet::new();
    let mut canonical = BTreeSet::new();
    for operation in fixture.operations {
        assert!(
            legacy.insert(operation.legacy_name.clone()),
            "duplicate legacy operation {}",
            operation.legacy_name
        );
        let parsed = OperationName::new(operation.canonical_name.clone()).unwrap_or_else(|error| {
            panic!("invalid mapping for {}: {error}", operation.legacy_name)
        });
        assert_eq!(parsed.as_str(), operation.canonical_name);
        assert!(
            canonical.insert(operation.canonical_name.clone()),
            "duplicate canonical operation {}",
            operation.canonical_name
        );
    }
}
