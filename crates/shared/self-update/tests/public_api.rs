use soma_self_update::{
    ArtifactTransportPolicy, MigrationOutcome, RecoveryAction, UpdateDirective, UpdateLayout,
    UpdatePolicy, Updater,
};

#[tokio::test]
async fn public_contract_is_constructible_without_product_types() {
    let construction_dir = std::env::current_dir().unwrap();
    let fixture = tempfile::Builder::new()
        .prefix("self-update-public-api-")
        .tempdir_in(&construction_dir)
        .unwrap();
    let relative = fixture.path().strip_prefix(&construction_dir).unwrap();
    let executable = relative.join("bin/example");
    let state_file = relative.join("state/update.json");
    std::fs::create_dir_all(construction_dir.join(executable.parent().unwrap())).unwrap();
    std::fs::create_dir_all(construction_dir.join(state_file.parent().unwrap())).unwrap();
    std::fs::write(construction_dir.join(&executable), b"old").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            construction_dir.join(&executable),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }
    let layout = UpdateLayout::new(&executable, &state_file);
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
    updater
        .stage(&[][..], &directive)
        .await
        .unwrap()
        .cleanup()
        .unwrap();
    let migrated = MigrationOutcome::MigratedIndeterminate {
        updater,
        diagnostic: "directory sync must be retried".into(),
    };
    assert_eq!(
        migrated.updater().layout().state_file(),
        construction_dir.join(&state_file)
    );
    assert_eq!(
        migrated.into_updater().layout().executable(),
        construction_dir.join(&executable)
    );
}
