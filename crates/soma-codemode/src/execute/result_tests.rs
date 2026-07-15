use std::sync::{Arc, Mutex};

use serde_json::json;

use super::result::response_from_runner;
use crate::protocol::CodeModeRunnerResult;

#[test]
fn runner_result_maps_to_response_result() {
    let response = response_from_runner(
        CodeModeRunnerResult::Json(json!(3)),
        Arc::new(Mutex::new(None)),
    );
    assert_eq!(response.result, Some(json!(3)));
}
