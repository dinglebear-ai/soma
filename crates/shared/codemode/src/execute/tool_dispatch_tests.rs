use crate::types::{CodeModeCaller, ToolScope};

use super::tool_dispatch::local_providers_allowed;

#[test]
fn local_providers_require_unscoped_admin_or_trusted_local() {
    let caller = CodeModeCaller::trusted_local("local");
    assert!(local_providers_allowed(&caller, &ToolScope::All));
    assert!(!local_providers_allowed(
        &caller,
        &ToolScope::Namespaces(["state".to_string()].into_iter().collect())
    ));
}
