use serde_json::json;

use super::*;

#[test]
fn parameter_readers_apply_defaults_and_reject_wrong_types() {
    assert!(bool_or(&json!({}), "all", true).unwrap());
    assert_eq!(u32_or(&json!({"lines": 25}), "lines", 100).unwrap(), 25);
    assert!(optional_str(&json!({"host": 7}), "host").is_err());
}
