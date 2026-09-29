use super::*;

#[test]
fn assessment_result_omits_optional_llm_fields() {
    let result = HookAssessResult {
        incident_id: "hook-1".into(),
        findings: hook_incident_findings::HookIncidentFindings::default(),
        assessment: None,
        prompt_preview: None,
    };
    let value = serde_json::to_value(result).unwrap();
    assert!(value.get("assessment").is_none());
    assert!(value.get("prompt_preview").is_none());
}

#[test]
fn error_signature_entry_preserves_operator_contract() {
    let json = serde_json::json!({
        "signature_hash":"abc", "template":"failed <n>", "sample_message":"failed 42",
        "severity":"err", "sample_hostname":"dookie", "sample_app_name":null,
        "first_seen_at":"2026-01-01T00:00:00Z", "last_seen_at":"2026-01-01T00:01:00Z",
        "total_count":3, "count_last_1h":2, "acknowledged_at":null
    });
    let entry: ErrorSignatureEntry = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(entry.total_count, 3);
    assert_eq!(serde_json::to_value(entry).unwrap(), json);
}
