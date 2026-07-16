use super::*;

#[test]
fn non_discovery_actions_require_admin_and_unknown_fails_closed() {
    let catalog = GatewayActionCatalog::standard();

    assert!(catalog
        .list()
        .into_iter()
        .filter(|action| !action.discovery)
        .all(|action| action.admin_required));
    assert!(catalog.get("gateway.nope").is_none());
}

#[test]
fn destructive_metadata_is_executable_test_data() {
    let remove = GatewayActionCatalog::standard()
        .get("gateway.remove")
        .expect("remove action");

    assert!(remove.destructive);
    assert!(remove.admin_required);
}

#[test]
fn spawn_sensitive_actions_are_marked() {
    let catalog = GatewayActionCatalog::standard();

    assert!(
        catalog
            .get("gateway.test")
            .expect("test action")
            .spawn_validation_required
    );
    assert!(
        catalog
            .get("gateway.add")
            .expect("add action")
            .spawn_validation_required
    );
    assert!(
        catalog
            .get("gateway.update")
            .expect("update action")
            .spawn_validation_required
    );
    assert!(
        catalog
            .get("gateway.import.approve")
            .expect("import approve action")
            .spawn_validation_required
    );
}
