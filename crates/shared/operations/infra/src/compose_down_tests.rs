use super::*;
use soma_ops::{OperationId, OperationName, Timestamp};

#[test]
fn volume_removal_requires_force() {
    let project = ComposeProjectRef::new("soma", "/srv/soma/compose.yaml").unwrap();
    let expected =
        ComposeRecreateFingerprint::new("soma", vec!["api".into()], "a".repeat(64)).unwrap();
    assert!(
        ComposeDownRequest::new(
            OperationId::new(),
            OperationName::new("compose.down").unwrap(),
            project,
            expected,
            false,
            true,
            Timestamp::from_unix_millis(10),
        )
        .is_err()
    );
}
