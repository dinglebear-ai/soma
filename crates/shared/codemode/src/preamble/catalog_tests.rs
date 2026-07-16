use super::catalog::generate_js_proxy_from_catalog;
use crate::types::ToolDescriptor;

#[test]
fn proxy_contains_canonical_tool_id() {
    let js = generate_js_proxy_from_catalog(&[ToolDescriptor::tool(
        "github",
        "list.tags",
        "",
        None,
        None,
    )])
    .unwrap();
    assert!(js.contains("\"github::list.tags\""));
    assert!(js.contains("codemode.github.list_tags"));
}
