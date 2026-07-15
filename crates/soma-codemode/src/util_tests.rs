use serde_json::json;

use super::util::{serialized_size, unknown_local_provider, utf8_prefix_by_bytes};

#[test]
fn serialized_size_counts_json_bytes() {
    assert_eq!(serialized_size(&json!({"a": 1})), 7);
}

#[test]
fn utf8_prefix_keeps_char_boundaries() {
    let text = format!("{}clair", char::from_u32(0x00e9).unwrap());
    let first = char::from_u32(0x00e9).unwrap().to_string();
    assert_eq!(utf8_prefix_by_bytes(&text, 1), "");
    assert_eq!(utf8_prefix_by_bytes(&text, 2), first);
}

#[test]
fn unknown_provider_uses_soma_wording() {
    assert!(unknown_local_provider("openapi")
        .user_message()
        .contains("Code Mode"));
}
