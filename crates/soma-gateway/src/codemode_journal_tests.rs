use super::*;

#[test]
fn journal_payloads_use_shared_redaction() {
    let entry = JournalEntry {
        namespace: "axon".to_owned(),
        action: "search".to_owned(),
        payload: serde_json::json!({"api_key": "secret"}),
    };

    let rendered = entry.redacted().to_string();
    assert!(!rendered.contains("secret"));
    assert!(rendered.contains("[redacted]"));
}
