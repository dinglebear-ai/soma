use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::catalog::UiLink;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodeModeExecutedCall {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodeModeExecutionError {
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodeModeExecutionResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub calls: Vec<CodeModeExecutedCall>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub logs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<CodeModeExecutionError>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<UiLink>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeModeExecutionSource {
    Inline,
    Snippet,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeModeSourceLookup {
    pub source: CodeModeExecutionSource,
    pub key: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeModeSourceStore {
    pub sources: Vec<CodeModeSourceLookup>,
}
