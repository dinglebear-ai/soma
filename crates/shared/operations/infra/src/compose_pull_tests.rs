use super::*;
use soma_ops::{OperationId, OperationName, Timestamp};

#[test]
fn compose_pull_requests_validate_service_filters() {
    let project = ComposeProjectRef::new("soma", "/srv/soma/compose.yaml").unwrap();
    let operation = OperationName::new("compose.pull").unwrap();
    assert!(
        ComposePullRequest::new(
            OperationId::new(),
            operation.clone(),
            project.clone(),
            Some("--all".into()),
            Timestamp::now(),
        )
        .is_err()
    );
    let request = ComposePullRequest::new(
        OperationId::new(),
        operation,
        project,
        Some("api.v2".into()),
        Timestamp::now(),
    )
    .unwrap();
    assert_eq!(request.service(), Some("api.v2"));
}
