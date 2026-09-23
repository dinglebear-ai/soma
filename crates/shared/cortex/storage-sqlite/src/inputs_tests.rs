use super::*;

#[test]
fn event_input_wire_values_match_donor_contract() {
    assert_eq!(HookStatus::Configured.as_str(), "configured");
    assert_eq!(
        HookEvidenceKind::RuntimeTranscript.as_str(),
        "runtime_transcript"
    );
    assert_eq!(McpEventKind::Result.as_str(), "result");
    assert_eq!(
        SkillEventKind::CodexSkillBlock.as_str(),
        "codex_skill_block"
    );
    assert_eq!(
        SkillEvidenceKind::StructuredJsonField.as_str(),
        "structured_json_field"
    );
}
