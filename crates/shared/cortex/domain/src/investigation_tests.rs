use super::*;

#[test]
fn claim_type_uses_stable_snake_case_wire_values() {
    assert_eq!(
        serde_json::to_value(InvestigationClaimType::SupportedCorrelation).unwrap(),
        "supported_correlation"
    );
    assert_eq!(
        serde_json::from_value::<InvestigationClaimType>(serde_json::json!("open_question"))
            .unwrap(),
        InvestigationClaimType::OpenQuestion
    );
}
