use super::*;

#[test]
fn inventory_scope_and_source_ids_are_host_bounded() {
    assert_eq!(scoped_inventory_key("docker:NAS", "Plex"), "nas:plex");
    assert_eq!(
        safe_inventory_source_id("compose:nas:/opt/stack/compose.yaml"),
        "compose:nas"
    );
    assert_eq!(safe_inventory_source_id("inventory"), "inventory");
}

#[test]
fn inventory_trust_maps_to_graph_vocabulary() {
    assert_eq!(trust(&TrustLevel::Verified), graph::TRUST_VERIFIED);
    assert_eq!(trust(&TrustLevel::Observed), graph::TRUST_VERIFIED);
    assert_eq!(trust(&TrustLevel::Claimed), graph::TRUST_CLAIMED);
    assert_eq!(trust(&TrustLevel::Inferred), graph::TRUST_INFERRED);
}

#[test]
fn projection_errors_are_bounded() {
    let value = "x".repeat(600);
    assert_eq!(truncate_excerpt(&value).len(), 512);
}
