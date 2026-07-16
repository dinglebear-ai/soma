use serde_json::json;

use super::schema::validate_code_mode_params_against_schema;

#[test]
fn validates_required_fields_and_types() {
    let schema = json!({
        "type": "object",
        "required": ["name"],
        "properties": {
            "name": {"type": "string"},
            "count": {"type": "integer"}
        },
        "additionalProperties": false
    });
    assert!(validate_code_mode_params_against_schema(
        &json!({"name": "ok", "count": 1}),
        Some(&schema)
    )
    .is_ok());
    assert!(validate_code_mode_params_against_schema(&json!({"count": 1}), Some(&schema)).is_err());
    assert!(validate_code_mode_params_against_schema(
        &json!({"name": "ok", "extra": true}),
        Some(&schema)
    )
    .is_err());
}
