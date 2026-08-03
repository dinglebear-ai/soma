use serde_json::json;
use soma_ops::OperationName;

use super::*;

#[tokio::test(flavor = "current_thread")]
async fn unsupported_mutations_fail_before_parameter_or_driver_access() {
    let runtime = crate::mutation_pull_test_support::runtime(None, None);
    let operation = OperationName::new("docker.prune").unwrap();
    let error = runtime
        .plan(
            &operation,
            &json!({}),
            &crate::mutation_pull_test_support::context(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, ExecutionError::UnsupportedOperation(name) if name == operation));
}
