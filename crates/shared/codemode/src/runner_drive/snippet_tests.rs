use super::snippet::snippet_tool_id;

#[test]
fn snippet_tool_ids_are_namespaced() {
    assert_eq!(snippet_tool_id("demo"), "snippet::demo");
}
