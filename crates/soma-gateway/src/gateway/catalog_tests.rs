use super::*;

#[test]
fn non_discovery_actions_require_admin_and_unknown_fails_closed() {
    let catalog = GatewayActionCatalog::standard();

    assert!(catalog
        .list()
        .into_iter()
        .filter(|action| !action.discovery)
        .all(|action| action.admin_required));
    assert!(catalog.get("gateway.nope").admin_required);
}

#[test]
fn destructive_metadata_is_executable_test_data() {
    let remove = GatewayActionCatalog::standard().get("gateway.remove");

    assert!(remove.destructive);
    assert!(remove.admin_required);
}

#[test]
fn spawn_sensitive_actions_are_marked() {
    let catalog = GatewayActionCatalog::standard();

    assert!(catalog.get("gateway.test").spawn_validation_required);
    assert!(catalog.get("gateway.add").spawn_validation_required);
    assert!(catalog.get("gateway.update").spawn_validation_required);
    assert!(
        catalog
            .get("gateway.import.approve")
            .spawn_validation_required
    );
}
