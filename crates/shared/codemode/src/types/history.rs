use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeModeHistoryKind {
    ToolCall,
    Step,
    Artifact,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodeModeHistoryEntry {
    pub kind: CodeModeHistoryKind,
    pub seq: u64,
    pub value: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CodeModeHistory {
    pub entries: Vec<CodeModeHistoryEntry>,
}
