use super::discovery::visible_tools;
use crate::types::{ToolDescriptor, ToolScope};

#[test]
fn discovery_filters_by_scope() {
    let entries = vec![
        ToolDescriptor::tool("git", "status", "", None, None),
        ToolDescriptor::tool("state", "read", "", None, None),
    ];
    let visible = visible_tools(
        &entries,
        &ToolScope::Namespaces(["git".to_string()].into_iter().collect()),
    );
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].namespace, "git");
}
