use serde_json::Value;

use crate::ToolError;

use super::provider::StateProvider;

pub async fn dispatch_state(method: &str, params: Value) -> Result<Value, ToolError> {
    StateProvider::default().dispatch(method, params).await
}
