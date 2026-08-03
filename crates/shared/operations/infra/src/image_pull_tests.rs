use super::*;
use soma_ops::{OperationId, OperationName, Timestamp};

#[test]
fn image_references_are_closed_and_canonicalized() {
    let operation = OperationName::new("docker.pull").unwrap();
    assert!(
        ImagePullRequest::new(
            OperationId::new(),
            operation.clone(),
            "--all",
            Timestamp::now(),
        )
        .is_err()
    );
    assert!(
        ImagePullRequest::new(OperationId::new(), operation, "bad image", Timestamp::now(),)
            .is_err()
    );
    assert_eq!(canonical_image_reference("alpine"), "alpine:latest");
    assert_eq!(canonical_image_reference("repo:v1"), "repo:v1");
    assert_eq!(
        canonical_image_reference("registry:5000/repo"),
        "registry:5000/repo:latest"
    );
}
