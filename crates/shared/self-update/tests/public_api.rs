use soma_self_update::{
    ArtifactTransportPolicy, RecoveryAction, UpdateDirective, UpdateLayout, UpdatePolicy, Updater,
};

#[test]
fn public_contract_is_constructible_without_product_types() {
    let layout = UpdateLayout::new("/opt/example/bin/example", "/opt/example/state/update.json");
    let updater = Updater::new(layout, UpdatePolicy::default());
    let directive = UpdateDirective::new(
        "1.2.3",
        "/v1/agent/binary",
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    )
    .unwrap();
    assert_eq!(directive.version(), "1.2.3");
    assert_eq!(
        updater.policy().transport(),
        ArtifactTransportPolicy::HttpsOnly
    );
    assert!(matches!(
        RecoveryAction::NoPendingUpdate,
        RecoveryAction::NoPendingUpdate
    ));
}
