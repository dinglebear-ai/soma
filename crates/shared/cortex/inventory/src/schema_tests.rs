use super::*;

#[test]
fn empty_inventory_uses_locked_schema_and_defaults() {
    let inventory =
        HomelabInventory::empty("run-1".to_string(), "2026-08-18T12:00:00Z".to_string());
    assert_eq!(inventory.schema, crate::limits::INVENTORY_SCHEMA);
    assert_eq!(inventory.run_id, "run-1");
    assert!(inventory.nodes.is_empty());
    assert!(inventory.services.is_empty());
    assert_eq!(inventory.summary, InventorySummary::default());
}

#[test]
fn trust_level_wire_values_match_donor_contract() {
    assert_eq!(
        serde_json::to_string(&TrustLevel::Verified).unwrap(),
        "\"verified\""
    );
    assert_eq!(
        serde_json::to_string(&TrustLevel::Observed).unwrap(),
        "\"observed\""
    );
    assert_eq!(
        serde_json::to_string(&TrustLevel::Claimed).unwrap(),
        "\"claimed\""
    );
    assert_eq!(
        serde_json::to_string(&TrustLevel::Inferred).unwrap(),
        "\"inferred\""
    );
}
