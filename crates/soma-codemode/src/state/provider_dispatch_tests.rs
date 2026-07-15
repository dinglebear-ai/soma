use serde_json::json;

use super::provider_dispatch::dispatch_state;

#[tokio::test]
async fn unknown_state_method_is_structured() {
    let error = dispatch_state("missing", json!({})).await.unwrap_err();
    assert_eq!(error.kind(), "unknown_action");
}
