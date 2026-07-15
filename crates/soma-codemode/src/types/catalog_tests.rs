use serde_json::json;

use super::catalog::{destructive_permitted, ToolDescriptor};

#[test]
fn tool_descriptor_generates_signature() {
    let descriptor = ToolDescriptor::tool("demo", "do.thing", "Do it", None, None);
    assert_eq!(descriptor.id, "demo::do.thing");
    assert!(descriptor.signature.contains("codemode.demo.do_thing"));
}

#[test]
fn destructive_permission_requires_confirm() {
    assert!(destructive_permitted(&json!({"confirm": true})));
    assert!(!destructive_permitted(&json!({})));
}
