use serde_json::json;

use super::provider_dispatch::dispatch_git;

#[tokio::test]
async fn unknown_git_method_is_structured() {
    let error = dispatch_git("missing", json!({})).await.unwrap_err();
    assert_eq!(error.kind(), "unknown_action");
}
