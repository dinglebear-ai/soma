use serde_json::json;
use soma_ops::OperationName;

use crate::SynapseCatalog;

#[test]
fn parameter_and_result_contracts_validate_closed_payloads() {
    let catalog = SynapseCatalog::embedded();
    let operation = OperationName::new("container.restart").unwrap();
    catalog
        .parameter_schema(&operation)
        .unwrap()
        .validate(
            &operation,
            "parameter",
            &json!({"host":"dookie","container_id":"soma"}),
        )
        .unwrap();
    catalog
        .result_schema(&operation)
        .unwrap()
        .validate(
            &operation,
            "result",
            &json!({"changed":true,"action":"restart","summary":"ok"}),
        )
        .unwrap();
}
