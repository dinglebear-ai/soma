use serde_json::json;

use super::tool_call::executed_call;

#[test]
fn executed_call_preserves_result() {
    let call = executed_call("demo::call", None, Some(json!(true)));
    assert_eq!(call.result, Some(json!(true)));
}
