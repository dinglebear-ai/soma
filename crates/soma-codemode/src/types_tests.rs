use super::types::{namespaced_tool_id, split_namespaced_id, CodeModeCaller, ToolScope};

#[test]
fn ids_and_scope_match_canonical_namespace_form() {
    let id = namespaced_tool_id("git", "status");
    assert_eq!(split_namespaced_id(&id), Some(("git", "status")));
    assert!(ToolScope::All.allows(&id));
    assert!(CodeModeCaller::trusted_local("me").capabilities.admin);
}
