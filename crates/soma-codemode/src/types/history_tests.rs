use serde_json::json;

use super::history::{CodeModeHistory, CodeModeHistoryEntry, CodeModeHistoryKind};

#[test]
fn history_entry_round_trips() {
    let history = CodeModeHistory {
        entries: vec![CodeModeHistoryEntry {
            kind: CodeModeHistoryKind::ToolCall,
            seq: 1,
            value: json!({"ok": true}),
        }],
    };
    assert_eq!(
        serde_json::from_value::<CodeModeHistory>(serde_json::to_value(&history).unwrap()).unwrap(),
        history
    );
}
