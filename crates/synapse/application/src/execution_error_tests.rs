use soma_ops::OperationName;

use super::*;

#[test]
fn unsupported_operations_preserve_identity() {
    let operation = OperationName::new("container.restart").unwrap();
    assert_eq!(
        ExecutionError::UnsupportedOperation(operation).to_string(),
        "canonical runtime cannot execute container.restart"
    );
}
