use super::step::step_error;
use crate::host::StepDecision;

#[test]
fn step_error_has_stable_kind() {
    assert!(matches!(step_error("boom"), StepDecision::Error { kind, .. } if kind == "step_error"));
}
