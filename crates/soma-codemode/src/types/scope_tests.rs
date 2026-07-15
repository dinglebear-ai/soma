use std::collections::BTreeSet;

use super::scope::ToolScope;

#[test]
fn scope_filters_by_namespace_or_tool() {
    assert!(ToolScope::Namespaces(BTreeSet::from(["git".to_string()])).allows("git::status"));
    assert!(!ToolScope::Namespaces(BTreeSet::from(["git".to_string()])).allows("state::read"));
    assert!(ToolScope::Tools(BTreeSet::from(["git::status".to_string()])).allows("git::status"));
}
