use serde_json::json;
use soma_ops::OperationName;

use super::*;

#[test]
fn result_validation_rejects_unknown_operations() {
    let operation = OperationName::new("unknown.read").unwrap();
    assert!(
        SynapseCatalog::embedded()
            .validate_result(&operation, &json!({}))
            .is_err()
    );
}
