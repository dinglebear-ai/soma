use serde_json::json;

use super::host::*;
use super::types::{CodeModeCaller, CodeModeSurface, ToolScope};

#[tokio::test]
async fn noop_host_has_empty_catalog_and_unknown_calls() {
    let host = NoopHost;
    let caller = CodeModeCaller::trusted_local("test");
    let render = host
        .list_tools(&caller, CodeModeSurface::Cli, &ToolScope::All, true, false)
        .await
        .unwrap();
    assert_eq!(render.serialized_size, 2);
    let error = host
        .call_tool(
            "demo::missing",
            json!({}),
            &caller,
            CodeModeSurface::Cli,
            &ToolScope::All,
            ExecCtx::none(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.kind(), "unknown_action");
}
