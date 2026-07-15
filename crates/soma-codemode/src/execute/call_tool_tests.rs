use serde_json::json;

use super::call_tool::call_host_tool;
use crate::host::NoopHost;
use crate::types::{CodeModeCaller, CodeModeSurface, ToolDescriptor, ToolScope};

#[tokio::test]
async fn scope_denial_happens_before_host_call() {
    let descriptor = ToolDescriptor::tool("git", "status", "status", None, None);
    let caller = CodeModeCaller::trusted_local("test");
    let error = call_host_tool(
        &NoopHost,
        &descriptor,
        json!({}),
        &caller,
        CodeModeSurface::Cli,
        &ToolScope::Tools(Default::default()),
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind(), "forbidden");
}
