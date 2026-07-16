use serde_json::json;

use super::config::CodeModeResultShapePolicy;
use super::shape::shape_final_result;

#[test]
fn truncate_policy_shapes_large_result() {
    let shaped = shape_final_result(
        Some(json!({"body": "x".repeat(1000)})),
        CodeModeResultShapePolicy::Truncate,
        300,
        300,
        1,
    );
    assert!(shaped.metadata.changed);
    assert!(shaped.metadata.truncated);
}
